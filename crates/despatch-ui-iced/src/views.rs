use despatch_application::{AppearanceChoice, Dialog, Reachability};
use despatch_domain::{AgentProvider, LabelColor, LauncherKind, Section, SubgroupId};
use iced::widget::text::Wrapping;
use iced::widget::{
    Column, Space, button, column, container, image, mouse_area, opaque, responsive, row, rule,
    scrollable, stack, text, text_editor, text_input, toggler, tooltip,
};
use iced::{Center, Element, Fill, Left, Length, Padding, Right, Theme};

use crate::app_shell::{AppShell, Drag, GROUP_NAME_ID, LAUNCHER_NAME_ID, Message, SEARCH_ID};
use crate::drag_layer::DragLayer;
use crate::editor::{Field, LauncherDraft, NameDraft, ReachabilityState};
use crate::icons::{IconSet, Symbol, Tint};
use crate::presenters::{self, GroupRow, SubgroupRow, Tile};
use crate::shortcuts::{self, Shortcut};
use crate::style::{self, Accent};
use crate::typography;

/// An icon rail: one column of symbols, each in a cell big enough to aim at without looking.
///
/// The width is not free: the traffic lights float over the content view at a position macOS
/// picks, spanning roughly 14 to 66 points from the left edge. A rail narrower than that leaves
/// the green button hanging over the content while its neighbours sit on the rail, so the rail is
/// sized to hold all three.
const SIDEBAR_WIDTH: f32 = 76.0;
const RAIL_INSET: f32 = 6.0;
const RAIL_SPACING: f32 = 4.0;
/// The rail's only fixed vertical measurement: everything else in it fills.
const RAIL_DIVIDER_HEIGHT: f32 = 13.0;
/// The margin left around a rail symbol, so it reads as filling its cell without touching the edge.
const RAIL_ICON_INSET: f32 = 4.0;
/// Shortens the rail's divider so it reads as a separator rather than a full-width edge.
const RAIL_DIVIDER_INSET: f32 = 16.0;
/// Keeps content clear of the traffic lights, which float over the full-size content view.
const TITLEBAR_INSET: f32 = 40.0;
const TILE_HEIGHT: f32 = 52.0;
/// One hover action: a square target, the gap to its neighbour, and the card's edge padding.
const ACTION_BUTTON_SIZE: f32 = 24.0;
const ACTION_SPACING: f32 = 2.0;
const ACTION_EDGE_PADDING: f32 = 8.0;
/// Reserved on the right of every card so hover actions never overlap the labels.
///
/// Measured from what the actions actually draw rather than guessed: a card is now only as wide as
/// its content needs, so a reserve that is too small no longer hides behind slack in the layout —
/// it puts the edit button on top of the last characters of the host.
const TILE_ACTIONS_WIDTH: f32 = actions_width(3.0);
const GROUP_ACTIONS_WIDTH: f32 = actions_width(2.0);
const BADGE_SIZE: f32 = 28.0;
/// The compact category chips that sit between a group card and its launchers.
const CHIP_HEIGHT: f32 = 30.0;
const CHIP_SPACING: f32 = 7.0;
const CHIP_PADDING: f32 = 10.0;
const CHIP_LABEL_SIZE: f32 = 12.0;
const CHIP_ICON: f32 = 12.0;
/// The label-color swatches in the group and category editor.
const SWATCH_SIZE: f32 = 24.0;
const SWATCH_SPACING: f32 = 6.0;
/// Keeps the palette inside the sheet instead of running off its right edge.
const SWATCHES_PER_ROW: usize = 7;
const CARD_PADDING: f32 = 10.0;
const CARD_SPACING: f32 = 8.0;
/// Breathing room between a card's longest label and the actions reserved at its right edge.
const CARD_LABEL_GAP: f32 = 8.0;
/// The narrowest a card is allowed to get, so a one-word name still reads as a card rather than a
/// button that happens to hold a badge.
const MIN_CARD_WIDTH: f32 = 170.0;
const NAME_SIZE: f32 = 13.0;
const DETAIL_SIZE: f32 = 11.0;
/// The vertical rhythm inside an editor sheet.
const SHEET_PADDING: f32 = 20.0;
const SHEET_SPACING: f32 = 14.0;
/// Room for the scrollbar, so nothing is ever drawn underneath one — in a long editor, and in the
/// section itself, where the toolbar above the list keeps the same gutter so both share a right
/// edge whether or not the list is long enough to have a scrollbar at all.
const SCROLLBAR_GUTTER: f32 = 14.0;
const FIELD_PADDING: Padding = Padding {
    top: 7.0,
    right: 10.0,
    bottom: 7.0,
    left: 10.0,
};
const FIELD_TEXT_SIZE: f32 = 14.0;
const BUTTON_PADDING: Padding = Padding {
    top: 8.0,
    right: 14.0,
    bottom: 8.0,
    left: 14.0,
};
const BUTTON_TEXT_SIZE: f32 = 13.0;

pub fn view(shell: &AppShell) -> Element<'_, Message> {
    let base: Element<'_, Message> = container(
        row![
            sidebar(shell),
            rule::vertical(1).style(style::separator),
            content(shell),
        ]
        .height(Fill),
    )
    .width(Fill)
    .height(Fill)
    .style(style::window)
    .into();

    // The window is always the same two layers, even when the upper one has nothing to show.
    //
    // Widget state — every scroll position among it — lives in a tree that iced reconciles against
    // the view. A tree whose root changes type is not reconciled but rebuilt from nothing, so a
    // window that were a `container` at rest and a `stack` while something floated above it would
    // throw away where the user had scrolled to the moment they pressed a card.
    let overlay = shell.application.state.dialog.as_ref().map_or_else(
        || dragged_card(shell).unwrap_or_else(|| Space::new().into()),
        |dialog| sheet_overlay(shell, dialog),
    );
    stack![base, overlay].width(Fill).height(Fill).into()
}

/// The card riding along with the pointer, drawn above everything else.
///
/// It is inert on purpose: it draws and nothing more. `stack` hands the cursor to the layers under
/// the topmost one that claims a mouse interaction, so a layer that claimed one here would take
/// the pointer away from the category chips — the very things the card is being carried to.
fn dragged_card(shell: &AppShell) -> Option<Element<'_, Message>> {
    let drag = shell.drag?;
    let tile = presenters::launcher_tile(&shell.application.state, drag.launcher_id)?;
    // The floating card is not inside the panel, so nothing constrains it but its own content.
    let width = uniform_card_width(card_width_needs(&shell.application.state), f32::INFINITY);
    let color = tile.color;
    let card = container(card_body(&shell.icons, &tile, width))
        .width(width)
        .height(TILE_HEIGHT)
        .padding(Padding::from([0.0, CARD_PADDING]))
        .style(move |theme| style::dragged_tile(theme, color.map(style::label_color)));

    // The layer follows the pointer itself, without a message per movement. See `drag_layer`.
    Some(DragLayer::new(card, drag.lifted, Message::DragLifted).into())
}

// ---------------------------------------------------------------- sidebar

/// The icon rail: one symbol per destination, run straight down from the top.
///
/// Every cell is in one sequence and every cell fills its share of the rail's height, so making
/// the window taller makes the cells taller instead of opening a pool of dead space under the last
/// one. A short divider marks where the destinations end and the utilities begin.
fn sidebar(shell: &AppShell) -> Element<'_, Message> {
    let selected = shell.application.state.selected_section;
    container(
        column![
            rail_item(
                &shell.icons,
                section_symbol(Section::Servers),
                section_title(Section::Servers).to_owned(),
                selected == Section::Servers,
                Message::SelectSection(Section::Servers),
            ),
            rail_item(
                &shell.icons,
                section_symbol(Section::Agents),
                section_title(Section::Agents).to_owned(),
                selected == Section::Agents,
                Message::SelectSection(Section::Agents),
            ),
            rail_divider(),
            rail_item(
                &shell.icons,
                Symbol::Settings,
                format!(
                    "Settings  {}",
                    shortcuts::equivalent(Shortcut::OpenSettings)
                ),
                false,
                Message::OpenSettings,
            ),
            rail_item(
                &shell.icons,
                Symbol::Reveal,
                config_label(shell),
                false,
                Message::RevealConfig,
            ),
        ]
        .spacing(RAIL_SPACING),
    )
    .width(SIDEBAR_WIDTH)
    .height(Fill)
    .padding(Padding {
        top: TITLEBAR_INSET,
        right: RAIL_INSET,
        bottom: RAIL_INSET,
        left: RAIL_INSET,
    })
    .style(style::sidebar)
    .into()
}

/// The short hairline between the destinations and the utilities below them.
///
/// It is the one part of the rail with a fixed height, so the cells around it take every point the
/// rail has left.
fn rail_divider() -> Element<'static, Message> {
    container(rule::horizontal(1).style(style::separator))
        .height(RAIL_DIVIDER_HEIGHT)
        .padding(Padding::from([0.0, RAIL_DIVIDER_INSET]))
        .center_y(Fill)
        .into()
}

/// One destination in the rail: a symbol centered in a cell large enough to aim at, with its name
/// on hover rather than under it, which is what keeps the rail one column wide.
fn rail_item(
    icons: &IconSet,
    icon: Symbol,
    name: String,
    selected: bool,
    message: Message,
) -> Element<'static, Message> {
    let tint = if selected {
        Tint::OnAccent
    } else {
        Tint::Accent
    };
    // The symbol is sized by its cell rather than by a constant: it fills the cell short of a hair
    // of inset, and `ContentFit::Contain` keeps it square, so it grows with the rail instead of
    // sitting as a small mark in the middle of a large target.
    let cell = button(
        container(rail_symbol(icons, icon, tint))
            .center_x(Fill)
            .center_y(Fill)
            .padding(RAIL_ICON_INSET),
    )
    .on_press(message)
    .width(Fill)
    .height(Fill)
    .padding(0)
    .style(move |theme, status| style::sidebar_row(theme, status, selected));

    tooltip(
        cell,
        container(text(name).size(12).line_height(1.2))
            .padding(Padding::from([5.0, 9.0]))
            .style(style::sheet),
        tooltip::Position::Right,
    )
    .gap(6)
    .into()
}

// ---------------------------------------------------------------- section panel

fn content(shell: &AppShell) -> Element<'_, Message> {
    let state = &shell.application.state;
    let section = state.selected_section;
    let (title, subtitle) = presenters::section_heading(state, section);

    let heading = container(
        column![
            text(title)
                .size(23)
                .font(typography::SEMIBOLD)
                .line_height(1.1)
                .wrapping(Wrapping::None),
            text(subtitle)
                .size(12)
                .line_height(1.25)
                .wrapping(Wrapping::None)
                .style(style::secondary_text),
        ]
        .spacing(1),
    )
    .clip(true)
    .width(Fill);

    // The Agents section has no groups to add: its one action is adding an agent.
    let primary = match section {
        Section::Servers => labelled_button(
            &shell.icons,
            add_group_label(section),
            Message::AddGroup(section),
        ),
        Section::Agents => labelled_button(&shell.icons, "Add agent", Message::AddLauncher),
    };
    // Everything above the scrolling list is inset by the same gutter the list keeps for its
    // scrollbar, so the Add action and the cards under it share one right edge. Without it the
    // toolbar runs the full width and its button ends underneath the scrollbar.
    let toolbar = container(
        row![heading, search_field(shell), primary]
            .spacing(12)
            .align_y(Center),
    )
    .padding(Padding {
        top: 0.0,
        right: SCROLLBAR_GUTTER,
        bottom: 0.0,
        left: 0.0,
    });

    let mut body = column![toolbar].spacing(16);
    if let Some(status) = presenters::status(state, shell.banner_error()) {
        body = body.push(container(banner(&shell.icons, &status)).padding(Padding {
            top: 0.0,
            right: SCROLLBAR_GUTTER,
            bottom: 0.0,
            left: 0.0,
        }));
    }
    body = body.push(section_body(shell));

    container(body)
        .width(Fill)
        .height(Fill)
        .padding(Padding {
            top: TITLEBAR_INSET,
            right: 20.0,
            bottom: 18.0,
            left: 20.0,
        })
        .style(style::content)
        .into()
}

/// The scrolling body of a section: its groups, then the categories of the open group, then the
/// launchers of the open category. Each level appears only once its parent is selected.
fn section_body(shell: &AppShell) -> Element<'_, Message> {
    let state = &shell.application.state;
    if state.selected_section == Section::Agents {
        return agent_section(shell);
    }
    server_section(shell)
}

/// The Agents section: launchers and nothing above them.
fn agent_section(shell: &AppShell) -> Element<'_, Message> {
    let state = &shell.application.state;
    let icons = shell.icons.clone();
    let agents = presenters::agent_tiles(state);
    let selected_launcher = state.selected_launcher;
    let hovered_tile = shell.hovered_tile;
    let searching = !state.search_query.trim().is_empty();
    let needs = card_width_needs(state);
    let drag = shell.drag;

    responsive(move |size| {
        agent_list(
            &icons,
            &agents,
            AgentList {
                selected_launcher,
                hovered_tile,
                drag,
                searching,
            },
            uniform_card_width(needs.iter().copied(), size.width),
            size.width,
        )
    })
    .into()
}

/// What the flat agent list needs to know about the pointer and the selection.
#[derive(Clone, Copy)]
struct AgentList {
    selected_launcher: Option<despatch_domain::LauncherId>,
    hovered_tile: Option<despatch_domain::LauncherId>,
    drag: Option<Drag>,
    searching: bool,
}

/// The Servers section: groups, then the categories of the open group, then its launchers.
fn server_section(shell: &AppShell) -> Element<'_, Message> {
    let state = &shell.application.state;
    let section = state.selected_section;
    let icons = shell.icons.clone();
    let groups = presenters::groups(state, section);
    let subgroups = presenters::subgroups(state);
    let tiles = presenters::selected_tiles(state);
    let selected_group = state.selected_group;
    let selected_subgroup = state.selected_subgroup;
    let selected_launcher = state.selected_launcher;
    let hovered_group = shell.hovered_group;
    let hovered_tile = shell.hovered_tile;
    let group_name = presenters::selected_group_name(state);
    let subgroup_name = presenters::selected_subgroup_name(state);
    let searching = !state.search_query.trim().is_empty();
    let needs = card_width_needs(state);
    let drag = shell.drag;
    let hovered_subgroup = shell.hovered_subgroup;

    responsive(move |size| {
        let available = size.width;
        let width = uniform_card_width(needs.iter().copied(), available);
        let mut body = column![block_label("Groups")].spacing(12);

        let mut cards = Vec::new();
        for group in &groups {
            let is_selected = selected_group == Some(group.id);
            cards.push(group_card(
                &icons,
                group,
                is_selected,
                hovered_group == Some(group.id),
                width,
            ));
        }
        cards.push(add_card(
            &icons,
            add_group_label(section),
            Message::AddGroup(section),
        ));
        body = body.push(card_rows(cards, width, available));

        let Some(group_name) = &group_name else {
            return scroll(body);
        };

        body = body.push(Space::new().height(18));
        body = body.push(quiet_label(format!("{group_name} categories")));
        body = body.push(category_rail(
            &icons,
            &subgroups,
            selected_subgroup,
            drag,
            hovered_subgroup,
            available,
        ));

        let Some(subgroup_name) = &subgroup_name else {
            return scroll(body);
        };
        let Some(subgroup_id) = selected_subgroup else {
            return scroll(body);
        };

        body = body.push(Space::new().height(20));
        body = body.push(level_header(
            &icons,
            subgroup_name.clone(),
            subgroup_id,
            add_item_label(section),
        ));
        if tiles.is_empty() && searching {
            body = body.push(empty_state(
                &icons,
                Symbol::Search,
                "No matches",
                "No launcher in this category matches your search.",
                Some(("Clear search", Message::ClearSearch)),
            ));
            return scroll(body);
        }
        let mut cells = Vec::new();
        for tile in &tiles {
            // The card being dragged wears the selected surface for the length of the gesture, so
            // the thing the pointer is carrying is visible while the eye is on the categories.
            cells.push(tile_cell(
                &icons,
                tile,
                card_state(tile.id, selected_launcher, hovered_tile, drag),
                width,
            ));
        }
        cells.push(add_card(
            &icons,
            add_item_label(section),
            Message::AddLauncher,
        ));
        body = body.push(card_rows(cells, width, available));
        scroll(body)
    })
    .into()
}

/// The Agents section: a flat list of launchers.
///
/// There is no group to open and no category to pick, so the section draws its launchers and
/// nothing else. The containers they live in still exist in the config; they are simply never
/// something the user has to navigate.
fn agent_list(
    icons: &IconSet,
    agents: &[Tile],
    list: AgentList,
    width: f32,
    available: f32,
) -> Element<'static, Message> {
    let AgentList {
        selected_launcher,
        hovered_tile,
        drag,
        searching,
    } = list;
    let mut body = column![].spacing(12);
    if agents.is_empty() && searching {
        body = body.push(empty_state(
            icons,
            Symbol::Search,
            "No matches",
            "No agent matches your search.",
            Some(("Clear search", Message::ClearSearch)),
        ));
        return scroll(body);
    }
    let mut cells = Vec::new();
    for tile in agents {
        cells.push(tile_cell(
            icons,
            tile,
            card_state(tile.id, selected_launcher, hovered_tile, drag),
            width,
        ));
    }
    cells.push(add_card(icons, "Add agent", Message::AddLauncher));
    scroll(body.push(card_rows(cells, width, available)))
}

/// How one launcher card should draw itself, given what is selected, hovered, and held.
///
/// The card being dragged wears the selected surface for the length of the gesture, so the thing
/// the pointer is carrying stays visible while the eye is on the categories. Its actions stand
/// down: they would otherwise sit under the pointer all the way to the drop.
fn card_state(
    launcher_id: despatch_domain::LauncherId,
    selected_launcher: Option<despatch_domain::LauncherId>,
    hovered_tile: Option<despatch_domain::LauncherId>,
    drag: Option<Drag>,
) -> CardState {
    let held = drag.is_some_and(|drag| drag.launcher_id == launcher_id);
    let selected = selected_launcher == Some(launcher_id) || held;
    let hovered = hovered_tile == Some(launcher_id);
    // Only a card that has actually left its place puts its actions away. A press that turns out to
    // be a click would otherwise blink them off and back on again.
    let carrying = drag.is_some_and(|drag| drag.lifted);
    CardState {
        status: match (held, hovered) {
            (true, _) => button::Status::Pressed,
            (false, true) => button::Status::Hovered,
            (false, false) => button::Status::Active,
        },
        selected,
        show_actions: !carrying && (selected || hovered),
    }
}

fn scroll(body: Column<'static, Message>) -> Element<'static, Message> {
    scrollable(container(body).padding(Padding {
        top: 0.0,
        right: SCROLLBAR_GUTTER,
        bottom: 8.0,
        left: 0.0,
    }))
    .height(Fill)
    .into()
}

/// The heading above the launchers of the open category: its name, the actions that edit it, and
/// the action that adds to it.
fn level_header(
    icons: &IconSet,
    title: String,
    subgroup_id: SubgroupId,
    action: &'static str,
) -> Element<'static, Message> {
    row![
        block_label(title),
        Space::new().width(Fill),
        icon_button(
            icons,
            Symbol::Edit,
            Tint::Secondary,
            "Rename category",
            Message::RenameSubgroup(subgroup_id),
        ),
        icon_button(
            icons,
            Symbol::Delete,
            Tint::Secondary,
            "Delete category",
            Message::DeleteSubgroupRequested(subgroup_id),
        ),
        Space::new().width(6),
        labelled_button(icons, action, Message::AddLauncher),
    ]
    .spacing(2)
    .align_y(Center)
    .into()
}

/// A quiet caption introducing a nested level.
fn quiet_label(label: String) -> Element<'static, Message> {
    text(label)
        .size(12)
        .line_height(1.0)
        .style(style::secondary_text)
        .into()
}

/// The compact, wrapping rail of category chips shown under the open group.
fn category_rail(
    icons: &IconSet,
    subgroups: &[SubgroupRow],
    selected: Option<SubgroupId>,
    drag: Option<Drag>,
    hovered_subgroup: Option<SubgroupId>,
    panel_width: f32,
) -> Element<'static, Message> {
    let mut widths: Vec<f32> = subgroups.iter().map(chip_width).collect();
    widths.push(add_chip_width());

    let mut rail = column![].spacing(CHIP_SPACING);
    let mut index = 0;
    for length in pack_rows(&widths, panel_width, CHIP_SPACING) {
        let mut chips = row![].spacing(CHIP_SPACING).align_y(Center);
        for _ in 0..length {
            chips = chips.push(match subgroups.get(index) {
                Some(subgroup) => category_chip(
                    icons,
                    subgroup,
                    selected == Some(subgroup.id),
                    drop_state(drag, hovered_subgroup, subgroup.id),
                ),
                None => add_chip(icons),
            });
            index += 1;
        }
        rail = rail.push(chips);
    }
    rail.into()
}

/// How a category should present itself to the drag in progress, if there is one.
///
/// The category the launcher already lives in stays idle: offering it as a target would promise a
/// move that does nothing.
fn drop_state(
    drag: Option<Drag>,
    hovered_subgroup: Option<SubgroupId>,
    subgroup_id: SubgroupId,
) -> DropState {
    match drag {
        // A press alone offers nothing: until the card leaves its place the gesture is still a
        // click, and lighting every other category up for it is an answer to a question the user
        // has not asked.
        Some(drag) if drag.lifted && drag.source != subgroup_id => {
            if hovered_subgroup == Some(subgroup_id) {
                DropState::Engaged
            } else {
                DropState::Available
            }
        }
        _ => DropState::Idle,
    }
}

/// Splits content-sized chips into rows that fit `available`, keeping at least one chip per row so
/// a chip wider than the panel is never dropped.
#[must_use]
fn pack_rows(widths: &[f32], available: f32, spacing: f32) -> Vec<usize> {
    let mut rows = Vec::new();
    let mut length = 0;
    let mut used = 0.0;
    for width in widths {
        let needed = if length == 0 {
            *width
        } else {
            used + spacing + width
        };
        if length > 0 && needed > available {
            rows.push(length);
            length = 0;
            used = 0.0;
        }
        used = if length == 0 {
            *width
        } else {
            used + spacing + width
        };
        length += 1;
    }
    if length > 0 {
        rows.push(length);
    }
    rows
}

/// What a category chip is to a drag in progress.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DropState {
    /// Nothing is being dragged, or this category is where the dragged launcher already lives.
    Idle,
    /// A launcher could be dropped here.
    Available,
    /// A launcher would land here if the pointer were released now.
    Engaged,
}

fn category_chip(
    icons: &IconSet,
    subgroup: &SubgroupRow,
    selected: bool,
    drop: DropState,
) -> Element<'static, Message> {
    // A colored category keeps its color on the folder symbol in every state, because the chip is
    // only ever tinted with it and never flooded.
    let tint = match (selected, subgroup.color) {
        (_, Some(color)) => Tint::Custom(color.rgb()),
        (true, None) => Tint::OnAccent,
        (false, None) => Tint::Accent,
    };
    let fill = subgroup.color.map(style::label_color);
    let subgroup_id = subgroup.id;
    let chip = button(
        row![
            symbol(icons, Symbol::Category, tint, CHIP_ICON),
            text(subgroup.name.clone())
                .size(CHIP_LABEL_SIZE)
                .font(typography::MEDIUM)
                .line_height(1.0)
                .wrapping(Wrapping::None),
            text(subgroup.item_count.to_string())
                .size(CHIP_LABEL_SIZE)
                .line_height(1.0)
                .style(move |theme: &Theme| if selected && fill.is_none() {
                    style::on_chip_text(theme)
                } else {
                    style::tertiary_text(theme)
                }),
        ]
        .spacing(6)
        .height(Fill)
        .align_y(Center),
    )
    .on_press(Message::SelectSubgroup(subgroup_id))
    .height(CHIP_HEIGHT)
    .padding(Padding::from([0.0, CHIP_PADDING]))
    .style(move |theme, status| {
        let base = style::chip(theme, status, selected, fill);
        match drop {
            DropState::Idle => base,
            DropState::Available => style::drop_target(theme, &base, false),
            DropState::Engaged => style::drop_target(theme, &base, true),
        }
    });

    // The chip reports the pointer so a drag knows where it would land. Hover is a plain cursor
    // move, which no button captures, so this works while a card is being held.
    hoverable(
        chip.into(),
        Message::HoverSubgroup(Some(subgroup_id)),
        Message::HoverSubgroup(None),
    )
}

fn add_chip(icons: &IconSet) -> Element<'static, Message> {
    button(
        row![
            symbol(icons, Symbol::Add, Tint::Secondary, 11.0),
            text("Add category")
                .size(CHIP_LABEL_SIZE)
                .font(typography::MEDIUM)
                .line_height(1.0),
        ]
        .spacing(7)
        .height(Fill)
        .align_y(Center),
    )
    .on_press(Message::AddSubgroup)
    .height(CHIP_HEIGHT)
    .padding(Padding::from([0.0, CHIP_PADDING]))
    .style(style::add_chip)
    .into()
}

/// The width a category chip needs, used to wrap the rail without hiding anything.
fn chip_width(subgroup: &SubgroupRow) -> f32 {
    let label = presenters::text_width(&subgroup.name, CHIP_LABEL_SIZE);
    let count = presenters::text_width(&subgroup.item_count.to_string(), CHIP_LABEL_SIZE);
    CHIP_PADDING * 2.0 + CHIP_ICON + 7.0 + label + 7.0 + count
}

fn add_chip_width() -> f32 {
    CHIP_PADDING * 2.0 + 11.0 + 7.0 + presenters::text_width("Add category", CHIP_LABEL_SIZE)
}

fn block_label(label: impl text::IntoFragment<'static>) -> Element<'static, Message> {
    text(label)
        .size(16)
        .font(typography::SEMIBOLD)
        .line_height(1.0)
        .into()
}

fn search_field(shell: &AppShell) -> Element<'_, Message> {
    let query = &shell.application.state.search_query;
    let mut field = row![
        symbol(&shell.icons, Symbol::Search, Tint::Secondary, 13.0),
        text_input("Search", query)
            .id(iced::widget::Id::new(SEARCH_ID))
            .on_input(Message::SearchChanged)
            .size(FIELD_TEXT_SIZE)
            .padding(0)
            .width(Fill)
            .style(style::borderless_input),
    ]
    .spacing(9)
    .align_y(Center);

    if !query.is_empty() {
        field = field.push(icon_button(
            &shell.icons,
            Symbol::Close,
            Tint::Secondary,
            "Clear search",
            Message::ClearSearch,
        ));
    }

    container(field)
        .width(220)
        .padding(FIELD_PADDING)
        .style(style::search_field)
        .into()
}

fn labelled_button(
    icons: &IconSet,
    label: &'static str,
    message: Message,
) -> Element<'static, Message> {
    button(
        row![
            symbol(icons, Symbol::Add, Tint::OnAccent, 11.0),
            text(label)
                .size(BUTTON_TEXT_SIZE)
                .font(typography::MEDIUM)
                .line_height(1.0),
        ]
        .spacing(7)
        .align_y(Center),
    )
    .on_press(message)
    .padding(BUTTON_PADDING)
    .style(style::primary_button)
    .into()
}

fn banner(icons: &IconSet, status: &presenters::StatusPresentation) -> Element<'static, Message> {
    let is_error = status.is_error;
    container(
        row![
            symbol(
                icons,
                if is_error {
                    Symbol::Error
                } else {
                    Symbol::Success
                },
                if is_error { Tint::Danger } else { Tint::Accent },
                15.0,
            ),
            text(status.label.clone()).size(14).line_height(1.3),
        ]
        .spacing(10)
        .align_y(Center),
    )
    .width(Fill)
    .padding(Padding::from([10.0, 13.0]))
    .style(move |theme| style::status(theme, is_error))
    .into()
}

fn group_card(
    icons: &IconSet,
    group: &GroupRow,
    selected: bool,
    hovered: bool,
    card_width: f32,
) -> Element<'static, Message> {
    let labels_width = label_width(card_width, GROUP_ACTIONS_WIDTH);
    let accent = presenters::section_accent(group.section);
    let color = group.color;
    let badge = container(symbol(
        icons,
        section_symbol(group.section),
        Tint::OnColor,
        17.0,
    ))
    .width(BADGE_SIZE)
    .height(BADGE_SIZE)
    .align_x(Center)
    .align_y(Center)
    .style(move |theme| style::badge(badge_color(theme, color, accent)));

    let labels = container(
        column![
            text(presenters::ellipsize(&group.name, labels_width, NAME_SIZE))
                .size(NAME_SIZE)
                .font(typography::MEDIUM)
                .line_height(1.15)
                .wrapping(Wrapping::None),
            text(presenters::group_card_detail(group))
                .size(DETAIL_SIZE)
                .line_height(1.25)
                .wrapping(Wrapping::None)
                .style(style::secondary_text),
        ]
        .spacing(2),
    )
    .clip(true)
    .width(Fill);

    let group_id = group.id;
    let surface = button(
        row![badge, labels, Space::new().width(GROUP_ACTIONS_WIDTH)]
            .spacing(CARD_SPACING)
            .height(Fill)
            .align_y(Center),
    )
    .on_press(Message::SelectGroup(group_id))
    .width(Fill)
    .height(TILE_HEIGHT)
    .padding(Padding::from([0.0, CARD_PADDING]))
    .style(move |theme, status| {
        style::tile(theme, status, selected, color.map(style::label_color))
    });

    let mut layers: Vec<Element<'static, Message>> = vec![surface.into()];
    if selected || hovered {
        layers.push(row_actions(
            icons,
            vec![
                (
                    Symbol::Edit,
                    Tint::Secondary,
                    "Rename group",
                    Message::RenameGroup(group_id),
                ),
                (
                    Symbol::Delete,
                    Tint::Danger,
                    "Delete group",
                    Message::DeleteGroupRequested(group_id),
                ),
            ],
        ));
    }
    hoverable(
        stack(layers).into(),
        Message::HoverGroup(Some(group_id)),
        Message::HoverGroup(None),
    )
}

/// Tracks the pointer over a whole card, actions included.
///
/// The tracker has to sit *outside* the stack: `stack` levitates the cursor for the layers under
/// whichever one reports a mouse interaction, so a `mouse_area` beneath the hover actions would be
/// told the pointer left the moment those actions appeared, and the card would flicker between
/// showing and hiding them instead of letting either one be pressed.
fn hoverable(
    content: Element<'static, Message>,
    on_enter: Message,
    on_exit: Message,
) -> Element<'static, Message> {
    mouse_area(content)
        .on_enter(on_enter)
        .on_exit(on_exit)
        .into()
}

fn add_card(icons: &IconSet, label: &'static str, message: Message) -> Element<'static, Message> {
    button(
        row![
            container(symbol(icons, Symbol::Add, Tint::Secondary, 14.0))
                .width(BADGE_SIZE)
                .height(BADGE_SIZE)
                .align_x(Center)
                .align_y(Center)
                .style(style::add_badge),
            text(label)
                .size(NAME_SIZE)
                .font(typography::MEDIUM)
                .line_height(1.0),
        ]
        .spacing(CARD_SPACING)
        .height(Fill)
        .align_y(Center),
    )
    .on_press(message)
    .width(Fill)
    .height(TILE_HEIGHT)
    .padding(Padding::from([0.0, CARD_PADDING]))
    .style(style::add_tile)
    .into()
}

fn empty_state(
    icons: &IconSet,
    icon: Symbol,
    title: &'static str,
    detail: &'static str,
    action: Option<(&'static str, Message)>,
) -> Element<'static, Message> {
    let mut body = column![
        symbol(icons, icon, Tint::Secondary, 32.0),
        text(title).size(17).font(typography::SEMIBOLD),
        text(detail)
            .size(14)
            .line_height(1.35)
            .style(style::secondary_text),
    ]
    .spacing(8)
    .align_x(Center);

    if let Some((label, message)) = action {
        body = body.push(
            button(text(label).size(14).font(typography::MEDIUM))
                .on_press(message)
                .padding(Padding::from([10.0, 16.0]))
                .style(style::secondary_button),
        );
    }

    container(body)
        .width(Fill)
        .padding(Padding::from([40.0, 0.0]))
        .center_x(Fill)
        .into()
}

/// The fill of a symbol backplate: the label color the user chose, or the section accent.
fn badge_color(theme: &Theme, color: Option<LabelColor>, fallback: Accent) -> iced::Color {
    color.map_or_else(|| fallback.color(theme), style::label_color)
}

/// The width one card would need to show everything it draws.
///
/// Cards used to take an equal share of the panel, which stretched a card holding one short host
/// across half the window and left the reader's eye travelling over empty surface to reach the
/// actions.
fn card_width_need(label_width: f32, actions_width: f32) -> f32 {
    CARD_PADDING.mul_add(2.0, BADGE_SIZE + CARD_SPACING)
        + label_width
        + CARD_LABEL_GAP
        + actions_width
}

/// What every card in the config would need, measured once so both sections agree on a width.
///
/// Groups and launchers are measured together and across sections: cards that resize as you move
/// between Servers and Agents, or as you open a category, read as a layout that cannot make up its
/// mind. The result is stable for a given config and only changes when its contents do.
fn card_width_needs(state: &despatch_application::AppState) -> Vec<f32> {
    let labels = presenters::card_labels(state);
    labels
        .groups
        .iter()
        .map(|(name, detail)| card_width_need(card_label_width(name, detail), GROUP_ACTIONS_WIDTH))
        .chain(labels.launchers.iter().map(|(name, detail)| {
            card_width_need(card_label_width(name, detail), TILE_ACTIONS_WIDTH)
        }))
        .collect()
}

/// The single width every card on a screen is given.
///
/// One width rather than a width per card: cards of three different sizes in one grid read as a
/// broken layout, not as a considerate one. It is the widest thing on the screen that still fits
/// the bounds, so the shortest card is as compact as the longest label allows.
fn uniform_card_width(needs: impl IntoIterator<Item = f32>, available: f32) -> f32 {
    let wanted = needs.into_iter().fold(MIN_CARD_WIDTH, f32::max);
    // A panel narrower than the widest card wins: a card is never wider than the room it has, and
    // only then is a label allowed to be cut short.
    wanted.min(available.max(0.0))
}

/// The room `count` hover actions occupy at the right edge of a card.
const fn actions_width(count: f32) -> f32 {
    count * ACTION_BUTTON_SIZE + (count - 1.0) * ACTION_SPACING + ACTION_EDGE_PADDING * 2.0
}

/// The room a card's two lines of text need, which is whichever of them is longer.
fn card_label_width(name: &str, detail: &str) -> f32 {
    presenters::text_width(name, NAME_SIZE).max(presenters::text_width(detail, DETAIL_SIZE))
}

/// Lays equally sized cards into rows that fit `available`.
///
/// A row ends where its last card ends: the leftover width stays empty rather than being handed
/// out to the cards, which is what keeps a row of two cards from looking like a row of two halves.
fn card_rows(
    cards: Vec<Element<'static, Message>>,
    width: f32,
    available: f32,
) -> Element<'static, Message> {
    let widths = vec![width; cards.len()];
    let mut cards = cards.into_iter();
    let mut rows = column![].spacing(CARD_SPACING);
    for length in pack_rows(&widths, available, CARD_SPACING) {
        let mut line = row![].spacing(CARD_SPACING);
        for _ in 0..length {
            let Some(card) = cards.next() else {
                break;
            };
            line = line.push(container(card).width(Length::Fixed(width)));
        }
        rows = rows.push(line);
    }
    rows.into()
}

/// The room a card's labels have once its badge, padding, and hover actions are reserved.
fn label_width(card_width: f32, actions_width: f32) -> f32 {
    (card_width - CARD_PADDING * 2.0 - BADGE_SIZE - CARD_SPACING - actions_width).max(0.0)
}

/// What a launcher card draws: its badge, its two lines, and the room its actions need.
///
/// Shared with the card that follows the pointer during a drag, so the thing being carried is the
/// thing that was picked up rather than a second drawing of it that can drift.
fn card_body(icons: &IconSet, tile: &Tile, card_width: f32) -> Element<'static, Message> {
    let labels_width = label_width(card_width, TILE_ACTIONS_WIDTH);
    let accent = tile.accent;
    let color = tile.color;
    // The badge takes the label color when there is one. Leaving it on the launcher-kind accent
    // made a recolored card carry a backplate in the old color, which read as a bug rather than as
    // "this one is an SSH launcher".
    let badge = container(symbol(icons, tile.symbol, Tint::OnColor, 17.0))
        .width(BADGE_SIZE)
        .height(BADGE_SIZE)
        .align_x(Center)
        .align_y(Center)
        .style(move |theme| style::badge(badge_color(theme, color, accent)));

    let labels = container(
        column![
            text(presenters::ellipsize(&tile.name, labels_width, NAME_SIZE))
                .size(NAME_SIZE)
                .font(typography::MEDIUM)
                .line_height(1.15)
                .wrapping(Wrapping::None),
            text(presenters::ellipsize(
                &tile.detail,
                labels_width,
                DETAIL_SIZE
            ))
            .size(DETAIL_SIZE)
            .line_height(1.25)
            .wrapping(Wrapping::None)
            .style(style::secondary_text),
        ]
        .spacing(2),
    )
    .clip(true)
    .width(Fill);

    row![badge, labels, Space::new().width(TILE_ACTIONS_WIDTH)]
        .spacing(CARD_SPACING)
        .height(Fill)
        .align_y(Center)
        .into()
}

/// What a launcher card is doing right now: how it draws, and whether it offers its actions.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CardState {
    /// Resting, under the pointer, or held down — the card's own surface, told in the vocabulary
    /// the shared [`style::tile`] speaks.
    status: button::Status,
    selected: bool,
    show_actions: bool,
}

/// A launcher card.
///
/// One click selects, two launch. The card is a `mouse_area` rather than a `button` because those
/// two have to be told apart from a third gesture — the press that turns into a drag — and a button
/// captures the press before anything else can look at it, publishing its own message on release.
/// That is also why a click used to be missed: a button only fires if the release lands back inside
/// its bounds, so a hand that drifted a few points between press and release launched nothing.
fn tile_cell(
    icons: &IconSet,
    tile: &Tile,
    state: CardState,
    card_width: f32,
) -> Element<'static, Message> {
    let CardState {
        status,
        selected,
        show_actions,
    } = state;
    let color = tile.color;
    let launcher_id = tile.id;
    let surface = mouse_area(
        container(card_body(icons, tile, card_width))
            .width(Fill)
            .height(TILE_HEIGHT)
            .padding(Padding::from([0.0, CARD_PADDING]))
            .style(move |theme| {
                style::tile_surface(theme, status, selected, color.map(style::label_color))
            }),
    )
    .interaction(iced::mouse::Interaction::Pointer)
    .on_press(Message::PressTile(launcher_id))
    .on_double_click(Message::LaunchTile(launcher_id));

    let mut layers: Vec<Element<'static, Message>> = vec![surface.into()];
    if show_actions {
        layers.push(row_actions(
            icons,
            vec![
                (
                    Symbol::Edit,
                    Tint::Secondary,
                    "Edit launcher",
                    Message::EditLauncher(launcher_id),
                ),
                (
                    Symbol::Duplicate,
                    Tint::Secondary,
                    "Duplicate launcher",
                    Message::DuplicateLauncher(launcher_id),
                ),
                (
                    Symbol::Delete,
                    Tint::Danger,
                    "Delete launcher",
                    Message::DeleteLauncherRequested(launcher_id),
                ),
            ],
        ));
    }
    hoverable(
        stack(layers).into(),
        Message::HoverTile(Some(launcher_id)),
        Message::HoverTile(None),
    )
}

/// The hover/selection actions floating over the right edge of a card.
fn row_actions(
    icons: &IconSet,
    actions: Vec<(Symbol, Tint, &'static str, Message)>,
) -> Element<'static, Message> {
    let mut controls = row![].spacing(ACTION_SPACING).align_y(Center);
    for (icon, tint, label, message) in actions {
        controls = controls.push(icon_button(icons, icon, tint, label, message));
    }
    container(controls)
        .width(Fill)
        .height(Fill)
        .align_x(Right)
        .align_y(Center)
        .padding(Padding::from([0.0, ACTION_EDGE_PADDING]))
        .into()
}

// ---------------------------------------------------------------- sheets

fn sheet_overlay<'a>(shell: &'a AppShell, dialog: &'a Dialog) -> Element<'a, Message> {
    let content = match dialog {
        Dialog::AddGroup { .. }
        | Dialog::EditGroup { .. }
        | Dialog::AddSubgroup { .. }
        | Dialog::EditSubgroup { .. } => shell.name_draft.as_ref().map(name_editor),
        Dialog::AddLauncher { .. } | Dialog::EditLauncher { .. } => shell
            .launcher_draft
            .as_ref()
            .map(|draft| launcher_editor(draft, &shell.icons)),
        Dialog::DeleteGroup { group_id } => shell.group(*group_id).map(|group| {
            confirm_sheet(
                "Delete group?",
                format!(
                    "“{}”, its {}, and their {} will be removed from the config file.",
                    group.name,
                    presenters::subgroup_count(group.subgroups.len()),
                    presenters::launcher_count(group.launcher_count())
                ),
                "Delete group",
            )
        }),
        Dialog::DeleteSubgroup { subgroup_id } => {
            shell.subgroup(*subgroup_id).map(|(_, subgroup)| {
                confirm_sheet(
                    "Delete category?",
                    format!(
                        "“{}” and its {} will be removed from the config file.",
                        subgroup.name,
                        presenters::launcher_count(subgroup.items.len())
                    ),
                    "Delete category",
                )
            })
        }
        Dialog::DeleteLauncher { launcher_id } => shell.launcher(*launcher_id).map(|(_, item)| {
            confirm_sheet(
                "Delete launcher?",
                format!("“{}” will be removed from the config file.", item.name()),
                "Delete",
            )
        }),
        Dialog::Settings => Some(settings_sheet(shell)),
    };
    let Some(content) = content else {
        return container(Space::new()).into();
    };

    opaque(
        container(content)
            .width(Fill)
            .height(Fill)
            .center(Fill)
            .padding(20)
            .style(style::modal_scrim),
    )
}

fn sheet_header(title: &'static str, detail: String) -> Element<'static, Message> {
    column![
        text(title)
            .size(18)
            .font(typography::SEMIBOLD)
            .line_height(1.15),
        text(detail)
            .size(12)
            .line_height(1.35)
            .style(style::secondary_text),
    ]
    .spacing(3)
    .into()
}

fn name_editor(draft: &NameDraft) -> Element<'_, Message> {
    let field_error = |field: Field| {
        draft
            .error
            .as_ref()
            .filter(|error| error.field == Some(field))
            .map(|error| error.message.as_str())
    };
    let mut form = column![
        sheet_header(draft.title(), draft.detail().to_owned()),
        form_field(
            draft.label(),
            &draft.name,
            draft.placeholder(),
            Message::NameChanged,
            field_error(Field::Name),
            Some(GROUP_NAME_ID),
        ),
    ]
    .spacing(SHEET_SPACING);
    if draft.has_color() {
        form = form.push(color_picker(
            "Color",
            draft.selected_color(),
            &draft.color,
            field_error(Field::Color),
            &ColorMessages {
                changed: Message::NameColorChanged,
                selected: Message::NameColorSelected,
                cleared: Message::NameColorCleared,
            },
        ));
    }
    let form = form.push(
        column![
            general_error(
                draft
                    .error
                    .as_ref()
                    .filter(|error| error.field.is_none())
                    .map(|error| error.message.as_str())
            ),
            sheet_actions(draft.action(), Message::SaveName, false),
        ]
        .spacing(SHEET_SPACING),
    );

    container(form)
        .width(390)
        .padding(SHEET_PADDING)
        .style(style::sheet)
        .into()
}

/// The messages a colour picker sends, so the same control serves the group editor and the
/// launcher editor without either knowing about the other.
struct ColorMessages {
    changed: fn(String) -> Message,
    selected: fn(LabelColor) -> Message,
    cleared: Message,
}

/// The label-color picker: the preset swatches, a well that clears the color, and a hex field for
/// any color the presets do not cover.
fn color_picker<'a>(
    label: &'static str,
    selected: Option<LabelColor>,
    typed: &'a str,
    error: Option<&'a str>,
    messages: &ColorMessages,
) -> Element<'a, Message> {
    let mut cells: Vec<Element<'static, Message>> =
        vec![clear_swatch(selected.is_none(), messages.cleared.clone())];
    cells.extend(
        presenters::LABEL_PALETTE
            .into_iter()
            .map(|color| swatch(color, selected == Some(color), messages.selected)),
    );

    let mut swatches = column![].spacing(SWATCH_SPACING);
    let mut cells = cells.into_iter().peekable();
    while cells.peek().is_some() {
        let mut line = row![].spacing(SWATCH_SPACING).align_y(Center);
        for cell in cells.by_ref().take(SWATCHES_PER_ROW) {
            line = line.push(cell);
        }
        swatches = swatches.push(line);
    }

    let control = column![
        swatches,
        text_input("#4f9dff", typed)
            .on_input(messages.changed)
            .padding(FIELD_PADDING)
            .size(FIELD_TEXT_SIZE)
            .width(126)
            .style(move |theme, status| style::form_input(theme, status, error.is_some())),
    ]
    .spacing(8);

    labelled(label, control.into(), error)
}

fn swatch(
    color: LabelColor,
    selected: bool,
    on_press: fn(LabelColor) -> Message,
) -> Element<'static, Message> {
    let fill = style::label_color(color);
    let mark: Element<'static, Message> = if selected {
        // The check mark keeps the selected swatch legible without relying on its halo alone.
        text("✓")
            .size(13)
            .line_height(1.0)
            .width(Fill)
            .align_x(Center)
            .into()
    } else {
        Space::new().width(Fill).height(Fill).into()
    };
    button(mark)
        .on_press(on_press(color))
        .width(SWATCH_SIZE)
        .height(SWATCH_SIZE)
        .padding(0)
        .style(move |_theme, status| style::swatch(status, fill, selected))
        .into()
}

fn clear_swatch(selected: bool, on_press: Message) -> Element<'static, Message> {
    button(
        text("—")
            .size(13)
            .line_height(1.0)
            .width(Fill)
            .align_x(Center),
    )
    .on_press(on_press)
    .width(SWATCH_SIZE)
    .height(SWATCH_SIZE)
    .padding(0)
    .style(move |theme, status| style::clear_swatch(theme, status, selected))
    .into()
}

fn launcher_editor<'a>(draft: &'a LauncherDraft, icons: &IconSet) -> Element<'a, Message> {
    let mut form =
        column![sheet_header(draft.title(), editor_detail(draft))].spacing(SHEET_SPACING);
    if draft.section == Section::Agents {
        form = form.push(provider_picker(draft));
    }
    form = form
        .push(draft_field(
            draft,
            "Name",
            Field::Name,
            name_placeholder(draft),
            Some(LAUNCHER_NAME_ID),
        ))
        .push(color_picker(
            "Color",
            draft.selected_color(),
            &draft.color,
            draft.field_error(Field::Color),
            &ColorMessages {
                changed: Message::DraftColorChanged,
                selected: Message::DraftColorSelected,
                cleared: Message::DraftColorCleared,
            },
        ));
    form = match draft.kind {
        LauncherKind::Ssh => ssh_fields(form, draft, icons),
        LauncherKind::Agent => agent_fields(form, draft, icons),
        LauncherKind::Custom => custom_fields(form, draft),
    };
    form = form
        .push(general_error(draft.general_error()))
        .push(sheet_actions(draft.action(), Message::SaveLauncher, false));

    // The scrollbar is drawn inside the scrollable, so the fields keep a gutter clear of it. That
    // gutter is taken out of the sheet's own right padding instead of being added to it, which
    // leaves the visible margins on both sides equal whether or not the editor scrolls.
    container(scrollable(container(form).padding(Padding {
        top: 0.0,
        right: SCROLLBAR_GUTTER,
        bottom: 0.0,
        left: 0.0,
    })))
    .width(500)
    .max_height(560.0)
    .padding(Padding {
        top: SHEET_PADDING,
        right: SHEET_PADDING - SCROLLBAR_GUTTER,
        bottom: SHEET_PADDING,
        left: SHEET_PADDING,
    })
    .style(style::sheet)
    .into()
}

fn ssh_fields<'a>(
    form: Column<'a, Message>,
    draft: &'a LauncherDraft,
    icons: &IconSet,
) -> Column<'a, Message> {
    let mut form = form
        .push(
            row![
                container(draft_field(
                    draft,
                    "Host",
                    Field::Host,
                    "api.example.com",
                    None
                ))
                .width(Fill),
                container(draft_field(draft, "Username", Field::User, "deploy", None)).width(Fill),
            ]
            .spacing(12),
        )
        .push(reachability_row(draft))
        .push(disclosure_header(icons, draft.advanced));
    if draft.advanced {
        form = form
            .push(
                row![
                    container(draft_field(draft, "Port override", Field::Port, "22", None))
                        .width(120),
                    container(identity_file_field(draft)).width(Fill),
                ]
                .spacing(12),
            )
            .push(args_field(
                draft,
                "Extra arguments",
                "Each non-empty line is passed to ssh as one literal argument.",
            ));
    }
    form
}

/// The reachability check: a button that opens a socket to the host, and the verdict it came back
/// with.
///
/// The check is offered rather than forced, and never blocks saving: a server that is switched off
/// today is still worth writing down.
fn reachability_row(draft: &LauncherDraft) -> Element<'_, Message> {
    let destination = draft.destination();
    let mut test = button(
        text(
            if matches!(
                draft.current_reachability(),
                Some(ReachabilityState::Checking { .. })
            ) {
                "Checking…"
            } else {
                "Test connection"
            },
        )
        .size(BUTTON_TEXT_SIZE)
        .font(typography::MEDIUM),
    )
    .padding(BUTTON_PADDING)
    .style(style::secondary_button);
    // Nothing to dial until there is a host, and a port that is not a port would test the wrong
    // destination.
    if destination.is_some()
        && !matches!(
            draft.current_reachability(),
            Some(ReachabilityState::Checking { .. })
        )
    {
        test = test.on_press(Message::TestReachability);
    }

    let verdict: Element<'_, Message> = match draft.current_reachability() {
        None | Some(ReachabilityState::Untested) => Space::new().width(0).into(),
        Some(ReachabilityState::Checking { destination }) => {
            reachability_text(format!("Opening a connection to {destination}…"), None)
        }
        Some(ReachabilityState::Checked {
            outcome,
            destination,
        }) => match outcome {
            Reachability::Open {
                service: Some(service),
            } => reachability_text(format!("Reachable · {service}"), Some(false)),
            Reachability::Open { service: None } => reachability_text(
                format!("{destination} accepted a connection, but did not identify itself."),
                Some(false),
            ),
            Reachability::Refused => reachability_text(
                format!("{destination} refused the connection. Nothing is listening on that port."),
                Some(true),
            ),
            Reachability::TimedOut => reachability_text(
                format!(
                    "{destination} did not answer. It may be off, or a firewall may be dropping the port."
                ),
                Some(true),
            ),
            Reachability::Unresolved => reachability_text(
                format!("{} does not resolve to an address.", draft.host.trim()),
                Some(true),
            ),
        },
        Some(ReachabilityState::Failed { message, .. }) => {
            reachability_text(format!("The check could not run: {message}"), Some(true))
        }
    };

    row![test, container(verdict).width(Fill)]
        .spacing(10)
        .align_y(Center)
        .into()
}

/// One line of check feedback. `is_error` picks the tone; `None` is a check still in flight.
fn reachability_text(message: String, is_error: Option<bool>) -> Element<'static, Message> {
    text(message)
        .size(11)
        .line_height(1.3)
        .style(move |theme: &Theme| match is_error {
            Some(true) => style::error_text(theme),
            Some(false) => style::success_text(theme),
            None => style::secondary_text(theme),
        })
        .into()
}

/// The row that opens and closes the additional settings of a launcher.
fn disclosure_header(icons: &IconSet, open: bool) -> Element<'static, Message> {
    button(
        row![
            text("Additional settings")
                .size(13)
                .font(typography::MEDIUM)
                .line_height(1.0),
            symbol(
                icons,
                if open {
                    Symbol::Collapse
                } else {
                    Symbol::Expand
                },
                Tint::Secondary,
                9.0,
            ),
        ]
        .spacing(6)
        .align_y(Center),
    )
    .on_press(Message::DraftAdvancedToggled)
    .width(Fill)
    // No horizontal padding: the label lines up with the field labels above and below it, and the
    // hover fill still spans the row because the button is full width.
    .padding(Padding {
        top: 6.0,
        right: 0.0,
        bottom: 6.0,
        left: 0.0,
    })
    .style(style::disclosure)
    .into()
}

/// The identity file field: a path anyone can type, plus a native file chooser next to it.
fn identity_file_field(draft: &LauncherDraft) -> Element<'_, Message> {
    let error = draft.field_error(Field::IdentityFile);
    let has_error = error.is_some();
    let control = row![
        text_input("$HOME/.ssh/id_ed25519", draft.value(Field::IdentityFile))
            .on_input(|value| Message::DraftFieldChanged(Field::IdentityFile, value))
            .padding(FIELD_PADDING)
            .size(FIELD_TEXT_SIZE)
            .width(Fill)
            .style(move |theme, status| style::form_input(theme, status, has_error)),
        button(
            text("Choose…")
                .size(BUTTON_TEXT_SIZE)
                .font(typography::MEDIUM)
        )
        .on_press(Message::DraftIdentityFileRequested)
        .padding(BUTTON_PADDING)
        .style(style::secondary_button),
    ]
    .spacing(8)
    .align_y(Center);
    labelled("Identity file", control.into(), error)
}

/// The agent editor: choose the command, then say what else to pass it.
///
/// Everything a built-in provider already knows — which program to run, where, and with what
/// environment — sits behind the disclosure, so adding an agent is picking one and pressing Add.
fn agent_fields<'a>(
    form: Column<'a, Message>,
    draft: &'a LauncherDraft,
    icons: &IconSet,
) -> Column<'a, Message> {
    let mut form = form
        .push(args_field(
            draft,
            "Parameters",
            "Optional. One argument per line, passed to the CLI exactly as written.",
        ))
        .push(disclosure_header(icons, draft.advanced));
    if draft.advanced {
        form = form
            .push(
                row![
                    container(draft_field(
                        draft,
                        "Executable",
                        Field::Executable,
                        "codex",
                        None
                    ))
                    .width(Fill),
                    container(draft_field(
                        draft,
                        "Working directory",
                        Field::WorkingDirectory,
                        "$HOME/projects/app",
                        None,
                    ))
                    .width(Fill),
                ]
                .spacing(12),
            )
            .push(env_field(draft));
    }
    form
}

fn custom_fields<'a>(form: Column<'a, Message>, draft: &'a LauncherDraft) -> Column<'a, Message> {
    let mut form = form.push(raw_shell_toggle(draft));
    if draft.raw_shell {
        form = form.push(draft_field(
            draft,
            "Shell command",
            Field::RawCommand,
            "make deploy",
            None,
        ));
    } else {
        form = form
            .push(draft_field(
                draft,
                "Executable",
                Field::Executable,
                "zsh",
                None,
            ))
            .push(args_field(
                draft,
                "Arguments",
                "One argument per line, passed to the command exactly as written.",
            ));
    }
    form.push(draft_field(
        draft,
        "Working directory",
        Field::WorkingDirectory,
        "$HOME/projects/app",
        None,
    ))
    .push(env_field(draft))
}

fn provider_picker(draft: &LauncherDraft) -> Element<'_, Message> {
    let selected = draft.selected_provider();
    let mut segments = row![].spacing(2);
    for provider in [
        Some(AgentProvider::Codex),
        Some(AgentProvider::Claude),
        Some(AgentProvider::Gemini),
        Some(AgentProvider::OpenCode),
        None,
    ] {
        let is_selected = selected == provider;
        let label = provider.map_or("Custom CLI", presenters::provider_name);
        segments = segments.push(
            button(
                text(label)
                    .size(BUTTON_TEXT_SIZE)
                    .font(typography::MEDIUM)
                    .line_height(1.0)
                    .width(Fill)
                    .align_x(Center),
            )
            .on_press(Message::DraftProviderSelected(provider))
            .width(Fill)
            .padding(Padding::from([7.0, 6.0]))
            .style(move |theme, status| style::segment(theme, status, is_selected)),
        );
    }

    labelled(
        "Provider",
        container(segments)
            .padding(2)
            .style(style::segment_track)
            .into(),
        None,
    )
}

fn raw_shell_toggle(draft: &LauncherDraft) -> Element<'_, Message> {
    let mut block = column![
        toggler(draft.raw_shell)
            .label("Run a raw shell command")
            .text_size(15)
            .size(22)
            .on_toggle(Message::DraftRawShellToggled),
    ]
    .spacing(6);
    if draft.raw_shell {
        block = block.push(
            text(
                "Raw shell commands are executed by your login shell with your privileges. \
                 Only enable this for commands you wrote and trust.",
            )
            .size(12)
            .line_height(1.35)
            .style(style::error_text),
        );
    }
    block.into()
}

fn draft_field<'a>(
    draft: &'a LauncherDraft,
    label: &'static str,
    field: Field,
    placeholder: &'static str,
    id: Option<&'static str>,
) -> Element<'a, Message> {
    form_field(
        label,
        draft.value(field),
        placeholder,
        move |value| Message::DraftFieldChanged(field, value),
        draft.field_error(field),
        id,
    )
}

fn form_field<'a>(
    label: &'static str,
    value: &'a str,
    placeholder: &'static str,
    on_input: impl Fn(String) -> Message + 'a,
    error: Option<&'a str>,
    id: Option<&'static str>,
) -> Element<'a, Message> {
    let has_error = error.is_some();
    let mut input = text_input(placeholder, value)
        .on_input(on_input)
        .padding(FIELD_PADDING)
        .size(FIELD_TEXT_SIZE)
        .style(move |theme, status| style::form_input(theme, status, has_error));
    if let Some(id) = id {
        input = input.id(iced::widget::Id::new(id));
    }
    labelled(label, input.into(), error)
}

fn args_field<'a>(
    draft: &'a LauncherDraft,
    label: &'static str,
    hint: &'static str,
) -> Element<'a, Message> {
    let error = draft.field_error(Field::Args);
    let has_error = error.is_some();
    let editor = column![
        text_editor(&draft.args)
            .placeholder("One argument per line")
            .on_action(Message::DraftArgsEdited)
            .font(typography::UI)
            .size(FIELD_TEXT_SIZE)
            .min_height(58.0)
            .max_height(88.0)
            .padding(9)
            .style(move |theme, status| style::form_editor(theme, status, has_error)),
        text(hint)
            .size(11)
            .line_height(1.3)
            .style(style::tertiary_text),
    ]
    .spacing(4);
    labelled(label, editor.into(), error)
}

/// The environment block: one `NAME=value` per line, exported before the command runs.
fn env_field(draft: &LauncherDraft) -> Element<'_, Message> {
    let error = draft.field_error(Field::Env);
    let has_error = error.is_some();
    let editor = column![
        text_editor(&draft.env)
            .placeholder("NAME=value, one per line")
            .on_action(Message::DraftEnvEdited)
            .font(typography::UI)
            .size(FIELD_TEXT_SIZE)
            .min_height(46.0)
            .max_height(88.0)
            .padding(9)
            .style(move |theme, status| style::form_editor(theme, status, has_error)),
        text(
            "Exported before the command runs. This is where a shell alias's variables go, since \
             an alias itself cannot be launched."
        )
        .size(11)
        .line_height(1.3)
        .style(style::tertiary_text),
    ]
    .spacing(4);
    labelled("Environment", editor.into(), error)
}

fn labelled<'a>(
    label: &'static str,
    control: Element<'a, Message>,
    error: Option<&'a str>,
) -> Element<'a, Message> {
    let mut field = column![
        text(label)
            .size(11)
            .font(typography::MEDIUM)
            .line_height(1.0)
            .style(style::secondary_text),
        control,
    ]
    .spacing(5)
    .align_x(Left);
    if let Some(error) = error {
        field = field.push(
            text(error)
                .size(11)
                .line_height(1.3)
                .style(style::error_text),
        );
    }
    field.into()
}

fn general_error(error: Option<&str>) -> Element<'_, Message> {
    error.map_or_else(
        || Element::from(Space::new().height(0)),
        |error| {
            text(error)
                .size(13)
                .line_height(1.3)
                .style(style::error_text)
                .into()
        },
    )
}

fn confirm_sheet(
    title: &'static str,
    detail: String,
    action: &'static str,
) -> Element<'static, Message> {
    container(
        column![
            sheet_header(title, detail),
            sheet_actions(action, Message::ConfirmDelete, true),
        ]
        .spacing(20),
    )
    .width(450)
    .padding(26)
    .style(style::sheet)
    .into()
}

/// Chooses between following the system appearance and pinning light or dark.
fn appearance_picker(selected: AppearanceChoice) -> Element<'static, Message> {
    let mut segments = row![].spacing(2);
    for choice in AppearanceChoice::ALL {
        let is_selected = choice == selected;
        segments = segments.push(
            button(
                text(choice.label())
                    .size(BUTTON_TEXT_SIZE)
                    .font(typography::MEDIUM)
                    .line_height(1.0)
                    .width(Fill)
                    .align_x(Center),
            )
            .on_press(Message::AppearanceSelected(choice))
            .width(Fill)
            .padding(Padding::from([7.0, 6.0]))
            .style(move |theme, status| style::segment(theme, status, is_selected)),
        );
    }

    labelled(
        "Appearance",
        container(segments)
            .padding(2)
            .style(style::segment_track)
            .into(),
        None,
    )
}

fn settings_sheet(shell: &AppShell) -> Element<'_, Message> {
    let path = shell.application.state.source_path.as_ref().map_or_else(
        || "No config selected".to_owned(),
        |path| path.display().to_string(),
    );

    let shortcuts_list = column(
        [
            (Shortcut::AddItem, "Add launcher"),
            (Shortcut::AddGroup, "Add group"),
            (Shortcut::FocusSearch, "Search"),
            (Shortcut::Launch, "Launch selected"),
            (Shortcut::Edit, "Edit selected"),
            (Shortcut::Delete, "Delete selected"),
        ]
        .map(|(shortcut, label)| {
            row![
                text(label).size(FIELD_TEXT_SIZE).line_height(1.0),
                Space::new().width(Fill),
                text(shortcuts::equivalent(shortcut))
                    .size(12)
                    .line_height(1.0)
                    .style(style::tertiary_text),
            ]
            .align_y(Center)
            .into()
        }),
    )
    .spacing(7);

    let body = column![
        sheet_header(
            "Settings",
            "Groups and launchers live in a shared JSON file. Secrets are never stored.".to_owned(),
        ),
        appearance_picker(shell.appearance),
        labelled(
            "Launches open in",
            container(
                text(shell.application.terminal_destination())
                    .size(FIELD_TEXT_SIZE)
                    .line_height(1.3)
            )
            .width(Fill)
            .padding(FIELD_PADDING)
            .style(style::search_field)
            .into(),
            None,
        ),
        labelled(
            "Config file",
            container(text(path).size(FIELD_TEXT_SIZE).line_height(1.3))
                .width(Fill)
                .padding(FIELD_PADDING)
                .style(style::search_field)
                .into(),
            None,
        ),
        row![
            button(
                text("Reveal in Finder")
                    .size(BUTTON_TEXT_SIZE)
                    .font(typography::MEDIUM)
            )
            .on_press(Message::RevealConfig)
            .padding(BUTTON_PADDING)
            .style(style::secondary_button),
            button(
                text("Reload")
                    .size(BUTTON_TEXT_SIZE)
                    .font(typography::MEDIUM)
            )
            .on_press(Message::ReloadConfig)
            .padding(BUTTON_PADDING)
            .style(style::secondary_button),
        ]
        .spacing(8),
        labelled("Keyboard", shortcuts_list.into(), None),
        row![
            Space::new().width(Fill),
            button(text("Done").size(BUTTON_TEXT_SIZE).font(typography::MEDIUM))
                .on_press(Message::DismissDialog)
                .padding(BUTTON_PADDING)
                .style(style::primary_button),
        ],
    ]
    .spacing(SHEET_SPACING);

    // The sheet scrolls for the same reason the launcher editor does: it must stay inside a short
    // window rather than running its last control off the bottom edge.
    container(scrollable(container(body).padding(Padding {
        top: 0.0,
        right: SCROLLBAR_GUTTER,
        bottom: 0.0,
        left: 0.0,
    })))
    .width(470)
    .max_height(560.0)
    .padding(Padding {
        top: SHEET_PADDING,
        right: SHEET_PADDING - SCROLLBAR_GUTTER,
        bottom: SHEET_PADDING,
        left: SHEET_PADDING,
    })
    .style(style::sheet)
    .into()
}

fn sheet_actions(
    label: &'static str,
    confirm: Message,
    destructive: bool,
) -> Element<'static, Message> {
    let confirm_button = button(text(label).size(BUTTON_TEXT_SIZE).font(typography::MEDIUM))
        .on_press(confirm)
        .padding(BUTTON_PADDING);
    row![
        Space::new().width(Fill),
        button(
            text("Cancel")
                .size(BUTTON_TEXT_SIZE)
                .font(typography::MEDIUM)
        )
        .on_press(Message::DismissDialog)
        .padding(BUTTON_PADDING)
        .style(style::secondary_button),
        if destructive {
            confirm_button.style(style::destructive_button)
        } else {
            confirm_button.style(style::primary_button)
        },
    ]
    .spacing(8)
    .align_y(Center)
    .into()
}

// ---------------------------------------------------------------- shared pieces

/// A rail symbol, sized by the cell holding it instead of by a fixed point size.
fn rail_symbol(icons: &IconSet, icon: Symbol, tint: Tint) -> Element<'static, Message> {
    icons.get(icon, tint).map_or_else(
        || Element::from(Space::new().width(Fill).height(Fill)),
        |handle| {
            image(handle)
                .width(Fill)
                .height(Fill)
                .filter_method(image::FilterMethod::Linear)
                .into()
        },
    )
}

fn symbol(icons: &IconSet, icon: Symbol, tint: Tint, size: f32) -> Element<'static, Message> {
    icons.get(icon, tint).map_or_else(
        || Element::from(Space::new().width(size).height(size)),
        |handle| {
            image(handle)
                .width(size)
                .height(size)
                .filter_method(image::FilterMethod::Linear)
                .into()
        },
    )
}

fn icon_button(
    icons: &IconSet,
    icon: Symbol,
    tint: Tint,
    label: &'static str,
    message: Message,
) -> Element<'static, Message> {
    tooltip(
        button(symbol(icons, icon, tint, 15.0))
            .on_press(message)
            .width(ACTION_BUTTON_SIZE)
            .height(ACTION_BUTTON_SIZE)
            .padding(0)
            .style(style::icon_button),
        container(text(label).size(12).line_height(1.2))
            .padding(Padding::from([5.0, 9.0]))
            .style(style::sheet),
        tooltip::Position::Bottom,
    )
    .gap(6)
    .into()
}

const fn section_title(section: Section) -> &'static str {
    match section {
        Section::Servers => "Servers",
        Section::Agents => "Agents",
    }
}

const fn section_symbol(section: Section) -> Symbol {
    match section {
        Section::Servers => Symbol::Servers,
        Section::Agents => Symbol::Agents,
    }
}

const fn add_group_label(section: Section) -> &'static str {
    match section {
        Section::Servers => "Add server group",
        Section::Agents => "Add agent group",
    }
}

const fn add_item_label(section: Section) -> &'static str {
    match section {
        Section::Servers => "Add server",
        Section::Agents => "Add agent",
    }
}

const fn name_placeholder(draft: &LauncherDraft) -> &'static str {
    match draft.kind {
        LauncherKind::Ssh => "API server",
        LauncherKind::Agent => "Codex",
        LauncherKind::Custom => "Repo shell",
    }
}

fn editor_detail(draft: &LauncherDraft) -> String {
    match draft.kind {
        LauncherKind::Ssh => {
            "Structured SSH connection. Key paths are stored, key contents never are.".to_owned()
        }
        LauncherKind::Agent => {
            "A built-in agent CLI. The executable and arguments stay editable.".to_owned()
        }
        LauncherKind::Custom => "Any command line, launched in your terminal.".to_owned(),
    }
}

fn config_label(shell: &AppShell) -> String {
    shell
        .application
        .state
        .source_path
        .as_ref()
        .and_then(|path| path.file_name())
        .map_or_else(
            || "No config selected".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        )
}

#[cfg(test)]
mod tests {
    use super::{
        ACTION_BUTTON_SIZE, ACTION_EDGE_PADDING, ACTION_SPACING, BADGE_SIZE, CARD_PADDING,
        CHIP_ICON, CHIP_PADDING, DropState, GROUP_ACTIONS_WIDTH, MIN_CARD_WIDTH, SubgroupRow,
        TILE_ACTIONS_WIDTH, card_label_width, card_width_need, chip_width, drop_state, label_width,
        pack_rows, uniform_card_width,
    };
    use crate::presenters::launcher_count;

    #[test]
    fn chips_wrap_onto_the_next_row_only_once_they_stop_fitting() {
        assert_eq!(pack_rows(&[100.0, 100.0, 100.0], 400.0, 10.0), vec![3]);
        assert_eq!(pack_rows(&[100.0, 100.0, 100.0], 210.0, 10.0), vec![2, 1]);
        assert_eq!(
            pack_rows(&[100.0, 100.0, 100.0], 100.0, 10.0),
            vec![1, 1, 1]
        );
    }

    #[test]
    fn a_chip_wider_than_the_panel_still_gets_a_row_of_its_own() {
        assert_eq!(pack_rows(&[900.0, 80.0], 300.0, 10.0), vec![1, 1]);
        assert!(pack_rows(&[], 300.0, 10.0).is_empty());
    }

    #[test]
    fn only_the_categories_a_drag_could_land_in_offer_themselves() {
        let source = despatch_domain::SubgroupId::new();
        let other = despatch_domain::SubgroupId::new();
        let pressed = crate::app_shell::Drag::new(despatch_domain::LauncherId::new(), source);
        let carried = crate::app_shell::Drag {
            lifted: true,
            ..pressed
        };

        assert_eq!(drop_state(None, Some(other), other), DropState::Idle);
        assert_eq!(
            drop_state(Some(pressed), Some(other), other),
            DropState::Idle,
            "a press that has not left its place is still a click"
        );
        assert_eq!(
            drop_state(Some(carried), None, source),
            DropState::Idle,
            "the category it already lives in promises nothing"
        );
        assert_eq!(drop_state(Some(carried), None, other), DropState::Available);
        assert_eq!(
            drop_state(Some(carried), Some(other), other),
            DropState::Engaged
        );
    }

    #[test]
    fn a_category_chip_reserves_room_for_its_icon_name_and_count() {
        let narrow = SubgroupRow {
            id: despatch_domain::SubgroupId::new(),
            name: "Web".to_owned(),
            item_count: 1,
            color: None,
        };
        let wide = SubgroupRow {
            id: despatch_domain::SubgroupId::new(),
            name: "Grafana dashboards".to_owned(),
            item_count: 128,
            color: None,
        };

        assert!(chip_width(&wide) > chip_width(&narrow));
        assert!(chip_width(&narrow) > CHIP_PADDING * 2.0 + CHIP_ICON);
    }

    #[test]
    fn every_card_on_a_screen_takes_the_width_the_longest_of_them_needs() {
        let short = card_width_need(card_label_width("Aurora", "root@a.example"), 92.0);
        let long = card_width_need(
            card_label_width("Aurora", "root@aurora.secure.example"),
            92.0,
        );

        assert!(long > short, "the longer host asks for more room");
        assert!(approximately(
            uniform_card_width([short, long], 1_400.0),
            uniform_card_width([long], 1_400.0)
        ));
        assert!(
            uniform_card_width([short, long], 1_400.0) < 1_400.0 / 3.0,
            "and the result still ends well short of a third of the panel"
        );
    }

    #[test]
    fn a_card_grows_to_whatever_its_longest_label_needs() {
        // No upper bound of its own: a card that would cut a host short instead gets wider, all
        // the way to the edge of the panel.
        assert!(approximately(uniform_card_width([620.0], 1_400.0), 620.0));
        assert!(approximately(uniform_card_width([4_000.0], 500.0), 500.0,));
        assert!(approximately(
            uniform_card_width([0.0], 500.0),
            MIN_CARD_WIDTH
        ));
        assert!(approximately(
            uniform_card_width(std::iter::empty(), 500.0),
            MIN_CARD_WIDTH
        ));
        // A panel narrower than the minimum wins, so nothing is ever laid out wider than its room.
        assert!(approximately(uniform_card_width([0.0], 90.0), 90.0));
        assert!(approximately(uniform_card_width([200.0], 0.0), 0.0));
    }

    #[test]
    fn the_reserved_strip_holds_every_action_that_floats_over_it() {
        // Three actions on a launcher, two on a group: labels stop where the buttons begin.
        assert!(approximately(
            TILE_ACTIONS_WIDTH,
            3.0f32.mul_add(ACTION_BUTTON_SIZE, 2.0 * ACTION_SPACING) + 2.0 * ACTION_EDGE_PADDING
        ));
        const { assert!(GROUP_ACTIONS_WIDTH < TILE_ACTIONS_WIDTH) }
    }

    #[test]
    fn a_cards_room_covers_its_badge_padding_labels_and_actions() {
        let need = card_width_need(120.0, TILE_ACTIONS_WIDTH);

        assert!(need > CARD_PADDING * 2.0 + BADGE_SIZE + 120.0 + TILE_ACTIONS_WIDTH);
    }

    #[test]
    fn labels_never_claim_the_room_reserved_for_badges_and_actions() {
        assert!(label_width(400.0, 100.0) < 400.0 - 100.0);
        assert!(approximately(label_width(40.0, 100.0), 0.0));
    }

    fn approximately(left: f32, right: f32) -> bool {
        (left - right).abs() < 0.01
    }

    #[test]
    fn destructive_confirmations_count_the_launchers_they_remove() {
        assert_eq!(launcher_count(0), "0 launchers");
        assert_eq!(launcher_count(1), "1 launcher");
        assert_eq!(launcher_count(4), "4 launchers");
    }
}
