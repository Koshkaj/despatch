use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU16;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::validation::{validate_single_line_text, validate_trimmed_text};
use crate::{LabelColor, PathTemplate, ValidationError};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct LauncherId(Uuid);

impl LauncherId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for LauncherId {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for LauncherId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, formatter)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SshPort(NonZeroU16);

impl SshPort {
    pub const DEFAULT: Self = Self(NonZeroU16::new(22).expect("22 is non-zero"));

    pub fn new(value: u16) -> Option<Self> {
        NonZeroU16::new(value).map(Self)
    }

    #[must_use]
    pub const fn get(self) -> u16 {
        self.0.get()
    }
}

impl Default for SshPort {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentProvider {
    Codex,
    Claude,
    Gemini,
    OpenCode,
    Custom,
}

impl AgentProvider {
    #[must_use]
    pub const fn default_executable(self) -> Option<&'static str> {
        match self {
            Self::Codex => Some("codex"),
            Self::Claude => Some("claude"),
            Self::Gemini => Some("gemini"),
            Self::OpenCode => Some("opencode"),
            Self::Custom => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LauncherKind {
    Ssh,
    Agent,
    Custom,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SshLauncher {
    pub id: LauncherId,
    pub name: String,
    pub host: String,
    pub user: String,
    pub port: SshPort,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_file: Option<PathTemplate>,
    /// Overrides the color inherited from the group, or `None` to follow it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<LabelColor>,
    #[serde(default)]
    pub extra_args: Vec<String>,
}

impl SshLauncher {
    pub fn new(
        name: impl Into<String>,
        host: impl Into<String>,
        user: impl Into<String>,
    ) -> Result<Self, ValidationError> {
        let id = LauncherId::new();
        let name = name.into();
        let host = host.into();
        let user = user.into();
        let launcher = Self {
            id,
            name: normalize_launcher_field(id, "name", &name)?,
            host: normalize_launcher_field(id, "host", &host)?,
            user: normalize_launcher_field(id, "user", &user)?,
            port: SshPort::DEFAULT,
            identity_file: None,
            color: None,
            extra_args: Vec::new(),
        };
        launcher.validate()?;
        Ok(launcher)
    }

    fn validate(&self) -> Result<(), ValidationError> {
        validate_launcher_name(self.id, &self.name)?;
        validate_required_field(self.id, "host", &self.host)?;
        validate_required_field(self.id, "user", &self.user)?;
        if let Some(path) = &self.identity_file {
            path.validate()
                .map_err(|violation| ValidationError::InvalidLauncherField {
                    launcher_id: self.id,
                    field: "identity_file",
                    violation,
                })?;
        }
        validate_args(self.id, &self.extra_args)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentLauncher {
    pub id: LauncherId,
    pub name: String,
    pub provider: AgentProvider,
    pub executable: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<PathTemplate>,
    /// Overrides the color inherited from the group, or `None` to follow it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<LabelColor>,
    /// Environment variables exported before the command runs.
    ///
    /// Sorted so the config file does not churn, and so the launch script is reproducible.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub args: Vec<String>,
}

impl AgentLauncher {
    pub fn preset(
        provider: AgentProvider,
        name: impl Into<String>,
    ) -> Result<Self, ValidationError> {
        let executable = provider.default_executable().unwrap_or_default().to_owned();
        let id = LauncherId::new();
        let name = name.into();
        let launcher = Self {
            id,
            name: normalize_launcher_field(id, "name", &name)?,
            provider,
            executable,
            working_directory: None,
            color: None,
            env: BTreeMap::new(),
            args: Vec::new(),
        };
        launcher.validate()?;
        Ok(launcher)
    }

    fn validate(&self) -> Result<(), ValidationError> {
        validate_launcher_name(self.id, &self.name)?;
        validate_required_field(self.id, "executable", &self.executable)?;
        validate_env(self.id, &self.env)?;
        if let Some(path) = &self.working_directory {
            path.validate()
                .map_err(|violation| ValidationError::InvalidLauncherField {
                    launcher_id: self.id,
                    field: "working_directory",
                    violation,
                })?;
        }
        validate_args(self.id, &self.args)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum CustomCommand {
    Structured {
        executable: String,
        args: Vec<String>,
    },
    RawShell {
        command: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CustomLauncher {
    pub id: LauncherId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<PathTemplate>,
    /// Overrides the color inherited from the group, or `None` to follow it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<LabelColor>,
    /// Environment variables exported before the command runs.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(flatten)]
    pub command: CustomCommand,
}

impl CustomLauncher {
    pub fn structured(
        name: impl Into<String>,
        executable: impl Into<String>,
    ) -> Result<Self, ValidationError> {
        let id = LauncherId::new();
        let name = name.into();
        let executable = executable.into();
        let launcher = Self {
            id,
            name: normalize_launcher_field(id, "name", &name)?,
            working_directory: None,
            color: None,
            env: BTreeMap::new(),
            command: CustomCommand::Structured {
                executable: normalize_launcher_field(id, "executable", &executable)?,
                args: Vec::new(),
            },
        };
        launcher.validate()?;
        Ok(launcher)
    }

    fn validate(&self) -> Result<(), ValidationError> {
        validate_launcher_name(self.id, &self.name)?;
        validate_env(self.id, &self.env)?;
        if let Some(path) = &self.working_directory {
            path.validate()
                .map_err(|violation| ValidationError::InvalidLauncherField {
                    launcher_id: self.id,
                    field: "working_directory",
                    violation,
                })?;
        }
        match &self.command {
            CustomCommand::Structured { executable, args } => {
                validate_required_field(self.id, "executable", executable)?;
                validate_args(self.id, args)
            }
            CustomCommand::RawShell { command } => {
                if command.is_empty() {
                    return Err(ValidationError::InvalidLauncherField {
                        launcher_id: self.id,
                        field: "command",
                        violation: crate::TextViolation::Empty,
                    });
                }
                if command.contains('\0') {
                    return Err(ValidationError::InvalidLauncherField {
                        launcher_id: self.id,
                        field: "command",
                        violation: crate::TextViolation::ContainsNul,
                    });
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Launcher {
    Ssh(SshLauncher),
    Agent(AgentLauncher),
    Custom(CustomLauncher),
}

impl Launcher {
    #[must_use]
    pub const fn id(&self) -> LauncherId {
        match self {
            Self::Ssh(launcher) => launcher.id,
            Self::Agent(launcher) => launcher.id,
            Self::Custom(launcher) => launcher.id,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Ssh(launcher) => &launcher.name,
            Self::Agent(launcher) => &launcher.name,
            Self::Custom(launcher) => &launcher.name,
        }
    }

    /// The color the launcher was given explicitly, if any. `None` means it follows its group.
    #[must_use]
    pub const fn color(&self) -> Option<LabelColor> {
        match self {
            Self::Ssh(launcher) => launcher.color,
            Self::Agent(launcher) => launcher.color,
            Self::Custom(launcher) => launcher.color,
        }
    }

    /// The environment the launcher exports before running. SSH launchers export nothing: their
    /// environment is the remote host's.
    #[must_use]
    pub fn env(&self) -> BTreeMap<String, String> {
        match self {
            Self::Ssh(_) => BTreeMap::new(),
            Self::Agent(launcher) => launcher.env.clone(),
            Self::Custom(launcher) => launcher.env.clone(),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> LauncherKind {
        match self {
            Self::Ssh(_) => LauncherKind::Ssh,
            Self::Agent(_) => LauncherKind::Agent,
            Self::Custom(_) => LauncherKind::Custom,
        }
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::Ssh(launcher) => launcher.validate(),
            Self::Agent(launcher) => launcher.validate(),
            Self::Custom(launcher) => launcher.validate(),
        }
    }

    pub fn duplicate_named(&self, name: impl Into<String>) -> Result<Self, ValidationError> {
        let id = LauncherId::new();
        let name = name.into();
        let name = normalize_launcher_field(id, "name", &name)?;
        let mut duplicate = self.clone();
        match &mut duplicate {
            Self::Ssh(launcher) => {
                launcher.id = id;
                launcher.name = name;
            }
            Self::Agent(launcher) => {
                launcher.id = id;
                launcher.name = name;
            }
            Self::Custom(launcher) => {
                launcher.id = id;
                launcher.name = name;
            }
        }
        duplicate.validate()?;
        Ok(duplicate)
    }
}

fn validate_launcher_name(id: LauncherId, name: &str) -> Result<(), ValidationError> {
    validate_trimmed_text(name).map_err(|violation| ValidationError::InvalidLauncherField {
        launcher_id: id,
        field: "name",
        violation,
    })
}

fn normalize_launcher_field(
    id: LauncherId,
    field: &'static str,
    value: &str,
) -> Result<String, ValidationError> {
    validate_single_line_text(value).map_err(|violation| {
        ValidationError::InvalidLauncherField {
            launcher_id: id,
            field,
            violation,
        }
    })?;
    Ok(value.trim().to_owned())
}

fn validate_required_field(
    id: LauncherId,
    field: &'static str,
    value: &str,
) -> Result<(), ValidationError> {
    validate_trimmed_text(value).map_err(|violation| ValidationError::InvalidLauncherField {
        launcher_id: id,
        field,
        violation,
    })
}

/// Rejects environment entries a shell could not export as one literal assignment.
///
/// A name is checked against the POSIX portable character set rather than merely for line breaks:
/// a name carrying `=` or a space does not become a variable, it becomes another command.
fn validate_env(id: LauncherId, env: &BTreeMap<String, String>) -> Result<(), ValidationError> {
    for (name, value) in env {
        let valid_name = !name.is_empty()
            && !name.starts_with(|character: char| character.is_ascii_digit())
            && name
                .chars()
                .all(|character| character == '_' || character.is_ascii_alphanumeric());
        if !valid_name {
            return Err(ValidationError::InvalidEnvironmentName {
                launcher_id: id,
                name: name.clone(),
            });
        }
        validate_single_line_text(value).map_err(|violation| {
            ValidationError::InvalidLauncherField {
                launcher_id: id,
                field: "env",
                violation,
            }
        })?;
    }
    Ok(())
}

fn validate_args(id: LauncherId, args: &[String]) -> Result<(), ValidationError> {
    for arg in args {
        validate_single_line_text(arg).map_err(|violation| {
            ValidationError::InvalidLauncherField {
                launcher_id: id,
                field: "args",
                violation,
            }
        })?;
    }
    Ok(())
}
