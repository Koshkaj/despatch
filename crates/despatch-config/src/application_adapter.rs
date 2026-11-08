use std::io::ErrorKind;
use std::path::Path;

use despatch_application::{
    ConfigDocument, ConfigFingerprint, ConfigRepository, PortError, PortErrorKind,
    RepositorySaveExpectation,
};
use despatch_domain::AppConfig;

use crate::{ConfigError, JsonConfigRepository, SaveExpectation};

impl ConfigRepository for JsonConfigRepository {
    fn load(&self, path: &Path) -> Result<ConfigDocument, PortError> {
        let loaded = Self::load(path).map_err(|error| port_error(&error))?;
        Ok(ConfigDocument {
            config: loaded.config,
            source_path: loaded.source_path,
            fingerprint: fingerprint(loaded.fingerprint.to_string()),
        })
    }

    fn fingerprint(&self, path: &Path) -> Result<ConfigFingerprint, PortError> {
        Self::fingerprint(path)
            .map(|current| fingerprint(current.to_string()))
            .map_err(|error| port_error(&error))
    }

    fn save(
        &self,
        path: &Path,
        config: &AppConfig,
        expectation: RepositorySaveExpectation,
    ) -> Result<ConfigFingerprint, PortError> {
        let expectation = match expectation {
            RepositorySaveExpectation::NewFile => SaveExpectation::NewFile,
            RepositorySaveExpectation::Matches(expected) => {
                let current = Self::load(path).map_err(|error| port_error(&error))?;
                if current.fingerprint.to_string() != expected.as_str() {
                    return Err(PortError::new(
                        PortErrorKind::Conflict,
                        "the config file changed externally",
                    ));
                }
                SaveExpectation::Matches(current.fingerprint)
            }
        };
        let saved = Self::save(path, config, expectation).map_err(|error| port_error(&error))?;
        Ok(fingerprint(saved.to_string()))
    }
}

fn fingerprint(value: String) -> ConfigFingerprint {
    ConfigFingerprint::new(value).expect("SHA-256 fingerprints are never empty")
}

fn port_error(error: &ConfigError) -> PortError {
    let kind = match error {
        ConfigError::Read { source, .. }
        | ConfigError::AtomicWrite { source, .. }
        | ConfigError::Backup { source, .. } => match source.kind() {
            ErrorKind::NotFound => PortErrorKind::NotFound,
            ErrorKind::AlreadyExists => PortErrorKind::AlreadyExists,
            ErrorKind::PermissionDenied => PortErrorKind::PermissionDenied,
            _ => PortErrorKind::Other,
        },
        ConfigError::Parse { .. }
        | ConfigError::UnsupportedSchemaVersion { .. }
        | ConfigError::Migration { .. }
        | ConfigError::Validation { .. }
        | ConfigError::Serialize(_) => PortErrorKind::InvalidData,
        ConfigError::FutureSchemaVersion { .. } => PortErrorKind::FutureVersion,
        ConfigError::AlreadyExists { .. } => PortErrorKind::AlreadyExists,
        ConfigError::Conflict { .. } | ConfigError::RevisionNotAdvanced { .. } => {
            PortErrorKind::Conflict
        }
    };
    PortError::new(kind, error.to_string())
}
