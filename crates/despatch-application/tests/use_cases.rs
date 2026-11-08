use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use despatch_application::{
    Application, ApplicationError, CommandLaunchRequest, ConfigDocument, ConfigFingerprint,
    ConfigRepository, ConfigStatus, ExternalChange, FileRevealer, HostProbe, LaunchRequest,
    LocalPreferences, PortError, PortErrorKind, Reachability, ReachabilityRequest, RecentLaunch,
    RecentLaunches, RepositorySaveExpectation, TerminalLauncher,
};
use despatch_domain::{
    AgentLauncher, AgentProvider, AppConfig, Group, Launcher, Section, SshLauncher, Subgroup,
};

#[derive(Clone, Debug)]
struct SaveCall {
    path: PathBuf,
    config: AppConfig,
}

#[derive(Default)]
struct RepositoryState {
    documents: HashMap<PathBuf, ConfigDocument>,
    save_calls: Vec<SaveCall>,
    next_save_error: Option<PortError>,
    fingerprint_sequence: u64,
}

#[derive(Default)]
struct FakeRepository {
    state: Mutex<RepositoryState>,
}

impl FakeRepository {
    fn seed(&self, path: &Path, config: AppConfig) {
        self.state
            .lock()
            .expect("repository lock")
            .documents
            .insert(
                path.to_owned(),
                ConfigDocument {
                    config,
                    source_path: path.to_owned(),
                    fingerprint: ConfigFingerprint::new("seed").expect("valid fingerprint"),
                },
            );
    }

    /// Replaces the file as another program would, leaving a fingerprint the app has not seen.
    fn edit_externally(&self, path: &Path, config: AppConfig) {
        self.state
            .lock()
            .expect("repository lock")
            .documents
            .insert(
                path.to_owned(),
                ConfigDocument {
                    config,
                    source_path: path.to_owned(),
                    fingerprint: ConfigFingerprint::new("external").expect("valid fingerprint"),
                },
            );
    }

    fn fail_next_save(&self, error: PortError) {
        self.state.lock().expect("repository lock").next_save_error = Some(error);
    }

    fn save_calls(&self) -> Vec<SaveCall> {
        self.state
            .lock()
            .expect("repository lock")
            .save_calls
            .clone()
    }

    fn document(&self, path: &Path) -> Option<ConfigDocument> {
        self.state
            .lock()
            .expect("repository lock")
            .documents
            .get(path)
            .cloned()
    }
}

impl ConfigRepository for FakeRepository {
    fn load(&self, path: &Path) -> Result<ConfigDocument, PortError> {
        self.document(path).ok_or_else(|| {
            PortError::new(
                PortErrorKind::NotFound,
                format!("{} not found", path.display()),
            )
        })
    }

    fn save(
        &self,
        path: &Path,
        config: &AppConfig,
        expectation: RepositorySaveExpectation,
    ) -> Result<ConfigFingerprint, PortError> {
        let mut state = self.state.lock().expect("repository lock");
        state.save_calls.push(SaveCall {
            path: path.to_owned(),
            config: config.clone(),
        });
        if let Some(error) = state.next_save_error.take() {
            return Err(error);
        }
        match expectation {
            RepositorySaveExpectation::NewFile if state.documents.contains_key(path) => {
                return Err(PortError::new(
                    PortErrorKind::AlreadyExists,
                    "config already exists",
                ));
            }
            RepositorySaveExpectation::Matches(expected) => {
                let actual = state
                    .documents
                    .get(path)
                    .ok_or_else(|| PortError::new(PortErrorKind::NotFound, "config not found"))?
                    .fingerprint
                    .clone();
                if actual != expected {
                    return Err(PortError::new(
                        PortErrorKind::Conflict,
                        "config changed externally",
                    ));
                }
            }
            RepositorySaveExpectation::NewFile => {}
        }
        state.fingerprint_sequence += 1;
        let fingerprint = ConfigFingerprint::new(format!("saved-{}", state.fingerprint_sequence))
            .expect("valid fingerprint");
        state.documents.insert(
            path.to_owned(),
            ConfigDocument {
                config: config.clone(),
                source_path: path.to_owned(),
                fingerprint: fingerprint.clone(),
            },
        );
        Ok(fingerprint)
    }
}

#[derive(Default)]
struct FakePreferences {
    selected_path: Mutex<Option<PathBuf>>,
    terminal: Mutex<Option<String>>,
}

impl LocalPreferences for FakePreferences {
    fn selected_config_path(&self) -> Result<Option<PathBuf>, PortError> {
        Ok(self.selected_path.lock().expect("preferences lock").clone())
    }

    fn set_selected_config_path(&self, path: &Path) -> Result<(), PortError> {
        *self.selected_path.lock().expect("preferences lock") = Some(path.to_owned());
        Ok(())
    }

    fn terminal(&self) -> Result<Option<String>, PortError> {
        Ok(self.terminal.lock().expect("preferences lock").clone())
    }

    fn set_terminal(&self, terminal: Option<&str>) -> Result<(), PortError> {
        *self.terminal.lock().expect("preferences lock") = terminal.map(ToOwned::to_owned);
        Ok(())
    }
}

#[derive(Default)]
struct FakeTerminal {
    requests: Mutex<Vec<LaunchRequest>>,
    picked: Mutex<Option<String>>,
}

impl TerminalLauncher for FakeTerminal {
    fn launch(&self, request: &LaunchRequest) -> Result<(), PortError> {
        self.requests
            .lock()
            .expect("terminal lock")
            .push(request.clone());
        Ok(())
    }

    fn chosen_terminal(&self) -> Option<String> {
        self.picked.lock().expect("terminal lock").clone()
    }

    fn choose_terminal(&self, id: Option<&str>) {
        *self.picked.lock().expect("terminal lock") = id.map(ToOwned::to_owned);
    }
}

#[derive(Default)]
struct FakeRevealer {
    paths: Mutex<Vec<PathBuf>>,
}

impl FileRevealer for FakeRevealer {
    fn reveal(&self, path: &Path) -> Result<(), PortError> {
        self.paths
            .lock()
            .expect("revealer lock")
            .push(path.to_owned());
        Ok(())
    }
}

/// The use-case tests never reach the network; a check always reports the host as unreachable.
struct OfflineProbe;

impl HostProbe for OfflineProbe {
    fn probe(&self, _: &ReachabilityRequest) -> Result<Reachability, PortError> {
        Ok(Reachability::TimedOut)
    }
}

struct Harness {
    app: Application,
    repository: Arc<FakeRepository>,
    preferences: Arc<FakePreferences>,
    terminal: Arc<FakeTerminal>,
    revealer: Arc<FakeRevealer>,
    path: PathBuf,
}

fn harness(config: AppConfig) -> Harness {
    let repository = Arc::new(FakeRepository::default());
    let preferences = Arc::new(FakePreferences::default());
    let terminal = Arc::new(FakeTerminal::default());
    let revealer = Arc::new(FakeRevealer::default());
    let path = PathBuf::from("/tmp/despatch-tests/config.json");
    repository.seed(&path, config);
    let mut app = Application::new(
        repository.clone(),
        preferences.clone(),
        terminal.clone(),
        revealer.clone(),
        Arc::new(OfflineProbe),
    );
    app.open_config(&path).expect("open seeded config");
    Harness {
        app,
        repository,
        preferences,
        terminal,
        revealer,
        path,
    }
}

fn ssh(name: &str) -> Launcher {
    Launcher::Ssh(SshLauncher::new(name, "example.com", "deploy").expect("valid SSH"))
}

/// A group holding one subgroup with the given launchers.
fn group_with(name: &str, section: Section, items: Vec<Launcher>) -> Group {
    let mut group = Group::new(name, section).expect("valid group");
    let mut subgroup = Subgroup::new("General").expect("valid subgroup");
    for item in items {
        subgroup
            .add_launcher(section, item)
            .expect("launcher fits the section");
    }
    group.add_subgroup(subgroup).expect("add subgroup");
    group
}

#[test]
fn opening_a_config_adds_nothing_to_it() {
    let config = AppConfig {
        groups: vec![Group::new("Production", Section::Servers).expect("valid group")],
        ..AppConfig::default()
    };
    let harness = harness(config.clone());

    assert_eq!(harness.app.state.config.groups, config.groups);
    assert!(
        harness.repository.save_calls().is_empty(),
        "opening is not an edit"
    );
}

#[test]
fn first_launch_creates_the_default_config_and_remembers_its_path() {
    let repository = Arc::new(FakeRepository::default());
    let preferences = Arc::new(FakePreferences::default());
    let terminal = Arc::new(FakeTerminal::default());
    let revealer = Arc::new(FakeRevealer::default());
    let path = Path::new("/tmp/despatch-first-launch/config.json");
    let mut app = Application::new(
        repository.clone(),
        preferences.clone(),
        terminal,
        revealer,
        Arc::new(OfflineProbe),
    );

    app.load_startup(path).expect("create default config");

    assert!(
        app.state.config.groups.is_empty(),
        "a first run starts empty"
    );
    // Identifiers are minted per config, so the shape is what is asserted, not equality.
    assert_eq!(
        app.state.config.schema_version,
        AppConfig::default().schema_version
    );
    assert_eq!(app.state.config.revision, 0);
    assert_eq!(app.state.config_status, ConfigStatus::Clean);
    let calls = repository.save_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].path, path);
    assert_eq!(
        preferences.selected_config_path().expect("read preference"),
        Some(path.to_owned())
    );
}

#[test]
fn every_crud_operation_persists_exactly_once() {
    let mut harness = harness(AppConfig::default());

    let production = harness
        .app
        .add_group("Production", Section::Servers)
        .expect("add production");
    assert_eq!(harness.repository.save_calls().len(), 1);
    harness
        .app
        .rename_group(production, "Primary")
        .expect("rename group");
    assert_eq!(harness.repository.save_calls().len(), 2);
    let web = harness
        .app
        .add_subgroup(production, "Web")
        .expect("add subgroup");
    assert_eq!(harness.repository.save_calls().len(), 3);
    harness
        .app
        .rename_subgroup(web, "Web tier")
        .expect("rename subgroup");
    assert_eq!(harness.repository.save_calls().len(), 4);
    let api = harness.app.add_launcher(web, ssh("API")).expect("add API");
    assert_eq!(harness.repository.save_calls().len(), 5);
    let database = harness
        .app
        .add_launcher(web, ssh("Database"))
        .expect("add database");
    assert_eq!(harness.repository.save_calls().len(), 6);
    let copy = harness
        .app
        .duplicate_launcher(web, api, "API copy")
        .expect("duplicate API");
    assert_eq!(harness.repository.save_calls().len(), 7);
    harness
        .app
        .reorder_launchers(web, &[copy, database, api])
        .expect("reorder launchers");
    assert_eq!(harness.repository.save_calls().len(), 8);

    let staging = harness
        .app
        .add_group("Staging", Section::Servers)
        .expect("add staging");
    let staging_web = harness
        .app
        .add_subgroup(staging, "Web")
        .expect("add staging subgroup");
    assert_eq!(harness.repository.save_calls().len(), 10);
    harness
        .app
        .move_launcher(copy, web, staging_web, 0)
        .expect("move copy");
    assert_eq!(harness.repository.save_calls().len(), 11);

    let mut replacement = harness.app.state.config.groups[0].subgroups[0]
        .items
        .iter()
        .find(|launcher| launcher.id() == api)
        .expect("find API")
        .clone();
    if let Launcher::Ssh(launcher) = &mut replacement {
        launcher.host = "api.internal".to_owned();
    }
    harness
        .app
        .edit_launcher(api, replacement)
        .expect("edit API");
    assert_eq!(harness.repository.save_calls().len(), 12);
    harness
        .app
        .delete_launcher(web, database)
        .expect("delete database");
    assert_eq!(harness.repository.save_calls().len(), 13);
    harness
        .app
        .reorder_groups(Section::Servers, &[staging, production])
        .expect("reorder groups");
    assert_eq!(harness.repository.save_calls().len(), 14);
    harness
        .app
        .delete_subgroup(staging_web)
        .expect("delete staging subgroup");
    assert_eq!(harness.repository.save_calls().len(), 15);
    harness.app.delete_group(staging).expect("delete staging");
    let calls = harness.repository.save_calls();
    assert_eq!(calls.len(), 16);
    assert_eq!(
        calls.last().expect("last save").config,
        harness.app.state.config
    );
    assert_eq!(harness.app.state.config_status, ConfigStatus::Clean);
}

#[test]
fn repository_failure_keeps_the_edited_state_dirty_and_recoverable() {
    let mut harness = harness(AppConfig::default());
    harness.repository.fail_next_save(PortError::new(
        PortErrorKind::PermissionDenied,
        "config is read-only",
    ));

    let result = harness.app.add_group("Production", Section::Servers);

    assert!(matches!(result, Err(ApplicationError::Repository(_))));
    assert_eq!(harness.app.state.config.groups[0].name, "Production");
    assert_eq!(harness.app.state.config.revision, 1);
    assert_eq!(harness.app.state.config_status, ConfigStatus::Dirty);
    assert_eq!(harness.repository.save_calls().len(), 1);
    harness.app.save_current().expect("retry save");
    assert_eq!(harness.app.state.config_status, ConfigStatus::Clean);
}

#[test]
fn a_config_file_nobody_else_touched_is_left_alone() {
    let mut harness = harness(AppConfig::default());
    harness
        .app
        .add_group("Production", Section::Servers)
        .expect("add group");

    assert_eq!(
        harness.app.check_config_file().expect("check"),
        ExternalChange::None,
        "the app's own save is not an outside edit"
    );
    assert_eq!(harness.app.state.config_status, ConfigStatus::Clean);
}

#[test]
fn an_edit_made_outside_the_app_is_loaded_when_nothing_here_is_unsaved() {
    let mut harness = harness(AppConfig::default());
    let mut edited = AppConfig::default();
    edited.groups.push(group_with(
        "Infraforge",
        Section::Servers,
        vec![ssh("alpha")],
    ));
    harness.repository.edit_externally(&harness.path, edited);

    assert_eq!(
        harness.app.check_config_file().expect("check"),
        ExternalChange::Reloaded
    );
    assert_eq!(harness.app.state.config.groups[0].name, "Infraforge");
    assert_eq!(harness.app.state.config_status, ConfigStatus::Clean);
    assert!(
        harness.repository.save_calls().is_empty(),
        "nothing written"
    );
}

#[test]
fn an_edit_made_outside_the_app_over_an_unsaved_change_is_a_conflict() {
    let mut harness = harness(AppConfig::default());
    harness
        .repository
        .fail_next_save(PortError::new(PortErrorKind::Other, "disk full"));
    assert!(
        harness
            .app
            .add_group("Local edit", Section::Servers)
            .is_err()
    );
    harness
        .repository
        .edit_externally(&harness.path, AppConfig::default());

    assert_eq!(
        harness.app.check_config_file().expect("check"),
        ExternalChange::Conflict
    );
    assert_eq!(harness.app.state.config_status, ConfigStatus::Conflict);
    assert_eq!(
        harness.app.state.config.groups[0].name, "Local edit",
        "the unsaved change is kept"
    );
}

#[test]
fn conflict_state_can_reload_external_data_or_save_a_copy() {
    let mut harness = harness(AppConfig::default());
    harness
        .repository
        .fail_next_save(PortError::new(PortErrorKind::Conflict, "external edit"));
    assert!(
        harness
            .app
            .add_group("Local edit", Section::Servers)
            .is_err()
    );
    assert_eq!(harness.app.state.config_status, ConfigStatus::Conflict);

    let copy_path = Path::new("/tmp/despatch-tests/config-copy.json");
    harness
        .app
        .save_copy(copy_path)
        .expect("save conflict copy");
    assert!(harness.repository.document(copy_path).is_some());

    harness.app.reload().expect("reload source");
    assert_eq!(harness.app.state.config.groups, [] as [Group; 0]);
    assert_eq!(harness.app.state.config_status, ConfigStatus::Clean);
}

#[test]
fn invalid_edit_never_calls_the_repository() {
    let production = group_with("Production", Section::Servers, vec![ssh("API")]);
    let mut harness = harness(AppConfig {
        groups: vec![production],
        ..AppConfig::default()
    });
    let group_id = harness.app.state.config.groups[0].id;

    assert!(harness.app.rename_group(group_id, "  ").is_err());
    assert_eq!(harness.repository.save_calls().len(), 0);
    assert_eq!(harness.app.state.config.groups[0].name, "Production");
}

#[test]
fn launch_selected_sends_one_structured_request_to_the_terminal() {
    let mut codex = AgentLauncher::preset(AgentProvider::Codex, "Codex").expect("valid preset");
    codex.args = vec!["--quiet".to_owned()];
    let agents = group_with("Tools", Section::Agents, vec![Launcher::Agent(codex)]);
    let subgroup_id = agents.subgroups[0].id;
    let launcher_id = agents.subgroups[0].items[0].id();
    let mut harness = harness(AppConfig {
        groups: vec![agents],
        ..AppConfig::default()
    });
    assert!(harness.app.state.select_subgroup(subgroup_id));
    assert!(harness.app.state.select_launcher(launcher_id));

    harness.app.launch_selected().expect("launch selected");

    let requests = harness.terminal.requests.lock().expect("terminal lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0],
        LaunchRequest::Command(CommandLaunchRequest {
            env: BTreeMap::new(),
            title: "Codex".to_owned(),
            executable: "codex".to_owned(),
            working_directory: None,
            args: vec!["--quiet".to_owned()],
            config_directory: harness.path.parent().expect("config parent").to_owned(),
        })
    );
}

#[test]
fn reveal_targets_only_the_active_config_file() {
    let harness = harness(AppConfig::default());

    harness.app.reveal_config().expect("reveal config");

    assert_eq!(
        *harness.revealer.paths.lock().expect("revealer lock"),
        vec![harness.path]
    );
}

#[test]
fn delete_removes_only_the_named_launcher() {
    let production = group_with(
        "Production",
        Section::Servers,
        vec![ssh("API"), ssh("Database")],
    );
    let subgroup_id = production.subgroups[0].id;
    let api = production.subgroups[0].items[0].id();
    let database = production.subgroups[0].items[1].id();
    let mut harness = harness(AppConfig {
        groups: vec![production],
        ..AppConfig::default()
    });

    harness
        .app
        .delete_launcher(subgroup_id, api)
        .expect("delete API");

    assert_eq!(
        harness.app.state.config.groups[0].subgroups[0].items.len(),
        1
    );
    assert_eq!(
        harness.app.state.config.groups[0].subgroups[0].items[0].id(),
        database
    );
}

#[test]
fn deleting_a_subgroup_removes_only_its_own_launchers() {
    let mut harness = harness(AppConfig::default());
    let production = harness
        .app
        .add_group("Production", Section::Servers)
        .expect("add group");
    let grafana = harness
        .app
        .add_subgroup(production, "Grafana")
        .expect("add subgroup");
    let databases = harness
        .app
        .add_subgroup(production, "Databases")
        .expect("add subgroup");
    harness
        .app
        .add_launcher(grafana, ssh("Dashboards"))
        .expect("add launcher");
    let kept = harness
        .app
        .add_launcher(databases, ssh("Primary"))
        .expect("add launcher");

    harness
        .app
        .delete_subgroup(grafana)
        .expect("delete subgroup");

    let group = &harness.app.state.config.groups[0];
    assert_eq!(group.subgroups.len(), 1);
    assert_eq!(group.subgroups[0].name, "Databases");
    assert_eq!(group.launcher_count(), 1);
    assert_eq!(group.subgroups[0].items[0].id(), kept);
}

#[test]
fn two_subgroups_of_one_group_cannot_share_a_name() {
    let mut harness = harness(AppConfig::default());
    let production = harness
        .app
        .add_group("Production", Section::Servers)
        .expect("add group");
    harness
        .app
        .add_subgroup(production, "Grafana")
        .expect("add subgroup");

    assert!(harness.app.add_subgroup(production, "grafana").is_err());
    assert_eq!(harness.app.state.config.groups[0].subgroups.len(), 1);
}

#[test]
fn a_renamed_subgroup_may_not_collide_with_a_sibling() {
    let mut harness = harness(AppConfig::default());
    let production = harness
        .app
        .add_group("Production", Section::Servers)
        .expect("add group");
    let grafana = harness
        .app
        .add_subgroup(production, "Grafana")
        .expect("add subgroup");
    harness
        .app
        .add_subgroup(production, "Databases")
        .expect("add subgroup");

    assert!(harness.app.rename_subgroup(grafana, "databases").is_err());
    harness
        .app
        .rename_subgroup(grafana, "Grafana dashboards")
        .expect("a free name is accepted");
    assert_eq!(
        harness.app.state.config.groups[0].subgroups[0].name,
        "Grafana dashboards"
    );
}

#[test]
fn deleting_a_group_clears_a_selection_that_pointed_into_it() {
    let mut harness = harness(AppConfig::default());
    let production = harness
        .app
        .add_group("Production", Section::Servers)
        .expect("add group");
    let web = harness
        .app
        .add_subgroup(production, "Web")
        .expect("add subgroup");
    let api = harness.app.add_launcher(web, ssh("API")).expect("add API");
    assert!(harness.app.state.select_subgroup(web));
    assert!(harness.app.state.select_launcher(api));

    harness.app.delete_group(production).expect("delete group");

    assert_eq!(harness.app.state.selected_group, None);
    assert_eq!(harness.app.state.selected_subgroup, None);
    assert_eq!(harness.app.state.selected_launcher, None);
}

#[test]
fn successful_open_updates_local_selected_path() {
    let harness = harness(AppConfig::default());

    assert_eq!(
        harness
            .preferences
            .selected_config_path()
            .expect("selected path"),
        Some(harness.path)
    );
}

// ---------------------------------------------------------------- recently used

#[derive(Default)]
struct FakeRecents {
    saved: Mutex<Vec<Vec<RecentLaunch>>>,
    stored: Mutex<Vec<RecentLaunch>>,
    fail: Mutex<bool>,
}

impl RecentLaunches for FakeRecents {
    fn load(&self) -> Result<Vec<RecentLaunch>, PortError> {
        Ok(self.stored.lock().expect("recents lock").clone())
    }

    fn save(&self, recents: &[RecentLaunch]) -> Result<(), PortError> {
        if *self.fail.lock().expect("recents lock") {
            return Err(PortError::new(PortErrorKind::PermissionDenied, "read-only"));
        }
        self.saved
            .lock()
            .expect("recents lock")
            .push(recents.to_vec());
        *self.stored.lock().expect("recents lock") = recents.to_vec();
        Ok(())
    }
}

struct RefusingTerminal;

impl TerminalLauncher for RefusingTerminal {
    fn launch(&self, _request: &LaunchRequest) -> Result<(), PortError> {
        Err(PortError::new(PortErrorKind::Unavailable, "no terminal"))
    }
}

/// A harness whose launches are remembered in `recents`, over three servers in one category.
fn recents_harness(recents: Arc<FakeRecents>) -> (Harness, Vec<despatch_domain::LauncherId>) {
    let servers = group_with(
        "Production",
        Section::Servers,
        vec![ssh("Primary"), ssh("Replica"), ssh("Bastion")],
    );
    let ids = servers.subgroups[0]
        .items
        .iter()
        .map(Launcher::id)
        .collect();
    let mut harness = harness(AppConfig {
        groups: vec![servers],
        ..AppConfig::default()
    });
    harness.app = harness.app.clone().with_recents(recents);
    (harness, ids)
}

fn launch(harness: &mut Harness, launcher_id: despatch_domain::LauncherId) {
    assert!(harness.app.state.select_launcher(launcher_id));
    harness.app.launch_selected().expect("launch");
}

#[test]
fn a_successful_launch_is_remembered_newest_first_without_duplicates() {
    let recents = Arc::new(FakeRecents::default());
    let (mut harness, ids) = recents_harness(recents.clone());

    launch(&mut harness, ids[0]);
    launch(&mut harness, ids[1]);
    launch(&mut harness, ids[0]);

    let remembered: Vec<_> = harness
        .app
        .recent_launches()
        .iter()
        .map(|recent| recent.launcher_id)
        .collect();
    assert_eq!(remembered, vec![ids[0], ids[1]]);
    assert_eq!(recents.saved.lock().expect("recents lock").len(), 3);
    // The history is machine-local: launching never writes the shared config.
    assert!(harness.repository.save_calls().is_empty());
}

#[test]
fn a_failed_launch_is_not_remembered() {
    let recents = Arc::new(FakeRecents::default());
    let (harness, ids) = recents_harness(recents.clone());
    let mut app = Application::new(
        harness.repository.clone(),
        harness.preferences.clone(),
        Arc::new(RefusingTerminal),
        harness.revealer.clone(),
        Arc::new(OfflineProbe),
    )
    .with_recents(recents.clone());
    app.open_config(&harness.path).expect("open config");
    assert!(app.state.select_launcher(ids[0]));

    assert!(app.launch_selected().is_err());

    assert_eq!(app.recent_launches(), [] as [RecentLaunch; 0]);
    assert!(recents.saved.lock().expect("recents lock").is_empty());
}

#[test]
fn a_history_that_cannot_be_written_never_fails_the_launch() {
    let recents = Arc::new(FakeRecents::default());
    *recents.fail.lock().expect("recents lock") = true;
    let (mut harness, ids) = recents_harness(recents);
    assert!(harness.app.state.select_launcher(ids[0]));

    harness
        .app
        .launch_selected()
        .expect("the launch still succeeds");

    assert_eq!(
        harness
            .terminal
            .requests
            .lock()
            .expect("terminal lock")
            .len(),
        1
    );
    assert_eq!(harness.app.recent_launches().len(), 1);
    assert!(
        harness
            .app
            .state
            .last_error
            .as_ref()
            .is_some_and(|error| error.message().contains("Recently used"))
    );
}

#[test]
fn launches_of_launchers_that_no_longer_exist_are_not_listed() {
    let recents = Arc::new(FakeRecents::default());
    let stranger = despatch_domain::LauncherId::new();
    recents
        .stored
        .lock()
        .expect("recents lock")
        .push(RecentLaunch {
            launcher_id: stranger,
            used_at: std::time::SystemTime::UNIX_EPOCH,
        });
    let (mut harness, ids) = recents_harness(recents.clone());
    assert_eq!(harness.app.recent_launches(), [] as [RecentLaunch; 0]);

    launch(&mut harness, ids[2]);

    // The stranger is dropped from the store on the next write, not just hidden.
    let stored = recents.stored.lock().expect("recents lock").clone();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].launcher_id, ids[2]);
}

#[test]
fn the_history_is_capped_and_can_be_cleared() {
    let recents = Arc::new(FakeRecents::default());
    let mut items = Vec::new();
    for index in 0..(Application::RECENT_LIMIT + 3) {
        items.push(ssh(&format!("Server {index}")));
    }
    let servers = group_with("Fleet", Section::Servers, items);
    let ids: Vec<_> = servers.subgroups[0]
        .items
        .iter()
        .map(Launcher::id)
        .collect();
    let mut harness = harness(AppConfig {
        groups: vec![servers],
        ..AppConfig::default()
    });
    harness.app = harness.app.clone().with_recents(recents.clone());
    for launcher_id in &ids {
        launch(&mut harness, *launcher_id);
    }

    let remembered = harness.app.recent_launches();
    assert_eq!(remembered.len(), Application::RECENT_LIMIT);
    assert_eq!(remembered[0].launcher_id, *ids.last().expect("a launcher"));

    harness.app.clear_recents().expect("clear");
    assert_eq!(harness.app.recent_launches(), [] as [RecentLaunch; 0]);
    assert!(recents.stored.lock().expect("recents lock").is_empty());
}

#[test]
fn a_picked_terminal_takes_effect_and_is_remembered_on_this_machine() {
    let harness = harness(AppConfig::default());

    harness
        .app
        .choose_terminal(Some("com.apple.Terminal"))
        .expect("pick a terminal");

    assert_eq!(
        harness.app.chosen_terminal().as_deref(),
        Some("com.apple.Terminal")
    );
    assert_eq!(
        harness.preferences.terminal().expect("read").as_deref(),
        Some("com.apple.Terminal")
    );
    harness.app.choose_terminal(None).expect("back to default");
    assert_eq!(harness.app.chosen_terminal(), None);
    assert_eq!(harness.preferences.terminal().expect("read"), None);
}

#[test]
fn the_remembered_terminal_is_applied_at_startup() {
    let mut harness = harness(AppConfig::default());
    harness
        .preferences
        .set_terminal(Some("com.mitchellh.ghostty"))
        .expect("remember a terminal");

    let path = harness.path.clone();
    harness.app.load_startup(&path).expect("start up");

    assert_eq!(
        harness.terminal.chosen_terminal().as_deref(),
        Some("com.mitchellh.ghostty")
    );
}
