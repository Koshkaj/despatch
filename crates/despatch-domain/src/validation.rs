use crate::{GroupId, LauncherId, Section, SubgroupId};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextViolation {
    Empty,
    NotTrimmed,
    ContainsNul,
    ContainsLineBreak,
}

impl std::fmt::Display for TextViolation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::Empty => "must not be empty",
            Self::NotTrimmed => "must not start or end with whitespace",
            Self::ContainsNul => "must not contain a NUL byte",
            Self::ContainsLineBreak => "must not contain a line break",
        };
        formatter.write_str(message)
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ValidationError {
    #[error("unsupported schema version {found}; expected {expected}")]
    UnsupportedSchemaVersion { found: u32, expected: u32 },
    #[error("group {group_id} has an invalid name: {violation}")]
    InvalidGroupName {
        group_id: GroupId,
        violation: TextViolation,
    },
    #[error("subgroup {subgroup_id} has an invalid name: {violation}")]
    InvalidSubgroupName {
        subgroup_id: SubgroupId,
        violation: TextViolation,
    },
    #[error("launcher {launcher_id} field '{field}' {violation}")]
    InvalidLauncherField {
        launcher_id: LauncherId,
        field: &'static str,
        violation: TextViolation,
    },
    #[error("duplicate group id {0}")]
    DuplicateGroupId(GroupId),
    #[error("duplicate subgroup id {0}")]
    DuplicateSubgroupId(SubgroupId),
    #[error("duplicate launcher id {0}")]
    DuplicateLauncherId(LauncherId),
    #[error("group name '{name}' occurs more than once in the {section:?} section")]
    DuplicateGroupName { section: Section, name: String },
    #[error("subgroup name '{name}' occurs more than once in group {group_id}")]
    DuplicateSubgroupName { group_id: GroupId, name: String },
    #[error("launcher name '{name}' occurs more than once in subgroup {subgroup_id}")]
    DuplicateLauncherName {
        subgroup_id: SubgroupId,
        name: String,
    },
    #[error("launcher {launcher_id} is not allowed in a {section:?} group")]
    LauncherSectionMismatch {
        launcher_id: LauncherId,
        section: Section,
    },
    #[error("the supplied {entity} ordering does not contain every id exactly once")]
    InvalidOrdering { entity: &'static str },
    #[error("group {0} was not found")]
    GroupNotFound(GroupId),
    #[error("subgroup {0} was not found")]
    SubgroupNotFound(SubgroupId),
    #[error("launcher {launcher_id} was not found in subgroup {subgroup_id}")]
    LauncherNotFound {
        subgroup_id: SubgroupId,
        launcher_id: LauncherId,
    },
    #[error("index {index} is outside the valid insertion range 0..={len}")]
    InvalidInsertionIndex { index: usize, len: usize },
    #[error(
        "launcher {launcher_id} has an environment variable named '{name}'; a name must start with a letter or underscore and contain only letters, digits, and underscores"
    )]
    InvalidEnvironmentName {
        launcher_id: LauncherId,
        name: String,
    },
}

pub(crate) fn validate_trimmed_text(value: &str) -> Result<(), TextViolation> {
    if value.is_empty() {
        return Err(TextViolation::Empty);
    }
    if value.trim() != value {
        return Err(TextViolation::NotTrimmed);
    }
    validate_single_line_text(value)
}

pub(crate) fn validate_single_line_text(value: &str) -> Result<(), TextViolation> {
    if value.contains('\0') {
        return Err(TextViolation::ContainsNul);
    }
    if value.contains(['\r', '\n']) {
        return Err(TextViolation::ContainsLineBreak);
    }
    Ok(())
}
