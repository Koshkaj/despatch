use std::time::{Duration, SystemTime};

use despatch_application::{AppState, ConfigStatus, LaunchOutcome, RecentLaunch};
use despatch_domain::{GroupId, LabelColor, Launcher, LauncherId, Section, SubgroupId};

use crate::icons::Symbol;

/// Where the panel's sidebar is pointing: one server group, or one of the rows at its foot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Destination {
    Group(GroupId),
    Recent,
    Settings,
}

/// One row of the sidebar's source list.
#[derive(Clone, Debug, PartialEq)]
pub struct SidebarRow {
    pub destination: Destination,
    pub name: String,
    pub symbol: Symbol,
    /// The count drawn at the right of the row, when the destination holds countable things.
    pub count: Option<usize>,
    /// A group's label color, which its symbol is drawn in.
    pub color: Option<LabelColor>,
}

/// One segment of a group's segmented control: a category.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Segment {
    pub id: SubgroupId,
    pub name: String,
    pub count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tile {
    pub id: LauncherId,
    pub name: String,
    pub detail: String,
    pub symbol: Symbol,
    /// The launcher's own color, or the color it inherits from its group.
    pub color: Option<LabelColor>,
}

/// One launcher in the list pane.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListRow {
    pub tile: Tile,
    /// The category the launcher lives in, which a drag moves it out of.
    pub source: SubgroupId,
    /// The quiet tag at the right edge: where the launcher lives when the list mixes categories,
    /// or when it was last opened in "Recently used". Nothing when the list is one category.
    pub trailing: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatusPresentation {
    pub label: String,
    pub is_error: bool,
}

/// The server groups, in config order, for the "Groups" section of the sidebar.
#[must_use]
pub fn sidebar_groups(state: &AppState) -> Vec<SidebarRow> {
    state
        .config
        .groups
        .iter()
        .filter(|group| group.section == Section::Servers)
        .map(|group| SidebarRow {
            destination: Destination::Group(group.id),
            name: group.name.clone(),
            symbol: Symbol::Group,
            count: Some(group.launcher_count()),
            color: group.color,
        })
        .collect()
}

/// The rows at the foot of the sidebar, under the groups: the launch history and settings.
#[must_use]
pub fn footer_rows(recent_count: usize) -> Vec<SidebarRow> {
    vec![
        SidebarRow {
            destination: Destination::Recent,
            name: "Recently used".to_owned(),
            symbol: Symbol::Recent,
            count: (recent_count > 0).then_some(recent_count),
            color: None,
        },
        SidebarRow {
            destination: Destination::Settings,
            name: "Settings".to_owned(),
            symbol: Symbol::Settings,
            count: None,
            color: None,
        },
    ]
}

/// The categories of `group_id`, in config order.
#[must_use]
pub fn segments(state: &AppState, group_id: GroupId) -> Vec<Segment> {
    state
        .config
        .groups
        .iter()
        .find(|group| group.id == group_id)
        .map(|group| {
            group
                .subgroups
                .iter()
                .map(|subgroup| Segment {
                    id: subgroup.id,
                    name: subgroup.name.clone(),
                    count: subgroup.items.len(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The launchers the list pane shows.
///
/// A search reaches across every group, whatever the sidebar points at, and tags each row with
/// where it lives. Otherwise a group lists the category chosen in its pills — or every category,
/// tagged, when the pill is "All" — and "Recently used" lists the history.
#[must_use]
pub fn list_rows(
    state: &AppState,
    destination: Option<Destination>,
    segment: Option<SubgroupId>,
    recents: &[RecentLaunch],
    now: SystemTime,
) -> Vec<ListRow> {
    if is_searching(state) {
        return search_rows(state);
    }
    match destination {
        Some(Destination::Group(group_id)) => group_rows(state, group_id, segment),
        Some(Destination::Recent) => recents
            .iter()
            .filter_map(|recent| {
                let row = located_row(state, recent.launcher_id)?;
                Some(ListRow {
                    trailing: Some(relative_time(recent.used_at, now)),
                    ..row
                })
            })
            .collect(),
        Some(Destination::Settings) | None => Vec::new(),
    }
}

/// Whether a search is narrowing the list.
#[must_use]
pub fn is_searching(state: &AppState) -> bool {
    !state.search_query.trim().is_empty()
}

fn group_rows(state: &AppState, group_id: GroupId, segment: Option<SubgroupId>) -> Vec<ListRow> {
    let Some(group) = state
        .config
        .groups
        .iter()
        .find(|group| group.id == group_id)
    else {
        return Vec::new();
    };
    group
        .subgroups
        .iter()
        .filter(|subgroup| segment.is_none_or(|segment| segment == subgroup.id))
        .flat_map(|subgroup| {
            subgroup.items.iter().map(move |launcher| ListRow {
                tile: tile(launcher, group.color),
                source: subgroup.id,
                trailing: segment.is_none().then(|| subgroup.name.clone()),
            })
        })
        .collect()
}

fn search_rows(state: &AppState) -> Vec<ListRow> {
    let matches = state.search_results().launcher_ids;
    state
        .config
        .groups
        .iter()
        .flat_map(|group| {
            group.subgroups.iter().flat_map(move |subgroup| {
                subgroup
                    .items
                    .iter()
                    .map(move |launcher| (group, subgroup, launcher))
            })
        })
        .filter(|(_, _, launcher)| matches.contains(&launcher.id()))
        .map(|(group, subgroup, launcher)| ListRow {
            tile: tile(launcher, group.color),
            source: subgroup.id,
            trailing: Some(breadcrumb(group, subgroup)),
        })
        .collect()
}

/// A launcher wherever it lives, tagged with where that is.
fn located_row(state: &AppState, launcher_id: LauncherId) -> Option<ListRow> {
    let (group, subgroup) = state.config.locate_launcher(launcher_id)?;
    let launcher = subgroup
        .items
        .iter()
        .find(|launcher| launcher.id() == launcher_id)?;
    Some(ListRow {
        tile: tile(launcher, group.color),
        source: subgroup.id,
        trailing: Some(breadcrumb(group, subgroup)),
    })
}

/// "Production › Web": the group and category a server lives in.
fn breadcrumb(group: &despatch_domain::Group, subgroup: &despatch_domain::Subgroup) -> String {
    format!("{} › {}", group.name, subgroup.name)
}

/// The heading of the list pane: its title on the left and what it holds on the right.
#[must_use]
pub fn pane_heading(
    state: &AppState,
    destination: Option<Destination>,
    shown: usize,
) -> (String, String) {
    if is_searching(state) {
        return (
            "Results".to_owned(),
            format!("{shown} {}", pluralize(shown, "match", "matches")),
        );
    }
    match destination {
        Some(Destination::Group(group_id)) => {
            let name = state
                .config
                .groups
                .iter()
                .find(|group| group.id == group_id)
                .map_or_else(String::new, |group| group.name.clone());
            (
                name,
                format!("{shown} {}", pluralize(shown, "server", "servers")),
            )
        }
        Some(Destination::Recent) => ("Recently used".to_owned(), launcher_count(shown)),
        Some(Destination::Settings) => ("Settings".to_owned(), String::new()),
        None => ("Despatch".to_owned(), String::new()),
    }
}

/// How long ago `then` was, the way a list says it: "Just now", "5 min ago", "Yesterday".
#[must_use]
pub fn relative_time(then: SystemTime, now: SystemTime) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    let elapsed = now.duration_since(then).unwrap_or(Duration::ZERO).as_secs();
    match elapsed {
        0..MINUTE => "Just now".to_owned(),
        MINUTE..HOUR => format!("{} min ago", elapsed / MINUTE),
        HOUR..DAY => format!("{} h ago", elapsed / HOUR),
        DAY..172_800 => "Yesterday".to_owned(),
        _ if elapsed < 30 * DAY => format!("{} days ago", elapsed / DAY),
        _ => "Over a month ago".to_owned(),
    }
}

/// The label colors offered as swatches in the group and launcher editors.
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

/// Every symbol the config draws in a label color, so each pair can be tinted once.
pub fn label_symbols(state: &AppState) -> Vec<(Symbol, LabelColor)> {
    let mut pairs: Vec<(Symbol, LabelColor)> = Vec::new();
    for group in &state.config.groups {
        if let Some(color) = group.color {
            pairs.push((Symbol::Group, color));
        }
        for launcher in group.launchers() {
            if let Some(color) = launcher.color().or(group.color) {
                pairs.push((tile(launcher, None).symbol, color));
            }
        }
    }
    pairs.sort_unstable_by_key(|(symbol, color)| (format!("{symbol:?}"), color.rgb()));
    pairs.dedup();
    pairs
}

/// The share of the font size taken by one character, on average and rounded up.
///
/// The system font is proportional, so no single advance is exact; this one is chosen to be at
/// least as wide as typical labels, which a test checks against the installed font. It only decides
/// where an ellipsis goes — a clipping container still guards whatever it gets wrong.
pub(crate) const ADVANCE: f32 = 0.6;

/// Shortens `text` to what fits in `width` points at `size`, ending with an ellipsis.
///
/// Iced has no ellipsis mode, so the text is measured here, glyph by glyph, with the advances of
/// the installed system font. [`ADVANCE`] is deliberately wider than real text, which is right for
/// sizing a control but would cut a name that fits; it is only used for characters the font table
/// does not cover, or when the font cannot be read.
#[must_use]
pub fn ellipsize(text: &str, width: f32, size: f32) -> String {
    let measure = |character: char| advance(character) * size;
    if text.chars().map(measure).sum::<f32>() <= width {
        return text.to_owned();
    }
    let mut used = measure('…');
    let mut kept = String::new();
    for character in text.chars() {
        used += measure(character);
        if used > width {
            break;
        }
        kept.push(character);
    }
    let kept = kept.trim_end();
    if kept.is_empty() {
        return "…".to_owned();
    }
    format!("{kept}…")
}

/// The advance of `character`, in ems, as the system font sets it, with room for the heavier
/// weights the interface also uses.
fn advance(character: char) -> f32 {
    /// Medium and Semibold run a few percent wider than the default instance the table is read
    /// from, and kerning is not applied.
    const WEIGHT_ALLOWANCE: f32 = 1.06;
    let measured = measured_advances().and_then(|table| {
        if character == '…' {
            return Some(table.ellipsis);
        }
        let index = usize::try_from(u32::from(character)).ok()?;
        table.advances.get(index).copied().flatten()
    });
    measured.map_or(ADVANCE, |width| width * WEIGHT_ALLOWANCE)
}

/// Advances, in ems, read once from the system font for the characters labels are mostly made of.
struct AdvanceTable {
    /// Indexed by code point, Basic Latin through Latin Extended-B.
    advances: Vec<Option<f32>>,
    ellipsis: f32,
}

fn measured_advances() -> Option<&'static AdvanceTable> {
    /// Basic Latin through Latin Extended-B: most names, hosts, and paths.
    const MEASURED: u32 = 0x250;
    static TABLE: std::sync::OnceLock<Option<AdvanceTable>> = std::sync::OnceLock::new();
    TABLE
        .get_or_init(|| {
            // Only the table is kept: the font file itself is several megabytes.
            let bytes = std::fs::read(crate::typography::SYSTEM_FONT_PATH).ok()?;
            let face = ttf_parser::Face::parse(&bytes, 0).ok()?;
            let em = f32::from(face.units_per_em());
            let advance = |character: char| {
                face.glyph_index(character)
                    .and_then(|glyph| face.glyph_hor_advance(glyph))
                    .map(|units| f32::from(units) / em)
            };
            Some(AdvanceTable {
                advances: (0..MEASURED)
                    .map(|code| char::from_u32(code).and_then(advance))
                    .collect(),
                ellipsis: advance('…')?,
            })
        })
        .as_ref()
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

/// How many of the segments measured in `widths` fit in `available` before an overflow menu has to
/// take the rest. All of them fit when the total does; otherwise room is kept for the menu itself.
#[must_use]
pub fn fitting_segments(widths: &[f32], available: f32, overflow_width: f32) -> usize {
    if widths.iter().sum::<f32>() <= available {
        return widths.len();
    }
    let mut used = overflow_width;
    widths
        .iter()
        .take_while(|width| {
            used += **width;
            used <= available
        })
        .count()
}

/// A server count with its unit, for headings and delete confirmations.
#[must_use]
pub fn launcher_count(count: usize) -> String {
    format!("{count} {}", pluralize(count, "server", "servers"))
}

/// A category count with its unit, for delete confirmations.
#[must_use]
pub fn subgroup_count(count: usize) -> String {
    format!("{count} {}", pluralize(count, "category", "categories"))
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
                label: "Opened in your terminal".to_owned(),
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
            label: "Config changed elsewhere".to_owned(),
            is_error: true,
        }),
        ConfigStatus::Unloaded | ConfigStatus::Clean => None,
    }
}

/// One launcher, presented as the row that stands for it — wherever in the config it lives.
///
/// The row that follows the pointer during a drag is built from this: the launcher leaves the list
/// it was drawn in, so the drawing can no longer come from that list's rows.
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
        symbol: Symbol::Server,
    }
}

/// Where a server connects: `user@host`, with the port when it is not the standard one.
fn detail(launcher: &Launcher) -> String {
    let Launcher::Ssh(ssh) = launcher else {
        return String::new();
    };
    if ssh.port == despatch_domain::SshPort::DEFAULT {
        format!("{}@{}", ssh.user, ssh.host)
    } else {
        format!("{}@{}:{}", ssh.user, ssh.host, ssh.port.get())
    }
}

fn pluralize(count: usize, one: &'static str, many: &'static str) -> &'static str {
    if count == 1 { one } else { many }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use despatch_application::{AppState, RecentLaunch};
    use despatch_domain::{
        AgentLauncher, AgentProvider, Group, GroupId, Launcher, Section, SshLauncher, SshPort,
        Subgroup,
    };

    use super::{
        Destination, advance, ellipsize, fitting_segments, footer_rows, launcher_count, list_rows,
        pane_heading, relative_time, segments, sidebar_groups, subgroup_count,
    };

    fn ssh(name: &str, host: &str) -> Launcher {
        Launcher::Ssh(SshLauncher::new(name, host, "deploy").expect("valid SSH"))
    }

    /// A Production group with two categories, and an Agents group with one agent, as a config
    /// from an earlier version may still hold. The agent must never be shown.
    fn state() -> (AppState, GroupId) {
        let mut production = Group::new("Production", Section::Servers).expect("valid group");
        for (category, servers) in [
            (
                "Databases",
                vec![ssh("Primary", "db-1"), ssh("Replica", "db-2")],
            ),
            ("Grafana", vec![ssh("Dashboards", "grafana")]),
        ] {
            let mut subgroup = Subgroup::new(category).expect("valid subgroup");
            for server in servers {
                subgroup
                    .add_launcher(Section::Servers, server)
                    .expect("a server fits");
            }
            production.add_subgroup(subgroup).expect("add subgroup");
        }
        let mut agents = Group::new("Agents", Section::Agents).expect("valid group");
        let mut tools = Subgroup::new("Assistants").expect("valid subgroup");
        tools
            .add_launcher(
                Section::Agents,
                Launcher::Agent(
                    AgentLauncher::preset(AgentProvider::Claude, "Claude").expect("preset"),
                ),
            )
            .expect("an agent fits");
        agents.add_subgroup(tools).expect("add subgroup");
        let production_id = production.id;
        let mut state = AppState::default();
        state.config.groups = vec![production, agents];
        (state, production_id)
    }

    fn names(rows: &[super::ListRow]) -> Vec<&str> {
        rows.iter().map(|row| row.tile.name.as_str()).collect()
    }

    #[test]
    fn the_sidebar_lists_server_groups_with_their_launcher_counts() {
        let (state, production) = state();

        let groups = sidebar_groups(&state);

        assert_eq!(
            groups.len(),
            1,
            "agent groups never appear among the groups"
        );
        assert_eq!(groups[0].destination, Destination::Group(production));
        assert_eq!(groups[0].count, Some(3));
    }

    #[test]
    fn the_sidebar_foot_holds_history_and_settings_and_counts_history_once_there_is_some() {
        let empty = footer_rows(0);
        let used = footer_rows(4);

        assert_eq!(empty[0].destination, Destination::Recent);
        assert_eq!(empty[0].count, None);
        assert_eq!(used[0].count, Some(4));
        assert_eq!(empty[1].destination, Destination::Settings);
        assert_eq!(empty.len(), 2);
    }

    #[test]
    fn all_lists_every_category_tagged_and_a_segment_lists_one_untagged() {
        let (state, production) = state();
        let databases = segments(&state, production)[0].id;
        let now = SystemTime::now();

        let all = list_rows(&state, Some(Destination::Group(production)), None, &[], now);
        let one = list_rows(
            &state,
            Some(Destination::Group(production)),
            Some(databases),
            &[],
            now,
        );

        assert_eq!(names(&all), ["Primary", "Replica", "Dashboards"]);
        assert_eq!(all[2].trailing.as_deref(), Some("Grafana"));
        assert_eq!(names(&one), ["Primary", "Replica"]);
        assert!(one.iter().all(|row| row.trailing.is_none()));
        assert!(one.iter().all(|row| row.source == databases));
    }

    #[test]
    fn segments_follow_config_order_with_their_counts() {
        let (state, production) = state();

        let segments = segments(&state, production);

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].name, "Databases");
        assert_eq!(segments[0].count, 2);
    }

    #[test]
    fn a_search_reaches_across_every_group_and_says_where_each_match_lives() {
        let (mut state, _) = state();
        state.set_search_query("REPLICA");

        let rows = list_rows(
            &state,
            Some(Destination::Settings),
            None,
            &[],
            SystemTime::now(),
        );

        assert_eq!(names(&rows), ["Replica"]);
        assert_eq!(rows[0].trailing.as_deref(), Some("Production › Databases"));
        assert_eq!(pane_heading(&state, None, rows.len()).0, "Results");
    }

    #[test]
    fn a_search_matching_a_host_tags_the_server_with_its_group_and_category() {
        let (mut state, _) = state();
        state.set_search_query("grafana");

        let rows = list_rows(&state, None, None, &[], SystemTime::now());

        assert_eq!(names(&rows), ["Dashboards"]);
        assert_eq!(rows[0].trailing.as_deref(), Some("Production › Grafana"));
    }

    #[test]
    fn recently_used_lists_history_newest_first_and_skips_launchers_that_are_gone() {
        let (state, production) = state();
        let rows = list_rows(
            &state,
            Some(Destination::Group(production)),
            None,
            &[],
            SystemTime::now(),
        );
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let recents = [
            RecentLaunch {
                launcher_id: rows[2].tile.id,
                used_at: now - Duration::from_secs(120),
            },
            RecentLaunch {
                launcher_id: despatch_domain::LauncherId::new(),
                used_at: now - Duration::from_secs(200),
            },
            RecentLaunch {
                launcher_id: rows[0].tile.id,
                used_at: now - Duration::from_secs(7_200),
            },
        ];

        let recent = list_rows(&state, Some(Destination::Recent), None, &recents, now);

        assert_eq!(names(&recent), ["Dashboards", "Primary"]);
        assert_eq!(recent[0].trailing.as_deref(), Some("2 min ago"));
        assert_eq!(recent[1].trailing.as_deref(), Some("2 h ago"));
    }

    #[test]
    fn an_agent_left_in_an_older_config_is_never_listed() {
        let (mut state, production) = state();
        state.set_search_query("claude");

        let rows = list_rows(
            &state,
            Some(Destination::Group(production)),
            None,
            &[],
            SystemTime::now(),
        );

        assert!(rows.is_empty(), "search found {:?}", names(&rows));
    }

    #[test]
    fn settings_lists_no_launchers() {
        let (state, _) = state();

        assert!(
            list_rows(
                &state,
                Some(Destination::Settings),
                None,
                &[],
                SystemTime::now()
            )
            .is_empty()
        );
    }

    #[test]
    fn a_non_default_ssh_port_is_shown_in_the_detail() {
        let (mut state, production) = state();
        if let Launcher::Ssh(server) = &mut state.config.groups[0].subgroups[0].items[0] {
            server.port = SshPort::new(2222).expect("valid port");
        }

        let rows = list_rows(
            &state,
            Some(Destination::Group(production)),
            None,
            &[],
            SystemTime::now(),
        );

        assert_eq!(rows[0].tile.detail, "deploy@db-1:2222");
        assert_eq!(rows[1].tile.detail, "deploy@db-2");
    }

    #[test]
    fn the_pane_heading_counts_what_it_shows_in_the_right_unit() {
        let (state, production) = state();

        assert_eq!(
            pane_heading(&state, Some(Destination::Group(production)), 1),
            ("Production".to_owned(), "1 server".to_owned())
        );
    }

    #[test]
    fn relative_times_read_the_way_a_list_says_them() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000_000);
        let ago = |seconds| relative_time(now - Duration::from_secs(seconds), now);

        assert_eq!(ago(5), "Just now");
        assert_eq!(ago(300), "5 min ago");
        assert_eq!(ago(3 * 3600), "3 h ago");
        assert_eq!(ago(30 * 3600), "Yesterday");
        assert_eq!(ago(5 * 86_400), "5 days ago");
        assert_eq!(ago(90 * 86_400), "Over a month ago");
        // A clock that went backwards is not a launch from the future.
        assert_eq!(
            relative_time(now + Duration::from_secs(60), now),
            "Just now"
        );
    }

    #[test]
    fn segments_overflow_only_when_they_do_not_all_fit() {
        assert_eq!(fitting_segments(&[60.0, 60.0, 60.0], 200.0, 50.0), 3);
        assert_eq!(fitting_segments(&[60.0, 60.0, 60.0, 60.0], 200.0, 50.0), 2);
        assert_eq!(fitting_segments(&[300.0], 200.0, 50.0), 0);
    }

    #[test]
    fn a_label_that_fits_is_left_alone() {
        assert_eq!(ellipsize("Primary", 200.0, 13.0), "Primary");
    }

    #[test]
    fn a_label_that_overflows_ends_in_an_ellipsis() {
        let shortened = ellipsize("a-very-long-host-name.example.com", 80.0, 13.0);

        assert!(shortened.ends_with('…'));
        let width: f32 = shortened
            .chars()
            .map(|character| advance(character) * 13.0)
            .sum();
        assert!(width <= 80.0, "the shortened label is {width} points wide");
    }

    #[test]
    fn a_group_name_that_fits_beside_the_hover_actions_is_not_shortened() {
        // About 66 points wide in the system font; the 0.6 em estimate would call it 78.
        assert_eq!(ellipsize("Production", 76.0, 13.0), "Production");
    }

    #[test]
    fn a_label_with_no_room_collapses_to_an_ellipsis() {
        assert_eq!(ellipsize("Primary", 4.0, 13.0), "…");
    }

    #[test]
    fn counts_use_the_right_unit() {
        assert_eq!(launcher_count(1), "1 server");
        assert_eq!(launcher_count(2), "2 servers");
        assert_eq!(subgroup_count(1), "1 category");
        assert_eq!(subgroup_count(3), "3 categories");
    }
}
