use despatch_application::AppState;
use despatch_domain::Section;

use crate::presenters;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    Group,
    Button,
    SearchField,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Node {
    pub role: Role,
    pub label: String,
    pub selected: bool,
}

/// The focus-ordered accessibility tree: the sidebar destinations, the section toolbar, then the
/// cards on screen.
#[must_use]
pub fn model(state: &AppState) -> Vec<Node> {
    let section = state.selected_section;
    let mut nodes = vec![
        Node {
            role: Role::Button,
            label: "Servers".to_owned(),
            selected: section == Section::Servers,
        },
        Node {
            role: Role::Button,
            label: "Agents".to_owned(),
            selected: section == Section::Agents,
        },
        Node {
            role: Role::SearchField,
            label: "Search launchers".to_owned(),
            selected: false,
        },
        Node {
            role: Role::Group,
            label: match section {
                Section::Servers => "Groups".to_owned(),
                Section::Agents => "Agents".to_owned(),
            },
            selected: false,
        },
    ];
    // The Agents section is a flat list, so it is announced as one: no groups, no categories.
    if section == Section::Agents {
        nodes.extend(presenters::agent_tiles(state).into_iter().map(|tile| Node {
            role: Role::Button,
            label: format!("{}, {}, {}", tile.name, tile.kind_label, tile.detail),
            selected: state.selected_launcher == Some(tile.id),
        }));
        nodes.push(Node {
            role: Role::Button,
            label: "Add agent".to_owned(),
            selected: false,
        });
        return nodes;
    }

    nodes.extend(
        presenters::groups(state, section)
            .into_iter()
            .map(|group| Node {
                role: Role::Button,
                label: format!(
                    "{}, {}",
                    group.name,
                    presenters::launcher_count(group.item_count)
                ),
                selected: state.selected_group == Some(group.id),
            }),
    );
    nodes.push(Node {
        role: Role::Button,
        label: match section {
            Section::Servers => "Add server group".to_owned(),
            Section::Agents => "Add agent group".to_owned(),
        },
        selected: false,
    });

    // The nested levels are announced only while their parent is open, matching what is drawn.
    let subgroups = presenters::subgroups(state);
    if let Some(group) = presenters::selected_group_name(state) {
        nodes.push(Node {
            role: Role::Group,
            label: group,
            selected: false,
        });
        nodes.extend(subgroups.into_iter().map(|subgroup| Node {
            role: Role::Button,
            label: format!(
                "{}, {}",
                subgroup.name,
                presenters::launcher_count(subgroup.item_count)
            ),
            selected: state.selected_subgroup == Some(subgroup.id),
        }));
        nodes.push(Node {
            role: Role::Button,
            label: "Add category".to_owned(),
            selected: false,
        });
    }

    if let Some(subgroup) = presenters::selected_subgroup_name(state) {
        nodes.push(Node {
            role: Role::Group,
            label: subgroup,
            selected: false,
        });
        nodes.extend(
            presenters::selected_tiles(state)
                .into_iter()
                .map(|tile| Node {
                    role: Role::Button,
                    label: format!("{}, {}, {}", tile.name, tile.kind_label, tile.detail),
                    selected: state.selected_launcher == Some(tile.id),
                }),
        );
    }
    nodes
}

#[cfg(test)]
mod tests {
    use despatch_application::AppState;

    use super::model;

    #[test]
    fn navigation_comes_first_and_every_node_carries_a_label() {
        let nodes = model(&AppState::default());
        let labels: Vec<_> = nodes.iter().map(|node| node.label.clone()).collect();

        assert_eq!(&labels[..3], ["Servers", "Agents", "Search launchers"]);
        assert!(labels.contains(&"Add server group".to_owned()));
        assert!(labels.iter().all(|label| !label.trim().is_empty()));
    }

    #[test]
    fn nested_levels_are_announced_only_once_their_parent_is_open() {
        use despatch_domain::{Group, Section, Subgroup};

        let mut group = Group::new("Production", Section::Servers).expect("valid group");
        let subgroup = Subgroup::new("Databases").expect("valid subgroup");
        let subgroup_id = subgroup.id;
        group.add_subgroup(subgroup).expect("add subgroup");
        let group_id = group.id;
        let mut state = AppState::default();
        state.config.groups.push(group);

        let closed: Vec<_> = model(&state).into_iter().map(|node| node.label).collect();
        assert!(!closed.contains(&"Databases".to_owned()));

        assert!(state.select_group(group_id));
        let opened: Vec<_> = model(&state).into_iter().map(|node| node.label).collect();
        assert!(opened.contains(&"Databases, 0 launchers".to_owned()));
        assert!(opened.contains(&"Add category".to_owned()));

        assert!(state.select_subgroup(subgroup_id));
        assert!(
            model(&state)
                .iter()
                .any(|node| node.label == "Databases" && !node.selected),
            "the open category becomes the heading of its launcher list"
        );
    }

    #[test]
    fn the_selected_destination_is_marked_without_relying_on_color() {
        let mut state = AppState::default();
        state.select_section(despatch_domain::Section::Agents);

        let nodes = model(&state);

        assert!(!nodes[0].selected, "Servers must not stay selected");
        assert!(nodes[1].selected, "Agents must report itself as selected");
    }
}
