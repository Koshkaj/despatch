#![doc = "Framework-free domain model and invariant logic for Despatch."]
#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc)]

mod color;
mod config;
mod group;
mod launcher;
mod path_template;
mod validation;

#[cfg(test)]
mod tests;

pub use color::{ColorViolation, LabelColor};
pub use config::{AppConfig, CURRENT_SCHEMA_VERSION};
pub use group::{Group, GroupId, Section, Subgroup, SubgroupId};
pub use launcher::{
    AgentLauncher, AgentProvider, CustomCommand, CustomLauncher, Launcher, LauncherId,
    LauncherKind, SshLauncher, SshPort,
};
pub use path_template::{ExpansionContext, PathExpansionError, PathTemplate};
pub use validation::{TextViolation, ValidationError};
