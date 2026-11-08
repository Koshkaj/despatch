use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

use despatch_domain::{AppConfig, CURRENT_SCHEMA_VERSION, ExpansionContext, ValidationError};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::atomic_write::{commit_sibling, commit_sibling_noclobber, stage_sibling};
use crate::migration::{BACKUP_EXTENSION, MigrationError, migrate_v1_to_v2};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContentFingerprint([u8; 32]);

impl ContentFingerprint {
    #[must_use]
    pub fn of(contents: &[u8]) -> Self {
        Self(Sha256::digest(contents).into())
    }
}

impl Display for ContentFingerprint {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SaveExpectation {
    NewFile,
    Matches(ContentFingerprint),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedConfig {
    pub config: AppConfig,
    pub source_path: PathBuf,
    pub fingerprint: ContentFingerprint,
    /// Whether the file on disk still uses an older schema and was upgraded while loading.
    pub migrated: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct JsonConfigRepository;

impl JsonConfigRepository {
    pub fn load(path: impl AsRef<Path>) -> Result<LoadedConfig, ConfigError> {
        let path = path.as_ref();
        let contents = fs::read(path).map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })?;
        let Decoded { config, migrated } = decode(path, &contents)?;
        if migrated {
            // The file itself is only rewritten by the next save, so keep the original readable
            // until then.
            back_up(path, &contents)?;
        }
        Ok(LoadedConfig {
            config,
            source_path: path.to_owned(),
            fingerprint: ContentFingerprint::of(&contents),
            migrated,
        })
    }

    /// The fingerprint of the file's current contents. Only reads; nothing is decoded or backed up.
    pub fn fingerprint(path: impl AsRef<Path>) -> Result<ContentFingerprint, ConfigError> {
        let path = path.as_ref();
        fs::read(path)
            .map(|contents| ContentFingerprint::of(&contents))
            .map_err(|source| ConfigError::Read {
                path: path.to_owned(),
                source,
            })
    }

    pub fn encode(config: &AppConfig) -> Result<Vec<u8>, ConfigError> {
        config
            .validate()
            .map_err(|source| ConfigError::Validation { path: None, source })?;
        let mut encoded = serde_json::to_vec_pretty(config).map_err(ConfigError::Serialize)?;
        encoded.push(b'\n');
        Ok(encoded)
    }

    pub fn save(
        path: impl AsRef<Path>,
        config: &AppConfig,
        expectation: SaveExpectation,
    ) -> Result<ContentFingerprint, ConfigError> {
        let path = path.as_ref();
        let contents = Self::encode(config)?;
        check_expectation(path, config, expectation)?;
        let staged = stage_sibling(path, &contents).map_err(|source| ConfigError::AtomicWrite {
            path: path.to_owned(),
            source,
        })?;

        // Recheck immediately before the atomic replacement so time spent serializing and
        // syncing the staged file cannot hide a concurrent external edit.
        check_expectation(path, config, expectation)?;
        let commit = match expectation {
            SaveExpectation::NewFile => commit_sibling_noclobber,
            SaveExpectation::Matches(_) => commit_sibling,
        };
        commit(staged, path).map_err(|source| ConfigError::AtomicWrite {
            path: path.to_owned(),
            source,
        })?;
        Ok(ContentFingerprint::of(&contents))
    }
}

#[must_use]
pub fn config_expansion_context(config_path: &Path) -> ExpansionContext {
    ExpansionContext::new(config_path.parent().unwrap_or_else(|| Path::new(".")))
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not read config at {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("config at {path} is not valid JSON: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "config at {path} uses future schema version {found}; this app supports through {supported}"
    )]
    FutureSchemaVersion {
        path: PathBuf,
        found: u32,
        supported: u32,
    },
    #[error("config at {path} uses unsupported schema version {found}")]
    UnsupportedSchemaVersion { path: PathBuf, found: u32 },
    #[error("config validation failed{path_suffix}: {source}", path_suffix = path.as_ref().map(|value| format!(" for {}", value.display())).unwrap_or_default())]
    Validation {
        path: Option<PathBuf>,
        #[source]
        source: ValidationError,
    },
    #[error("could not serialize config: {0}")]
    Serialize(#[source] serde_json::Error),
    #[error("refusing to replace existing config at {path}")]
    AlreadyExists { path: PathBuf },
    #[error("config at {path} changed externally (expected {expected}, found {actual})")]
    Conflict {
        path: PathBuf,
        expected: ContentFingerprint,
        actual: ContentFingerprint,
    },
    #[error(
        "config revision must advance beyond {current_revision}; attempted {attempted_revision}"
    )]
    RevisionNotAdvanced {
        current_revision: u64,
        attempted_revision: u64,
    },
    #[error("could not migrate config at {path}: {source}")]
    Migration {
        path: PathBuf,
        #[source]
        source: MigrationError,
    },
    #[error("could not back up config at {path} before migrating it: {source}")]
    Backup {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not atomically save config at {path}: {source}")]
    AtomicWrite {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Deserialize)]
struct SchemaHeader {
    schema_version: u32,
}

struct Decoded {
    config: AppConfig,
    migrated: bool,
}

fn decode(path: &Path, contents: &[u8]) -> Result<Decoded, ConfigError> {
    let header: SchemaHeader =
        serde_json::from_slice(contents).map_err(|source| ConfigError::Parse {
            path: path.to_owned(),
            source,
        })?;
    if header.schema_version > CURRENT_SCHEMA_VERSION {
        return Err(ConfigError::FutureSchemaVersion {
            path: path.to_owned(),
            found: header.schema_version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }

    let config = match header.schema_version {
        CURRENT_SCHEMA_VERSION => parse(path, contents)?,
        1 => {
            let mut document: Value =
                serde_json::from_slice(contents).map_err(|source| ConfigError::Parse {
                    path: path.to_owned(),
                    source,
                })?;
            migrate_v1_to_v2(&mut document).map_err(|source| ConfigError::Migration {
                path: path.to_owned(),
                source,
            })?;
            serde_json::from_value(document).map_err(|source| ConfigError::Parse {
                path: path.to_owned(),
                source,
            })?
        }
        found => {
            return Err(ConfigError::UnsupportedSchemaVersion {
                path: path.to_owned(),
                found,
            });
        }
    };

    config
        .validate()
        .map_err(|source| ConfigError::Validation {
            path: Some(path.to_owned()),
            source,
        })?;
    Ok(Decoded {
        config,
        migrated: header.schema_version != CURRENT_SCHEMA_VERSION,
    })
}

fn parse(path: &Path, contents: &[u8]) -> Result<AppConfig, ConfigError> {
    serde_json::from_slice(contents).map_err(|source| ConfigError::Parse {
        path: path.to_owned(),
        source,
    })
}

/// Keeps a recoverable copy of a pre-migration file, without overwriting an earlier backup.
fn back_up(path: &Path, contents: &[u8]) -> Result<(), ConfigError> {
    let mut backup = path.as_os_str().to_owned();
    backup.push(".");
    backup.push(BACKUP_EXTENSION);
    let backup = PathBuf::from(backup);
    if backup.exists() {
        return Ok(());
    }
    fs::write(&backup, contents).map_err(|source| ConfigError::Backup {
        path: backup,
        source,
    })
}

fn check_expectation(
    path: &Path,
    new_config: &AppConfig,
    expectation: SaveExpectation,
) -> Result<(), ConfigError> {
    match expectation {
        SaveExpectation::NewFile => {
            if path.exists() {
                return Err(ConfigError::AlreadyExists {
                    path: path.to_owned(),
                });
            }
        }
        SaveExpectation::Matches(expected) => {
            let contents = fs::read(path).map_err(|source| ConfigError::Read {
                path: path.to_owned(),
                source,
            })?;
            let actual = ContentFingerprint::of(&contents);
            if actual != expected {
                return Err(ConfigError::Conflict {
                    path: path.to_owned(),
                    expected,
                    actual,
                });
            }
            let current = decode(path, &contents)?.config;
            if new_config.revision <= current.revision {
                return Err(ConfigError::RevisionNotAdvanced {
                    current_revision: current.revision,
                    attempted_revision: new_config.revision,
                });
            }
        }
    }
    Ok(())
}
