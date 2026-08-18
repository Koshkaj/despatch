use despatch_application::{AppState, ConfigStatus, LaunchOutcome};
use despatch_domain::{
    AgentProvider, CustomCommand, GroupId, LabelColor, Launcher, LauncherId, LauncherKind, Section,
    SubgroupId,
};

use crate::icons::Symbol;
use crate::style::Accent;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupRow {
    pub id: GroupId,
    pub name: String,
    pub subgroup_count: usize,
    pub item_count: usize,
    pub section: Section,
    pub color: Option<LabelColor>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubgroupRow {
    pub id: SubgroupId,
    pub name: String,
    pub item_count: usize,
    pub color: Option<LabelColor>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tile {
    pub id: LauncherId,
    pub name: String,
    pub detail: String,
    pub kind: LauncherKind,
    pub kind_label: &'static str,
    pub symbol: Symbol,
    pub accent: Accent,
    /// The launcher's own color, or the color it inherits from its group.
    pub color: Option<LabelColor>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatusPresentation {
    pub label: String,
    pub is_error: bool,
}

/// The two lines every card in the config draws, split by the kind of card that draws them.
///
/// It covers the whole config rather than the open section, so one card width can be chosen for
/// the entire interface: cards that change size as you move between Servers and Agents read as two
/// unrelated designs, not one.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CardLabels {
    pub groups: Vec<(String, String)>,
    pub launchers: Vec<(String, String)>,
}

#[must_use]
pub fn card_labels(state: &AppState) -> CardLabels {
    let mut labels = CardLabels::default();
    for group in &state.config.groups {
        labels.groups.push((
            group.name.clone(),
            format!(
                "{} · {}",
                subgroup_count(group.subgroups.len()),
                launcher_count(group.launcher_count())
            ),
        ));
        for launcher in group.launchers() {
            labels
                .launchers
                .push((launcher.name().to_owned(), detail(launcher)));
        }
    }
    labels
}

#[must_use]
pub fn groups(state: &AppState, section: Section) -> Vec<GroupRow> {
    state
        .config
        .groups
        .iter()
        .filter(|group| group.section == section)
        .map(|group| GroupRow {
            id: group.id,
            name: group.name.clone(),
            subgroup_count: group.subgroups.len(),
            item_count: group.launcher_count(),
            section: group.section,
            color: group.color,
        })
        .collect()
}

/// The categories of the selected group, or nothing while no group is open.
#[must_use]
pub fn subgroups(state: &AppState) -> Vec<SubgroupRow> {
    selected_group(state)
        .map(|group| {
            group
                .subgroups
                .iter()
                .map(|subgroup| SubgroupRow {
                    id: subgroup.id,
                    name: subgroup.name.clone(),
                    item_count: subgroup.items.len(),
                    // A category has no color of its own; it wears its group's.
                    color: group.color,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The launchers of the selected subgroup. Nothing is listed while no subgroup is open.
#[must_use]
pub fn selected_tiles(state: &AppState) -> Vec<Tile> {
    let query = state.search_query.trim().to_lowercase();
    let Some(subgroup_id) = state.selected_subgroup else {
        return Vec::new();
    };
    state
        .config
        .locate_subgroup(subgroup_id)
        .map(|(group, subgroup)| {
            subgroup
                .items
                .iter()
                .filter(|launcher| {
                    query.is_empty() || searchable_text(launcher).to_lowercase().contains(&query)
                })
                .map(|launcher| tile(launcher, group.color))
                .collect()
        })
        .unwrap_or_default()
}

/// Every agent, as one flat list.
///
/// The Agents section shows launchers directly: there is nothing to drill through, so this ignores
/// the selection entirely and reads the whole section.
#[must_use]
pub fn agent_tiles(state: &AppState) -> Vec<Tile> {
    let query = state.search_query.trim().to_lowercase();
    state
        .config
        .groups
        .iter()
        .filter(|group| group.section == Section::Agents)
        .flat_map(|group| {
            group
                .launchers()
                .map(move |launcher| (group.color, launcher))
        })
        .filter(|(_, launcher)| {
            query.is_empty() || searchable_text(launcher).to_lowercase().contains(&query)
        })
        .map(|(inherited, launcher)| tile(launcher, inherited))
        .collect()
}

/// The heading of a section panel: its name and what it currently holds.
#[must_use]
pub fn section_heading(state: &AppState, section: Section) -> (String, String) {
    let groups = groups(state, section);
    let launchers: usize = groups.iter().map(|group| group.item_count).sum();
    let title = match section {
        Section::Servers => "Servers",
        Section::Agents => "Agents",
    };
    // Agents are one flat list, so counting the containers they happen to sit in would describe
    // something the section never shows.
    let subtitle = match section {
        Section::Agents => launcher_count(launchers),
        Section::Servers if groups.is_empty() => "No groups yet".to_owned(),
        Section::Servers => format!(
            "{} {} · {}",
            groups.len(),
            pluralize(groups.len(), "group", "groups"),
            launcher_count(launchers)
        ),
    };
    (title.to_owned(), subtitle)
}

/// The label colors offered as swatches in the group and category editor.
///
/// They are a starting point, not a limit: any other color can be typed as hex next to them.
pub const LABEL_PALETTE: [LabelColor; 12] = [
    LabelColor::from_rgb(0xFF, 0x3B, 0x30),
    LabelColor::from_rgb(0xFF, 0x95, 0x00),
    LabelColor::from_rgb(0xFF, 0xCC, 0x00),
    LabelColor::from_rgb(0x34, 0xC7, 0x59),
    LabelColor::from_rgb(0x00, 0xC7, 0xBE),
    LabelColor::from_rgb(0x30, 0xB0, 0xC7),
    LabelColor::from_rgb(0x32, 0xAD, 0xE6),
    LabelColor::from_rgb(0x00, 0x7A, 0xFF),
    LabelColor::from_rgb(0x58, 0x56, 0xD6),
    LabelColor::from_rgb(0xAF, 0x52, 0xDE),
    LabelColor::from_rgb(0xFF, 0x2D, 0x55),
    LabelColor::from_rgb(0xA2, 0x84, 0x5E),
];

/// Every label color the config currently uses, so their symbols can be tinted once.
pub fn label_colors(state: &AppState) -> Vec<LabelColor> {
    let mut colors: Vec<LabelColor> = state
        .config
        .groups
        .iter()
        .flat_map(|group| {
            std::iter::once(group.color)
                .chain(group.launchers().map(despatch_domain::Launcher::color))
        })
        .flatten()
        .collect();
    colors.sort_unstable_by_key(|color| color.rgb());
    colors.dedup();
    colors
}

/// The backplate color a section's group cards use.
#[must_use]
pub const fn section_accent(section: Section) -> Accent {
    match section {
        Section::Servers => Accent::Blue,
        Section::Agents => Accent::Indigo,
    }
}

/// The share of the font size taken by one character.
///
/// The bundled typeface (see [`crate::typography::BUNDLED`]) is monospaced, so this is not an
/// average but the exact advance every glyph in it has: 600 units on a 1000-unit em. Changing the
/// font means measuring the new one and changing this with it, or every content-sized control in
/// the app is laid out to the wrong width.
pub(crate) const ADVANCE: f32 = 0.6;

/// Shortens `text` to what fits in `width` points at `size`, ending with an ellipsis.
///
/// Iced has no ellipsis mode, so the budget is computed from the advance of the bundled font. A
/// clipping container still guards whatever this gets wrong.
#[must_use]
pub fn ellipsize(text: &str, width: f32, size: f32) -> String {
    let budget = budget(width, size);
    if budget < 2 {
        return "…".to_owned();
    }
    if text.chars().count() <= budget {
        return text.to_owned();
    }
    let kept: String = text.chars().take(budget - 1).collect();
    format!("{}…", kept.trim_end())
}

/// Estimates how wide `text` renders at `size`, for laying out content-sized controls.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "a label is far shorter than f32's exact integer range"
)]
pub fn text_width(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * size * ADVANCE
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a character budget is a small non-negative count by construction"
)]
fn budget(width: f32, size: f32) -> usize {
    if width <= 0.0 || size <= 0.0 {
        return 0;
    }
    (width / (size * ADVANCE)).floor().max(0.0) as usize
}

/// A launcher count with its unit, for card subtitles and delete confirmations.
#[must_use]
pub fn launcher_count(count: usize) -> String {
    format!("{count} {}", pluralize(count, "launcher", "launchers"))
}

#[must_use]
pub fn status(state: &AppState, startup_error: Option<&str>) -> Option<StatusPresentation> {
    if let Some(error) = startup_error {
        return Some(StatusPresentation {
            label: error.to_owned(),
            is_error: true,
        });
    }
    if let Some(error) = &state.last_error {
        return Some(StatusPresentation {
            label: error.message().to_owned(),
            is_error: true,
        });
    }
    if let Some(outcome) = &state.last_launch {
        return Some(match outcome {
            LaunchOutcome::Success { .. } => StatusPresentation {
                label: "Opened in your associated terminal".to_owned(),
                is_error: false,
            },
            LaunchOutcome::Failure { error, .. } => StatusPresentation {
                label: error.message().to_owned(),
                is_error: true,
            },
        });
    }
    match state.config_status {
        ConfigStatus::Dirty => Some(StatusPresentation {
            label: "Unsaved changes".to_owned(),
            is_error: false,
        }),
        ConfigStatus::Conflict => Some(StatusPresentation {
            label: "Config changed elsewhere — reload or save a copy".to_owned(),
            is_error: true,
        }),
        ConfigStatus::Unloaded | ConfigStatus::Clean => None,
    }
}

fn selected_group(state: &AppState) -> Option<&despatch_domain::Group> {
    state.selected_group.and_then(|selected| {
        state
            .config
            .groups
            .iter()
            .find(|group| group.id == selected)
    })
}

fn selected_subgroup(state: &AppState) -> Option<&despatch_domain::Subgroup> {
    state
        .selected_subgroup
        .and_then(|selected| state.config.locate_subgroup(selected))
        .map(|(_, subgroup)| subgroup)
}

/// The name of the open group, used as the heading above its categories.
#[must_use]
pub fn selected_group_name(state: &AppState) -> Option<String> {
    selected_group(state).map(|group| group.name.clone())
}

/// The name of the open subgroup, used as the heading above its launchers.
#[must_use]
pub fn selected_subgroup_name(state: &AppState) -> Option<String> {
    selected_subgroup(state).map(|subgroup| subgroup.name.clone())
}

/// The subtitle of a group card: how many categories it holds, and how many launchers in total.
#[must_use]
pub fn group_card_detail(group: &GroupRow) -> String {
    format!(
        "{} · {}",
        subgroup_count(group.subgroup_count),
        launcher_count(group.item_count)
    )
}

/// A category count with its unit, for group card subtitles.
#[must_use]
pub fn subgroup_count(count: usize) -> String {
    format!("{count} {}", pluralize(count, "category", "categories"))
}

/// One launcher, presented as the card that stands for it — wherever in the config it lives.
///
/// The card that follows the pointer during a drag is built from this: the launcher leaves the
/// grid it was drawn in, so the drawing can no longer come from that grid's rows.
#[must_use]
pub fn launcher_tile(state: &AppState, launcher_id: LauncherId) -> Option<Tile> {
    state
        .config
        .locate_launcher(launcher_id)
        .and_then(|(group, subgroup)| {
            subgroup
                .items
                .iter()
                .find(|item| item.id() == launcher_id)
                .map(|launcher| tile(launcher, group.color))
        })
}

fn tile(launcher: &Launcher, inherited: Option<LabelColor>) -> Tile {
    Tile {
        id: launcher.id(),
        color: launcher.color().or(inherited),
        name: launcher.name().to_owned(),
        detail: detail(launcher),
        kind: launcher.kind(),
        kind_label: match launcher {
            Launcher::Ssh(_) => "SSH",
            Launcher::Agent(_) => "Agent",
            Launcher::Custom(_) => "Command",
        },
        symbol: match launcher {
            Launcher::Ssh(_) => Symbol::Server,
            Launcher::Agent(agent) => provider_symbol(agent.provider),
            Launcher::Custom(_) => Symbol::Custom,
        },
        accent: match launcher {
            Launcher::Ssh(_) => Accent::Blue,
            Launcher::Agent(agent) => provider_accent(agent.provider),
            Launcher::Custom(_) => Accent::Gray,
        },
    }
}

fn detail(launcher: &Launcher) -> String {
    match launcher {
        Launcher::Ssh(ssh) => {
            if ssh.port == despatch_domain::SshPort::DEFAULT {
                format!("{}@{}", ssh.user, ssh.host)
            } else {
                format!("{}@{}:{}", ssh.user, ssh.host, ssh.port.get())
            }
        }
        Launcher::Agent(agent) => {
            let command = std::iter::once(agent.executable.as_str())
                .chain(agent.args.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join(" ");
            format!("{} · {command}", provider_name(agent.provider))
        }
        Launcher::Custom(custom) => match &custom.command {
            CustomCommand::Structured { executable, args } => std::iter::once(executable.as_str())
                .chain(args.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join(" "),
            CustomCommand::RawShell { .. } => "Raw shell command".to_owned(),
        },
    }
}

fn searchable_text(launcher: &Launcher) -> String {
    match launcher {
        Launcher::Ssh(ssh) => format!("{} {} {}", ssh.name, ssh.user, ssh.host),
        Launcher::Agent(agent) => format!(
            "{} {} {} {}",
            agent.name,
            provider_name(agent.provider),
            agent.executable,
            agent.args.join(" ")
        ),
        Launcher::Custom(custom) => match &custom.command {
            CustomCommand::Structured { executable, args } => {
                format!("{} {} {}", custom.name, executable, args.join(" "))
            }
            CustomCommand::RawShell { command } => format!("{} {command}", custom.name),
        },
    }
}

#[must_use]
pub const fn provider_name(provider: AgentProvider) -> &'static str {
    provider.display_name()
}

/// The mark a provider is drawn with.
///
/// These are stand-ins chosen from the system symbol set, not the providers' logos: bundling those
/// means bundling someone else's trademark, which is a licensing call and not one to make quietly.
#[must_use]
pub const fn provider_symbol(provider: AgentProvider) -> Symbol {
    match provider {
        AgentProvider::Codex => Symbol::Codex,
        AgentProvider::Claude => Symbol::Claude,
        AgentProvider::Gemini => Symbol::Gemini,
        AgentProvider::OpenCode => Symbol::OpenCode,
        AgentProvider::Custom => Symbol::Custom,
    }
}

#[must_use]
pub const fn provider_accent(provider: AgentProvider) -> Accent {
    match provider {
        AgentProvider::Codex => Accent::Indigo,
        AgentProvider::Claude => Accent::Orange,
        AgentProvider::Gemini => Accent::Teal,
        AgentProvider::OpenCode => Accent::Green,
        AgentProvider::Custom => Accent::Gray,
    }
}

fn pluralize(count: usize, one: &'static str, many: &'static str) -> &'static str {
    if count == 1 { one } else { many }
}

#[cfg(test)]
mod tests {
    use despatch_application::AppState;
    use despatch_domain::{
        AgentLauncher, AgentProvider, Group, Launcher, Section, SshLauncher, SshPort, Subgroup,
    };

    use crate::icons::Symbol;
    use crate::style::Accent;

    use super::{
        ellipsize, groups, launcher_count, section_heading, selected_tiles, subgroup_count,
        subgroups, text_width,
    };

    fn state_with_group(items: Vec<Launcher>, section: Section) -> AppState {
        let mut group = Group::new("Production", section).expect("valid group");
        let mut subgroup = Subgroup::new("General").expect("valid subgroup");
        for item in items {
            subgroup
                .add_launcher(section, item)
                .expect("launcher fits the section");
        }
        let subgroup_id = subgroup.id;
        group.add_subgroup(subgroup).expect("add subgroup");
        let mut state = AppState::default();
        state.config.groups.push(group);
        assert!(state.select_subgroup(subgroup_id));
        state
    }

    #[test]
    fn group_counts_and_ssh_details_are_presented() {
        let state = state_with_group(
            vec![Launcher::Ssh(
                SshLauncher::new("API", "api.example.com", "deploy").expect("valid SSH"),
            )],
            Section::Servers,
        );

        assert_eq!(groups(&state, Section::Servers)[0].item_count, 1);
        assert_eq!(selected_tiles(&state)[0].detail, "deploy@api.example.com");
        assert_eq!(selected_tiles(&state)[0].symbol, Symbol::Server);
        assert_eq!(selected_tiles(&state)[0].accent, Accent::Blue);
    }

    #[test]
    fn a_non_default_ssh_port_is_shown_in_the_tile_detail() {
        let mut ssh = SshLauncher::new("API", "api.example.com", "deploy").expect("valid SSH");
        ssh.port = SshPort::new(2222).expect("valid port");
        let state = state_with_group(vec![Launcher::Ssh(ssh)], Section::Servers);

        assert_eq!(
            selected_tiles(&state)[0].detail,
            "deploy@api.example.com:2222"
        );
    }

    #[test]
    fn agent_tiles_show_the_provider_and_the_resolved_command() {
        let mut agent =
            AgentLauncher::preset(AgentProvider::Claude, "Claude").expect("valid agent");
        agent.args = vec!["--continue".to_owned()];
        let state = state_with_group(vec![Launcher::Agent(agent)], Section::Agents);

        assert_eq!(
            selected_tiles(&state)[0].detail,
            "Claude · claude --continue"
        );
        assert_eq!(selected_tiles(&state)[0].accent, Accent::Orange);
    }

    #[test]
    fn the_section_heading_counts_its_groups_and_launchers() {
        let state = state_with_group(
            vec![
                Launcher::Ssh(SshLauncher::new("API", "api.example.com", "deploy").expect("ssh")),
                Launcher::Ssh(SshLauncher::new("Cache", "cache.example.com", "ops").expect("ssh")),
            ],
            Section::Servers,
        );

        assert_eq!(
            section_heading(&state, Section::Servers),
            ("Servers".to_owned(), "1 group · 2 launchers".to_owned())
        );
    }

    #[test]
    fn an_empty_section_says_so_instead_of_counting_zero() {
        let state = AppState::default();

        assert_eq!(
            section_heading(&state, Section::Servers),
            ("Servers".to_owned(), "No groups yet".to_owned())
        );
    }

    #[test]
    fn the_agents_heading_counts_launchers_rather_than_the_containers_it_never_shows() {
        let state = state_with_group(
            vec![
                Launcher::Agent(
                    AgentLauncher::preset(AgentProvider::Claude, "Claude").expect("agent"),
                ),
                Launcher::Agent(
                    AgentLauncher::preset(AgentProvider::Codex, "Codex").expect("agent"),
                ),
            ],
            Section::Agents,
        );

        assert_eq!(
            section_heading(&state, Section::Agents),
            ("Agents".to_owned(), "2 launchers".to_owned())
        );
    }

    #[test]
    fn card_labels_cover_every_section_so_one_width_can_serve_them_all() {
        let state = state_with_group(
            vec![Launcher::Agent(
                AgentLauncher::preset(AgentProvider::Claude, "Claude").expect("agent"),
            )],
            Section::Agents,
        );

        let labels = super::card_labels(&state);

        assert_eq!(labels.groups.len(), 1, "the group it lives in is measured");
        assert_eq!(
            labels.launchers,
            vec![("Claude".to_owned(), "Claude · claude".to_owned())],
            "and so is the launcher, whichever section it belongs to"
        );
    }

    #[test]
    fn agents_are_listed_flat_without_opening_anything() {
        let mut state = state_with_group(
            vec![
                Launcher::Agent(
                    AgentLauncher::preset(AgentProvider::Claude, "Claude").expect("agent"),
                ),
                Launcher::Agent(
                    AgentLauncher::preset(AgentProvider::Gemini, "Gemini").expect("agent"),
                ),
            ],
            Section::Agents,
        );
        state.clear_group_selection();

        let names: Vec<String> = super::agent_tiles(&state)
            .into_iter()
            .map(|tile| tile.name)
            .collect();
        assert_eq!(
            names,
            ["Claude", "Gemini"],
            "nothing has to be opened first"
        );

        state.set_search_query("gem");
        assert_eq!(
            super::agent_tiles(&state).len(),
            1,
            "search still narrows it"
        );
    }

    #[test]
    fn a_label_that_fits_is_left_alone() {
        assert_eq!(
            ellipsize("deploy@api.example.com", 400.0, 14.0),
            "deploy@api.example.com"
        );
    }

    #[test]
    fn a_label_that_overflows_ends_in_an_ellipsis() {
        let shortened = ellipsize("postgres@db-primary.internal.example.com:2222", 120.0, 14.0);

        assert!(shortened.ends_with('…'), "{shortened}");
        assert!(shortened.chars().count() < 44);
        assert!(shortened.starts_with("postgres@db"));
    }

    #[test]
    fn a_label_with_no_room_collapses_to_an_ellipsis() {
        assert_eq!(ellipsize("API server", 0.0, 14.0), "…");
        assert_eq!(ellipsize("API server", 8.0, 14.0), "…");
    }

    #[test]
    fn launcher_counts_use_the_right_unit() {
        assert_eq!(launcher_count(0), "0 launchers");
        assert_eq!(launcher_count(1), "1 launcher");
        assert_eq!(launcher_count(7), "7 launchers");
    }

    #[test]
    fn subgroups_are_listed_only_while_their_group_is_open() {
        let mut state = state_with_group(
            vec![Launcher::Ssh(
                SshLauncher::new("API", "api.example.com", "deploy").expect("ssh"),
            )],
            Section::Servers,
        );

        assert_eq!(subgroups(&state).len(), 1);
        assert_eq!(subgroups(&state)[0].name, "General");
        assert_eq!(subgroups(&state)[0].item_count, 1);

        state.clear_group_selection();
        assert!(subgroups(&state).is_empty());
    }

    #[test]
    fn tiles_disappear_as_soon_as_the_subgroup_is_closed() {
        let mut state = state_with_group(
            vec![Launcher::Ssh(
                SshLauncher::new("API", "api.example.com", "deploy").expect("ssh"),
            )],
            Section::Servers,
        );
        assert_eq!(selected_tiles(&state).len(), 1);

        state.clear_subgroup_selection();

        assert!(selected_tiles(&state).is_empty());
        assert_eq!(subgroups(&state).len(), 1, "the group stays open");
    }

    #[test]
    fn a_wider_label_is_estimated_wider_than_a_narrower_one() {
        assert!(text_width("Databases", 13.0) > text_width("Web", 13.0));
        assert!(text_width("Databases", 16.0) > text_width("Databases", 13.0));
        assert!(text_width("", 13.0).abs() < f32::EPSILON);
    }

    #[test]
    fn category_counts_use_the_right_unit() {
        assert_eq!(subgroup_count(0), "0 categories");
        assert_eq!(subgroup_count(1), "1 category");
        assert_eq!(subgroup_count(3), "3 categories");
    }

    #[test]
    fn a_search_only_narrows_the_tiles_it_matches() {
        let mut state = state_with_group(
            vec![
                Launcher::Ssh(SshLauncher::new("API", "api.example.com", "deploy").expect("ssh")),
                Launcher::Ssh(SshLauncher::new("Cache", "cache.example.com", "ops").expect("ssh")),
            ],
            Section::Servers,
        );

        state.set_search_query("cache");

        assert_eq!(selected_tiles(&state).len(), 1);
        assert_eq!(selected_tiles(&state)[0].name, "Cache");
    }
}
