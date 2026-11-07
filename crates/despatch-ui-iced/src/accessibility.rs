use despatch_application::AppState;
use despatch_domain::{LauncherId, SubgroupId};

use crate::app_shell::AppShell;
use crate::presenters::{self, Destination, ListRow};
use crate::views::layout::{self, Rect};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    Group,
    Button,
    SearchField,
}

/// What pressing an element does — what `VoiceOver`'s VO-Space or Switch Control sends.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    FocusSearch,
    TogglePin,
    Open(Destination),
    AddGroup,
    Segment(Option<SubgroupId>),
    AddCategory,
    Launch(LauncherId),
    /// The footer's add action, whatever it is for the open destination.
    AddItem,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub role: Role,
    pub label: String,
    pub selected: bool,
    /// Where the element is drawn, so `VoiceOver` can outline it. `None` for an element that is
    /// not on screen, such as a row scrolled out of view.
    pub frame: Option<Rect>,
    pub action: Option<Action>,
}

/// What the panel is showing, as far as an accessibility tree needs to know.
pub struct Scene<'a> {
    pub state: &'a AppState,
    pub destination: Option<Destination>,
    pub segment: Option<SubgroupId>,
    pub rows: &'a [ListRow],
    pub recent_count: usize,
    pub pinned: bool,
    /// The footer's add action, when the open destination has one.
    pub add_label: Option<&'static str>,
}

/// The accessibility tree of the panel as it is right now.
#[must_use]
pub fn model(shell: &AppShell) -> Vec<Node> {
    let rows = shell.rows();
    describe(&Scene {
        state: &shell.application.state,
        destination: shell.destination,
        segment: shell.segment,
        rows: &rows,
        recent_count: shell.application.recent_launches().len(),
        pinned: shell.pinned,
        add_label: crate::views::add_action(shell).map(|(label, _)| label),
    })
}

/// The focus-ordered accessibility tree, in the order the panel is read: the header, the sidebar,
/// the list pane, and the footer. Every element carries where it is drawn, and every control what
/// pressing it does.
#[must_use]
pub fn describe(scene: &Scene<'_>) -> Vec<Node> {
    let state = scene.state;
    let mut nodes = vec![
        node(
            Role::SearchField,
            "Search servers",
            false,
            Some(layout::search_field()),
            Some(Action::FocusSearch),
        ),
        node(
            Role::Button,
            if scene.pinned {
                "Pinned open"
            } else {
                "Keep open"
            },
            scene.pinned,
            Some(layout::pin()),
            Some(Action::TogglePin),
        ),
        node(
            Role::Group,
            "Groups",
            false,
            Some(layout::groups_heading()),
            None,
        ),
    ];
    let groups = presenters::sidebar_groups(state);
    for (index, row) in groups.into_iter().enumerate() {
        nodes.push(destination_node(scene, row, layout::group_row(index)));
    }
    nodes.push(node(
        Role::Button,
        "Add group",
        false,
        Some(layout::add_group()),
        Some(Action::AddGroup),
    ));
    for (index, row) in presenters::footer_rows(scene.recent_count)
        .into_iter()
        .enumerate()
    {
        nodes.push(destination_node(scene, row, layout::footer_row(index)));
    }

    let (title, count) = presenters::pane_heading(state, scene.destination, scene.rows.len());
    nodes.push(node(
        Role::Group,
        format!("{title}, {count}"),
        false,
        Some(layout::pane_heading()),
        None,
    ));

    // The segmented control is announced only for a group, which is the only place it is drawn.
    let mut segmented = false;
    if !presenters::is_searching(state)
        && let Some(Destination::Group(group_id)) = scene.destination
    {
        let (segment_nodes, has_segments) = segment_nodes(scene, group_id);
        segmented = has_segments;
        nodes.extend(segment_nodes);
    }

    nodes.extend(row_and_footer_nodes(scene, segmented));
    nodes
}

/// The launcher rows, then the footer's controls.
fn row_and_footer_nodes(scene: &Scene<'_>, segmented: bool) -> Vec<Node> {
    let state = scene.state;
    let mut nodes: Vec<Node> = Vec::new();
    nodes.extend(scene.rows.iter().enumerate().map(|(index, row)| {
        let mut label = format!("{}, SSH, {}", row.tile.name, row.tile.detail);
        if let Some(trailing) = &row.trailing {
            label.push_str(", ");
            label.push_str(trailing);
        }
        node(
            Role::Button,
            label,
            state.selected_launcher == Some(row.tile.id),
            layout::row(index, segmented),
            Some(Action::Launch(row.tile.id)),
        )
    }));
    if let Some(label) = scene.add_label {
        nodes.push(node(
            Role::Button,
            label,
            false,
            Some(layout::footer_add(label)),
            Some(Action::AddItem),
        ));
    }
    nodes
}

/// The segmented control of the open group, and whether it is drawn at all: a group with no
/// category shows an empty state with its own Add category button instead.
fn segment_nodes(scene: &Scene<'_>, group_id: despatch_domain::GroupId) -> (Vec<Node>, bool) {
    let segments = presenters::segments(scene.state, group_id);
    if segments.is_empty() {
        let add = node(
            Role::Button,
            "Add category",
            false,
            None,
            Some(Action::AddCategory),
        );
        return (vec![add], false);
    }
    let names: Vec<&str> = segments
        .iter()
        .map(|segment| segment.name.as_str())
        .collect();
    // Segments that went into the "More" menu are not on screen, so they get no frame.
    let fits = crate::views::segments_shown(&names, layout::pane_heading()[2]) == names.len() + 1;
    let frame = |index: usize| fits.then(|| layout::segment(&names, index));
    let add_frame = fits.then(|| layout::add_segment(&names));
    let mut nodes = vec![node(
        Role::Button,
        "All",
        scene.segment.is_none(),
        frame(0),
        Some(Action::Segment(None)),
    )];
    nodes.extend(segments.iter().enumerate().map(|(index, segment)| {
        node(
            Role::Button,
            format!(
                "{}, {}",
                segment.name,
                presenters::launcher_count(segment.count)
            ),
            scene.segment == Some(segment.id),
            frame(index + 1),
            Some(Action::Segment(Some(segment.id))),
        )
    }));
    nodes.push(node(
        Role::Button,
        "Add category",
        false,
        add_frame,
        Some(Action::AddCategory),
    ));
    (nodes, true)
}

fn destination_node(scene: &Scene<'_>, row: presenters::SidebarRow, frame: Rect) -> Node {
    let label = match row.count {
        Some(count) => format!("{}, {count}", row.name),
        None => row.name,
    };
    node(
        Role::Button,
        label,
        scene.destination == Some(row.destination),
        Some(frame),
        Some(Action::Open(row.destination)),
    )
}

fn node(
    role: Role,
    label: impl Into<String>,
    selected: bool,
    frame: Option<Rect>,
    action: Option<Action>,
) -> Node {
    Node {
        role,
        label: label.into(),
        selected,
        frame,
        action,
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use despatch_application::AppState;
    use despatch_domain::{Group, Section, Subgroup};

    use super::{Scene, describe};
    use crate::presenters::{Destination, list_rows};

    fn labels(scene: &Scene<'_>) -> Vec<String> {
        describe(scene).into_iter().map(|node| node.label).collect()
    }

    #[test]
    fn the_header_comes_first_and_every_node_carries_a_label() {
        let state = AppState::default();
        let scene = Scene {
            state: &state,
            destination: None,
            segment: None,
            rows: &[],
            recent_count: 0,
            pinned: false,
            add_label: None,
        };
        let labels = labels(&scene);

        assert_eq!(&labels[..3], ["Search servers", "Keep open", "Groups"]);
        assert!(labels.contains(&"Add group".to_owned()));
        assert!(labels.contains(&"Recently used".to_owned()));
        assert!(labels.contains(&"Settings".to_owned()));
        assert!(!labels.iter().any(|label| label.starts_with("Agents")));
        assert!(labels.iter().all(|label| !label.trim().is_empty()));
    }

    #[test]
    fn a_group_announces_its_segments_and_marks_the_open_one() {
        let mut group = Group::new("Production", Section::Servers).expect("valid group");
        let subgroup = Subgroup::new("Databases").expect("valid subgroup");
        let subgroup_id = subgroup.id;
        group.add_subgroup(subgroup).expect("add subgroup");
        let group_id = group.id;
        let mut state = AppState::default();
        state.config.groups.push(group);
        let destination = Some(Destination::Group(group_id));
        let rows = list_rows(&state, destination, None, &[], SystemTime::now());

        let all = describe(&Scene {
            state: &state,
            destination,
            segment: None,
            rows: &rows,
            recent_count: 0,
            pinned: false,
            add_label: None,
        });
        let one = describe(&Scene {
            state: &state,
            destination,
            segment: Some(subgroup_id),
            rows: &rows,
            recent_count: 0,
            pinned: false,
            add_label: None,
        });

        assert!(all.iter().any(|node| node.label == "All" && node.selected));
        assert!(
            all.iter()
                .any(|node| node.label == "Production, 0" && node.selected)
        );
        assert!(
            one.iter()
                .any(|node| node.label == "Databases, 0 servers" && node.selected)
        );
        assert!(one.iter().any(|node| node.label == "Add category"));
    }

    #[test]
    fn the_selected_destination_is_marked_without_relying_on_color() {
        let state = AppState::default();

        let nodes = describe(&Scene {
            state: &state,
            destination: Some(Destination::Recent),
            segment: None,
            rows: &[],
            recent_count: 2,
            pinned: true,
            add_label: None,
        });

        assert!(
            nodes
                .iter()
                .any(|node| node.label == "Recently used, 2" && node.selected)
        );
        assert!(
            nodes
                .iter()
                .any(|node| node.label == "Pinned open" && node.selected)
        );
        assert!(!nodes.iter().any(|node| node.label == "All"));
    }

    #[test]
    fn every_control_is_drawn_inside_the_panel_and_can_be_pressed() {
        use crate::app_shell::{PANEL_HEIGHT, PANEL_WIDTH};

        let mut group = Group::new("Production", Section::Servers).expect("valid group");
        let mut web = Subgroup::new("Web").expect("valid subgroup");
        web.add_launcher(
            Section::Servers,
            despatch_domain::Launcher::Ssh(
                despatch_domain::SshLauncher::new("API", "api.example.com", "deploy")
                    .expect("valid SSH"),
            ),
        )
        .expect("a server fits");
        group.add_subgroup(web).expect("add subgroup");
        let opened = Destination::Group(group.id);
        let destination = Some(opened);
        let mut state = AppState::default();
        state.config.groups.push(group);
        let rows = list_rows(&state, destination, None, &[], SystemTime::now());

        let nodes = describe(&Scene {
            state: &state,
            destination,
            segment: None,
            rows: &rows,
            recent_count: 0,
            pinned: false,
            add_label: Some("New server"),
        });

        for node in &nodes {
            let [x, y, width, height] = node
                .frame
                .unwrap_or_else(|| panic!("{} has no frame", node.label));
            assert!(
                x >= 0.0
                    && y >= 0.0
                    && width > 0.0
                    && height > 0.0
                    && x + width <= PANEL_WIDTH
                    && y + height <= PANEL_HEIGHT,
                "{} is drawn outside the panel: {:?}",
                node.label,
                node.frame
            );
            assert_eq!(
                node.action.is_some(),
                node.role != super::Role::Group,
                "{}: controls are pressable, headings are not",
                node.label
            );
        }
        let production = nodes
            .iter()
            .find(|node| node.label.starts_with("Production, 1"))
            .expect("the group row");
        assert_eq!(production.frame, Some(crate::views::layout::group_row(0)));
        assert_eq!(production.action, Some(super::Action::Open(opened)));
    }

    #[test]
    fn a_row_scrolled_out_of_view_has_no_frame_to_outline() {
        assert!(crate::views::layout::row(0, true).is_some());
        assert!(crate::views::layout::row(40, true).is_none());
    }
}
