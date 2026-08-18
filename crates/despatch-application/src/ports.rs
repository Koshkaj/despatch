use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use despatch_domain::{AppConfig, PathTemplate, SshPort};
use thiserror::Error;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ConfigFingerprint(String);

impl ConfigFingerprint {
    pub fn new(value: impl Into<String>) -> Result<Self, PortError> {
        let value = value.into();
        if value.is_empty() {
            return Err(PortError::new(
                PortErrorKind::InvalidData,
                "config fingerprint must not be empty",
            ));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigDocument {
    pub config: AppConfig,
    pub source_path: PathBuf,
    pub fingerprint: ConfigFingerprint,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepositorySaveExpectation {
    NewFile,
    Matches(ConfigFingerprint),
}

pub trait ConfigRepository: Send + Sync {
    fn load(&self, path: &Path) -> Result<ConfigDocument, PortError>;

    fn save(
        &self,
        path: &Path,
        config: &AppConfig,
        expectation: RepositorySaveExpectation,
    ) -> Result<ConfigFingerprint, PortError>;
}

/// Which appearance the interface is drawn in.
///
/// This is a machine-local preference, not a config setting: a shared config file must not decide
/// what someone else's screen looks like.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AppearanceChoice {
    /// Follow the system setting, and change with it.
    #[default]
    System,
    Light,
    Dark,
}

impl AppearanceChoice {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    /// The stored form. Kept stable: it is written into the user's local preferences.
    #[must_use]
    pub const fn code(self) -> i64 {
        match self {
            Self::System => 0,
            Self::Light => 1,
            Self::Dark => 2,
        }
    }

    /// Reads a stored value back, falling back to following the system for anything else. A
    /// preference nobody can parse is not worth failing over.
    #[must_use]
    pub const fn from_code(code: i64) -> Self {
        match code {
            1 => Self::Light,
            2 => Self::Dark,
            _ => Self::System,
        }
    }
}

pub trait LocalPreferences: Send + Sync {
    fn selected_config_path(&self) -> Result<Option<PathBuf>, PortError>;
    fn set_selected_config_path(&self, path: &Path) -> Result<(), PortError>;

    /// The appearance the user picked. Defaults to following the system.
    fn appearance(&self) -> Result<AppearanceChoice, PortError> {
        Ok(AppearanceChoice::System)
    }

    /// Remembers the appearance the user picked. A store that cannot remember it is not an error
    /// worth interrupting anyone over; the choice simply does not survive a restart.
    fn set_appearance(&self, _choice: AppearanceChoice) -> Result<(), PortError> {
        Ok(())
    }
}

pub trait TerminalLauncher: Send + Sync {
    fn launch(&self, request: &LaunchRequest) -> Result<(), PortError>;

    /// A human-readable name for the terminal launches will open in, for the settings sheet.
    ///
    /// Defaults to the platform's own association, which needs no explanation.
    fn destination(&self) -> String {
        "Your default terminal".to_owned()
    }
}

pub trait FileRevealer: Send + Sync {
    fn reveal(&self, path: &Path) -> Result<(), PortError>;
}

/// What to check before a server is saved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReachabilityRequest {
    pub host: String,
    pub port: SshPort,
    pub timeout: Duration,
}

/// What a reachability check found.
///
/// Every variant is a fact about the host, not about the user's credentials: the check opens a
/// socket and reads what the server volunteers, and never authenticates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Reachability {
    /// The port accepted a connection. `service` carries the server's own identification string
    /// when it sent one, which for SSH looks like `SSH-2.0-OpenSSH_9.6`.
    Open { service: Option<String> },
    /// The host answered and refused the port. Something is there; nothing is listening here.
    Refused,
    /// Nothing answered in time. A filtered port and an unreachable host look the same.
    TimedOut,
    /// The hostname does not resolve.
    Unresolved,
}

/// Finds the program a launcher names, the same way launching it would.
///
/// This exists so a launcher can be checked when it is written down rather than when it is first
/// run: a name that resolves to nothing is a typo, and the moment to say so is while the editor is
/// still open.
pub trait ExecutableLocator: Send + Sync {
    /// The resolved path, or the reason it could not be found.
    fn locate(&self, executable: &str) -> Result<PathBuf, PortError>;
}

/// Checks that something is listening where a server says it is.
///
/// This is deliberately weaker than a login: it must never send credentials, run `ssh`, or write
/// anything to the socket, so a check cannot lock an account or land in an auth log as a failure.
pub trait HostProbe: Send + Sync {
    fn probe(&self, request: &ReachabilityRequest) -> Result<Reachability, PortError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LaunchRequest {
    Ssh(SshLaunchRequest),
    Command(CommandLaunchRequest),
    RawShell(RawShellLaunchRequest),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SshLaunchRequest {
    pub title: String,
    pub host: String,
    pub user: String,
    pub port: SshPort,
    pub identity_file: Option<PathTemplate>,
    pub extra_args: Vec<String>,
    pub config_directory: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandLaunchRequest {
    pub title: String,
    pub executable: String,
    pub working_directory: Option<PathTemplate>,
    pub env: BTreeMap<String, String>,
    pub args: Vec<String>,
    pub config_directory: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawShellLaunchRequest {
    pub title: String,
    pub command: String,
    pub working_directory: Option<PathTemplate>,
    pub env: BTreeMap<String, String>,
    pub config_directory: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortErrorKind {
    NotFound,
    AlreadyExists,
    InvalidData,
    FutureVersion,
    Conflict,
    PermissionDenied,
    Unavailable,
    Other,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("{message}")]
pub struct PortError {
    kind: PortErrorKind,
    message: String,
}

impl PortError {
    #[must_use]
    pub fn new(kind: PortErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> PortErrorKind {
        self.kind
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}
