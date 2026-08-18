use std::path::PathBuf;

use despatch_domain::{GroupId, LauncherId, SubgroupId, ValidationError};
use thiserror::Error;

use crate::PortError;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ApplicationError {
    #[error("no config file is currently open")]
    NoConfigSource,
    #[error("group {0} was not found")]
    GroupNotFound(GroupId),
    #[error("subgroup {0} was not found")]
    SubgroupNotFound(SubgroupId),
    #[error("launcher {0} was not found")]
    LauncherNotFound(LauncherId),
    #[error("an edited launcher must preserve its stable id {expected}; found {found}")]
    LauncherIdChanged {
        expected: LauncherId,
        found: LauncherId,
    },
    #[error("no launcher is selected")]
    NoLauncherSelected,
    #[error("config path {0} has no parent directory")]
    ConfigPathHasNoParent(PathBuf),
    #[error("config revision cannot advance beyond u64::MAX")]
    RevisionOverflow,
    #[error("the requested edit is invalid: {0}")]
    Validation(#[from] ValidationError),
    #[error("config persistence failed: {0}")]
    Repository(PortError),
    #[error("local preferences failed: {0}")]
    Preferences(PortError),
    #[error("terminal launch failed: {0}")]
    Terminal(PortError),
    #[error("Finder reveal failed: {0}")]
    FileReveal(PortError),
    #[error("the reachability check could not run: {0}")]
    HostProbe(PortError),
}
