use std::collections::HashSet;
use std::path::PathBuf;

use crate::{
    AgentLauncher, AgentProvider, AppConfig, ExpansionContext, Group, LabelColor, Launcher,
    PathExpansionError, PathTemplate, Section, SshLauncher, SshPort, Subgroup, TextViolation,
    ValidationError,
};

fn ssh(name: &str) -> Launcher {
    Launcher::Ssh(SshLauncher::new(name, "example.com", "deploy").expect("valid SSH launcher"))
}

fn agent(name: &str, provider: AgentProvider) -> Launcher {
    Launcher::Agent(AgentLauncher::preset(provider, name).expect("valid agent launcher"))
}

/// A group holding one subgroup with the given launchers.
fn group_with(name: &str, section: Section, items: Vec<Launcher>) -> Group {
    let mut group = Group::new(name, section).expect("valid group");
    let mut subgroup = Subgroup::new("General").expect("valid subgroup");
    for item in items {
        subgroup
            .add_launcher(section, item)
            .expect("valid launcher for the section");
    }
    group.add_subgroup(subgroup).expect("add subgroup");
    group
}

#[test]
fn a_valid_empty_config_passes_validation() {
    AppConfig::default()
        .validate()
        .expect("an empty config is valid");
}

#[test]
fn duplicate_group_ids_fail_validation() {
    let group = Group::new("Production", Section::Servers).expect("valid group");
    let mut duplicate = group.clone();
    duplicate.name = "Staging".to_owned();
    let config = AppConfig {
        groups: vec![group, duplicate],
        ..AppConfig::default()
    };

    assert!(matches!(
        config.validate(),
        Err(ValidationError::DuplicateGroupId(_))
    ));
}

#[test]
fn group_names_are_unique_case_insensitively_within_a_section() {
    let config = AppConfig {
        groups: vec![
            Group::new("ÜBER", Section::Servers).expect("valid group"),
            Group::new("über", Section::Servers).expect("valid group"),
        ],
        ..AppConfig::default()
    };

    assert!(matches!(
        config.validate(),
        Err(ValidationError::DuplicateGroupName {
            section: Section::Servers,
            ..
        })
    ));
}

#[test]
fn same_group_name_is_allowed_once_per_section() {
    let config = AppConfig {
        groups: vec![
            Group::new("Production", Section::Servers).expect("valid group"),
            Group::new("Production", Section::Agents).expect("valid group"),
        ],
        ..AppConfig::default()
    };

    config
        .validate()
        .expect("sections have separate namespaces");
}

#[test]
fn whitespace_only_group_and_launcher_names_fail() {
    assert!(matches!(
        Group::new("   ", Section::Servers),
        Err(ValidationError::InvalidGroupName {
            violation: TextViolation::Empty,
            ..
        })
    ));
    assert!(matches!(
        SshLauncher::new("\t", "example.com", "deploy"),
        Err(ValidationError::InvalidLauncherField {
            field: "name",
            violation: TextViolation::Empty,
            ..
        })
    ));
}

#[test]
fn launcher_kinds_are_restricted_to_their_section() {
    let mut agents = Subgroup::new("Tools").expect("valid subgroup");
    assert!(matches!(
        agents.add_launcher(Section::Agents, ssh("Server")),
        Err(ValidationError::LauncherSectionMismatch { .. })
    ));

    let mut servers = Subgroup::new("Web").expect("valid subgroup");
    assert!(matches!(
        servers.add_launcher(Section::Servers, agent("Codex", AgentProvider::Codex)),
        Err(ValidationError::LauncherSectionMismatch { .. })
    ));
}

#[test]
fn subgroup_names_are_unique_case_insensitively_within_a_group() {
    let mut group = Group::new("Production", Section::Servers).expect("valid group");
    group
        .add_subgroup(Subgroup::new("Grafana").expect("valid subgroup"))
        .expect("first subgroup");

    assert!(matches!(
        group.add_subgroup(Subgroup::new("grafana").expect("valid subgroup")),
        Err(ValidationError::DuplicateSubgroupName { .. })
    ));
}

#[test]
fn the_same_subgroup_name_may_exist_in_two_groups() {
    let config = AppConfig {
        groups: vec![
            group_with("Production", Section::Servers, vec![ssh("API")]),
            group_with("Staging", Section::Servers, vec![ssh("API")]),
        ],
        ..AppConfig::default()
    };

    config
        .validate()
        .expect("groups have separate subgroup namespaces");
}

#[test]
fn whitespace_only_subgroup_names_fail() {
    assert!(matches!(
        Subgroup::new("  "),
        Err(ValidationError::InvalidSubgroupName {
            violation: TextViolation::Empty,
            ..
        })
    ));
}

#[test]
fn a_group_counts_the_launchers_of_every_subgroup() {
    let mut group = Group::new("Production", Section::Servers).expect("valid group");
    let mut grafana = Subgroup::new("Grafana").expect("valid subgroup");
    grafana
        .add_launcher(Section::Servers, ssh("Dashboards"))
        .expect("add launcher");
    let mut databases = Subgroup::new("Databases").expect("valid subgroup");
    databases
        .add_launcher(Section::Servers, ssh("Primary"))
        .expect("add launcher");
    databases
        .add_launcher(Section::Servers, ssh("Replica"))
        .expect("add launcher");
    group.add_subgroup(grafana).expect("add subgroup");
    group.add_subgroup(databases).expect("add subgroup");

    assert_eq!(group.launcher_count(), 3);
    assert_eq!(group.launchers().count(), 3);
}

#[test]
fn ssh_port_accepts_the_full_nonzero_u16_range() {
    assert_eq!(SshPort::new(1).map(SshPort::get), Some(1));
    assert_eq!(SshPort::new(u16::MAX).map(SshPort::get), Some(u16::MAX));
    assert_eq!(SshPort::new(0), None);
}

#[test]
fn single_value_fields_reject_nul_and_line_breaks() {
    for invalid_host in ["api\0.example", "api.example\n"] {
        assert!(matches!(
            SshLauncher::new("API", invalid_host, "deploy"),
            Err(ValidationError::InvalidLauncherField { field: "host", .. })
        ));
    }
    for invalid_user in ["de\0ploy", "deploy\r"] {
        assert!(matches!(
            SshLauncher::new("API", "api.example", invalid_user),
            Err(ValidationError::InvalidLauncherField { field: "user", .. })
        ));
    }

    let mut launcher = SshLauncher::new("API", "api.example", "deploy").expect("valid launcher");
    launcher.extra_args.push("-o\nProxyJump=host".to_owned());
    assert!(matches!(
        Launcher::Ssh(launcher).validate(),
        Err(ValidationError::InvalidLauncherField { field: "args", .. })
    ));

    assert_eq!(
        PathTemplate::new("bad\0path"),
        Err(TextViolation::ContainsNul)
    );
    assert_eq!(
        PathTemplate::new("bad\npath"),
        Err(TextViolation::ContainsLineBreak)
    );
    assert_eq!(PathTemplate::new(""), Err(TextViolation::Empty));
}

#[test]
fn built_in_agent_presets_have_expected_executables() {
    let cases = [
        (AgentProvider::Codex, "codex"),
        (AgentProvider::Claude, "claude"),
        (AgentProvider::Gemini, "gemini"),
        (AgentProvider::OpenCode, "opencode"),
    ];

    for (provider, expected) in cases {
        let launcher = AgentLauncher::preset(provider, expected).expect("valid preset");
        assert_eq!(launcher.executable, expected);
    }
}

#[test]
fn reorder_operations_preserve_ids_and_membership() {
    let mut production = group_with(
        "Production",
        Section::Servers,
        vec![ssh("API"), ssh("Database")],
    );
    let items = &mut production.subgroups[0];
    let original_item_ids: HashSet<_> = items.items.iter().map(Launcher::id).collect();
    let reversed_item_ids: Vec<_> = items.items.iter().rev().map(Launcher::id).collect();
    items
        .reorder_items(&reversed_item_ids)
        .expect("valid item ordering");
    assert_eq!(items.items[0].id(), reversed_item_ids[0]);
    assert_eq!(
        items.items.iter().map(Launcher::id).collect::<HashSet<_>>(),
        original_item_ids
    );

    let staging = Group::new("Staging", Section::Servers).expect("valid group");
    let agent_group = Group::new("Tools", Section::Agents).expect("valid group");
    let mut config = AppConfig {
        groups: vec![production, agent_group, staging],
        ..AppConfig::default()
    };
    let original_group_ids: HashSet<_> = config.groups.iter().map(|group| group.id).collect();
    let server_ids: Vec<_> = config
        .groups
        .iter()
        .filter(|group| group.section == Section::Servers)
        .rev()
        .map(|group| group.id)
        .collect();
    config
        .reorder_groups(Section::Servers, &server_ids)
        .expect("valid group ordering");
    assert_eq!(config.groups[0].id, server_ids[0]);
    assert_eq!(config.groups[1].section, Section::Agents);
    assert_eq!(
        config
            .groups
            .iter()
            .map(|group| group.id)
            .collect::<HashSet<_>>(),
        original_group_ids
    );
}

#[test]
fn moving_a_launcher_changes_only_parent_and_order() {
    let source = group_with(
        "Production",
        Section::Servers,
        vec![ssh("API"), ssh("Database")],
    );
    let target = group_with("Staging", Section::Servers, vec![ssh("Cache")]);
    let moved_id = source.subgroups[0].items[0].id();
    let untouched_id = source.subgroups[0].items[1].id();
    let source_id = source.subgroups[0].id;
    let target_first_id = target.subgroups[0].items[0].id();
    let target_id = target.subgroups[0].id;

    let mut config = AppConfig {
        groups: vec![source, target],
        ..AppConfig::default()
    };
    config
        .move_launcher(moved_id, source_id, target_id, 1)
        .expect("move launcher");

    assert_eq!(config.groups[0].subgroups[0].items.len(), 1);
    assert_eq!(config.groups[0].subgroups[0].items[0].id(), untouched_id);
    assert_eq!(
        config.groups[1].subgroups[0]
            .items
            .iter()
            .map(Launcher::id)
            .collect::<Vec<_>>(),
        vec![target_first_id, moved_id]
    );
    assert_eq!(config.groups[0].subgroups[0].id, source_id);
    assert_eq!(config.groups[1].subgroups[0].id, target_id);
    config.validate().expect("the move preserves every rule");
}

#[test]
fn a_launcher_can_move_between_subgroups_of_one_group() {
    let mut group = Group::new("Production", Section::Servers).expect("valid group");
    let mut grafana = Subgroup::new("Grafana").expect("valid subgroup");
    grafana
        .add_launcher(Section::Servers, ssh("Dashboards"))
        .expect("add launcher");
    let databases = Subgroup::new("Databases").expect("valid subgroup");
    let moved = grafana.items[0].id();
    let from = grafana.id;
    let to = databases.id;
    group.add_subgroup(grafana).expect("add subgroup");
    group.add_subgroup(databases).expect("add subgroup");

    let mut config = AppConfig {
        groups: vec![group],
        ..AppConfig::default()
    };
    config
        .move_launcher(moved, from, to, 0)
        .expect("move between subgroups");

    assert_eq!(config.groups[0].subgroups[0].items, [] as [Launcher; 0]);
    assert_eq!(config.groups[0].subgroups[1].items[0].id(), moved);
}

#[test]
fn portable_paths_expand_without_shell_evaluation() {
    let context = ExpansionContext::new("/shared/config")
        .with_variable("HOME", "/Users/example")
        .with_variable("PROJECT", "despatch");

    assert_eq!(
        PathTemplate::new("$HOME/.ssh/id_ed25519")
            .expect("valid template")
            .expand(&context)
            .expect("defined variable"),
        PathBuf::from("/Users/example/.ssh/id_ed25519")
    );
    assert_eq!(
        PathTemplate::new("${HOME}/src/${PROJECT}")
            .expect("valid template")
            .expand(&context)
            .expect("defined variables"),
        PathBuf::from("/Users/example/src/despatch")
    );
    assert_eq!(
        PathTemplate::new("../project")
            .expect("valid template")
            .expand(&context)
            .expect("relative path"),
        PathBuf::from("/shared/config/../project")
    );
    assert_eq!(
        PathTemplate::new("$$HOME/$(touch pwned);`id`")
            .expect("valid template")
            .expand(&context)
            .expect("literal shell syntax"),
        PathBuf::from("/shared/config/$HOME/$(touch pwned);`id`")
    );
    assert_eq!(
        PathTemplate::new("$MISSING/file")
            .expect("valid template")
            .expand(&context),
        Err(PathExpansionError::MissingVariable("MISSING".to_owned()))
    );
}

#[test]
fn a_color_belongs_to_a_group_or_to_a_single_launcher_and_to_nothing_in_between() {
    let yellow = LabelColor::from_rgb(0xFF, 0xCC, 0x00);
    let green = LabelColor::from_rgb(0x34, 0xC7, 0x59);

    let mut plain = SshLauncher::new("API", "api.example.com", "deploy").expect("valid SSH");
    assert_eq!(
        plain.color, None,
        "a launcher starts out following its group"
    );
    plain.color = Some(green);

    let mut group = group_with("Production", Section::Servers, vec![Launcher::Ssh(plain)]);
    group.color = Some(yellow);
    group.validate().expect("a colored group is valid");

    assert_eq!(group.color, Some(yellow));
    assert_eq!(
        group.launchers().next().expect("launcher").color(),
        Some(green)
    );
}

#[test]
fn a_launcher_color_survives_a_round_trip_and_is_absent_from_the_file_when_unset() {
    let mut ssh = SshLauncher::new("API", "api.example.com", "deploy").expect("valid SSH");
    let uncolored = serde_json::to_string(&ssh).expect("serialize");
    assert!(
        !uncolored.contains("color"),
        "an inherited color must not be written into the config: {uncolored}"
    );

    ssh.color = Some(LabelColor::from_rgb(0xAF, 0x52, 0xDE));
    let colored = serde_json::to_string(&ssh).expect("serialize");
    assert!(colored.contains("\"color\":\"#af52de\""), "{colored}");
    assert_eq!(
        serde_json::from_str::<SshLauncher>(&colored).expect("deserialize"),
        ssh
    );
}
