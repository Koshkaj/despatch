#![doc = "Versioned, conflict-aware JSON persistence for Despatch."]
#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc)]

mod application_adapter;
mod atomic_write;
mod json_repository;
mod migration;
mod recents_repository;

pub use json_repository::{
    ConfigError, ContentFingerprint, JsonConfigRepository, LoadedConfig, SaveExpectation,
    config_expansion_context,
};
pub use migration::{BACKUP_EXTENSION, DEFAULT_SUBGROUP_NAME, MigrationError};
pub use recents_repository::JsonRecentLaunches;
