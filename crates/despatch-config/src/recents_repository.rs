use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use despatch_application::{PortError, PortErrorKind, RecentLaunch, RecentLaunches};
use despatch_domain::LauncherId;
use serde::{Deserialize, Serialize};

use crate::atomic_write::{commit_sibling, stage_sibling};

/// The version written into the history file, so a later format can be told apart.
const RECENTS_VERSION: u32 = 1;

/// The machine-local launch history, as a small JSON file beside the default config.
///
/// It is kept out of `NSUserDefaults` because the safe setters there only take numbers and URLs,
/// and out of the shared config because it is usage, not configuration: two machines sharing one
/// config file each keep their own history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsonRecentLaunches {
    path: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
struct RecentsFile {
    version: u32,
    recent: Vec<RecentEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RecentEntry {
    launcher_id: LauncherId,
    /// Seconds since the Unix epoch.
    used_at: u64,
}

impl JsonRecentLaunches {
    #[must_use]
    pub const fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Where the history lives for a config at `config_path`: beside it, as `recents.json`.
    #[must_use]
    pub fn beside(config_path: &Path) -> Self {
        let directory = config_path.parent().unwrap_or_else(|| Path::new("."));
        Self::new(directory.join("recents.json"))
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl RecentLaunches for JsonRecentLaunches {
    /// Reads the history. A missing file is an empty history; a damaged one is too, because a
    /// convenience list is not worth refusing to start over — and it is never the shared config.
    fn load(&self) -> Result<Vec<RecentLaunch>, PortError> {
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(io_error(&self.path, &error)),
        };
        let Ok(file) = serde_json::from_slice::<RecentsFile>(&bytes) else {
            return Ok(Vec::new());
        };
        if file.version != RECENTS_VERSION {
            return Ok(Vec::new());
        }
        Ok(file
            .recent
            .into_iter()
            .map(|entry| RecentLaunch {
                launcher_id: entry.launcher_id,
                used_at: SystemTime::UNIX_EPOCH + Duration::from_secs(entry.used_at),
            })
            .collect())
    }

    fn save(&self, recents: &[RecentLaunch]) -> Result<(), PortError> {
        let file = RecentsFile {
            version: RECENTS_VERSION,
            recent: recents
                .iter()
                .map(|recent| RecentEntry {
                    launcher_id: recent.launcher_id,
                    used_at: recent
                        .used_at
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .map_or(0, |elapsed| elapsed.as_secs()),
                })
                .collect(),
        };
        let mut bytes = serde_json::to_vec_pretty(&file)
            .map_err(|error| PortError::new(PortErrorKind::InvalidData, error.to_string()))?;
        bytes.push(b'\n');
        let staged =
            stage_sibling(&self.path, &bytes).map_err(|error| io_error(&self.path, &error))?;
        commit_sibling(staged, &self.path).map_err(|error| io_error(&self.path, &error))
    }
}

fn io_error(path: &Path, error: &std::io::Error) -> PortError {
    let kind = match error.kind() {
        ErrorKind::NotFound => PortErrorKind::NotFound,
        ErrorKind::PermissionDenied => PortErrorKind::PermissionDenied,
        _ => PortErrorKind::Other,
    };
    PortError::new(kind, format!("{}: {error}", path.display()))
}
