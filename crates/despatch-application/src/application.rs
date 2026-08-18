use std::path::{Path, PathBuf};
use std::sync::Arc;

use despatch_domain::{
    AppConfig, CustomCommand, Group, GroupId, LabelColor, Launcher, LauncherId, Section, Subgroup,
    SubgroupId,
};

use crate::{
    AppState, ApplicationError, CommandLaunchRequest, ConfigDocument, ConfigRepository,
    ConfigStatus, FileRevealer, HostProbe, LaunchOutcome, LaunchRequest, LocalPreferences,
    PortError, PortErrorKind, RawShellLaunchRequest, Reachability, ReachabilityRequest,
    RepositorySaveExpectation, SshLaunchRequest, TerminalLauncher,
};

#[derive(Clone)]
pub struct Application {
    pub state: AppState,
    repository: Arc<dyn ConfigRepository>,
    preferences: Arc<dyn LocalPreferences>,
    terminal: Arc<dyn TerminalLauncher>,
    file_revealer: Arc<dyn FileRevealer>,
    host_probe: Arc<dyn HostProbe>,
    executables: Arc<dyn crate::ExecutableLocator>,
}

impl Application {
    #[must_use]
    pub fn new(
        repository: Arc<dyn ConfigRepository>,
        preferences: Arc<dyn LocalPreferences>,
        terminal: Arc<dyn TerminalLauncher>,
        file_revealer: Arc<dyn FileRevealer>,
        host_probe: Arc<dyn HostProbe>,
        executables: Arc<dyn crate::ExecutableLocator>,
    ) -> Self {
        Self {
            state: AppState::default(),
            repository,
            preferences,
            terminal,
            file_revealer,
            host_probe,
            executables,
        }
    }

    /// Checks that the program a launcher names can actually be found.
    ///
    /// A shell alias is not a program: it lives in the user's shell configuration and cannot be
    /// executed directly, so it fails here. That is the correct answer — a launcher naming an
    /// alias would never have run — and the way to reproduce one is an absolute path plus the
    /// environment variables the alias sets.
    pub fn locate_executable(&self, executable: &str) -> Result<std::path::PathBuf, PortError> {
        self.executables.locate(executable)
    }

    /// How long a reachability check waits before calling a host unreachable.
    pub const REACHABILITY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4);

    /// The checker a reachability probe runs on, cloned so the call can be made off the UI thread.
    #[must_use]
    pub fn host_probe(&self) -> Arc<dyn HostProbe> {
        Arc::clone(&self.host_probe)
    }

    /// Checks that something is listening at `host:port` without authenticating against it.
    pub fn check_reachability(
        probe: &Arc<dyn HostProbe>,
        host: impl Into<String>,
        port: despatch_domain::SshPort,
    ) -> Result<Reachability, ApplicationError> {
        probe
            .probe(&ReachabilityRequest {
                host: host.into(),
                port,
                timeout: Self::REACHABILITY_TIMEOUT,
            })
            .map_err(ApplicationError::HostProbe)
    }

    pub fn load_startup(&mut self, default_path: &Path) -> Result<(), ApplicationError> {
        let selected_path = self
            .preferences
            .selected_config_path()
            .map_err(ApplicationError::Preferences)?;
        let path = selected_path.as_deref().unwrap_or(default_path);
        match self.repository.load(path) {
            Ok(document) => self.activate_document(document),
            Err(error) if selected_path.is_none() && error.kind() == PortErrorKind::NotFound => {
                self.create_config(path)
            }
            Err(error) => {
                self.state.last_error = Some(error.clone());
                Err(ApplicationError::Repository(error))
            }
        }
    }

    pub fn open_config(&mut self, path: &Path) -> Result<(), ApplicationError> {
        let document = self
            .repository
            .load(path)
            .map_err(ApplicationError::Repository)?;
        self.activate_document(document)
    }

    /// Creates a config file, seeded with the agent CLIs this app knows how to run.
    pub fn create_config(&mut self, path: &Path) -> Result<(), ApplicationError> {
        let config = AppConfig::seeded();
        let fingerprint = self
            .repository
            .save(path, &config, RepositorySaveExpectation::NewFile)
            .map_err(ApplicationError::Repository)?;
        self.activate_document(ConfigDocument {
            config,
            source_path: path.to_owned(),
            fingerprint,
        })
    }

    pub fn save_copy(&mut self, path: &Path) -> Result<(), ApplicationError> {
        self.repository
            .save(path, &self.state.config, RepositorySaveExpectation::NewFile)
            .map_err(ApplicationError::Repository)?;
        Ok(())
    }

    pub fn reload(&mut self) -> Result<(), ApplicationError> {
        let path = self
            .state
            .source_path
            .clone()
            .ok_or(ApplicationError::NoConfigSource)?;
        let document = self
            .repository
            .load(&path)
            .map_err(ApplicationError::Repository)?;
        self.state.load_document(document);
        Ok(())
    }

    pub fn save_current(&mut self) -> Result<(), ApplicationError> {
        let path = self
            .state
            .source_path
            .clone()
            .ok_or(ApplicationError::NoConfigSource)?;
        let fingerprint = self
            .state
            .fingerprint
            .clone()
            .ok_or(ApplicationError::NoConfigSource)?;
        match self.repository.save(
            &path,
            &self.state.config,
            RepositorySaveExpectation::Matches(fingerprint),
        ) {
            Ok(fingerprint) => {
                self.state.fingerprint = Some(fingerprint);
                self.state.config_status = ConfigStatus::Clean;
                self.state.last_error = None;
                Ok(())
            }
            Err(error) => {
                self.state.config_status = if error.kind() == PortErrorKind::Conflict {
                    ConfigStatus::Conflict
                } else {
                    ConfigStatus::Dirty
                };
                self.state.last_error = Some(error.clone());
                Err(ApplicationError::Repository(error))
            }
        }
    }

    pub fn add_group(
        &mut self,
        name: impl Into<String>,
        section: Section,
    ) -> Result<GroupId, ApplicationError> {
        let group = Group::new(name, section)?;
        let id = group.id;
        self.apply_edit(move |config| {
            config.groups.push(group);
            Ok(())
        })?;
        Ok(id)
    }

    pub fn rename_group(
        &mut self,
        group_id: GroupId,
        name: impl Into<String>,
    ) -> Result<(), ApplicationError> {
        let name = name.into();
        self.apply_edit(move |config| {
            let group = find_group_mut(config, group_id)?;
            let validated = Group::with_id(group_id, name, group.section)?;
            group.name = validated.name;
            Ok(())
        })
    }

    /// Sets, or clears with `None`, the label color a group is drawn in.
    ///
    /// The color cascades to the group's categories and to every launcher in them that has no
    /// color of its own, so this is the only place a whole block of the interface is recolored.
    pub fn set_group_color(
        &mut self,
        group_id: GroupId,
        color: Option<LabelColor>,
    ) -> Result<(), ApplicationError> {
        self.apply_edit(move |config| {
            find_group_mut(config, group_id)?.color = color;
            Ok(())
        })
    }

    pub fn reorder_groups(
        &mut self,
        section: Section,
        ordered_ids: &[GroupId],
    ) -> Result<(), ApplicationError> {
        let ordered_ids = ordered_ids.to_vec();
        self.apply_edit(move |config| {
            config.reorder_groups(section, &ordered_ids)?;
            Ok(())
        })
    }

    pub fn delete_group(&mut self, group_id: GroupId) -> Result<(), ApplicationError> {
        self.apply_edit(move |config| {
            let index = config
                .groups
                .iter()
                .position(|group| group.id == group_id)
                .ok_or(ApplicationError::GroupNotFound(group_id))?;
            config.groups.remove(index);
            Ok(())
        })
    }

    pub fn add_subgroup(
        &mut self,
        group_id: GroupId,
        name: impl Into<String>,
    ) -> Result<SubgroupId, ApplicationError> {
        let subgroup = Subgroup::new(name)?;
        let id = subgroup.id;
        self.apply_edit(move |config| {
            find_group_mut(config, group_id)?.add_subgroup(subgroup)?;
            Ok(())
        })?;
        Ok(id)
    }

    pub fn rename_subgroup(
        &mut self,
        subgroup_id: SubgroupId,
        name: impl Into<String>,
    ) -> Result<(), ApplicationError> {
        let name = name.into();
        self.apply_edit(move |config| {
            let group = config
                .locate_subgroup(subgroup_id)
                .map(|(group, _)| group.id)
                .ok_or(ApplicationError::SubgroupNotFound(subgroup_id))?;
            let validated = Subgroup::with_id(subgroup_id, name)?;
            let group = find_group_mut(config, group)?;
            if group.subgroups.iter().any(|subgroup| {
                subgroup.id != subgroup_id
                    && subgroup.name.to_lowercase() == validated.name.to_lowercase()
            }) {
                return Err(ApplicationError::Validation(
                    despatch_domain::ValidationError::DuplicateSubgroupName {
                        group_id: group.id,
                        name: validated.name,
                    },
                ));
            }
            group
                .subgroup_mut(subgroup_id)
                .ok_or(ApplicationError::SubgroupNotFound(subgroup_id))?
                .name = validated.name;
            Ok(())
        })
    }

    pub fn delete_subgroup(&mut self, subgroup_id: SubgroupId) -> Result<(), ApplicationError> {
        self.apply_edit(move |config| {
            for group in &mut config.groups {
                if let Some(index) = group
                    .subgroups
                    .iter()
                    .position(|subgroup| subgroup.id == subgroup_id)
                {
                    group.subgroups.remove(index);
                    return Ok(());
                }
            }
            Err(ApplicationError::SubgroupNotFound(subgroup_id))
        })
    }

    pub fn reorder_subgroups(
        &mut self,
        group_id: GroupId,
        ordered_ids: &[SubgroupId],
    ) -> Result<(), ApplicationError> {
        let ordered_ids = ordered_ids.to_vec();
        self.apply_edit(move |config| {
            find_group_mut(config, group_id)?.reorder_subgroups(&ordered_ids)?;
            Ok(())
        })
    }

    pub fn add_launcher(
        &mut self,
        subgroup_id: SubgroupId,
        launcher: Launcher,
    ) -> Result<LauncherId, ApplicationError> {
        let launcher_id = launcher.id();
        self.apply_edit(move |config| {
            let (section, subgroup) = config
                .subgroup_mut(subgroup_id)
                .ok_or(ApplicationError::SubgroupNotFound(subgroup_id))?;
            subgroup.add_launcher(section, launcher)?;
            Ok(())
        })?;
        Ok(launcher_id)
    }

    pub fn edit_launcher(
        &mut self,
        launcher_id: LauncherId,
        replacement: Launcher,
    ) -> Result<(), ApplicationError> {
        if replacement.id() != launcher_id {
            return Err(ApplicationError::LauncherIdChanged {
                expected: launcher_id,
                found: replacement.id(),
            });
        }
        self.apply_edit(move |config| {
            let subgroup_id = config
                .locate_launcher(launcher_id)
                .map(|(_, subgroup)| subgroup.id)
                .ok_or(ApplicationError::LauncherNotFound(launcher_id))?;
            let (section, subgroup) = config
                .subgroup_mut(subgroup_id)
                .ok_or(ApplicationError::SubgroupNotFound(subgroup_id))?;
            if !section.accepts(&replacement) {
                return Err(ApplicationError::Validation(
                    despatch_domain::ValidationError::LauncherSectionMismatch {
                        launcher_id,
                        section,
                    },
                ));
            }
            let index = subgroup
                .items
                .iter()
                .position(|item| item.id() == launcher_id)
                .ok_or(ApplicationError::LauncherNotFound(launcher_id))?;
            subgroup.items[index] = replacement;
            Ok(())
        })
    }

    pub fn reorder_launchers(
        &mut self,
        subgroup_id: SubgroupId,
        ordered_ids: &[LauncherId],
    ) -> Result<(), ApplicationError> {
        let ordered_ids = ordered_ids.to_vec();
        self.apply_edit(move |config| {
            config
                .subgroup_mut(subgroup_id)
                .ok_or(ApplicationError::SubgroupNotFound(subgroup_id))?
                .1
                .reorder_items(&ordered_ids)?;
            Ok(())
        })
    }

    pub fn move_launcher(
        &mut self,
        launcher_id: LauncherId,
        source_subgroup_id: SubgroupId,
        target_subgroup_id: SubgroupId,
        target_index: usize,
    ) -> Result<(), ApplicationError> {
        self.apply_edit(move |config| {
            config.move_launcher(
                launcher_id,
                source_subgroup_id,
                target_subgroup_id,
                target_index,
            )?;
            Ok(())
        })
    }

    pub fn duplicate_launcher(
        &mut self,
        subgroup_id: SubgroupId,
        launcher_id: LauncherId,
        duplicate_name: impl Into<String>,
    ) -> Result<LauncherId, ApplicationError> {
        let duplicate_name = duplicate_name.into();
        let mut new_id = None;
        self.apply_edit(|config| {
            let (section, subgroup) = config
                .subgroup_mut(subgroup_id)
                .ok_or(ApplicationError::SubgroupNotFound(subgroup_id))?;
            let launcher = subgroup
                .items
                .iter()
                .find(|launcher| launcher.id() == launcher_id)
                .ok_or(ApplicationError::LauncherNotFound(launcher_id))?;
            let duplicate = launcher.duplicate_named(duplicate_name)?;
            new_id = Some(duplicate.id());
            subgroup.add_launcher(section, duplicate)?;
            Ok(())
        })?;
        new_id.ok_or(ApplicationError::LauncherNotFound(launcher_id))
    }

    pub fn delete_launcher(
        &mut self,
        subgroup_id: SubgroupId,
        launcher_id: LauncherId,
    ) -> Result<(), ApplicationError> {
        self.apply_edit(move |config| {
            let (_, subgroup) = config
                .subgroup_mut(subgroup_id)
                .ok_or(ApplicationError::SubgroupNotFound(subgroup_id))?;
            let index = subgroup
                .items
                .iter()
                .position(|launcher| launcher.id() == launcher_id)
                .ok_or(ApplicationError::LauncherNotFound(launcher_id))?;
            subgroup.items.remove(index);
            Ok(())
        })
    }

    pub fn launch_selected(&mut self) -> Result<(), ApplicationError> {
        let launcher_id = self
            .state
            .selected_launcher
            .ok_or(ApplicationError::NoLauncherSelected)?;
        let launcher = find_launcher(&self.state.config, launcher_id)?;
        let request = launch_request(
            launcher,
            config_directory(
                self.state
                    .source_path
                    .as_deref()
                    .ok_or(ApplicationError::NoConfigSource)?,
            )?,
        );
        match self.terminal.launch(&request) {
            Ok(()) => {
                self.state.last_launch = Some(LaunchOutcome::Success { launcher_id });
                Ok(())
            }
            Err(error) => {
                self.state.last_launch = Some(LaunchOutcome::Failure {
                    launcher_id,
                    error: error.clone(),
                });
                Err(ApplicationError::Terminal(error))
            }
        }
    }

    /// The appearance the user picked, or the system default when nothing is stored.
    #[must_use]
    pub fn appearance(&self) -> crate::AppearanceChoice {
        self.preferences.appearance().unwrap_or_default()
    }

    /// Remembers the appearance the user picked.
    pub fn set_appearance(&self, choice: crate::AppearanceChoice) -> Result<(), ApplicationError> {
        self.preferences
            .set_appearance(choice)
            .map_err(ApplicationError::Preferences)
    }

    /// The terminal a launch will open in, for the settings sheet.
    #[must_use]
    pub fn terminal_destination(&self) -> String {
        self.terminal.destination()
    }

    pub fn reveal_config(&self) -> Result<(), ApplicationError> {
        let path = self
            .state
            .source_path
            .as_deref()
            .ok_or(ApplicationError::NoConfigSource)?;
        self.file_revealer
            .reveal(path)
            .map_err(ApplicationError::FileReveal)
    }

    fn activate_document(&mut self, document: ConfigDocument) -> Result<(), ApplicationError> {
        self.preferences
            .set_selected_config_path(&document.source_path)
            .map_err(ApplicationError::Preferences)?;
        self.state.load_document(document);
        self.backfill_agent_defaults();
        Ok(())
    }

    /// Fills in the built-in agents when a freshly opened config has no Agents section.
    ///
    /// This happens in memory and writes nothing. Opening a file is not an edit, and rewriting
    /// someone's config the moment they open it — bumping its revision, racing another machine
    /// editing the same shared file — is a poor trade for launchers that work either way. The next
    /// real edit persists them along with whatever it was.
    /// The category that holds agents, creating the container if the config has none.
    ///
    /// Agents are presented as one flat list, so nothing in the interface asks which group or
    /// category a new one belongs to. The container still exists — a launcher always lives in a
    /// subgroup — it is simply never shown.
    pub fn agent_container(&mut self) -> Result<SubgroupId, ApplicationError> {
        if let Some(subgroup_id) = self.state.config.agent_container() {
            return Ok(subgroup_id);
        }
        let mut created = None;
        self.apply_edit(|config| {
            config.ensure_agent_defaults();
            created = config.agent_container();
            Ok(())
        })?;
        created.ok_or(ApplicationError::NoConfigSource)
    }

    fn backfill_agent_defaults(&mut self) {
        let _ = self.state.config.ensure_agent_defaults();
    }

    fn apply_edit(
        &mut self,
        edit: impl FnOnce(&mut AppConfig) -> Result<(), ApplicationError>,
    ) -> Result<(), ApplicationError> {
        if self.state.source_path.is_none() || self.state.fingerprint.is_none() {
            return Err(ApplicationError::NoConfigSource);
        }
        let mut edited = self.state.config.clone();
        edit(&mut edited)?;
        edited.validate()?;
        edited.revision = self
            .state
            .config
            .revision
            .checked_add(1)
            .ok_or(ApplicationError::RevisionOverflow)?;
        self.state.config = edited;
        self.state.config_status = ConfigStatus::Dirty;
        self.state.repair_selection();
        self.save_current()
    }
}

fn find_group_mut(
    config: &mut AppConfig,
    group_id: GroupId,
) -> Result<&mut Group, ApplicationError> {
    config
        .groups
        .iter_mut()
        .find(|group| group.id == group_id)
        .ok_or(ApplicationError::GroupNotFound(group_id))
}

fn find_launcher(
    config: &AppConfig,
    launcher_id: LauncherId,
) -> Result<&Launcher, ApplicationError> {
    config
        .launchers()
        .find(|(_, launcher)| launcher.id() == launcher_id)
        .map(|(_, launcher)| launcher)
        .ok_or(ApplicationError::LauncherNotFound(launcher_id))
}

fn config_directory(config_path: &Path) -> Result<PathBuf, ApplicationError> {
    config_path
        .parent()
        .map(Path::to_owned)
        .ok_or_else(|| ApplicationError::ConfigPathHasNoParent(config_path.to_owned()))
}

fn launch_request(launcher: &Launcher, config_directory: PathBuf) -> LaunchRequest {
    match launcher {
        Launcher::Ssh(ssh) => LaunchRequest::Ssh(SshLaunchRequest {
            title: ssh.name.clone(),
            host: ssh.host.clone(),
            user: ssh.user.clone(),
            port: ssh.port,
            identity_file: ssh.identity_file.clone(),
            extra_args: ssh.extra_args.clone(),
            config_directory,
        }),
        Launcher::Agent(agent) => LaunchRequest::Command(CommandLaunchRequest {
            title: agent.name.clone(),
            executable: agent.executable.clone(),
            working_directory: agent.working_directory.clone(),
            env: agent.env.clone(),
            args: agent.args.clone(),
            config_directory,
        }),
        Launcher::Custom(custom) => match &custom.command {
            CustomCommand::Structured { executable, args } => {
                LaunchRequest::Command(CommandLaunchRequest {
                    title: custom.name.clone(),
                    executable: executable.clone(),
                    working_directory: custom.working_directory.clone(),
                    env: custom.env.clone(),
                    args: args.clone(),
                    config_directory,
                })
            }
            CustomCommand::RawShell { command } => LaunchRequest::RawShell(RawShellLaunchRequest {
                title: custom.name.clone(),
                command: command.clone(),
                working_directory: custom.working_directory.clone(),
                env: custom.env.clone(),
                config_directory,
            }),
        },
    }
}
