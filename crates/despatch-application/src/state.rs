use std::path::PathBuf;

use despatch_domain::{
    AppConfig, CustomCommand, GroupId, Launcher, LauncherId, Section, SubgroupId,
};

use crate::{ConfigDocument, ConfigFingerprint, PortError, RecentLaunch};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigStatus {
    Unloaded,
    Clean,
    Dirty,
    Conflict,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Dialog {
    AddGroup { section: Section },
    EditGroup { group_id: GroupId },
    DeleteGroup { group_id: GroupId },
    AddSubgroup { group_id: GroupId },
    EditSubgroup { subgroup_id: SubgroupId },
    DeleteSubgroup { subgroup_id: SubgroupId },
    AddLauncher { subgroup_id: SubgroupId },
    EditLauncher { launcher_id: LauncherId },
    DeleteLauncher { launcher_id: LauncherId },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LaunchOutcome {
    Success {
        launcher_id: LauncherId,
    },
    Failure {
        launcher_id: LauncherId,
        error: PortError,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SearchResults {
    pub group_ids: Vec<GroupId>,
    pub subgroup_ids: Vec<SubgroupId>,
    pub launcher_ids: Vec<LauncherId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppState {
    pub config: AppConfig,
    pub source_path: Option<PathBuf>,
    pub fingerprint: Option<ConfigFingerprint>,
    pub config_status: ConfigStatus,
    pub selected_section: Section,
    pub selected_group: Option<GroupId>,
    pub selected_subgroup: Option<SubgroupId>,
    pub selected_launcher: Option<LauncherId>,
    pub search_query: String,
    pub dialog: Option<Dialog>,
    pub last_launch: Option<LaunchOutcome>,
    pub last_error: Option<PortError>,
    /// Launches remembered on this machine, newest first. Never part of the shared config.
    pub recents: Vec<RecentLaunch>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            config: AppConfig::default(),
            source_path: None,
            fingerprint: None,
            config_status: ConfigStatus::Unloaded,
            selected_section: Section::Servers,
            selected_group: None,
            selected_subgroup: None,
            selected_launcher: None,
            search_query: String::new(),
            dialog: None,
            last_launch: None,
            last_error: None,
            recents: Vec::new(),
        }
    }
}

impl AppState {
    pub fn load_document(&mut self, document: ConfigDocument) {
        self.config = document.config;
        self.source_path = Some(document.source_path);
        self.fingerprint = Some(document.fingerprint);
        self.config_status = ConfigStatus::Clean;
        self.last_error = None;
        self.repair_selection();
    }

    /// Switches sections. Nothing inside the section is selected, so the panel starts by showing
    /// only its groups.
    pub fn select_section(&mut self, section: Section) {
        self.selected_section = section;
        self.clear_group_selection();
    }

    pub fn select_group(&mut self, group_id: GroupId) -> bool {
        let Some(group) = self.config.groups.iter().find(|group| group.id == group_id) else {
            return false;
        };
        self.selected_section = group.section;
        self.selected_group = Some(group.id);
        self.selected_subgroup = None;
        self.selected_launcher = None;
        true
    }

    /// Deselects the group, which also hides its subgroups and launchers.
    pub fn clear_group_selection(&mut self) {
        self.selected_group = None;
        self.selected_subgroup = None;
        self.selected_launcher = None;
    }

    pub fn select_subgroup(&mut self, subgroup_id: SubgroupId) -> bool {
        let Some((group, _)) = self.config.locate_subgroup(subgroup_id) else {
            return false;
        };
        self.selected_section = group.section;
        self.selected_group = Some(group.id);
        self.selected_subgroup = Some(subgroup_id);
        self.selected_launcher = None;
        true
    }

    /// Deselects the subgroup, which hides its launchers but keeps the group open.
    pub fn clear_subgroup_selection(&mut self) {
        self.selected_subgroup = None;
        self.selected_launcher = None;
    }

    /// Selects a launcher wherever it lives, opening the group and category that hold it.
    ///
    /// Selection follows the launcher rather than requiring the launcher to be inside whatever is
    /// already open: "All", search results, and "Recently used" list launchers from categories that
    /// are not open.
    pub fn select_launcher(&mut self, launcher_id: LauncherId) -> bool {
        let Some((group, subgroup)) = self.config.locate_launcher(launcher_id) else {
            return false;
        };
        self.selected_section = group.section;
        self.selected_group = Some(group.id);
        self.selected_subgroup = Some(subgroup.id);
        self.selected_launcher = Some(launcher_id);
        true
    }

    pub fn set_search_query(&mut self, query: impl Into<String>) {
        self.search_query = query.into();
    }

    #[must_use]
    pub fn search_results(&self) -> SearchResults {
        let query = self.search_query.trim().to_lowercase();
        let mut results = SearchResults::default();
        // Only servers are searched. A config may still hold agent groups from an earlier version;
        // they are kept in the file but never shown.
        for group in self
            .config
            .groups
            .iter()
            .filter(|group| group.section == Section::Servers)
        {
            let group_matches = query.is_empty() || group.name.to_lowercase().contains(&query);
            let mut group_has_match = group_matches;
            for subgroup in &group.subgroups {
                let subgroup_matches =
                    group_matches || subgroup.name.to_lowercase().contains(&query);
                let matching_launchers: Vec<_> = subgroup
                    .items
                    .iter()
                    .filter(|launcher| subgroup_matches || launcher_matches(launcher, &query))
                    .map(Launcher::id)
                    .collect();
                if subgroup_matches || !matching_launchers.is_empty() {
                    group_has_match = true;
                    results.subgroup_ids.push(subgroup.id);
                    results.launcher_ids.extend(matching_launchers);
                }
            }
            if group_has_match {
                results.group_ids.push(group.id);
            }
        }
        results
    }

    /// Drops any selection that no longer exists, without silently selecting something else.
    pub fn repair_selection(&mut self) {
        let group_is_valid = self.selected_group.is_some_and(|selected| {
            self.config
                .groups
                .iter()
                .any(|group| group.id == selected && group.section == self.selected_section)
        });
        if !group_is_valid {
            self.clear_group_selection();
            return;
        }

        let subgroup_is_valid = self.selected_subgroup.is_some_and(|subgroup_id| {
            self.config
                .locate_subgroup(subgroup_id)
                .is_some_and(|(group, _)| Some(group.id) == self.selected_group)
        });
        if !subgroup_is_valid {
            self.clear_subgroup_selection();
            return;
        }

        let launcher_is_valid = self.selected_launcher.is_some_and(|launcher_id| {
            self.selected_subgroup.is_some_and(|subgroup_id| {
                self.config
                    .locate_subgroup(subgroup_id)
                    .is_some_and(|(_, subgroup)| {
                        subgroup.items.iter().any(|item| item.id() == launcher_id)
                    })
            })
        });
        if !launcher_is_valid {
            self.selected_launcher = None;
        }
    }
}

fn launcher_matches(launcher: &Launcher, query: &str) -> bool {
    if query.is_empty() || launcher.name().to_lowercase().contains(query) {
        return true;
    }
    match launcher {
        Launcher::Ssh(ssh) => [ssh.host.as_str(), ssh.user.as_str()]
            .iter()
            .any(|value| value.to_lowercase().contains(query)),
        Launcher::Agent(agent) => format!(
            "{:?} {} {} {}",
            agent.provider,
            agent.executable,
            agent
                .working_directory
                .as_ref()
                .map_or("", |path| path.as_str()),
            agent.args.join(" ")
        )
        .to_lowercase()
        .contains(query),
        Launcher::Custom(custom) => {
            let command = match &custom.command {
                CustomCommand::Structured { executable, args } => {
                    format!("{executable} {}", args.join(" "))
                }
                CustomCommand::RawShell { command } => command.clone(),
            };
            format!(
                "{} {command}",
                custom
                    .working_directory
                    .as_ref()
                    .map_or("", |path| path.as_str())
            )
            .to_lowercase()
            .contains(query)
        }
    }
}
