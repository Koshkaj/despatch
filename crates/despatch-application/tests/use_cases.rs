use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use despatch_application::{
    Application, ApplicationError, CommandLaunchRequest, ConfigDocument, ConfigFingerprint,
    ConfigRepository, ConfigStatus, FileRevealer, HostProbe, LaunchRequest, LocalPreferences,
    PortError, PortErrorKind, Reachability, ReachabilityRequest, RepositorySaveExpectation,
    TerminalLauncher,
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
}

impl LocalPreferences for FakePreferences {
    fn selected_config_path(&self) -> Result<Option<PathBuf>, PortError> {
        Ok(self.selected_path.lock().expect("preferences lock").clone())
    }

    fn set_selected_config_path(&self, path: &Path) -> Result<(), PortError> {
        *self.selected_path.lock().expect("preferences lock") = Some(path.to_owned());
        Ok(())
    }
}

#[derive(Default)]
struct FakeTerminal {
    requests: Mutex<Vec<LaunchRequest>>,
}

impl TerminalLauncher for FakeTerminal {
    fn launch(&self, request: &LaunchRequest) -> Result<(), PortError> {
        self.requests
            .lock()
            .expect("terminal lock")
            .push(request.clone());
        Ok(())
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

/// The use-case tests accept any program name; resolution is the platform adapter's business.
struct AnyExecutable;

impl despatch_application::ExecutableLocator for AnyExecutable {
    fn locate(&self, executable: &str) -> Result<PathBuf, PortError> {
        Ok(PathBuf::from("/usr/bin").join(executable))
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
    let seeded_its_own_agents = config
        .groups
        .iter()
        .any(|group| group.section == Section::Agents);
    repository.seed(&path, config);
    let mut app = Application::new(
        repository.clone(),
        preferences.clone(),
        terminal.clone(),
        revealer.clone(),
        Arc::new(OfflineProbe),
        Arc::new(AnyExecutable),
    );
    app.open_config(&path).expect("open seeded config");
    // Opening a config backfills the built-in agents when it has no Agents section. These tests
    // are about the use cases rather than the backfill, so a test that asked for no agents keeps
    // none.
    if !seeded_its_own_agents {
        app.state
            .config
            .groups
            .retain(|group| group.section != Section::Agents);
    }
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
fn a_config_without_agents_gains_the_built_in_ones_on_open_without_being_rewritten() {
    // A config that predates the built-in agents, or has only ever held servers.
    let harness = harness(AppConfig {
        groups: vec![Group::new("Production", Section::Servers).expect("valid group")],
        ..AppConfig::default()
    });
    let repository = harness.repository.clone();
    let path = harness.path.clone();
    drop(harness);

    let mut app = Application::new(
        repository.clone(),
        Arc::new(FakePreferences::default()),
        Arc::new(FakeTerminal::default()),
        Arc::new(FakeRevealer::default()),
        Arc::new(OfflineProbe),
        Arc::new(AnyExecutable),
    );
    app.open_config(&path).expect("open config");

    let agents: Vec<&str> = app
        .state
        .config
        .groups
        .iter()
        .find(|group| group.section == Section::Agents)
        .expect("the Agents section is filled in")
        .subgroups[0]
        .items
        .iter()
        .map(despatch_domain::Launcher::name)
        .collect();
    assert_eq!(agents, ["Claude", "Codex", "Gemini", "OpenCode"]);
    assert!(
        repository.save_calls().is_empty(),
        "opening a config must not rewrite it"
    );
    assert_eq!(
        app.state.config_status,
        ConfigStatus::Clean,
        "and must not look like an unsaved edit"
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
        Arc::new(AnyExecutable),
    );

    app.load_startup(path).expect("create default config");

    // A first run is seeded rather than empty: the agent CLIs this app can build a launcher for
    // are there from the start, so the Agents section is not a dead end.
    let agents: Vec<&str> = app.state.config.groups[0].subgroups[0]
        .items
        .iter()
        .map(despatch_domain::Launcher::name)
        .collect();
    assert_eq!(agents, ["Claude", "Codex", "Gemini", "OpenCode"]);
    assert_eq!(app.state.config.groups[0].section, Section::Agents);
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
        calls[0].config.groups[0].subgroups[0].items.len(),
        4,
        "the seeded launchers are written, not just held in memory"
    );
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
    assert!(harness.app.state.config.groups.is_empty());
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
