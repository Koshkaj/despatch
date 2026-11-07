//! The menu-bar panel: a header with search and a pin, a source-list sidebar, the list pane, and a
//! footer bar — with sheets and the dragged row drawn above it.

mod editors;
mod settings;

use despatch_domain::{LabelColor, LauncherId, SubgroupId};
use iced::widget::text::Wrapping;
use iced::widget::{
    Column, Space, button, column, container, image, mouse_area, pick_list, responsive, row, rule,
    scrollable, stack, text, text_input, tooltip,
};
use iced::{Center, Element, Fill, Padding, Right, Theme, window};

use crate::app_shell::{AppShell, ContextTarget, Drag, Message, PANEL_WIDTH, SEARCH_ID};
use crate::drag_layer::DragLayer;
use crate::icons::{IconSet, Symbol, Tint};
use crate::presenters::{self, Destination, ListRow, Segment, SidebarRow};
use crate::shortcuts::{self, Shortcut};
use crate::style;
use crate::typography;

/// The sidebar's width: room for a group name, its count, and the hover actions that replace it.
const SIDEBAR_WIDTH: f32 = 184.0;
const HEADER_HEIGHT: f32 = 52.0;
const FOOTER_HEIGHT: f32 = 38.0;
const SIDEBAR_ROW_HEIGHT: f32 = 28.0;
/// A source-list row's symbol, a little taller than its 13-point label like `AppKit`'s.
const SIDEBAR_SYMBOL: f32 = 18.0;
const ROW_HEIGHT: f32 = 50.0;
/// The rounded square a row's symbol sits on.
const ICON_TILE: f32 = 30.0;
const ROW_PADDING: f32 = 10.0;
const ROW_SPACING: f32 = 10.0;
const PANE_PADDING: f32 = 18.0;
const SEGMENT_HEIGHT: f32 = 28.0;
/// The "+" pill that adds a category, and the "More" menu that takes the categories that do not
/// fit.
pub(crate) const ADD_SEGMENT_WIDTH: f32 = 28.0;
pub(crate) const MORE_SEGMENT_WIDTH: f32 = 84.0;
/// The space between two category pills.
pub(crate) const SEGMENT_GAP: f32 = 8.0;
const ACTION_BUTTON_SIZE: f32 = 24.0;
const ACTION_SPACING: f32 = 2.0;
/// The widest a row's trailing tag gets before it is shortened.
const TRAILING_MAX: f32 = 190.0;
/// Room for the scrollbar, so nothing is ever drawn underneath one.
const SCROLLBAR_GUTTER: f32 = 12.0;
/// The list's width: the panel less the sidebar, the pane's padding, and the scrollbar's gutter.
/// The panel is a fixed size, so this is too, and labels are shortened against it.
const LIST_WIDTH: f32 = PANEL_WIDTH - SIDEBAR_WIDTH - 1.0 - PANE_PADDING * 2.0 - SCROLLBAR_GUTTER;
const NAME_SIZE: f32 = 13.0;
const DETAIL_SIZE: f32 = 12.0;

pub(crate) const FIELD_PADDING: Padding = Padding {
    top: 5.0,
    right: 8.0,
    bottom: 5.0,
    left: 8.0,
};
pub(crate) const FIELD_TEXT_SIZE: f32 = 13.0;
pub(crate) const BUTTON_PADDING: Padding = Padding {
    top: 5.0,
    right: 12.0,
    bottom: 5.0,
    left: 12.0,
};
pub(crate) const BUTTON_TEXT_SIZE: f32 = 13.0;

pub fn view(shell: &AppShell, _window: window::Id) -> Element<'_, Message> {
    let body = column![
        header(shell),
        hairline(),
        row![sidebar(shell), vertical_hairline(), pane(shell)].height(Fill),
        hairline(),
        footer(shell),
    ];
    // The panel's own border is one point wide; the content sits inside it rather than painting
    // the pane's ground over it.
    let panel = container(container(body).padding(1))
        .width(Fill)
        .height(Fill)
        .style(style::panel);

    // The panel is always the same two layers, even when the upper one has nothing to show.
    //
    // Widget state — every scroll position among it — lives in a tree that iced reconciles against
    // the view. A tree whose root changes type is not reconciled but rebuilt from nothing, so a
    // panel that were a `container` at rest and a `stack` while something floated above it would
    // throw away where the user had scrolled to the moment they pressed a row.
    let overlay = shell.application.state.dialog.as_ref().map_or_else(
        || {
            dragged_row(shell)
                .or_else(|| dragged_group(shell))
                .unwrap_or_else(|| Space::new().into())
        },
        |dialog| editors::sheet_overlay(shell, dialog),
    );
    stack![panel, overlay].width(Fill).height(Fill).into()
}

/// The row riding along with the pointer, drawn above everything else.
///
/// It is inert on purpose: it draws and nothing more. `stack` hands the cursor to the layers under
/// the topmost one that claims a mouse interaction, so a layer that claimed one here would take the
/// pointer away from the segments — the very things the row is being carried to.
fn dragged_row(shell: &AppShell) -> Option<Element<'_, Message>> {
    let drag = shell.drag?;
    let tile = presenters::launcher_tile(&shell.application.state, drag.launcher_id)?;
    let width = 300.0;
    let card = container(
        row![
            icon_tile(&shell.icons, tile.symbol, tile.color),
            row_labels(&tile.name, &tile.detail, width - 70.0),
        ]
        .spacing(ROW_SPACING)
        .align_y(Center),
    )
    .width(width)
    .height(ROW_HEIGHT)
    .padding(Padding::from([0.0, ROW_PADDING]))
    .center_y(ROW_HEIGHT)
    .style(style::dragged_row);

    // The layer follows the pointer itself, without a message per movement. See `drag_layer`.
    Some(DragLayer::new(card, drag.lifted, Message::DragLifted).into())
}

/// The group row riding along with the pointer while a group is moved within the sidebar.
fn dragged_group(shell: &AppShell) -> Option<Element<'_, Message>> {
    let drag = shell.group_drag?;
    let group = shell.group(drag.group_id)?;
    let tint = group
        .color
        .map_or(Tint::Secondary, |color| Tint::Custom(color.rgb()));
    let row_view = container(
        row![
            symbol(&shell.icons, Symbol::Group, tint, 15.0),
            text(presenters::ellipsize(&group.name, 120.0, 13.0))
                .size(13)
                .wrapping(Wrapping::None),
        ]
        .spacing(8)
        .align_y(Center),
    )
    .width(SIDEBAR_WIDTH - 16.0)
    .height(SIDEBAR_ROW_HEIGHT)
    .padding(Padding::from([0.0, 10.0]))
    .center_y(SIDEBAR_ROW_HEIGHT)
    .style(style::dragged_row);
    Some(DragLayer::new(row_view, drag.lifted, Message::GroupDragLifted).into())
}

// ---------------------------------------------------------------- header

fn header(shell: &AppShell) -> Element<'_, Message> {
    let title = container(
        row![
            symbol(&shell.icons, Symbol::App, Tint::Label, 18.0),
            text("Despatch")
                .size(15)
                .font(typography::SEMIBOLD)
                .wrapping(Wrapping::None),
        ]
        .spacing(8)
        .align_y(Center),
    )
    // The title takes the sidebar's width, so the search field starts where the pane does.
    .width(SIDEBAR_WIDTH - 14.0);

    let pinned = shell.pinned;
    let pin = with_tooltip(
        button(symbol(
            &shell.icons,
            if pinned { Symbol::Pinned } else { Symbol::Pin },
            if pinned {
                Tint::Accent
            } else {
                Tint::Secondary
            },
            15.0,
        ))
        .on_press(Message::TogglePin)
        .width(28)
        .height(28)
        .padding(6)
        .style(move |theme, status| style::toggle_button(theme, status, pinned))
        .into(),
        if pinned {
            "Pinned — stays open when you click elsewhere"
        } else {
            "Keep open when you click elsewhere"
        },
        tooltip::Position::Bottom,
    );

    container(
        row![title, search_field(shell), pin]
            .spacing(12)
            .align_y(Center),
    )
    .height(HEADER_HEIGHT)
    .padding(Padding::from([0.0, 14.0]))
    .center_y(HEADER_HEIGHT)
    .into()
}

fn search_field(shell: &AppShell) -> Element<'_, Message> {
    let query = &shell.application.state.search_query;
    let trailing: Element<'_, Message> = if query.is_empty() {
        text(shortcuts::equivalent(Shortcut::FocusSearch))
            .size(12)
            .style(style::tertiary_text)
            .into()
    } else {
        button(symbol(&shell.icons, Symbol::Clear, Tint::Secondary, 13.0))
            .on_press(Message::ClearSearch)
            .padding(0)
            .style(|theme: &Theme, _status| button::Style {
                background: None,
                ..style::icon_button(theme, button::Status::Active)
            })
            .into()
    };
    container(
        row![
            symbol(&shell.icons, Symbol::Search, Tint::Secondary, 13.0),
            text_input("Search servers", query)
                .id(iced::widget::Id::new(SEARCH_ID))
                .on_input(Message::SearchChanged)
                .on_submit(Message::SearchSubmitted)
                .size(FIELD_TEXT_SIZE)
                .padding(0)
                .width(Fill)
                .style(style::borderless_input),
            trailing,
        ]
        .spacing(7)
        .align_y(Center),
    )
    .width(Fill)
    .height(28)
    .padding(Padding::from([0.0, 9.0]))
    .center_y(28)
    .style(style::search_field)
    .into()
}

// ---------------------------------------------------------------- sidebar

fn sidebar(shell: &AppShell) -> Element<'_, Message> {
    let state = &shell.application.state;
    let mut list = column![sidebar_heading(
        &shell.icons,
        "Groups",
        Some(("Add group  ⇧⌘N", Message::AddGroup))
    )]
    .spacing(1);
    let groups = presenters::sidebar_groups(state);
    if groups.is_empty() {
        list = list.push(
            container(text("No groups yet").size(12).style(style::tertiary_text))
                .padding(Padding::from([4.0, 10.0])),
        );
    }
    for group in &groups {
        list = list.push(sidebar_row(shell, group));
    }
    // Recently used and Settings stay at the foot of the sidebar, below however many groups scroll
    // above them.
    let mut foot = column![].spacing(1);
    for row in presenters::footer_rows(shell.application.recent_launches().len()) {
        foot = foot.push(sidebar_row(shell, &row));
    }

    container(column![scroll(list).height(Fill), foot].spacing(6))
        .width(SIDEBAR_WIDTH)
        .height(Fill)
        .padding(Padding {
            top: 10.0,
            right: 8.0,
            bottom: 8.0,
            left: 8.0,
        })
        .into()
}

/// A quiet heading in the source list, with an optional action at its right edge.
fn sidebar_heading(
    icons: &IconSet,
    label: &'static str,
    action: Option<(&'static str, Message)>,
) -> Element<'static, Message> {
    let mut heading = row![
        text(label)
            .size(11)
            .font(typography::SEMIBOLD)
            .style(style::section_heading_text),
        Space::new().width(Fill),
    ]
    .align_y(Center)
    .height(22);
    if let Some((tip, message)) = action {
        heading = heading.push(with_tooltip(
            button(symbol(icons, Symbol::Add, Tint::Secondary, 11.0))
                .on_press(message)
                .width(20)
                .height(20)
                .padding(4)
                .style(style::icon_button)
                .into(),
            tip,
            tooltip::Position::Right,
        ));
    }
    container(heading)
        .padding(Padding {
            top: 0.0,
            right: 2.0,
            bottom: 2.0,
            left: 10.0,
        })
        .into()
}

/// One destination in the source list.
///
/// A group's row trades its count for Rename and Delete while the pointer is over it, so a group can
/// be managed where it is listed without growing the sidebar a column of buttons.
fn sidebar_row(shell: &AppShell, row_data: &SidebarRow) -> Element<'static, Message> {
    let selected = shell.destination == Some(row_data.destination);
    let group_id = match row_data.destination {
        Destination::Group(group_id) => Some(group_id),
        _ => None,
    };
    let hovered = group_id.is_some()
        && shell.hovered_group == group_id
        && shell.drag.is_none()
        && shell.group_drag.is_none_or(|drag| !drag.lifted);
    let tint = match (selected, row_data.color) {
        (true, _) => Tint::OnAccent,
        (false, Some(color)) => Tint::Custom(color.rgb()),
        (false, None) => Tint::Secondary,
    };
    let count: Element<'static, Message> = match row_data.count {
        Some(count) if !hovered => text(count.to_string())
            .size(12)
            .style(move |theme: &Theme| {
                if selected {
                    style::on_accent_secondary_text(theme)
                } else {
                    style::tertiary_text(theme)
                }
            })
            .into(),
        _ => Space::new().width(0).into(),
    };
    // Room kept clear at the right for the count, or for the actions that replace it on hover.
    let reserve = match (hovered, row_data.count) {
        (true, _) => 46.0,
        (false, Some(count)) => presenters::text_width(&count.to_string(), 12.0) + 6.0,
        (false, None) => 0.0,
    };
    let content = row![
        symbol(&shell.icons, row_data.symbol, tint, SIDEBAR_SYMBOL),
        text(presenters::ellipsize(
            &row_data.name,
            SIDEBAR_WIDTH - 16.0 - 20.0 - SIDEBAR_SYMBOL - 8.0 - reserve,
            13.0
        ))
        .size(13)
        .wrapping(Wrapping::None)
        .width(Fill),
        count,
    ]
    .spacing(8)
    .align_y(Center);

    let Some(group_id) = group_id else {
        // A button lays its content out from the top, so the row centers it itself.
        return button(container(content).center_y(Fill))
            .on_press(Message::SelectDestination(row_data.destination))
            .width(Fill)
            .height(SIDEBAR_ROW_HEIGHT)
            .padding(Padding::from([0.0, 10.0]))
            .style(move |theme, status| style::source_list_row(theme, status, selected))
            .into();
    };
    // A group row is pressed rather than clicked: the press opens the group at once, like a source
    // list, and the same press can carry the group to another place in the list. A button would
    // settle the press itself and leave nothing to drag.
    let carrying = shell.group_drag.filter(|drag| drag.lifted);
    let held = carrying.is_some_and(|drag| drag.group_id == group_id);
    let target = carrying.is_some_and(|drag| drag.group_id != group_id)
        && shell.hovered_group == Some(group_id);
    let status = if held {
        button::Status::Pressed
    } else if shell.hovered_group == Some(group_id) {
        button::Status::Hovered
    } else {
        button::Status::Active
    };
    let surface = mouse_area(
        container(content)
            .width(Fill)
            .height(SIDEBAR_ROW_HEIGHT)
            .padding(Padding::from([0.0, 10.0]))
            .center_y(SIDEBAR_ROW_HEIGHT)
            .style(move |theme| style::source_list_surface(theme, status, selected, target)),
    )
    .on_press(Message::PressGroup(group_id))
    .on_right_press(Message::ContextMenu(ContextTarget::Group(group_id)));
    let hovered = hovered && carrying.is_none();
    let mut layers: Vec<Element<'static, Message>> = vec![surface.into()];
    if hovered {
        layers.push(group_row_actions(&shell.icons, group_id, selected));
    }
    hoverable(
        stack(layers).into(),
        Message::HoverGroup(Some(group_id)),
        Message::LeaveGroup(group_id),
    )
}

/// The Rename and Delete buttons that take the place of a hovered group row's count.
fn group_row_actions(
    icons: &IconSet,
    group_id: despatch_domain::GroupId,
    selected: bool,
) -> Element<'static, Message> {
    let tint = if selected {
        Tint::OnAccent
    } else {
        Tint::Secondary
    };
    container(
        row![
            small_action(
                icons,
                Symbol::Edit,
                tint,
                "Rename group",
                Message::RenameGroup(group_id)
            ),
            small_action(
                icons,
                Symbol::Delete,
                tint,
                "Delete group",
                Message::DeleteGroupRequested(group_id)
            ),
        ]
        .spacing(0),
    )
    .width(Fill)
    .height(SIDEBAR_ROW_HEIGHT)
    .align_x(Right)
    .center_y(SIDEBAR_ROW_HEIGHT)
    .padding(Padding::from([0.0, 3.0]))
    .into()
}

// ---------------------------------------------------------------- pane

fn pane(shell: &AppShell) -> Element<'_, Message> {
    let state = &shell.application.state;
    let searching = presenters::is_searching(state);
    let rows = shell.rows();
    let (title, count) = presenters::pane_heading(state, shell.destination, rows.len());

    let mut heading = row![
        text(presenters::ellipsize(&title, 260.0, 17.0))
            .size(17)
            .font(typography::SEMIBOLD)
            .wrapping(Wrapping::None),
        Space::new().width(Fill),
    ]
    .spacing(4)
    .align_y(Center)
    .height(26);
    // With one category open, the count says which, and that category's own actions follow it —
    // next to its name, so they cannot be mistaken for the group's.
    let open_category = shell
        .segment
        .filter(|_| !searching && matches!(shell.destination, Some(Destination::Group(_))))
        .and_then(|segment| shell.subgroup(segment))
        .map(|(_, subgroup)| (subgroup.id, subgroup.name.clone()));
    let count = match &open_category {
        Some((_, name)) => format!("{count} in {}", presenters::ellipsize(name, 120.0, 12.0)),
        None => count,
    };
    heading = heading.push(
        text(count)
            .size(12)
            .style(style::secondary_text)
            .wrapping(Wrapping::None),
    );
    if let Some((segment, _)) = open_category {
        heading = heading
            .push(Space::new().width(2))
            .push(small_action(
                &shell.icons,
                Symbol::Edit,
                Tint::Secondary,
                "Rename category",
                Message::RenameSubgroup(segment),
            ))
            .push(small_action(
                &shell.icons,
                Symbol::Delete,
                Tint::Secondary,
                "Delete category",
                Message::DeleteSubgroupRequested(segment),
            ));
    }

    let mut body = column![container(heading).padding(Padding {
        top: 0.0,
        right: SCROLLBAR_GUTTER,
        bottom: 0.0,
        left: 0.0,
    })]
    .spacing(12);

    if !searching && shell.destination == Some(Destination::Settings) {
        body = body.push(settings::pane(shell));
    } else {
        if !searching && let Some(Destination::Group(group_id)) = shell.destination {
            let segments = presenters::segments(state, group_id);
            if segments.is_empty() {
                return pane_container(body.push(empty_state(
                    &shell.icons,
                    Symbol::Category,
                    "No categories yet",
                    "A category sorts this group's servers into sets.",
                    Some(("Add category", Message::AddSubgroup)),
                )));
            }
            body = body.push(
                container(segmented_control(shell, segments)).padding(Padding {
                    top: 0.0,
                    right: SCROLLBAR_GUTTER,
                    bottom: 0.0,
                    left: 0.0,
                }),
            );
        }
        body = body.push(list(shell, &rows));
    }
    pane_container(body)
}

fn pane_container(body: Column<'_, Message>) -> Element<'_, Message> {
    container(body)
        .width(Fill)
        .height(Fill)
        .padding(Padding {
            top: 14.0,
            right: PANE_PADDING - SCROLLBAR_GUTTER,
            bottom: 0.0,
            left: PANE_PADDING,
        })
        .style(style::pane)
        .into()
}

/// The open group's categories as a segmented control: "All", each category, and "+".
///
/// Segments share the width equally, like `NSSegmentedControl`. When there are more categories than
/// fit, the ones that do not are gathered into a "More" menu at the end, so the control never wraps
/// onto a second line or squeezes a name into an ellipsis.
fn segmented_control(shell: &AppShell, segments: Vec<Segment>) -> Element<'_, Message> {
    let selected = shell.segment;
    let drag = shell.drag;
    let hovered_subgroup = shell.hovered_subgroup;
    let add_icon = shell.icons.get(Symbol::Add, Tint::Secondary);

    responsive(move |size| {
        let names: Vec<&str> = segments
            .iter()
            .map(|segment| segment.name.as_str())
            .collect();
        let fitting = segments_shown(&names, size.width)
            .saturating_sub(1)
            .min(segments.len());
        let (shown, hidden) = segments.split_at(fitting);

        let mut pills = row![segment_button(
            "All".to_owned(),
            selected.is_none(),
            Message::SelectSegment(None),
            DropState::Idle,
        )]
        .spacing(SEGMENT_GAP)
        .height(Fill);
        for segment in shown {
            let id = segment.id;
            pills = pills.push(
                mouse_area(segment_button(
                    segment.name.clone(),
                    selected == Some(id),
                    Message::SelectSegment(Some(id)),
                    drop_state(drag, hovered_subgroup, id),
                ))
                .on_enter(Message::HoverSubgroup(Some(id)))
                .on_exit(Message::LeaveSubgroup(id))
                .on_right_press(Message::ContextMenu(ContextTarget::Category(id))),
            );
        }
        if !hidden.is_empty() {
            pills = pills.push(overflow_segment(hidden, selected));
        }
        let add = button(
            container(add_icon.clone().map_or_else(
                || Element::from(Space::new()),
                |handle| {
                    image(handle)
                        .width(11)
                        .height(11)
                        .filter_method(image::FilterMethod::Linear)
                        .into()
                },
            ))
            .center(Fill),
        )
        .on_press(Message::AddSubgroup)
        .width(ADD_SEGMENT_WIDTH)
        .height(Fill)
        .padding(0)
        .style(|theme, status| style::pill(theme, status, false));
        pills = pills.push(with_tooltip(
            add.into(),
            "Add category",
            tooltip::Position::Bottom,
        ));

        container(pills).width(Fill).height(SEGMENT_HEIGHT).into()
    })
    .height(SEGMENT_HEIGHT)
    .into()
}

/// How many pills, "All" included, fit in `width` beside the `+`; the rest go into "More".
pub(crate) fn segments_shown(names: &[&str], width: f32) -> usize {
    let extents: Vec<f32> = std::iter::once("All")
        .chain(names.iter().copied())
        .map(|name| segment_width(name) + SEGMENT_GAP)
        .collect();
    presenters::fitting_segments(
        &extents,
        width - ADD_SEGMENT_WIDTH,
        MORE_SEGMENT_WIDTH + SEGMENT_GAP,
    )
}

/// The "More" segment: a pop-up holding the categories that did not fit, which reads as selected
/// while one of them is the open category.
fn overflow_segment(hidden: &[Segment], selected: Option<SubgroupId>) -> Element<'static, Message> {
    let choices: Vec<SegmentChoice> = hidden
        .iter()
        .map(|segment| SegmentChoice {
            id: segment.id,
            name: segment.name.clone(),
        })
        .collect();
    let current = choices
        .iter()
        .find(|choice| Some(choice.id) == selected)
        .cloned();
    let is_current = current.is_some();
    pick_list(choices, current, |choice| {
        Message::SelectSegment(Some(choice.id))
    })
    .placeholder("More")
    .menu_style(style::menu)
    .text_size(13)
    .padding(Padding::from([4.0, 8.0]))
    .width(MORE_SEGMENT_WIDTH)
    .style(move |theme: &Theme, _status| {
        let segment = style::pill(theme, button::Status::Active, is_current);
        iced::widget::pick_list::Style {
            text_color: segment.text_color,
            placeholder_color: segment.text_color,
            handle_color: segment.text_color,
            background: segment
                .background
                .unwrap_or(iced::Background::Color(iced::Color::TRANSPARENT)),
            border: segment.border,
        }
    })
    .into()
}

/// A category offered by the overflow menu of the segmented control.
#[derive(Clone, Debug, Eq, PartialEq)]
struct SegmentChoice {
    id: SubgroupId,
    name: String,
}

impl std::fmt::Display for SegmentChoice {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.name)
    }
}

/// A pill's width: its name, never shortened, with room either side.
pub(crate) fn segment_width(name: &str) -> f32 {
    (presenters::text_width(name, 13.0) + 26.0).max(44.0)
}

fn segment_button(
    label: String,
    selected: bool,
    message: Message,
    drop: DropState,
) -> Element<'static, Message> {
    let width = segment_width(&label);
    button(
        text(label)
            .size(13)
            .font(if selected {
                typography::MEDIUM
            } else {
                typography::UI
            })
            .wrapping(Wrapping::None)
            .width(Fill)
            .align_x(Center),
    )
    .on_press(message)
    .width(width)
    .height(Fill)
    .padding(Padding::from([4.0, 0.0]))
    .style(move |theme, status| {
        let base = style::pill(theme, status, selected);
        match drop {
            DropState::Idle => base,
            DropState::Available => style::drop_target(theme, &base, false),
            DropState::Engaged => style::drop_target(theme, &base, true),
        }
    })
    .into()
}

/// What a category segment is to a drag in progress.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DropState {
    /// Nothing is being dragged, or this category is where the dragged launcher already lives.
    Idle,
    /// A launcher could be dropped here.
    Available,
    /// A launcher would land here if the pointer were released now.
    Engaged,
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
        // A press alone offers nothing: until the row leaves its place the gesture is still a
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

/// The launcher rows, or what to do when there are none.
fn list<'a>(shell: &'a AppShell, rows: &[ListRow]) -> Element<'a, Message> {
    let state = &shell.application.state;
    if rows.is_empty() {
        return list_empty_state(shell);
    }
    let mut list = column![].spacing(0);
    for (index, row_data) in rows.iter().enumerate() {
        if index > 0 {
            list = list.push(
                container(rule::horizontal(1).style(style::separator)).padding(Padding {
                    top: 0.0,
                    right: ROW_PADDING,
                    bottom: 0.0,
                    left: ROW_PADDING + ICON_TILE + ROW_SPACING,
                }),
            );
        }
        let id = row_data.tile.id;
        let held = shell.drag.is_some_and(|drag| drag.launcher_id == id);
        // Only a row that has actually left its place puts its actions away: a press that turns out
        // to be a click would otherwise blink them off and back on again.
        let carrying = shell.drag.is_some_and(|drag| drag.lifted);
        let selected = state.selected_launcher == Some(id);
        let hovered = shell.hovered_tile == Some(id);
        list = list.push(launcher_row(
            &shell.icons,
            row_data,
            RowState {
                selected,
                hovered,
                held,
                show_actions: !carrying && (selected || hovered),
            },
        ));
    }
    scroll(container(list).padding(Padding {
        top: 0.0,
        right: SCROLLBAR_GUTTER,
        bottom: 10.0,
        left: 0.0,
    }))
    .height(Fill)
    .into()
}

fn list_empty_state(shell: &AppShell) -> Element<'_, Message> {
    let state = &shell.application.state;
    if presenters::is_searching(state) {
        return empty_state(
            &shell.icons,
            Symbol::Search,
            "No matches",
            "No server matches your search.",
            Some(("Clear search", Message::ClearSearch)),
        );
    }
    match shell.destination {
        None => empty_state(
            &shell.icons,
            Symbol::Group,
            "No server groups yet",
            "A group holds the servers you connect to, sorted into categories.",
            Some(("Add group", Message::AddGroup)),
        ),
        Some(Destination::Group(_)) => empty_state(
            &shell.icons,
            Symbol::Server,
            "No servers here yet",
            "Add a server to connect to it from the menu bar.",
            Some(("Add server", Message::AddLauncher)),
        ),
        Some(Destination::Recent) => empty_state(
            &shell.icons,
            Symbol::Recent,
            "Nothing opened yet",
            "Servers you open appear here, newest first.",
            None,
        ),
        Some(Destination::Settings) => Space::new().into(),
    }
}

/// How one row should draw itself.
#[derive(Clone, Copy, Debug, PartialEq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "four independent visual states of one row, named rather than packed"
)]
struct RowState {
    selected: bool,
    hovered: bool,
    held: bool,
    show_actions: bool,
}

/// A launcher row.
///
/// One click selects, two launch. The row is a `mouse_area` rather than a `button` because those two
/// have to be told apart from a third gesture — the press that turns into a drag — and a button
/// captures the press before anything else can look at it, publishing its own message on release.
/// That is also why a click used to be missed: a button only fires if the release lands back inside
/// its bounds, so a hand that drifted a few points between press and release launched nothing.
fn launcher_row(icons: &IconSet, row_data: &ListRow, state: RowState) -> Element<'static, Message> {
    let tile = &row_data.tile;
    let launcher_id = tile.id;
    let actions_width = 3.0f32.mul_add(ACTION_BUTTON_SIZE, 2.0 * ACTION_SPACING);
    let trailing_text = row_data
        .trailing
        .as_deref()
        .map(|trailing| presenters::ellipsize(trailing, TRAILING_MAX, DETAIL_SIZE));
    let trailing_width = if state.show_actions {
        actions_width
    } else {
        trailing_text.as_deref().map_or(0.0, |trailing| {
            presenters::text_width(trailing, DETAIL_SIZE).min(TRAILING_MAX)
        })
    };
    let labels_width =
        LIST_WIDTH - ROW_PADDING * 2.0 - ICON_TILE - ROW_SPACING * 2.0 - trailing_width;
    let trailing: Element<'static, Message> = match (state.show_actions, trailing_text) {
        (false, Some(trailing)) => text(trailing)
            .size(DETAIL_SIZE)
            .wrapping(Wrapping::None)
            .style(style::tertiary_text)
            .into(),
        _ => Space::new()
            .width(if state.show_actions {
                actions_width
            } else {
                0.0
            })
            .into(),
    };
    let content = row![
        icon_tile(icons, tile.symbol, tile.color),
        container(row_labels(&tile.name, &tile.detail, labels_width))
            .width(Fill)
            .clip(true),
        trailing,
    ]
    .spacing(ROW_SPACING)
    .align_y(Center);

    let RowState {
        selected,
        hovered,
        held,
        show_actions,
    } = state;
    let surface = mouse_area(
        container(content)
            .width(Fill)
            .height(ROW_HEIGHT)
            .padding(Padding::from([0.0, ROW_PADDING]))
            .center_y(ROW_HEIGHT)
            .style(move |theme| style::list_row(theme, selected, hovered, held)),
    )
    .interaction(iced::mouse::Interaction::Pointer)
    .on_press(Message::PressTile(launcher_id))
    .on_double_click(Message::LaunchTile(launcher_id))
    .on_right_press(Message::ContextMenu(ContextTarget::Launcher(launcher_id)));

    let mut layers: Vec<Element<'static, Message>> = vec![surface.into()];
    if show_actions {
        layers.push(row_actions(icons, launcher_id));
    }
    hoverable(
        stack(layers).into(),
        Message::HoverTile(Some(launcher_id)),
        Message::LeaveTile(launcher_id),
    )
}

/// A row's two lines: its name and what it connects to.
fn row_labels(name: &str, detail: &str, width: f32) -> Element<'static, Message> {
    column![
        text(presenters::ellipsize(name, width, NAME_SIZE))
            .size(NAME_SIZE)
            .font(typography::MEDIUM)
            .wrapping(Wrapping::None),
        text(presenters::ellipsize(detail, width, DETAIL_SIZE))
            .size(DETAIL_SIZE)
            .wrapping(Wrapping::None)
            .style(style::secondary_text),
    ]
    .spacing(2)
    .into()
}

/// The rounded square holding a row's symbol, tinted with the launcher's label color if it has one.
fn icon_tile(
    icons: &IconSet,
    icon: Symbol,
    color: Option<LabelColor>,
) -> Element<'static, Message> {
    let tint = color.map_or(Tint::Label, |color| Tint::Custom(color.rgb()));
    container(symbol(icons, icon, tint, 16.0))
        .width(ICON_TILE)
        .height(ICON_TILE)
        .center(ICON_TILE)
        .style(move |theme| style::icon_tile(theme, color.map(style::label_color)))
        .into()
}

/// The actions floating over the right edge of a hovered or selected row.
fn row_actions(icons: &IconSet, launcher_id: LauncherId) -> Element<'static, Message> {
    container(
        row![
            small_action(
                icons,
                Symbol::Edit,
                Tint::Secondary,
                "Edit  ⌘E",
                Message::EditLauncher(launcher_id)
            ),
            small_action(
                icons,
                Symbol::Duplicate,
                Tint::Secondary,
                "Duplicate",
                Message::DuplicateLauncher(launcher_id)
            ),
            small_action(
                icons,
                Symbol::Delete,
                Tint::Danger,
                "Delete",
                Message::DeleteLauncherRequested(launcher_id)
            ),
        ]
        .spacing(ACTION_SPACING),
    )
    .width(Fill)
    .height(ROW_HEIGHT)
    .align_x(Right)
    .center_y(ROW_HEIGHT)
    .padding(Padding::from([0.0, ROW_PADDING - 4.0]))
    .into()
}

// ---------------------------------------------------------------- footer

fn footer(shell: &AppShell) -> Element<'_, Message> {
    let state = &shell.application.state;
    // Settings lives at the foot of the sidebar, so the bar starts with the add action.
    let mut bar = row![].spacing(8).align_y(Center);
    if let Some((label, message)) = add_action(shell) {
        bar = bar.push(
            button(
                row![
                    symbol(&shell.icons, Symbol::Add, Tint::Label, 12.0),
                    text(label).size(13),
                ]
                .spacing(6)
                .align_y(Center),
            )
            .on_press(message)
            .padding(Padding::from([4.0, 8.0]))
            .style(|theme: &Theme, status| button::Style {
                text_color: crate::theme::label(theme),
                ..style::icon_button(theme, status)
            }),
        );
    }
    bar = bar.push(Space::new().width(Fill));

    // An event or a state takes the hint's place, so news appears where the eye already is.
    if let Some(status) = presenters::status(state, shell.banner_error()) {
        let is_error = status.is_error;
        bar = bar.push(
            row![
                symbol(
                    &shell.icons,
                    if is_error {
                        Symbol::Error
                    } else {
                        Symbol::Success
                    },
                    if is_error {
                        Tint::Danger
                    } else {
                        Tint::Secondary
                    },
                    13.0,
                ),
                text(presenters::ellipsize(&status.label, 300.0, 12.0))
                    .size(12)
                    .wrapping(Wrapping::None)
                    .style(move |theme: &Theme| style::status_text(theme, is_error)),
            ]
            .spacing(6)
            .align_y(Center),
        );
        if state.config_status == despatch_application::ConfigStatus::Conflict {
            bar = bar
                .push(footer_button("Reload", Message::ReloadConfig))
                .push(footer_button("Save Copy…", Message::SaveConfigCopy));
        }
    } else {
        let hint = footer_hint(shell);
        bar = bar.push(
            row![
                symbol(&shell.icons, Symbol::Click, Tint::Secondary, 13.0),
                text(hint).size(12).style(style::tertiary_text),
            ]
            .spacing(6)
            .align_y(Center),
        );
    }

    container(bar)
        .height(FOOTER_HEIGHT)
        .padding(Padding::from([0.0, 10.0]))
        .center_y(FOOTER_HEIGHT)
        .into()
}

/// The footer's add action, which follows what the pane is showing.
pub(crate) fn add_action(shell: &AppShell) -> Option<(&'static str, Message)> {
    match shell.destination {
        None => Some(("New group", Message::AddGroup)),
        Some(Destination::Group(group_id)) => {
            if presenters::segments(&shell.application.state, group_id).is_empty() {
                Some(("New category", Message::AddSubgroup))
            } else {
                Some(("New server", Message::AddLauncher))
            }
        }
        Some(Destination::Recent | Destination::Settings) => None,
    }
}

fn footer_hint(shell: &AppShell) -> &'static str {
    if shell.destination == Some(Destination::Settings)
        && !presenters::is_searching(&shell.application.state)
    {
        return "Changes are saved to the config file as you make them";
    }
    "Double-click or press Return to connect"
}

fn footer_button(label: &'static str, message: Message) -> Element<'static, Message> {
    button(text(label).size(12))
        .on_press(message)
        .padding(Padding::from([3.0, 9.0]))
        .style(style::secondary_button)
        .into()
}

// ---------------------------------------------------------------- shared pieces

/// A vertical scroller with a thin overlay scrollbar, the macOS kind.
pub(crate) fn scroll<'a>(
    content: impl Into<Element<'a, Message>>,
) -> iced::widget::Scrollable<'a, Message> {
    scrollable(content)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new()
                .width(6)
                .scroller_width(6)
                .margin(2),
        ))
        .style(style::scroller)
}

fn hairline() -> Element<'static, Message> {
    rule::horizontal(1).style(style::separator).into()
}

fn vertical_hairline() -> Element<'static, Message> {
    rule::vertical(1).style(style::separator).into()
}

fn empty_state(
    icons: &IconSet,
    icon: Symbol,
    title: &'static str,
    detail: &'static str,
    action: Option<(&'static str, Message)>,
) -> Element<'static, Message> {
    let mut body = column![
        symbol(icons, icon, Tint::Secondary, 28.0),
        text(title).size(15).font(typography::SEMIBOLD),
        text(detail)
            .size(12)
            .line_height(1.35)
            .align_x(Center)
            .style(style::secondary_text),
    ]
    .spacing(7)
    .max_width(320)
    .align_x(Center);

    if let Some((label, message)) = action {
        body = body.push(Space::new().height(4)).push(
            button(text(label).size(BUTTON_TEXT_SIZE).font(typography::MEDIUM))
                .on_press(message)
                .padding(BUTTON_PADDING)
                .style(style::primary_button),
        );
    }

    container(body)
        .width(Fill)
        .padding(Padding {
            top: 56.0,
            right: SCROLLBAR_GUTTER,
            bottom: 0.0,
            left: 0.0,
        })
        .center_x(Fill)
        .into()
}

/// Tracks the pointer over a whole row, actions included.
///
/// The tracker has to sit *outside* the stack: `stack` levitates the cursor for the layers under
/// whichever one reports a mouse interaction, so a `mouse_area` beneath the hover actions would be
/// told the pointer left the moment those actions appeared, and the row would flicker between
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

pub(crate) fn symbol(
    icons: &IconSet,
    icon: Symbol,
    tint: Tint,
    size: f32,
) -> Element<'static, Message> {
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

/// A small icon-only action with a tooltip naming it, as every icon-only control has.
fn small_action(
    icons: &IconSet,
    icon: Symbol,
    tint: Tint,
    label: &'static str,
    message: Message,
) -> Element<'static, Message> {
    with_tooltip(
        button(symbol(icons, icon, tint, 13.0))
            .on_press(message)
            .width(ACTION_BUTTON_SIZE)
            .height(ACTION_BUTTON_SIZE)
            .padding(5.5)
            .style(style::icon_button)
            .into(),
        label,
        tooltip::Position::Bottom,
    )
}

fn with_tooltip(
    content: Element<'static, Message>,
    label: &'static str,
    position: tooltip::Position,
) -> Element<'static, Message> {
    tooltip(
        content,
        container(text(label).size(11).line_height(1.2))
            .padding(Padding::from([4.0, 8.0]))
            .style(style::tooltip),
        position,
    )
    .gap(4)
    .into()
}

/// Where things are in the panel, in window points from its top-left corner, worked out from the
/// same constants the layout uses. The panel is a fixed size, so these are exact.
///
/// The accessibility tree gives each element its frame from here, so `VoiceOver` outlines what is
/// on screen; the live self-test aims its posted pointer events with the same points, and a point
/// that misses shows up as a failed check there.
pub mod layout {
    use super::{
        ADD_SEGMENT_WIDTH, FOOTER_HEIGHT, HEADER_HEIGHT, PANE_PADDING, ROW_HEIGHT, SEGMENT_GAP,
        SEGMENT_HEIGHT, SIDEBAR_ROW_HEIGHT, SIDEBAR_WIDTH,
    };
    use crate::app_shell::{PANEL_HEIGHT, PANEL_WIDTH};

    /// A rectangle: x, y, width, height.
    pub type Rect = [f32; 4];

    /// The panel's border, the header, and the hairline under it.
    const BODY_TOP: f32 = 1.0 + HEADER_HEIGHT + 1.0;
    /// Where the footer's hairline starts; nothing in the body is drawn below it.
    pub const BODY_BOTTOM: f32 = PANEL_HEIGHT - 1.0 - FOOTER_HEIGHT - 1.0;
    const SIDEBAR_TOP: f32 = BODY_TOP + 10.0;
    const SIDEBAR_LEFT: f32 = 1.0 + 8.0;
    const SIDEBAR_INNER: f32 = SIDEBAR_WIDTH - 16.0;
    /// A sidebar heading: 22 points and 2 of padding, then the column's 1-point spacing.
    const SIDEBAR_HEADING: f32 = 24.0 + 1.0;
    const SIDEBAR_ROW: f32 = SIDEBAR_ROW_HEIGHT + 1.0;
    const PANE_LEFT: f32 = 1.0 + SIDEBAR_WIDTH + 1.0 + PANE_PADDING;
    const CONTROL_WIDTH: f32 = PANEL_WIDTH - 2.0 - SIDEBAR_WIDTH - 1.0 - PANE_PADDING * 2.0;
    const HEADING_TOP: f32 = BODY_TOP + 14.0;
    const SEGMENTS_TOP: f32 = HEADING_TOP + 26.0 + 12.0;
    const LIST_TOP: f32 = SEGMENTS_TOP + SEGMENT_HEIGHT + 12.0;
    const HEADER_FIELD_TOP: f32 = 1.0 + (HEADER_HEIGHT - 28.0) / 2.0;
    const SEARCH_LEFT: f32 = 1.0 + 14.0 + (SIDEBAR_WIDTH - 14.0) + 12.0;
    const SEARCH_WIDTH: f32 = PANEL_WIDTH - 2.0 - 28.0 - (SIDEBAR_WIDTH - 14.0) - 24.0 - 28.0;

    #[must_use]
    pub const fn search_field() -> Rect {
        [SEARCH_LEFT, HEADER_FIELD_TOP, SEARCH_WIDTH, 28.0]
    }

    #[must_use]
    pub const fn pin() -> Rect {
        [
            SEARCH_LEFT + SEARCH_WIDTH + 12.0,
            HEADER_FIELD_TOP,
            28.0,
            28.0,
        ]
    }

    #[must_use]
    pub const fn groups_heading() -> Rect {
        [SIDEBAR_LEFT, SIDEBAR_TOP, SIDEBAR_INNER, 24.0]
    }

    /// The `+` beside the Groups heading.
    #[must_use]
    pub const fn add_group() -> Rect {
        [
            SIDEBAR_LEFT + SIDEBAR_INNER - 2.0 - 20.0,
            SIDEBAR_TOP + 1.0,
            20.0,
            20.0,
        ]
    }

    #[must_use]
    pub fn group_row(index: usize) -> Rect {
        [
            SIDEBAR_LEFT,
            SIDEBAR_TOP + SIDEBAR_HEADING + SIDEBAR_ROW * index_f32(index),
            SIDEBAR_INNER,
            SIDEBAR_ROW_HEIGHT,
        ]
    }

    /// The `index`-th row at the foot of the sidebar (Recently used, then Settings).
    #[must_use]
    pub fn footer_row(index: usize) -> Rect {
        const ROWS: usize = 2;
        let below = index_f32(ROWS - index);
        [
            SIDEBAR_LEFT,
            BODY_BOTTOM - 8.0 - SIDEBAR_ROW_HEIGHT * below - 1.0 * (below - 1.0),
            SIDEBAR_INNER,
            SIDEBAR_ROW_HEIGHT,
        ]
    }

    #[must_use]
    pub const fn pane_heading() -> Rect {
        [PANE_LEFT, HEADING_TOP, CONTROL_WIDTH, 26.0]
    }

    /// Where the pill after the first `count` pills ("All" first, then `names`) starts.
    fn pill_left(names: &[&str], count: usize) -> f32 {
        std::iter::once("All")
            .chain(names.iter().copied())
            .take(count)
            .map(|name| super::segment_width(name) + SEGMENT_GAP)
            .fold(PANE_LEFT, |left, extent| left + extent)
    }

    /// The `index`-th pill ("All" is 0) of a group whose categories are `names`, when they all fit.
    #[must_use]
    pub fn segment(names: &[&str], index: usize) -> Rect {
        let name = if index == 0 { "All" } else { names[index - 1] };
        [
            pill_left(names, index),
            SEGMENTS_TOP,
            super::segment_width(name),
            SEGMENT_HEIGHT,
        ]
    }

    /// The `+` pill after the categories `names`.
    #[must_use]
    pub fn add_segment(names: &[&str]) -> Rect {
        [
            pill_left(names, names.len() + 1),
            SEGMENTS_TOP,
            ADD_SEGMENT_WIDTH,
            SEGMENT_HEIGHT,
        ]
    }

    /// The `index`-th launcher row; `segmented` when a segmented control sits above the list.
    /// `None` once the row is below the visible part of the list.
    #[must_use]
    pub fn row(index: usize, segmented: bool) -> Option<Rect> {
        let top = if segmented {
            LIST_TOP
        } else {
            LIST_TOP - SEGMENT_HEIGHT - 12.0
        };
        let y = top + (ROW_HEIGHT + 1.0) * index_f32(index);
        (y + ROW_HEIGHT <= BODY_BOTTOM).then_some([PANE_LEFT, y, CONTROL_WIDTH, ROW_HEIGHT])
    }

    /// The footer's add button, which is as wide as its label.
    #[must_use]
    pub fn footer_add(label: &str) -> Rect {
        [
            11.0,
            BODY_BOTTOM + 1.0 + (FOOTER_HEIGHT - 22.0) / 2.0,
            12.0 + 6.0 + crate::presenters::text_width(label, 13.0) + 16.0,
            22.0,
        ]
    }

    /// The center of a rectangle, where a pointer aimed at it lands.
    #[cfg(feature = "capture")]
    #[must_use]
    pub fn center(rect: Rect) -> (f32, f32) {
        (rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0)
    }

    #[expect(clippy::cast_precision_loss, reason = "indexes into a panel are tiny")]
    const fn index_f32(index: usize) -> f32 {
        index as f32
    }
}

#[cfg(test)]
mod tests {
    use super::{DropState, LIST_WIDTH, drop_state, segment_width};

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
    fn a_segment_is_never_narrower_than_its_name() {
        assert!(segment_width("Production bastions") > segment_width("Web"));
        assert!(
            segment_width("") >= 44.0,
            "a pill is never narrower than a capsule"
        );
    }

    #[test]
    fn the_list_leaves_real_room_for_labels() {
        const { assert!(LIST_WIDTH > 400.0) }
    }
}
