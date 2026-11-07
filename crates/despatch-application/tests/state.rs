use std::path::PathBuf;

use despatch_application::{AppState, ConfigDocument, ConfigFingerprint};
use despatch_domain::{
    AgentLauncher, AgentProvider, AppConfig, Group, Launcher, Section, SshLauncher, Subgroup,
};

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

fn state_with_groups() -> AppState {
    let servers = group_with(
        "Production",
        Section::Servers,
        vec![Launcher::Ssh(
            SshLauncher::new("API", "api.example.com", "deploy").expect("valid SSH"),
        )],
    );
    let mut codex = AgentLauncher::preset(AgentProvider::Codex, "Assistant").expect("valid agent");
    codex.args = vec!["--profile".to_owned(), "release".to_owned()];
    let agents = group_with(
        "Project tools",
        Section::Agents,
        vec![Launcher::Agent(codex)],
    );

    let mut state = AppState::default();
    state.load_document(ConfigDocument {
        config: AppConfig {
            groups: vec![servers, agents],
            ..AppConfig::default()
        },
        source_path: PathBuf::from("/tmp/config.json"),
        fingerprint: ConfigFingerprint::new("fingerprint").expect("valid fingerprint"),
    });
    state
}

#[test]
fn selecting_a_different_section_clears_the_whole_selection() {
    let mut state = state_with_groups();
    let server_subgroup = state.config.groups[0].subgroups[0].id;
    let server = state.config.groups[0].subgroups[0].items[0].id();

    assert!(state.select_subgroup(server_subgroup));
    assert!(state.select_launcher(server));
    state.select_section(Section::Agents);

    assert_eq!(state.selected_section, Section::Agents);
    assert_eq!(state.selected_group, None, "a section starts unselected");
    assert_eq!(state.selected_subgroup, None);
    assert_eq!(state.selected_launcher, None);
}

#[test]
fn deselecting_a_group_hides_its_subgroups_and_launchers() {
    let mut state = state_with_groups();
    let subgroup = state.config.groups[0].subgroups[0].id;
    let launcher = state.config.groups[0].subgroups[0].items[0].id();

    assert!(state.select_subgroup(subgroup));
    assert!(state.select_launcher(launcher));

    state.clear_group_selection();

    assert_eq!(state.selected_group, None);
    assert_eq!(state.selected_subgroup, None);
    assert_eq!(state.selected_launcher, None);
}

#[test]
fn deselecting_a_subgroup_keeps_the_group_open_but_hides_its_launchers() {
    let mut state = state_with_groups();
    let group = state.config.groups[0].id;
    let subgroup = state.config.groups[0].subgroups[0].id;
    let launcher = state.config.groups[0].subgroups[0].items[0].id();

    assert!(state.select_subgroup(subgroup));
    assert!(state.select_launcher(launcher));

    state.clear_subgroup_selection();

    assert_eq!(state.selected_group, Some(group));
    assert_eq!(state.selected_subgroup, None);
    assert_eq!(state.selected_launcher, None);
}

#[test]
fn selecting_a_group_collapses_any_open_subgroup() {
    let mut state = state_with_groups();
    let group = state.config.groups[0].id;
    let subgroup = state.config.groups[0].subgroups[0].id;

    assert!(state.select_subgroup(subgroup));
    assert!(state.select_group(group));

    assert_eq!(state.selected_group, Some(group));
    assert_eq!(state.selected_subgroup, None);
}

#[test]
fn selecting_a_launcher_opens_whatever_holds_it() {
    let mut state = state_with_groups();
    let server_subgroup = state.config.groups[0].subgroups[0].id;
    let agent_group = state.config.groups[1].id;
    let agent_subgroup = state.config.groups[1].subgroups[0].id;
    let agent = state.config.groups[1].subgroups[0].items[0].id();

    assert!(state.select_subgroup(server_subgroup));
    // The Agents section lists launchers flat and never opens a category, so a launcher must be
    // selectable without one already being open.
    assert!(state.select_launcher(agent));

    assert_eq!(state.selected_section, state.config.groups[1].section);
    assert_eq!(state.selected_group, Some(agent_group));
    assert_eq!(state.selected_subgroup, Some(agent_subgroup));
    assert_eq!(state.selected_launcher, Some(agent));
}

#[test]
fn a_launcher_that_is_in_no_config_cannot_be_selected() {
    let mut state = state_with_groups();

    assert!(!state.select_launcher(despatch_domain::LauncherId::new()));
    assert_eq!(state.selected_launcher, None);
}

#[test]
fn selecting_a_subgroup_also_opens_its_group_and_section() {
    let mut state = state_with_groups();
    let agent_subgroup = state.config.groups[1].subgroups[0].id;

    assert!(state.select_subgroup(agent_subgroup));

    assert_eq!(state.selected_section, Section::Agents);
    assert_eq!(state.selected_group, Some(state.config.groups[1].id));
    assert_eq!(state.selected_subgroup, Some(agent_subgroup));
}

#[test]
fn search_is_case_insensitive_across_names_and_hosts() {
    let mut state = state_with_groups();

    state.set_search_query("API.EXAMPLE");
    assert_eq!(state.search_results().launcher_ids.len(), 1);
    assert_eq!(
        state.search_results().launcher_ids[0],
        state.config.groups[0].subgroups[0].items[0].id()
    );

    state.set_search_query("production");
    let results = state.search_results();
    assert_eq!(results.group_ids, vec![state.config.groups[0].id]);
    assert_eq!(results.launcher_ids.len(), 1);
}

#[test]
fn agents_left_in_an_older_config_are_never_found() {
    let mut state = state_with_groups();

    for query in ["CODEX", "release", "project TOOLS", "Assistant"] {
        state.set_search_query(query);
        let results = state.search_results();
        assert!(results.launcher_ids.is_empty(), "{query} found an agent");
        assert!(results.group_ids.is_empty(), "{query} found an agent group");
    }
}

#[test]
fn searching_a_subgroup_name_matches_every_launcher_it_holds() {
    let mut state = state_with_groups();

    state.set_search_query("GENERAL");
    let results = state.search_results();

    assert_eq!(
        results.subgroup_ids.len(),
        1,
        "only the server group's category"
    );
    assert_eq!(results.launcher_ids.len(), 1);
}
