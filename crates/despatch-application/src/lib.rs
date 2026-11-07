#![doc = "Framework-free application state, ports, and use cases for Despatch."]
#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc)]

mod application;
mod errors;
mod ports;
mod state;

pub use application::Application;
pub use errors::ApplicationError;
pub use ports::{
    AppearanceChoice, CommandLaunchRequest, ConfigDocument, ConfigFingerprint, ConfigRepository,
    EphemeralRecentLaunches, FileRevealer, HostProbe, LaunchRequest, LocalPreferences, PortError,
    PortErrorKind, RawShellLaunchRequest, Reachability, ReachabilityRequest, RecentLaunch,
    RecentLaunches, RepositorySaveExpectation, SshLaunchRequest, TerminalLauncher, TerminalOption,
};
pub use state::{AppState, ConfigStatus, Dialog, LaunchOutcome, SearchResults};
