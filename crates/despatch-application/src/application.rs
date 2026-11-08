use std::path::{Path, PathBuf};
use std::sync::Arc;

use despatch_domain::{
    AppConfig, CustomCommand, Group, GroupId, LabelColor, Launcher, LauncherId, Section, Subgroup,
    SubgroupId,
};

use crate::{
    AppState, ApplicationError, CommandLaunchRequest, ConfigDocument, ConfigRepository,
    ConfigStatus, EphemeralRecentLaunches, FileRevealer, HostProbe, LaunchOutcome, LaunchRequest,
    LocalPreferences, PortError, PortErrorKind, RawShellLaunchRequest, Reachability,
    ReachabilityRequest, RecentLaunch, RecentLaunches, RepositorySaveExpectation, SshLaunchRequest,
    TerminalLauncher,
};

#[derive(Clone)]
pub struct Application {
    pub state: AppState,
    repository: Arc<dyn ConfigRepository>,
    preferences: Arc<dyn LocalPreferences>,
    terminal: Arc<dyn TerminalLauncher>,
    file_revealer: Arc<dyn FileRevealer>,
    host_probe: Arc<dyn HostProbe>,
    recents: Arc<dyn RecentLaunches>,
}

impl Application {
    /// How many launches "Recently used" remembers.
    pub const RECENT_LIMIT: usize = 20;

    #[must_use]
    pub fn new(
        repository: Arc<dyn ConfigRepository>,
        preferences: Arc<dyn LocalPreferences>,
        terminal: Arc<dyn TerminalLauncher>,
        file_revealer: Arc<dyn FileRevealer>,
        host_probe: Arc<dyn HostProbe>,
    ) -> Self {
        Self {
            state: AppState::default(),
            repository,
            preferences,
            terminal,
            file_revealer,
            host_probe,
            recents: Arc::new(EphemeralRecentLaunches),
        }
    }

    /// Keeps the launch history in `store`, starting from what it already holds.
    ///
    /// A history that cannot be read starts empty rather than failing startup: it is a convenience,
    /// and nothing about the config depends on it.
    #[must_use]
    pub fn with_recents(mut self, store: Arc<dyn RecentLaunches>) -> Self {
        self.state.recents = store.load().unwrap_or_default();
        self.state.recents.truncate(Self::RECENT_LIMIT);
        self.recents = store;
        self
    }

    /// The remembered launches whose launcher still exists in the open config, newest first.
    ///
    /// Launchers deleted since, or belonging to another config, are left out rather than shown as
    /// entries that can no longer be opened.
    #[must_use]
    pub fn recent_launches(&self) -> Vec<RecentLaunch> {
        self.state
            .recents
            .iter()
            .filter(|recent| {
                self.state
                    .config
                    .locate_launcher(recent.launcher_id)
                    .is_some_and(|(group, _)| group.section == Section::Servers)
            })
            .copied()
            .collect()
    }

    /// Forgets every remembered launch.
    pub fn clear_recents(&mut self) -> Result<(), ApplicationError> {
        self.state.recents.clear();
        self.recents
            .save(&self.state.recents)
            .map_err(ApplicationError::Preferences)
    }

    /// Moves `launcher_id` to the front of the history and writes it.
    ///
    /// Entries for launchers the open config no longer has are dropped on the way, so the history
    /// cannot fill up with things that can never be opened again. A history that cannot be written
    /// never fails the launch it records: the terminal has already opened.
    fn remember_launch(&mut self, launcher_id: LauncherId) {
        let config = &self.state.config;
        self.state.recents.retain(|recent| {
            recent.launcher_id != launcher_id
                && config.locate_launcher(recent.launcher_id).is_some()
        });
        self.state.recents.insert(
            0,
            RecentLaunch {
                launcher_id,
                used_at: std::time::SystemTime::now(),
            },
        );
        self.state.recents.truncate(Self::RECENT_LIMIT);
        if let Err(error) = self.recents.save(&self.state.recents) {
            self.state.last_error = Some(PortError::new(
                error.kind(),
                format!("Recently used could not be saved: {}", error.message()),
            ));
        }
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
        self.restore_terminal();
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

    /// Creates an empty config file.
    pub fn create_config(&mut self, path: &Path) -> Result<(), ApplicationError> {
        let config = AppConfig::default();
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

    /// Looks at the config file on disk and takes in an edit made outside the app.
    ///
    /// Every change made here is saved at once and its fingerprint kept, so a file whose
    /// fingerprint differs was changed by something else: a sync service, a `git pull`, an editor.
    /// With nothing unsaved here it is loaded; with an unsaved change it is a conflict, and neither
    /// side is overwritten. The file is only decoded once its fingerprint has changed.
    pub fn check_config_file(&mut self) -> Result<ExternalChange, ApplicationError> {
        let Some(path) = self.state.source_path.clone() else {
            return Ok(ExternalChange::None);
        };
        let current = self
            .repository
            .fingerprint(&path)
            .map_err(ApplicationError::Repository)?;
        if self.state.fingerprint.as_ref() == Some(&current) {
            return Ok(ExternalChange::None);
        }
        match self.state.config_status {
            ConfigStatus::Clean | ConfigStatus::Unloaded => {
                let document = self
                    .repository
                    .load(&path)
                    .map_err(ApplicationError::Repository)?;
                self.state.load_document(document);
                Ok(ExternalChange::Reloaded)
            }
            ConfigStatus::Dirty | ConfigStatus::Conflict => {
                self.state.config_status = ConfigStatus::Conflict;
                Ok(ExternalChange::Conflict)
            }
        }
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
                self.remember_launch(launcher_id);
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

    /// The terminals installed here that launches can be sent to.
    #[must_use]
    pub fn terminal_options(&self) -> Vec<crate::TerminalOption> {
        self.terminal.available_terminals()
    }

    /// The name of the terminal the system's default goes to, when it names one.
    #[must_use]
    pub fn default_terminal_name(&self) -> Option<String> {
        self.terminal.default_terminal_name()
    }

    /// The terminal picked over the system's default, if any.
    #[must_use]
    pub fn chosen_terminal(&self) -> Option<String> {
        self.terminal.chosen_terminal()
    }

    /// Sends later launches to the terminal `id`, or back to the system's default with `None`,
    /// and remembers the choice on this machine. The choice takes effect even if it cannot be
    /// remembered.
    pub fn choose_terminal(&self, id: Option<&str>) -> Result<(), ApplicationError> {
        self.terminal.choose_terminal(id);
        self.preferences
            .set_terminal(id)
            .map_err(ApplicationError::Preferences)
    }

    /// Applies the terminal remembered from an earlier run. A preference that cannot be read
    /// leaves launches with the system's default, which is where they would go anyway.
    fn restore_terminal(&self) {
        if let Ok(Some(id)) = self.preferences.terminal() {
            self.terminal.choose_terminal(Some(&id));
        }
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
        Ok(())
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

/// What a look at the config file on disk found.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalChange {
    /// The file is what the app last loaded or saved.
    None,
    /// The file was changed outside the app and, with nothing unsaved here, has been loaded.
    Reloaded,
    /// The file was changed outside the app while a change here is unsaved; nothing was
    /// overwritten, and the config is in conflict until the user reloads or saves a copy.
    Conflict,
}
