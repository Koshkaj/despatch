use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use despatch_application::{AppearanceChoice, Application, Dialog, Reachability};
use despatch_domain::{
    Group, GroupId, LabelColor, Launcher, LauncherId, Section, Subgroup, SubgroupId,
};
use iced::keyboard;
use iced::widget::text_editor;
use iced::{Subscription, Task, Theme};

use crate::editor::{
    CategoryChoice, Field, LauncherDraft, NameDraft, NameTarget, ReachabilityState, Target,
};
use crate::icons::{IconSet, Tint};
use crate::presenters::{self, Destination, ListRow};
use crate::shortcuts::{self, Shortcut};
use crate::status_item::{self, MenuBar, PanelEvent};
use crate::theme::Appearance;
use crate::{accessibility, accessibility::Node};

pub const SEARCH_ID: &str = "launcher-search";
pub const GROUP_NAME_ID: &str = "group-name";
pub const LAUNCHER_NAME_ID: &str = "launcher-name";

/// The size of the panel that hangs from the menu bar.
pub const PANEL_WIDTH: f32 = 720.0;
pub const PANEL_HEIGHT: f32 = 520.0;

/// A launcher held by the pointer, on its way to another category.
///
/// The source is captured when the drag starts rather than looked up when it ends: the drop moves
/// the launcher out of a category, and reading that category from the config afterwards would be
/// reading it after the move already changed it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    pub launcher_id: LauncherId,
    pub source: SubgroupId,
    /// Whether the pointer has moved far enough for this to be a drag. Until it has, the press is
    /// still an ordinary click and nothing follows the cursor.
    ///
    /// Where the pointer *is* belongs to [`crate::drag_layer`], which follows it without a message:
    /// this only records that the crossing happened, which it is told once.
    pub lifted: bool,
}

impl Drag {
    pub(crate) const fn new(launcher_id: LauncherId, source: SubgroupId) -> Self {
        Self {
            launcher_id,
            source,
            lifted: false,
        }
    }
}

/// A server group held by the pointer, on its way to another place in the sidebar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupDrag {
    pub group_id: GroupId,
    /// Whether the pointer has moved far enough for this to be a drag rather than a click.
    pub lifted: bool,
}

/// What a right click landed on, which decides what its menu offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextTarget {
    Launcher(LauncherId),
    Group(GroupId),
    Category(SubgroupId),
}

/// One line of a context menu: a title and the message choosing it sends, or a separator.
#[derive(Clone, Debug)]
pub enum ContextEntry {
    Action(&'static str, Message),
    Separator,
}

/// Whether Despatch opens at login, as last read from the system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenAtLogin {
    Off,
    On,
    /// Registered, waiting for the user to allow it in System Settings.
    NeedsApproval,
    /// Not offered: the app is not running from its bundle, so there is nothing to register.
    Unavailable,
}

pub struct AppShell {
    pub application: Application,
    pub startup_error: Option<String>,
    pub action_error: Option<String>,
    pub icons: IconSet,
    pub name_draft: Option<NameDraft>,
    pub launcher_draft: Option<LauncherDraft>,
    pub hovered_tile: Option<LauncherId>,
    pub hovered_group: Option<GroupId>,
    /// The category segment under the pointer, which is the drop target while a launcher is being
    /// dragged.
    pub hovered_subgroup: Option<SubgroupId>,
    /// The launcher currently held by the pointer, if the press landed on a row.
    pub drag: Option<Drag>,
    /// The server group currently held by the pointer, if the press landed on its sidebar row.
    pub group_drag: Option<GroupDrag>,
    pub appearance: AppearanceChoice,
    /// What the sidebar points at. `None` only while there is nothing to point at: no server
    /// groups and no library destination chosen.
    pub destination: Option<Destination>,
    /// The category the open group's segmented control is on; `None` is "All".
    pub segment: Option<SubgroupId>,
    /// The segment each group was last left on, so returning to a group returns to it.
    remembered_segments: HashMap<GroupId, SubgroupId>,
    /// Whether the panel stays open when the user clicks elsewhere or launches something.
    pub pinned: bool,
    /// Whether the panel is on screen.
    pub panel_visible: bool,
    /// Whether the user asked the system to reduce motion; the panel then appears without fading.
    pub reduce_motion: bool,
    /// Whether Despatch opens at login, once the system has been asked.
    pub open_at_login: Option<OpenAtLogin>,
    /// The terminals Settings offers, the system's default first. Read when Settings opens rather
    /// than on every frame, because finding installed applications asks Launch Services.
    pub terminals: Vec<TerminalPick>,
    menu_bar: MenuBar,
    /// Identifies the status line currently on screen, so a dismissal that arrives after the line
    /// was replaced is recognised as stale and ignored.
    status_token: u64,
    /// What the status line is showing, used only to notice when it becomes something else.
    status_fingerprint: Option<String>,
    theme: Theme,
    accessibility_nodes: Vec<Node>,
    /// The tree last sent to the window, so an unchanged one is not sent again.
    installed_accessibility: Option<Vec<Node>>,
    window_id: Option<iced::window::Id>,
}

impl AppShell {
    #[must_use]
    pub fn new(application: Application, startup_error: Option<String>) -> Self {
        let choice = application.appearance();
        #[cfg_attr(
            not(feature = "capture"),
            expect(
                unused_mut,
                reason = "only capture runs override the resolved appearance"
            )
        )]
        let mut appearance = Appearance::resolve(choice);
        #[cfg(feature = "capture")]
        if let Some(dark) = crate::capture::appearance_override() {
            appearance.dark = dark;
        }
        #[cfg(feature = "capture")]
        if crate::capture::increased_contrast_override() {
            appearance.increase_contrast = true;
        }
        #[cfg(feature = "capture")]
        if crate::capture::reduce_motion_override() {
            appearance.reduce_motion = true;
        }
        let theme = appearance.theme();
        let (destination, segment) = initial_destination(&application);
        let mut shell = Self {
            application,
            startup_error,
            action_error: None,
            icons: IconSet::load(&theme),
            name_draft: None,
            launcher_draft: None,
            hovered_tile: None,
            hovered_group: None,
            hovered_subgroup: None,
            drag: None,
            group_drag: None,
            appearance: choice,
            destination,
            segment,
            remembered_segments: HashMap::new(),
            pinned: false,
            panel_visible: false,
            reduce_motion: appearance.reduce_motion,
            open_at_login: None,
            terminals: Vec::new(),
            menu_bar: MenuBar::default(),
            status_token: 0,
            status_fingerprint: None,
            theme,
            accessibility_nodes: Vec::new(),
            installed_accessibility: None,
            window_id: None,
        };
        refresh_terminals(&mut shell);
        shell.accessibility_nodes = accessibility::model(&shell);
        prepare_label_icons(&mut shell);
        // A capture run opens its scene before the first frame is built, so the screenshot shows
        // the requested screen rather than the launch state.
        #[cfg(feature = "capture")]
        for message in capture_scene(&shell) {
            let _ = apply(&mut shell, message);
        }
        shell
    }

    #[must_use]
    pub fn theme(&self) -> Theme {
        self.theme.clone()
    }

    /// The error the status line shows, if anything needs the user's attention.
    #[must_use]
    pub fn banner_error(&self) -> Option<&str> {
        self.action_error
            .as_deref()
            .or(self.startup_error.as_deref())
    }

    /// The launchers the list pane shows right now, in the order it shows them.
    #[must_use]
    pub fn rows(&self) -> Vec<ListRow> {
        presenters::list_rows(
            &self.application.state,
            self.destination,
            self.segment,
            &self.application.recent_launches(),
            SystemTime::now(),
        )
    }

    /// The group the sidebar points at, if it points at one.
    #[must_use]
    pub fn open_group(&self) -> Option<&Group> {
        match self.destination {
            Some(Destination::Group(group_id)) => self.group(group_id),
            _ => None,
        }
    }

    #[must_use]
    pub fn launcher(&self, launcher_id: LauncherId) -> Option<(&Subgroup, &Launcher)> {
        self.application
            .state
            .config
            .locate_launcher(launcher_id)
            .and_then(|(_, subgroup)| {
                subgroup
                    .items
                    .iter()
                    .find(|item| item.id() == launcher_id)
                    .map(|launcher| (subgroup, launcher))
            })
    }

    #[must_use]
    pub fn subgroup(&self, subgroup_id: SubgroupId) -> Option<(&Group, &Subgroup)> {
        self.application.state.config.locate_subgroup(subgroup_id)
    }

    #[must_use]
    pub fn group(&self, group_id: GroupId) -> Option<&Group> {
        self.application
            .state
            .config
            .groups
            .iter()
            .find(|group| group.id == group_id)
    }
}

/// Where the panel opens: wherever the application's selection already points, else the first
/// server group, else nothing.
fn initial_destination(application: &Application) -> (Option<Destination>, Option<SubgroupId>) {
    let state = &application.state;
    if state.selected_section == Section::Servers
        && let Some(group_id) = state.selected_group
    {
        return (Some(Destination::Group(group_id)), state.selected_subgroup);
    }
    (first_server_group(application), None)
}

fn first_server_group(application: &Application) -> Option<Destination> {
    application
        .state
        .config
        .groups
        .iter()
        .find(|group| group.section == Section::Servers)
        .map(|group| Destination::Group(group.id))
}

#[derive(Clone, Debug)]
pub enum Message {
    SelectDestination(Destination),
    /// Picks a segment of the open group: a category, or `None` for "All".
    SelectSegment(Option<SubgroupId>),
    SelectTile(LauncherId),
    /// A row was pressed: it becomes the selection, and the press is the start of a possible drag.
    PressTile(LauncherId),
    /// The held row has travelled far enough to become a drag. Sent once per gesture.
    DragLifted,
    HoverTile(Option<LauncherId>),
    HoverGroup(Option<GroupId>),
    HoverSubgroup(Option<SubgroupId>),
    /// The pointer left this row. It clears the hover only if this row still holds it: moving
    /// up or left, the row entered is told before the row left, and an unconditional clear would
    /// wipe out the hover that was just set.
    LeaveTile(LauncherId),
    /// The pointer left this group's sidebar row. See [`Message::LeaveTile`].
    LeaveGroup(GroupId),
    /// The pointer left this category's segment. See [`Message::LeaveTile`].
    LeaveSubgroup(SubgroupId),
    PointerReleased,
    PointerCancelled,
    /// A server group's sidebar row was pressed: it opens, and the press may become a drag.
    PressGroup(GroupId),
    /// The held group has travelled far enough to become a drag. Sent once per gesture.
    GroupDragLifted,
    /// A right click: show the native menu for what it landed on.
    ContextMenu(ContextTarget),
    LaunchTile(LauncherId),
    SearchChanged(String),
    /// Return in the search field: launch the selected result, or the first one.
    SearchSubmitted,
    ClearSearch,

    AddGroup,
    RenameGroup(GroupId),
    DeleteGroupRequested(GroupId),
    AddSubgroup,
    RenameSubgroup(SubgroupId),
    DeleteSubgroupRequested(SubgroupId),
    NameChanged(String),
    NameColorChanged(String),
    NameColorSelected(LabelColor),
    NameColorCleared,
    SaveName,

    AddLauncher,
    EditLauncher(LauncherId),
    DuplicateLauncher(LauncherId),
    DeleteLauncherRequested(LauncherId),
    DraftFieldChanged(Field, String),
    DraftArgsEdited(text_editor::Action),
    DraftCategorySelected(SubgroupId),
    DraftColorChanged(String),
    DraftColorSelected(LabelColor),
    DraftColorCleared,
    DraftAdvancedToggled,
    DraftIdentityFileRequested,
    TestReachability,
    ReachabilityChecked {
        destination: String,
        outcome: Result<Reachability, String>,
    },
    SaveLauncher,

    AppearanceSelected(AppearanceChoice),
    DismissStatus(u64),

    ConfirmDelete,
    RevealConfig,
    ReloadConfig,
    OpenConfig,
    NewConfig,
    SaveConfigCopy,
    ClearRecents,
    OpenAtLoginToggled(bool),
    TerminalPicked(TerminalPick),
    DismissDialog,
    /// Escape: closes whatever is on top — a sheet, then a search, then the panel itself.
    Escape,

    /// Something happened to the menu-bar item or around it.
    Panel(PanelEvent),
    TogglePin,
    HidePanel,
    Quit,
    /// A call to the panel window finished.
    PanelCommand(Result<(), String>),
    /// The panel has drawn since it was shown; its shadow can follow its shape now.
    RefreshShadow,

    Shortcut(Shortcut),
    WindowOpened(iced::window::Id),
    AccessibilityInstalled(Result<(), String>),
    #[cfg(feature = "capture")]
    CaptureReady(iced::window::Screenshot),
    /// One step of a live self-test. Development builds only.
    #[cfg(feature = "capture")]
    LiveStep(usize),
    /// The window's opacity a moment after the panel was shown, during a live self-test.
    #[cfg(feature = "capture")]
    LiveMotion(Result<f64, String>),
    /// The accessibility elements and the window frame, read back during a live self-test.
    #[cfg(feature = "capture")]
    LiveAccessibilityFrames(
        Vec<despatch_platform_macos::AccessibilityReport>,
        (f64, f64, f64, f64),
    ),
    /// The line chosen from a native context menu during a live self-test.
    #[cfg(feature = "capture")]
    LiveMenuChosen(SubgroupId, Option<usize>),
    /// The accessibility labels the panel's view published, read back during a live self-test.
    #[cfg(feature = "capture")]
    LiveAccessibility(Vec<String>),
    /// What the window server said after a live step, against what the step expected.
    #[cfg(feature = "capture")]
    LiveReport {
        label: &'static str,
        expected_visible: bool,
        report: Result<despatch_platform_macos::PanelReport, String>,
    },
}

/// How long a launch result or an action error stays on screen before it clears itself.
const STATUS_LIFETIME: Duration = Duration::from_secs(10);

/// How long a new menu-bar item takes to be given its place in the menu bar.
const MENU_BAR_SETTLE: Duration = Duration::from_millis(300);

/// How long after showing the panel its shadow is recomputed: long enough for the GPU to have
/// presented the first frame, short enough that nobody sees the panel without one.
const SHADOW_SETTLE: Duration = Duration::from_millis(90);

pub fn update(shell: &mut AppShell, message: Message) -> Task<Message> {
    let task = apply(shell, message);
    repair_destination(shell);
    shell.accessibility_nodes = accessibility::model(shell);
    prepare_label_icons(shell);
    let dismissal = schedule_status_dismissal(shell);
    Task::batch([task, refresh_accessibility(shell), dismissal])
}

/// Sends the accessibility tree to the window when it differs from the one last installed there.
///
/// Installing reports back with a message of its own, and every message runs `update`. Sending the
/// tree unconditionally therefore installed it again in answer to its own report, hundreds of times
/// a second, and kept the app busy while nothing on screen changed.
fn refresh_accessibility(shell: &mut AppShell) -> Task<Message> {
    let Some(window_id) = shell.window_id else {
        return Task::none();
    };
    if shell.installed_accessibility.as_ref() == Some(&shell.accessibility_nodes) {
        return Task::none();
    }
    let nodes = shell.accessibility_nodes.clone();
    shell.installed_accessibility = Some(nodes.clone());
    #[cfg(target_os = "macos")]
    {
        let sender = shell.menu_bar.sender();
        let on_press: despatch_platform_macos::PressHandler = std::sync::Arc::new(move |index| {
            // A closed channel means the interface is shutting down; there is no one to tell.
            let _ = sender.unbounded_send(PanelEvent::AccessibilityPress(index));
        });
        iced::window::run(window_id, move |window| {
            crate::accessibility_bridge::install(window, &nodes, &on_press)
        })
        .map(Message::AccessibilityInstalled)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window_id, nodes);
        Task::none()
    }
}

/// Keeps the sidebar pointing at something that exists after an edit, a delete, or a reload.
///
/// A group that disappeared hands over to the first server group left; a segment whose category
/// disappeared falls back to "All". Nothing is chosen inside the list on the user's behalf.
fn repair_destination(shell: &mut AppShell) {
    let state = &shell.application.state;
    let group_is_valid = |group_id: GroupId| {
        state
            .config
            .groups
            .iter()
            .any(|group| group.id == group_id && group.section == Section::Servers)
    };
    match shell.destination {
        Some(Destination::Group(group_id)) if !group_is_valid(group_id) => {
            shell.destination = first_server_group(&shell.application);
            shell.segment = None;
        }
        None => shell.destination = first_server_group(&shell.application),
        _ => {}
    }
    let open_group = match shell.destination {
        Some(Destination::Group(group_id)) => Some(group_id),
        _ => None,
    };
    let segment_is_valid = shell.segment.is_some_and(|segment| {
        shell
            .application
            .state
            .config
            .locate_subgroup(segment)
            .is_some_and(|(group, _)| Some(group.id) == open_group)
    });
    if !segment_is_valid {
        shell.segment = None;
    }
    let config = &shell.application.state.config;
    shell
        .remembered_segments
        .retain(|_, segment| config.locate_subgroup(*segment).is_some());
}

/// Starts the clock on a status line the moment it appears, and only then.
///
/// Only *events* expire — a launch result or a failed action, which describe something that
/// happened. A conflicted or unsaved config is a state rather than an event, and a status line
/// that hides itself while the condition is still true would be lying.
fn schedule_status_dismissal(shell: &mut AppShell) -> Task<Message> {
    let fingerprint = status_fingerprint(shell);
    if fingerprint == shell.status_fingerprint {
        return Task::none();
    }
    shell.status_fingerprint.clone_from(&fingerprint);
    let Some(_) = fingerprint else {
        return Task::none();
    };
    shell.status_token = shell.status_token.wrapping_add(1);
    after(STATUS_LIFETIME, Message::DismissStatus(shell.status_token))
}

/// What the status line is saying, or `None` when it is saying nothing that expires.
fn status_fingerprint(shell: &AppShell) -> Option<String> {
    let state = &shell.application.state;
    if let Some(error) = &shell.action_error {
        return Some(format!("action:{error}"));
    }
    if let Some(error) = &state.last_error {
        return Some(format!("error:{}", error.message()));
    }
    match &state.last_launch {
        Some(despatch_application::LaunchOutcome::Success { launcher_id }) => {
            Some(format!("launched:{launcher_id}"))
        }
        Some(despatch_application::LaunchOutcome::Failure { launcher_id, error }) => {
            Some(format!("failed:{launcher_id}:{}", error.message()))
        }
        None => None,
    }
}

/// Sends `message` once `delay` has passed.
///
/// The wait happens on a thread of its own rather than on the task executor: this build runs Iced
/// on a thread pool with no timer, and parking one of its threads would take it away from the work
/// it exists for.
fn after(delay: Duration, message: Message) -> Task<Message> {
    let (sender, receiver) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        // A closed receiver means the window is gone; nothing to tell.
        let _ = sender.send(());
    });
    Task::future(async move {
        let _ = receiver.await;
        message
    })
}

/// Renders every symbol the config draws in a label color, in that color.
///
/// The tinted masks are cached, so a color is rendered the first time it appears and then reused
/// instead of being uploaded again on every frame.
fn prepare_label_icons(shell: &mut AppShell) {
    for (symbol, color) in presenters::label_symbols(&shell.application.state) {
        shell.icons.prepare(symbol, Tint::Custom(color.rgb()));
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "one arm per user action reads best flat"
)]
fn apply(shell: &mut AppShell, message: Message) -> Task<Message> {
    match message {
        Message::SelectDestination(destination) => select_destination(shell, destination),
        Message::SelectSegment(segment) => select_segment(shell, segment),
        Message::SelectTile(launcher_id) => {
            let _ = shell.application.state.select_launcher(launcher_id);
        }
        Message::PressTile(launcher_id) => begin_drag(shell, launcher_id),
        Message::DragLifted => {
            if let Some(drag) = &mut shell.drag {
                drag.lifted = true;
            }
        }
        Message::HoverTile(launcher_id) => shell.hovered_tile = launcher_id,
        Message::HoverGroup(group_id) => shell.hovered_group = group_id,
        Message::HoverSubgroup(subgroup_id) => shell.hovered_subgroup = subgroup_id,
        Message::LeaveTile(launcher_id) => {
            if shell.hovered_tile == Some(launcher_id) {
                shell.hovered_tile = None;
            }
        }
        Message::LeaveGroup(group_id) => {
            if shell.hovered_group == Some(group_id) {
                shell.hovered_group = None;
            }
        }
        Message::LeaveSubgroup(subgroup_id) => {
            if shell.hovered_subgroup == Some(subgroup_id) {
                shell.hovered_subgroup = None;
            }
        }
        Message::PointerReleased => {
            finish_group_drag(shell);
            return finish_drag(shell);
        }
        Message::PointerCancelled => {
            // The pointer left the window, so its release will never arrive here. Holding the drag
            // open would leave a row stuck to a cursor that is somewhere else entirely.
            shell.drag = None;
            shell.group_drag = None;
            shell.hovered_subgroup = None;
        }
        Message::PressGroup(group_id) => {
            if shell.application.state.dialog.is_none() {
                select_destination(shell, Destination::Group(group_id));
                shell.group_drag = Some(GroupDrag {
                    group_id,
                    lifted: false,
                });
            }
        }
        Message::GroupDragLifted => {
            if let Some(drag) = &mut shell.group_drag {
                drag.lifted = true;
            }
        }
        Message::ContextMenu(target) => return show_context_menu(shell, target),
        Message::LaunchTile(launcher_id) => return launch(shell, launcher_id),
        Message::SearchChanged(query) => {
            shell.application.state.set_search_query(query);
            shell.hovered_tile = None;
        }
        Message::SearchSubmitted => {
            let rows = shell.rows();
            let selected = shell.application.state.selected_launcher;
            let target = rows
                .iter()
                .find(|row| Some(row.tile.id) == selected)
                .or_else(|| rows.first())
                .map(|row| row.tile.id);
            if let Some(launcher_id) = target {
                return launch(shell, launcher_id);
            }
        }
        Message::ClearSearch => shell.application.state.set_search_query(String::new()),

        Message::AddGroup => {
            return open_name_editor(
                shell,
                NameDraft::new(NameTarget::NewGroup),
                Dialog::AddGroup {
                    section: Section::Servers,
                },
            );
        }
        Message::RenameGroup(group_id) => {
            let Some((name, color)) = shell
                .group(group_id)
                .map(|group| (group.name.clone(), group.color))
            else {
                return Task::none();
            };
            return open_name_editor(
                shell,
                NameDraft::rename(NameTarget::RenameGroup(group_id), name, color),
                Dialog::EditGroup { group_id },
            );
        }
        Message::DeleteGroupRequested(group_id) => {
            shell.application.state.dialog = Some(Dialog::DeleteGroup { group_id });
        }
        Message::AddSubgroup => {
            let Some(group_id) = shell.open_group().map(|group| group.id) else {
                return Task::none();
            };
            return open_name_editor(
                shell,
                NameDraft::new(NameTarget::NewSubgroup(group_id)),
                Dialog::AddSubgroup { group_id },
            );
        }
        Message::RenameSubgroup(subgroup_id) => {
            let Some(name) = shell
                .subgroup(subgroup_id)
                .map(|(_, subgroup)| subgroup.name.clone())
            else {
                return Task::none();
            };
            return open_name_editor(
                shell,
                NameDraft::rename(NameTarget::RenameSubgroup(subgroup_id), name, None),
                Dialog::EditSubgroup { subgroup_id },
            );
        }
        Message::DeleteSubgroupRequested(subgroup_id) => {
            shell.application.state.dialog = Some(Dialog::DeleteSubgroup { subgroup_id });
        }
        Message::NameChanged(name) => {
            if let Some(draft) = &mut shell.name_draft {
                draft.set_name(name);
            }
        }
        Message::NameColorChanged(color) => {
            if let Some(draft) = &mut shell.name_draft {
                draft.set_color(color);
            }
        }
        Message::NameColorSelected(color) => {
            if let Some(draft) = &mut shell.name_draft {
                draft.toggle_color(color);
            }
        }
        Message::NameColorCleared => {
            if let Some(draft) = &mut shell.name_draft {
                draft.set_color(String::new());
            }
        }
        Message::SaveName => save_name(shell),

        Message::AddLauncher => return add_launcher(shell),
        Message::EditLauncher(launcher_id) => return edit_launcher(shell, launcher_id),
        Message::DuplicateLauncher(launcher_id) => duplicate_launcher(shell, launcher_id),
        Message::DeleteLauncherRequested(launcher_id) => {
            let _ = shell.application.state.select_launcher(launcher_id);
            shell.application.state.dialog = Some(Dialog::DeleteLauncher { launcher_id });
        }
        Message::DraftFieldChanged(field, value) => {
            if let Some(draft) = &mut shell.launcher_draft {
                draft.set(field, value);
            }
        }
        Message::DraftArgsEdited(action) => {
            if let Some(draft) = &mut shell.launcher_draft {
                draft.edit_args(action);
            }
        }
        Message::DraftCategorySelected(subgroup_id) => {
            if let Some(draft) = &mut shell.launcher_draft {
                draft.select_category(subgroup_id);
            }
        }
        Message::DraftColorChanged(color) => {
            if let Some(draft) = &mut shell.launcher_draft {
                draft.set_color(color);
            }
        }
        Message::DraftColorSelected(color) => {
            if let Some(draft) = &mut shell.launcher_draft {
                draft.toggle_color(color);
            }
        }
        Message::DraftColorCleared => {
            if let Some(draft) = &mut shell.launcher_draft {
                draft.set_color(String::new());
            }
        }
        Message::DraftAdvancedToggled => {
            if let Some(draft) = &mut shell.launcher_draft {
                draft.toggle_advanced();
            }
        }
        Message::DraftIdentityFileRequested => choose_identity_file(shell),
        Message::TestReachability => return test_reachability(shell),
        Message::ReachabilityChecked {
            destination,
            outcome,
        } => {
            if let Some(draft) = &mut shell.launcher_draft {
                draft.reachability = match outcome {
                    Ok(outcome) => ReachabilityState::Checked {
                        destination,
                        outcome,
                    },
                    Err(message) => ReachabilityState::Failed {
                        destination,
                        message,
                    },
                };
            }
        }
        Message::SaveLauncher => save_launcher(shell),

        Message::AppearanceSelected(choice) => select_appearance(shell, choice),
        Message::DismissStatus(token) => {
            // A status line that has already been replaced keeps its successor on screen.
            if token == shell.status_token {
                shell.action_error = None;
                shell.application.state.last_error = None;
                shell.application.state.last_launch = None;
            }
        }
        Message::ConfirmDelete => confirm_delete(shell),
        Message::RevealConfig => {
            let outcome = shell.application.reveal_config();
            report(shell, outcome);
        }
        Message::ReloadConfig => {
            let outcome = shell.application.reload();
            report(shell, outcome);
        }
        Message::OpenConfig => open_config(shell),
        Message::NewConfig => new_config(shell),
        Message::SaveConfigCopy => save_config_copy(shell),
        Message::ClearRecents => {
            let outcome = shell.application.clear_recents();
            report(shell, outcome);
        }
        Message::OpenAtLoginToggled(enabled) => set_open_at_login(shell, enabled),
        Message::TerminalPicked(pick) => {
            let outcome = shell.application.choose_terminal(pick.id.as_deref());
            report(shell, outcome);
        }
        Message::DismissDialog => dismiss_dialog(shell),
        Message::Escape => return escape(shell),

        Message::Panel(event) => return panel_event(shell, event),
        Message::TogglePin => shell.pinned = !shell.pinned,
        Message::HidePanel => return hide_panel(shell, true),
        Message::Quit => return iced::exit(),
        Message::PanelCommand(result) => {
            if let Err(error) = result {
                shell.action_error = Some(format!("The panel could not be shown: {error}"));
            }
        }
        Message::RefreshShadow => {
            if let Some(window_id) = shell.window_id {
                return status_item::refresh_shadow(window_id).map(Message::PanelCommand);
            }
        }

        Message::Shortcut(shortcut) => return apply_shortcut(shell, shortcut),
        Message::WindowOpened(window_id) => return window_opened(shell, window_id),
        Message::AccessibilityInstalled(result) => {
            if let Err(error) = result {
                // Retried when the tree next changes, not at once: an immediate retry of a failure
                // that persists would answer its own report forever.
                shell.startup_error = Some(format!("Accessibility bridge: {error}"));
            }
        }
        #[cfg(feature = "capture")]
        Message::CaptureReady(screenshot) => return crate::capture::finish(&screenshot),
        #[cfg(feature = "capture")]
        Message::LiveStep(step) => return live_step(shell, step),
        #[cfg(feature = "capture")]
        Message::LiveMotion(alpha) => match alpha {
            Ok(alpha) if shell.reduce_motion => check(
                "with Reduce Motion the panel appears at once, without fading",
                (alpha - 1.0).abs() < f64::EPSILON,
                &format!("opacity {alpha:.2} 40 ms after showing"),
            ),
            Ok(alpha) => check(
                "the panel fades in when motion is not reduced",
                alpha < 1.0,
                &format!("opacity {alpha:.2} 40 ms after showing"),
            ),
            Err(error) => check("the panel's opacity can be read", false, &error),
        },
        #[cfg(feature = "capture")]
        Message::LiveAccessibilityFrames(elements, (wx, wy, ww, wh)) => {
            let pressable: Vec<_> = elements
                .iter()
                .filter(|element| element.pressable)
                .collect();
            let outside: Vec<&str> = pressable
                .iter()
                .filter(|element| {
                    let (x, y, w, h) = element.frame_on_screen;
                    w <= 0.0
                        || h <= 0.0
                        || x < wx
                        || y < wy
                        || x + w > wx + ww + 0.5
                        || y + h > wy + wh + 0.5
                })
                .map(|element| element.label.as_str())
                .collect();
            check(
                "every pressable element has a frame on screen, inside the panel",
                pressable.len() >= 10 && outside.is_empty(),
                &format!("{} pressable, outside: {outside:?}", pressable.len()),
            );
            let groups = presenters::sidebar_groups(&shell.application.state).len();
            let expected = crate::views::layout::footer_row(1).map(f64::from);
            let settings = elements
                .iter()
                .find(|element| element.label.starts_with("Settings"))
                .map(|element| element.frame_in_view);
            check(
                "the Settings element is framed exactly where its row is drawn",
                settings == Some((expected[0], expected[1], expected[2], expected[3])),
                &format!("frame {settings:?}, row {expected:?}, {groups} groups"),
            );
        }
        #[cfg(feature = "capture")]
        Message::LiveMenuChosen(category, chosen) => {
            check(
                "a native menu opens and reports the line chosen from it",
                chosen == Some(0),
                &format!("chosen {chosen:?}"),
            );
            let message = chosen.and_then(|chosen| {
                context_entries(ContextTarget::Category(category))
                    .into_iter()
                    .filter_map(|entry| match entry {
                        ContextEntry::Action(_, message) => Some(message),
                        ContextEntry::Separator => None,
                    })
                    .nth(chosen)
            });
            if let Some(message) = message {
                return apply(shell, message);
            }
        }
        #[cfg(feature = "capture")]
        Message::LiveAccessibility(published) => {
            let expected: Vec<String> = shell
                .accessibility_nodes
                .iter()
                .map(|node| {
                    if node.selected {
                        format!("{}, selected", node.label)
                    } else {
                        node.label.clone()
                    }
                })
                .collect();
            check(
                "VoiceOver is given the panel's accessibility tree, in order",
                published == expected && published.len() > 10,
                &format!(
                    "{} elements, first {:?}",
                    published.len(),
                    published.first()
                ),
            );
        }
        #[cfg(feature = "capture")]
        Message::LiveReport {
            label,
            expected_visible,
            report,
        } => match report {
            Ok(report) => {
                let verdict = if report.visible == expected_visible
                    && shell.panel_visible == expected_visible
                {
                    "PASS"
                } else {
                    "FAIL"
                };
                eprintln!(
                    "live: {verdict} {label}: expected visible={expected_visible}, window visible={} key={} frame={:?}, shell visible={}",
                    report.visible, report.key, report.frame, shell.panel_visible
                );
            }
            Err(error) => eprintln!("live: FAIL {label}: {error}"),
        },
    }
    Task::none()
}

// ---------------------------------------------------------------- navigation

/// Points the sidebar at `destination`.
///
/// A search is cleared on the way, because the list it narrowed is not the one being asked for.
/// Returning to a group returns to the segment it was left on.
fn select_destination(shell: &mut AppShell, destination: Destination) {
    shell.application.state.set_search_query(String::new());
    shell.hovered_tile = None;
    let state = &mut shell.application.state;
    match destination {
        Destination::Group(group_id) => {
            if !state.select_group(group_id) {
                return;
            }
            shell.segment = shell.remembered_segments.get(&group_id).copied();
            state.selected_subgroup = shell.segment;
        }
        Destination::Recent => state.selected_launcher = None,
        Destination::Settings => {
            state.selected_launcher = None;
            refresh_terminals(shell);
        }
    }
    shell.destination = Some(destination);
}

/// A terminal Settings offers for launches; no `id` is the system's default.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalPick {
    pub id: Option<String>,
    pub label: String,
}

impl std::fmt::Display for TerminalPick {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.label)
    }
}

impl AppShell {
    /// The terminal launches go to, as Settings shows it.
    #[must_use]
    pub fn picked_terminal(&self) -> Option<&TerminalPick> {
        let chosen = self.application.chosen_terminal();
        self.terminals.iter().find(|pick| pick.id == chosen)
    }
}

/// Asks which terminals are installed, for the Settings picker.
fn refresh_terminals(shell: &mut AppShell) {
    let default = TerminalPick {
        id: None,
        label: shell.application.default_terminal_name().map_or_else(
            || "System default".to_owned(),
            |name| format!("System default ({name})"),
        ),
    };
    shell.terminals = std::iter::once(default)
        .chain(
            shell
                .application
                .terminal_options()
                .into_iter()
                .map(|option| TerminalPick {
                    id: Some(option.id),
                    label: option.name,
                }),
        )
        .collect();
}

/// Moves the open group's segmented control to `segment`, remembering it for the group.
fn select_segment(shell: &mut AppShell, segment: Option<SubgroupId>) {
    let Some(group_id) = shell.open_group().map(|group| group.id) else {
        return;
    };
    if let Some(subgroup_id) = segment
        && shell
            .subgroup(subgroup_id)
            .is_none_or(|(group, _)| group.id != group_id)
    {
        return;
    }
    shell.segment = segment;
    match segment {
        Some(subgroup_id) => {
            shell.remembered_segments.insert(group_id, subgroup_id);
        }
        None => {
            shell.remembered_segments.remove(&group_id);
        }
    }
    let state = &mut shell.application.state;
    state.selected_subgroup = segment;
    state.selected_launcher = None;
    shell.hovered_tile = None;
}

/// Launches `launcher_id`, and puts the panel away once the terminal has it — unless pinned.
fn launch(shell: &mut AppShell, launcher_id: LauncherId) -> Task<Message> {
    if shell.application.state.dialog.is_some()
        || !shell.application.state.select_launcher(launcher_id)
    {
        return Task::none();
    }
    let outcome = shell.application.launch_selected();
    let launched = outcome.is_ok();
    report(shell, outcome);
    if launched && !shell.pinned {
        // The terminal is taking the focus, so nothing is handed back.
        return hide_panel(shell, false);
    }
    Task::none()
}

/// Closes whatever is on top: a sheet, a held row, a search, and finally the panel.
fn escape(shell: &mut AppShell) -> Task<Message> {
    if shell.application.state.dialog.is_some() {
        dismiss_dialog(shell);
    } else if shell.drag.is_some() || shell.group_drag.is_some() {
        shell.drag = None;
        shell.group_drag = None;
        shell.hovered_subgroup = None;
    } else if presenters::is_searching(&shell.application.state) {
        shell.application.state.set_search_query(String::new());
    } else {
        return hide_panel(shell, true);
    }
    Task::none()
}

// ---------------------------------------------------------------- the panel

fn window_opened(shell: &mut AppShell, window_id: iced::window::Id) -> Task<Message> {
    // The opening is reported both by the task that opened the window and by the window events.
    if shell.window_id == Some(window_id) {
        return Task::none();
    }
    shell.window_id = Some(window_id);
    #[cfg(feature = "capture")]
    if crate::capture::requested().is_some() {
        shell.panel_visible = true;
        return crate::capture::schedule(window_id).map(Message::CaptureReady);
    }
    shell.menu_bar.install();
    shell.open_at_login = Some(read_open_at_login());
    #[cfg(feature = "capture")]
    let live = if crate::capture::live().is_some() {
        after(Duration::from_millis(1_500), Message::LiveStep(0))
    } else {
        Task::none()
    };
    #[cfg(not(feature = "capture"))]
    let live = Task::none();
    // The panel opens once on launch, so starting the app visibly does something; from then on it
    // lives behind the menu-bar item. It waits a moment first: a status item that was just
    // created has not been laid out in the menu bar, and the panel hangs from where it is.
    Task::batch([
        status_item::configure(window_id).map(Message::PanelCommand),
        after(MENU_BAR_SETTLE, Message::Panel(PanelEvent::Open)),
        live,
    ])
}

fn panel_event(shell: &mut AppShell, event: PanelEvent) -> Task<Message> {
    #[cfg(feature = "capture")]
    if crate::capture::live().is_some() {
        eprintln!("live: menu-bar event {event:?}");
    }
    match event {
        PanelEvent::Toggle => {
            if shell.panel_visible {
                hide_panel(shell, true)
            } else {
                show_panel(shell)
            }
        }
        PanelEvent::Open => show_panel(shell),
        PanelEvent::OpenSettings => {
            select_destination(shell, Destination::Settings);
            show_panel(shell)
        }
        PanelEvent::RevealConfig => {
            let outcome = shell.application.reveal_config();
            report(shell, outcome);
            Task::none()
        }
        PanelEvent::Quit => iced::exit(),
        // A click anywhere outside the panel makes another app active. A pinned panel stays.
        PanelEvent::Deactivated if shell.panel_visible && !shell.pinned => hide_panel(shell, false),
        PanelEvent::Deactivated => Task::none(),
        PanelEvent::AccessibilityPress(index) => accessibility_press(shell, index),
    }
}

/// Carries out what an assistive technology pressed.
///
/// The index is into the tree `VoiceOver` was given — the one last installed — not into whatever
/// the interface would describe now, so a press always means the element that was announced.
fn accessibility_press(shell: &mut AppShell, index: usize) -> Task<Message> {
    let Some(action) = shell
        .installed_accessibility
        .as_ref()
        .and_then(|nodes| nodes.get(index))
        .and_then(|node| node.action)
    else {
        return Task::none();
    };
    match action {
        accessibility::Action::FocusSearch => focus(SEARCH_ID),
        accessibility::Action::TogglePin => apply(shell, Message::TogglePin),
        accessibility::Action::Open(destination) => {
            apply(shell, Message::SelectDestination(destination))
        }
        accessibility::Action::AddGroup => apply(shell, Message::AddGroup),
        accessibility::Action::Segment(segment) => apply(shell, Message::SelectSegment(segment)),
        accessibility::Action::AddCategory => apply(shell, Message::AddSubgroup),
        accessibility::Action::Launch(launcher_id) => {
            apply(shell, Message::LaunchTile(launcher_id))
        }
        accessibility::Action::AddItem => match crate::views::add_action(shell) {
            Some((_, message)) => apply(shell, message),
            None => Task::none(),
        },
    }
}

/// Puts the panel under the menu-bar item, in front, with the search field ready for typing.
fn show_panel(shell: &mut AppShell) -> Task<Message> {
    shell.panel_visible = true;
    shell.menu_bar.set_highlighted(true);
    let Some(window_id) = shell.window_id else {
        return Task::none();
    };
    let top_left = shell.menu_bar.panel_top_left(PANEL_WIDTH);
    Task::batch([
        status_item::show(window_id, top_left, !shell.reduce_motion).map(Message::PanelCommand),
        after(SHADOW_SETTLE, Message::RefreshShadow),
        focus(SEARCH_ID),
    ])
}

/// Takes the panel off screen. Its state — the open sheet, the scroll position, the search — is
/// kept, so reopening it carries on where the user left off.
fn hide_panel(shell: &mut AppShell, return_focus: bool) -> Task<Message> {
    if !shell.panel_visible {
        return Task::none();
    }
    shell.panel_visible = false;
    shell.menu_bar.set_highlighted(false);
    shell.drag = None;
    shell.group_drag = None;
    shell.hovered_tile = None;
    shell.hovered_group = None;
    shell.hovered_subgroup = None;
    let Some(window_id) = shell.window_id else {
        return Task::none();
    };
    // Without the item there is no way back to a hidden panel, so it is never hidden.
    if !shell.menu_bar.is_installed() {
        shell.panel_visible = true;
        return Task::none();
    }
    status_item::hide(window_id, return_focus).map(Message::PanelCommand)
}

// ---------------------------------------------------------------- editing

/// Opens the shared name editor for a group or a category.
fn open_name_editor(shell: &mut AppShell, draft: NameDraft, dialog: Dialog) -> Task<Message> {
    shell.name_draft = Some(draft);
    shell.launcher_draft = None;
    shell.application.state.dialog = Some(dialog);
    focus(GROUP_NAME_ID)
}

/// The categories of `group_id`, as choices an editor can offer.
fn category_choices(shell: &AppShell, group_id: GroupId) -> Vec<CategoryChoice> {
    shell
        .group(group_id)
        .map(|group| {
            group
                .subgroups
                .iter()
                .map(|subgroup| CategoryChoice {
                    id: subgroup.id,
                    name: subgroup.name.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Opens the launcher editor for a new launcher where the panel is pointing.
///
/// A server goes into the segment that is open; with "All" open, into the first category, with a
/// choice of the others. A group with no category yet asks for one first, since a launcher always
/// lives in a category.
fn add_launcher(shell: &mut AppShell) -> Task<Message> {
    let draft = match shell.destination {
        Some(Destination::Group(group_id)) => {
            let choices = category_choices(shell, group_id);
            let Some(target) = shell
                .segment
                .or_else(|| choices.first().map(|choice| choice.id))
            else {
                return apply(shell, Message::AddSubgroup);
            };
            LauncherDraft::new(target).with_categories(choices)
        }
        Some(Destination::Recent | Destination::Settings) | None => {
            if shell.destination.is_none() {
                return apply(shell, Message::AddGroup);
            }
            return Task::none();
        }
    };
    let subgroup_id = draft.subgroup_id;
    shell.launcher_draft = Some(draft);
    shell.name_draft = None;
    shell.application.state.dialog = Some(Dialog::AddLauncher { subgroup_id });
    focus(LAUNCHER_NAME_ID)
}

fn edit_launcher(shell: &mut AppShell, launcher_id: LauncherId) -> Task<Message> {
    let Some((group_id, subgroup_id)) = shell
        .application
        .state
        .config
        .locate_launcher(launcher_id)
        .map(|(group, subgroup)| (group.id, subgroup.id))
    else {
        return Task::none();
    };
    let Some((_, launcher)) = shell.launcher(launcher_id) else {
        return Task::none();
    };
    let Some(draft) = LauncherDraft::edit(subgroup_id, launcher) else {
        return Task::none();
    };
    let draft = draft.with_categories(category_choices(shell, group_id));
    shell.launcher_draft = Some(draft);
    shell.name_draft = None;
    let _ = shell.application.state.select_launcher(launcher_id);
    shell.application.state.dialog = Some(Dialog::EditLauncher { launcher_id });
    focus(LAUNCHER_NAME_ID)
}

/// What a saved name editor should leave selected.
enum Selection {
    Group(GroupId),
    Subgroup(SubgroupId),
}

fn save_name(shell: &mut AppShell) {
    let Some(draft) = &shell.name_draft else {
        return;
    };
    let target = draft.target;
    let values = match draft.build() {
        Ok(values) => values,
        Err(error) => {
            if let Some(draft) = &mut shell.name_draft {
                draft.error = Some(error);
            }
            return;
        }
    };
    let name = values.name;
    let outcome = match target {
        NameTarget::NewGroup => shell
            .application
            .add_group(name, Section::Servers)
            .map(Selection::Group),
        NameTarget::RenameGroup(group_id) => shell
            .application
            .rename_group(group_id, name)
            .map(|()| Selection::Group(group_id)),
        NameTarget::NewSubgroup(group_id) => shell
            .application
            .add_subgroup(group_id, name)
            .map(Selection::Subgroup),
        NameTarget::RenameSubgroup(subgroup_id) => shell
            .application
            .rename_subgroup(subgroup_id, name)
            .map(|()| Selection::Subgroup(subgroup_id)),
    };
    // The color is written only when it actually changed, so renaming does not spend a second
    // config revision restating the color a group already had.
    let outcome = outcome.and_then(|selection| match selection {
        Selection::Group(group_id) => {
            if shell.group(group_id).and_then(|group| group.color) == values.color {
                Ok(Selection::Group(group_id))
            } else {
                shell
                    .application
                    .set_group_color(group_id, values.color)
                    .map(|()| Selection::Group(group_id))
            }
        }
        // A category has no color of its own: it is drawn in the color of its group.
        Selection::Subgroup(subgroup_id) => Ok(Selection::Subgroup(subgroup_id)),
    });
    match outcome {
        Ok(Selection::Group(group_id)) => {
            if matches!(target, NameTarget::NewGroup) {
                select_destination(shell, Destination::Group(group_id));
            }
            dismiss_dialog(shell);
        }
        Ok(Selection::Subgroup(subgroup_id)) => {
            if matches!(target, NameTarget::NewSubgroup(_)) {
                select_segment(shell, Some(subgroup_id));
            }
            dismiss_dialog(shell);
        }
        Err(error) => {
            if let Some(draft) = &mut shell.name_draft {
                draft.error = Some(crate::editor::FormError {
                    field: None,
                    message: error.to_string(),
                });
            }
        }
    }
}

fn save_launcher(shell: &mut AppShell) {
    let Some(draft) = &shell.launcher_draft else {
        return;
    };
    let launcher = match draft.build() {
        Ok(launcher) => launcher,
        Err(error) => {
            if let Some(draft) = &mut shell.launcher_draft {
                draft.error = Some(error);
            }
            return;
        }
    };
    let launcher_id = launcher.id();
    let subgroup_id = draft.subgroup_id;
    let origin = draft.origin;
    let outcome = match draft.target {
        Target::New => shell
            .application
            .add_launcher(subgroup_id, launcher)
            .map(|_| ()),
        Target::Existing(existing) => {
            shell
                .application
                .edit_launcher(existing, launcher)
                .and_then(|()| {
                    if origin == subgroup_id {
                        return Ok(());
                    }
                    // A launcher moved to another category lands at its end, as a drop does.
                    let index = shell
                        .subgroup(subgroup_id)
                        .map_or(0, |(_, subgroup)| subgroup.items.len());
                    shell
                        .application
                        .move_launcher(existing, origin, subgroup_id, index)
                })
        }
    };
    match outcome {
        Ok(()) => {
            // A launcher saved into a category the list is not showing is followed there, so it
            // never seems to vanish on save.
            if shell.segment.is_some_and(|segment| segment != subgroup_id) {
                select_segment(shell, Some(subgroup_id));
            }
            let _ = shell.application.state.select_launcher(launcher_id);
            dismiss_dialog(shell);
        }
        Err(error) => {
            if let Some(draft) = &mut shell.launcher_draft {
                draft.error = Some(crate::editor::FormError {
                    field: None,
                    message: error.to_string(),
                });
            }
        }
    }
}

/// Switches the interface between light, dark, and following the system.
///
/// Every symbol is rasterized once per theme, so the icon set is rebuilt here rather than tinted
/// on the fly; without that the old tints would survive the switch and disappear against the new
/// ground.
fn select_appearance(shell: &mut AppShell, choice: AppearanceChoice) {
    shell.appearance = choice;
    let appearance = Appearance::resolve(choice);
    shell.reduce_motion = appearance.reduce_motion;
    shell.theme = appearance.theme();
    shell.icons = IconSet::load(&shell.theme);
    prepare_label_icons(shell);
    report(shell, shell.application.set_appearance(choice));
}

/// Checks that something is listening where the open draft says its server is.
///
/// The check runs off the UI thread and reports back through [`Message::ReachabilityChecked`],
/// carrying the destination it ran against so a reply that arrives after the host was edited is
/// recognised as stale rather than shown against the new address.
fn test_reachability(shell: &mut AppShell) -> Task<Message> {
    let Some(draft) = &mut shell.launcher_draft else {
        return Task::none();
    };
    let Some((host, port)) = draft.destination() else {
        return Task::none();
    };
    let destination = format!("{host}:{}", port.get());
    draft.reachability = ReachabilityState::Checking {
        destination: destination.clone(),
    };

    let probe = shell.application.host_probe();
    Task::perform(
        async move {
            let outcome = Application::check_reachability(&probe, host, port)
                .map_err(|error| error.to_string());
            (destination, outcome)
        },
        |(destination, outcome)| Message::ReachabilityChecked {
            destination,
            outcome,
        },
    )
}

/// Asks the system for an SSH key and writes the choice into the open draft.
///
/// The chosen path is stored home-relative when it can be, so a config shared between machines
/// keeps pointing at the right key. Nothing happens when the panel is cancelled.
fn choose_identity_file(shell: &mut AppShell) {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        let request = despatch_platform_macos::FileChoiceRequest {
            title: "Choose an identity file".to_owned(),
            message: "Despatch stores the path to this key. Its contents are never read or \
                      copied into the config file."
                .to_owned(),
            prompt: "Use key".to_owned(),
            directory: home.as_ref().map(|home| home.join(".ssh")),
            shows_hidden_files: true,
        };
        let Some(path) = despatch_platform_macos::choose_file(&request) else {
            return;
        };
        let value = home.as_deref().map_or_else(
            || path.to_string_lossy().into_owned(),
            |home| despatch_platform_macos::portable_path(&path, home),
        );
        if let Some(draft) = &mut shell.launcher_draft {
            draft.set(Field::IdentityFile, value);
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = shell;
    }
}

/// Opens an existing config file chosen in the system open panel.
fn open_config(shell: &mut AppShell) {
    #[cfg(target_os = "macos")]
    {
        let request = despatch_platform_macos::FileChoiceRequest {
            title: "Open a Despatch config".to_owned(),
            message: "Choose the JSON file that holds your groups and servers.".to_owned(),
            prompt: "Open".to_owned(),
            directory: config_directory(shell),
            shows_hidden_files: false,
        };
        let Some(path) = despatch_platform_macos::choose_file(&request) else {
            return;
        };
        let outcome = shell.application.open_config(&path);
        report(shell, outcome);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = shell;
    }
}

/// Creates a new config file where the user says, and switches to it.
fn new_config(shell: &mut AppShell) {
    #[cfg(target_os = "macos")]
    {
        let Some(path) = choose_save_path(shell, "New Despatch config", "Create") else {
            return;
        };
        let outcome = shell.application.create_config(&path);
        report(shell, outcome);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = shell;
    }
}

/// Writes a copy of the open config where the user says, without switching to it.
fn save_config_copy(shell: &mut AppShell) {
    #[cfg(target_os = "macos")]
    {
        let Some(path) = choose_save_path(shell, "Save a copy of the config", "Save") else {
            return;
        };
        let outcome = shell.application.save_copy(&path);
        report(shell, outcome);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = shell;
    }
}

#[cfg(target_os = "macos")]
fn choose_save_path(shell: &AppShell, title: &str, prompt: &str) -> Option<std::path::PathBuf> {
    despatch_platform_macos::choose_save_path(&despatch_platform_macos::SavePathRequest {
        title: title.to_owned(),
        prompt: prompt.to_owned(),
        directory: config_directory(shell),
        file_name: "despatch.json".to_owned(),
    })
}

#[cfg(target_os = "macos")]
fn config_directory(shell: &AppShell) -> Option<std::path::PathBuf> {
    shell
        .application
        .state
        .source_path
        .as_deref()
        .and_then(std::path::Path::parent)
        .map(std::path::Path::to_owned)
}

fn duplicate_launcher(shell: &mut AppShell, launcher_id: LauncherId) {
    let Some((subgroup, launcher)) = shell.launcher(launcher_id) else {
        return;
    };
    let subgroup_id = subgroup.id;
    let taken: Vec<String> = subgroup
        .items
        .iter()
        .map(|item| item.name().to_owned())
        .collect();
    let name = duplicate_name(&taken, launcher.name());

    match shell
        .application
        .duplicate_launcher(subgroup_id, launcher_id, name)
    {
        Ok(duplicate_id) => {
            let _ = shell.application.state.select_launcher(duplicate_id);
            shell.action_error = None;
        }
        Err(error) => shell.action_error = Some(error.to_string()),
    }
}

// ---------------------------------------------------------------- context menus

/// What the native menu offers for `target`, and the message each line sends.
///
/// The same actions the row's hover buttons and the editors offer — a right click is a faster way
/// to them, never the only one.
#[must_use]
pub fn context_entries(target: ContextTarget) -> Vec<ContextEntry> {
    match target {
        ContextTarget::Launcher(launcher_id) => vec![
            ContextEntry::Action("Open", Message::LaunchTile(launcher_id)),
            ContextEntry::Separator,
            ContextEntry::Action("Edit…", Message::EditLauncher(launcher_id)),
            ContextEntry::Action("Duplicate", Message::DuplicateLauncher(launcher_id)),
            ContextEntry::Separator,
            ContextEntry::Action("Delete…", Message::DeleteLauncherRequested(launcher_id)),
        ],
        ContextTarget::Group(group_id) => vec![
            ContextEntry::Action("Rename…", Message::RenameGroup(group_id)),
            ContextEntry::Separator,
            ContextEntry::Action("Delete Group…", Message::DeleteGroupRequested(group_id)),
        ],
        ContextTarget::Category(subgroup_id) => vec![
            ContextEntry::Action("Rename…", Message::RenameSubgroup(subgroup_id)),
            ContextEntry::Separator,
            ContextEntry::Action(
                "Delete Category…",
                Message::DeleteSubgroupRequested(subgroup_id),
            ),
        ],
    }
}

/// Shows the native menu for `target` at the pointer and carries out the line chosen.
fn show_context_menu(shell: &mut AppShell, target: ContextTarget) -> Task<Message> {
    if shell.application.state.dialog.is_some() {
        return Task::none();
    }
    if let ContextTarget::Launcher(launcher_id) = target {
        let _ = shell.application.state.select_launcher(launcher_id);
    }
    let entries = context_entries(target);
    let actions: Vec<Message> = entries
        .iter()
        .filter_map(|entry| match entry {
            ContextEntry::Action(_, message) => Some(message.clone()),
            ContextEntry::Separator => None,
        })
        .collect();
    let Some(chosen) = choose_from_menu(&entries) else {
        return Task::none();
    };
    actions
        .get(chosen)
        .cloned()
        .map_or_else(Task::none, |message| apply(shell, message))
}

#[cfg(target_os = "macos")]
fn choose_from_menu(entries: &[ContextEntry]) -> Option<usize> {
    let items: Vec<despatch_platform_macos::ContextMenuItem> = entries
        .iter()
        .map(|entry| match entry {
            ContextEntry::Action(title, _) => {
                despatch_platform_macos::ContextMenuItem::Action((*title).to_owned())
            }
            ContextEntry::Separator => despatch_platform_macos::ContextMenuItem::Separator,
        })
        .collect();
    despatch_platform_macos::show_context_menu(&items)
}

#[cfg(not(target_os = "macos"))]
fn choose_from_menu(_entries: &[ContextEntry]) -> Option<usize> {
    None
}

// ---------------------------------------------------------------- open at login

fn read_open_at_login() -> OpenAtLogin {
    #[cfg(target_os = "macos")]
    {
        match despatch_platform_macos::login_item_status() {
            despatch_platform_macos::LoginItemStatus::Off => OpenAtLogin::Off,
            despatch_platform_macos::LoginItemStatus::On => OpenAtLogin::On,
            despatch_platform_macos::LoginItemStatus::NeedsApproval => OpenAtLogin::NeedsApproval,
            despatch_platform_macos::LoginItemStatus::Unavailable => OpenAtLogin::Unavailable,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        OpenAtLogin::Unavailable
    }
}

/// Turns opening at login on or off, then shows whatever the system now reports.
fn set_open_at_login(shell: &mut AppShell, enabled: bool) {
    #[cfg(target_os = "macos")]
    {
        let outcome = despatch_platform_macos::set_open_at_login(enabled);
        shell.open_at_login = Some(read_open_at_login());
        shell.action_error = outcome
            .err()
            .map(|error| format!("Open at login could not be changed: {error}"));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (shell, enabled);
    }
}

// ---------------------------------------------------------------- dragging

/// Takes a press on a row as both a selection and the start of a possible drag.
///
/// The row reports its own press, so the launcher picked up is the one under the pointer by
/// construction rather than whichever row last reported a hover — a hover that can go stale
/// whenever the list redraws under a still pointer.
fn begin_drag(shell: &mut AppShell, launcher_id: LauncherId) {
    if shell.application.state.dialog.is_some() {
        return;
    }
    let _ = shell.application.state.select_launcher(launcher_id);
    let Some((subgroup, _)) = shell.launcher(launcher_id) else {
        return;
    };
    shell.drag = Some(Drag::new(launcher_id, subgroup.id));
}

/// Drops the dragged launcher into the category segment under the pointer, if there is one.
///
/// A release that lands anywhere else ends the drag and changes nothing, which is what makes the
/// gesture abortable: drag away from every segment and let go.
fn finish_drag(shell: &mut AppShell) -> Task<Message> {
    let Some(drag) = shell.drag.take() else {
        return Task::none();
    };
    // A press that never travelled is a click.
    if !drag.lifted {
        return Task::none();
    }
    let Some(target) = shell.hovered_subgroup else {
        return Task::none();
    };
    if target == drag.source {
        return Task::none();
    }
    let Some((_, subgroup)) = shell.subgroup(target) else {
        return Task::none();
    };
    // Dropped launchers land at the end of the category, where the eye last was.
    let index = subgroup.items.len();

    let outcome = shell
        .application
        .move_launcher(drag.launcher_id, drag.source, target, index);
    let moved = outcome.is_ok();
    report(shell, outcome);
    if moved {
        // The row is no longer where the pointer left it, so the hover it reported is stale.
        shell.hovered_tile = None;
        // A list showing one category follows the launcher to the one it landed in, so the drop
        // visibly arrives somewhere instead of making the row vanish.
        if shell.segment.is_some() {
            select_segment(shell, Some(target));
        }
        let _ = shell.application.state.select_launcher(drag.launcher_id);
    }
    Task::none()
}

/// Drops the dragged group onto the place of the group row under the pointer.
///
/// The group takes the target's place and the rest shift to make room, the way rows reorder in a
/// Finder sidebar. A release anywhere else, or on the group itself, changes nothing.
fn finish_group_drag(shell: &mut AppShell) {
    let Some(drag) = shell.group_drag.take() else {
        return;
    };
    if !drag.lifted {
        return;
    }
    let Some(target) = shell
        .hovered_group
        .filter(|target| *target != drag.group_id)
    else {
        return;
    };
    let mut order: Vec<GroupId> = shell
        .application
        .state
        .config
        .groups
        .iter()
        .filter(|group| group.section == Section::Servers)
        .map(|group| group.id)
        .collect();
    let (Some(from), Some(to)) = (
        order.iter().position(|id| *id == drag.group_id),
        order.iter().position(|id| *id == target),
    ) else {
        return;
    };
    let moved = order.remove(from);
    order.insert(to, moved);
    let outcome = shell.application.reorder_groups(Section::Servers, &order);
    report(shell, outcome);
}

fn confirm_delete(shell: &mut AppShell) {
    let outcome = match shell.application.state.dialog {
        Some(Dialog::DeleteGroup { group_id }) => Some(shell.application.delete_group(group_id)),
        Some(Dialog::DeleteSubgroup { subgroup_id }) => {
            Some(shell.application.delete_subgroup(subgroup_id))
        }
        Some(Dialog::DeleteLauncher { launcher_id }) => shell
            .launcher(launcher_id)
            .map(|(subgroup, _)| subgroup.id)
            .map(|subgroup_id| shell.application.delete_launcher(subgroup_id, launcher_id)),
        _ => None,
    };
    match outcome {
        Some(Ok(())) => {
            shell.action_error = None;
            dismiss_dialog(shell);
        }
        Some(Err(error)) => {
            shell.action_error = Some(error.to_string());
            dismiss_dialog(shell);
        }
        None => dismiss_dialog(shell),
    }
}

/// Picks the first free `<name> copy`, `<name> copy 2`, … so a duplicate never collides with an
/// existing launcher name in the same category.
#[must_use]
pub fn duplicate_name(taken: &[String], base: &str) -> String {
    let is_taken = |candidate: &str| {
        taken
            .iter()
            .any(|name| name.eq_ignore_ascii_case(candidate))
    };
    let first = format!("{base} copy");
    if !is_taken(&first) {
        return first;
    }
    // One category cannot hold more launchers than `usize` can count, so a free suffix exists.
    (2..=usize::MAX)
        .map(|index| format!("{base} copy {index}"))
        .find(|candidate| !is_taken(candidate))
        .unwrap_or(first)
}

fn dismiss_dialog(shell: &mut AppShell) {
    shell.application.state.dialog = None;
    shell.name_draft = None;
    shell.launcher_draft = None;
}

fn report<T, E: std::fmt::Display>(shell: &mut AppShell, outcome: Result<T, E>) {
    shell.action_error = outcome.err().map(|error| error.to_string());
}

// ---------------------------------------------------------------- keyboard

pub fn subscription(shell: &AppShell) -> Subscription<Message> {
    let keyboard = iced::event::listen_with(|event, status, _window| {
        let iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            modifiers,
            repeat: false,
            ..
        }) = event
        else {
            return None;
        };
        // Escape always reaches the panel, even from inside a field: it is how a menu closes.
        if key == keyboard::Key::Named(keyboard::key::Named::Escape) {
            return Some(Message::Escape);
        }
        let shortcut = shortcuts::interpret(&key, modifiers)?;
        // A key a focused field already used — Delete removing a character, an arrow moving the
        // caret — is the field's, not a command's.
        if status == iced::event::Status::Captured && !shortcut.applies_while_typing() {
            return None;
        }
        Some(Message::Shortcut(shortcut))
    });
    let mut subscriptions = vec![
        keyboard,
        iced::window::open_events().map(Message::WindowOpened),
        shell.menu_bar.events().map(Message::Panel),
    ];
    // The rest of a drag — the travel and the release, which happen over segments and empty space —
    // comes from the event stream, which a widget's capture cannot hide. It is listened to only
    // while a row is actually being carried: every message rebuilds and re-lays-out the whole
    // panel, and pointer movement arrives far faster than that.
    if shell.drag.is_some() || shell.group_drag.is_some() {
        subscriptions.push(iced::event::listen_with(carried_pointer));
    }
    Subscription::batch(subscriptions)
}

/// The pointer events that matter while a row is being carried.
#[expect(
    clippy::needless_pass_by_value,
    reason = "the signature is fixed by iced::event::listen_with"
)]
fn carried_pointer(
    event: iced::Event,
    _status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
            Some(Message::PointerReleased)
        }
        // A pointer that has left, and a window that is no longer in front, will both never see
        // the release that ends the gesture, so the press being held is dropped with them.
        iced::Event::Mouse(iced::mouse::Event::CursorLeft)
        | iced::Event::Window(iced::window::Event::Unfocused) => Some(Message::PointerCancelled),
        _ => None,
    }
}

fn apply_shortcut(shell: &mut AppShell, shortcut: Shortcut) -> Task<Message> {
    let dialog_open = shell.application.state.dialog.is_some();
    match shortcut {
        Shortcut::AddItem => return apply(shell, Message::AddLauncher),
        Shortcut::AddGroup => return apply(shell, Message::AddGroup),
        Shortcut::FocusSearch => return focus(SEARCH_ID),
        Shortcut::OpenSettings => select_destination(shell, Destination::Settings),
        Shortcut::Quit => return iced::exit(),
        Shortcut::Close => return hide_panel(shell, true),
        Shortcut::Launch if !dialog_open => {
            if let Some(launcher_id) = shell.application.state.selected_launcher {
                return launch(shell, launcher_id);
            }
        }
        Shortcut::Edit if !dialog_open => {
            if let Some(launcher_id) = shell.application.state.selected_launcher {
                return apply(shell, Message::EditLauncher(launcher_id));
            }
        }
        Shortcut::Delete if !dialog_open => {
            if let Some(launcher_id) = shell.application.state.selected_launcher {
                shell.application.state.dialog = Some(Dialog::DeleteLauncher { launcher_id });
            }
        }
        Shortcut::Previous if !dialog_open => navigate_rows(shell, -1),
        Shortcut::Next if !dialog_open => navigate_rows(shell, 1),
        Shortcut::PreviousSegment if !dialog_open => navigate_segments(shell, -1),
        Shortcut::NextSegment if !dialog_open => navigate_segments(shell, 1),
        Shortcut::Group(index) if !dialog_open => {
            if let Some(destination) = presenters::sidebar_groups(&shell.application.state)
                .get(index)
                .map(|row| row.destination)
            {
                select_destination(shell, destination);
            }
        }
        Shortcut::Launch
        | Shortcut::Edit
        | Shortcut::Delete
        | Shortcut::Previous
        | Shortcut::Next
        | Shortcut::PreviousSegment
        | Shortcut::NextSegment
        | Shortcut::Group(_) => {}
    }
    Task::none()
}

/// Moves the selection up or down the rows the list is showing, wrapping at either end.
fn navigate_rows(shell: &mut AppShell, direction: isize) {
    let rows = shell.rows();
    if rows.is_empty() {
        return;
    }
    let current = shell
        .application
        .state
        .selected_launcher
        .and_then(|selected| rows.iter().position(|row| row.tile.id == selected));
    let next = match current {
        Some(0) | None if direction < 0 => rows.len() - 1,
        Some(index) if direction < 0 => index - 1,
        Some(index) => (index + 1) % rows.len(),
        None => 0,
    };
    let _ = shell.application.state.select_launcher(rows[next].tile.id);
}

/// Steps the open group's segmented control left or right, through "All" and each category.
fn navigate_segments(shell: &mut AppShell, direction: isize) {
    let Some(group) = shell.open_group() else {
        return;
    };
    let options: Vec<Option<SubgroupId>> = std::iter::once(None)
        .chain(group.subgroups.iter().map(|subgroup| Some(subgroup.id)))
        .collect();
    let current = options
        .iter()
        .position(|option| *option == shell.segment)
        .unwrap_or(0);
    let next = if direction < 0 {
        current.checked_sub(1).unwrap_or(options.len() - 1)
    } else {
        (current + 1) % options.len()
    };
    select_segment(shell, options[next]);
}

// ---------------------------------------------------------------- plumbing

fn focus(id: &'static str) -> Task<Message> {
    iced::widget::operation::focus(iced::widget::Id::new(id))
}

/// One step of the live self-test.
///
/// It drives the real panel the way a person would — the menu-bar item through `AppKit`, the
/// pointer and keyboard through events posted to the app's own queue, which winit reads as real
/// input — and checks after each step what the window server and the shell say. Every check prints
/// `live: PASS …` or `live: FAIL …`. Each check is a step of its own, after the action before it
/// has settled, because a check queued with the next action would be answered after it landed.
#[cfg(feature = "capture")]
#[expect(
    clippy::too_many_lines,
    reason = "the script reads top to bottom as the sequence it performs"
)]
fn live_step(shell: &mut AppShell, step: usize) -> Task<Message> {
    use crate::views::layout;
    use despatch_platform_macos::PointerEvent;

    let Some(window_id) = shell.window_id else {
        return Task::none();
    };
    let next = |delay_ms: u64| after(Duration::from_millis(delay_ms), Message::LiveStep(step + 1));
    let report = |label: &'static str, expected_visible: bool| {
        status_item::on_view(window_id, despatch_platform_macos::panel_report).map(move |report| {
            Message::LiveReport {
                label,
                expected_visible,
                report,
            }
        })
    };
    let pointer = |events: Vec<(PointerEvent, (f32, f32), isize)>| {
        status_item::on_view(window_id, move |view| {
            for (event, (x, y), clicks) in events {
                despatch_platform_macos::post_pointer(
                    view,
                    event,
                    f64::from(x),
                    f64::from(y),
                    clicks,
                )?;
            }
            Ok(())
        })
        .map(Message::PanelCommand)
    };
    let click = |at: (f32, f32)| {
        pointer(vec![
            (PointerEvent::Moved, at, 0),
            (PointerEvent::Down, at, 1),
            (PointerEvent::Up, at, 1),
        ])
    };
    let keys = |presses: Vec<(&'static str, u16, bool)>| {
        status_item::on_view(window_id, move |view| {
            for (characters, code, command) in presses {
                despatch_platform_macos::post_key(view, characters, code, command)?;
            }
            Ok(())
        })
        .map(Message::PanelCommand)
    };
    let group_named = |shell: &AppShell, name: &str| {
        shell
            .application
            .state
            .config
            .groups
            .iter()
            .find(|group| group.name == name)
            .map(|group| group.id)
    };
    // The pill at `index` ("All" is 0) of the group on screen.
    let pill = |shell: &AppShell, index: usize| {
        let Some(Destination::Group(group_id)) = shell.destination else {
            return layout::Rect::default();
        };
        let segments = presenters::segments(&shell.application.state, group_id);
        let names: Vec<&str> = segments
            .iter()
            .map(|segment| segment.name.as_str())
            .collect();
        layout::segment(&names, index)
    };
    let category_named = |shell: &AppShell, name: &str| {
        shell
            .application
            .state
            .config
            .groups
            .iter()
            .flat_map(|group| &group.subgroups)
            .find(|subgroup| subgroup.name == name)
            .map(|subgroup| subgroup.id)
    };
    let names_in = |shell: &AppShell, category: &str| -> Vec<String> {
        category_named(shell, category)
            .and_then(|id| shell.subgroup(id))
            .map(|(_, subgroup)| {
                subgroup
                    .items
                    .iter()
                    .map(|item| item.name().to_owned())
                    .collect()
            })
            .unwrap_or_default()
    };
    let marker = crate::capture::live()
        .unwrap_or_default()
        .join("launched-marker");

    match step {
        // ---- the menu-bar item, Escape, and deactivation
        0 => {
            let path = crate::capture::live()
                .unwrap_or_default()
                .join("live-1-open.png");
            Task::batch([
                report("opened under the menu-bar item at launch", true),
                status_item::on_view(window_id, move |view| {
                    let outline = despatch_platform_macos::capture_panel(view, &path)?;
                    check(
                        "the panel's corners are transparent in the composited window",
                        outline.corner_alpha < 0.25,
                        &format!("corner alpha {:.2}", outline.corner_alpha),
                    );
                    check(
                        "the panel is opaque just inside its corners",
                        outline.inside_alpha > 0.99,
                        &format!("inside alpha {:.2}", outline.inside_alpha),
                    );
                    check(
                        "the window casts a shadow below the panel",
                        outline.shadow_alpha > 0.05,
                        &format!("shadow alpha {:.2}", outline.shadow_alpha),
                    );
                    Ok(())
                })
                .map(Message::PanelCommand),
                status_item::on_view(window_id, despatch_platform_macos::accessibility_labels).map(
                    |labels| match labels {
                        Ok(labels) => Message::LiveAccessibility(labels),
                        Err(error) => Message::PanelCommand(Err(error)),
                    },
                ),
                next(800),
            ])
        }
        1 | 4 | 8 => {
            shell.menu_bar.perform_click();
            next(1_000)
        }
        2 => Task::batch([
            report("closed by clicking the menu-bar item", false),
            next(400),
        ]),
        3 => next(400),
        5 => Task::batch([
            report("reopened by clicking the menu-bar item again", true),
            next(600),
        ]),
        6 => Task::batch([apply(shell, Message::Escape), next(1_000)]),
        7 => Task::batch([report("closed by Escape", false), next(400)]),
        9 => Task::batch([
            report("reopened by the menu-bar item after Escape", true),
            apply(shell, Message::TogglePin),
            next(400),
        ]),
        10 => Task::batch([
            apply(shell, Message::Panel(PanelEvent::Deactivated)),
            next(800),
        ]),
        11 => Task::batch([
            report("a pinned panel stays when the app is deactivated", true),
            next(400),
        ]),

        // ---- pointer: the segmented control
        12 => {
            let production = group_named(shell, "Production");
            check(
                "the panel opened on the first group, All",
                production.is_some_and(|id| shell.destination == Some(Destination::Group(id)))
                    && shell.segment.is_none(),
                &format!(
                    "destination {:?}, segment {:?}",
                    shell.destination, shell.segment
                ),
            );
            Task::batch([click(layout::center(pill(shell, 2))), next(600)])
        }
        13 => {
            check(
                "clicking the Workers segment opens it",
                shell.segment == category_named(shell, "Workers"),
                &format!("segment {:?}", shell.segment),
            );
            Task::batch([click(layout::center(pill(shell, 1))), next(600)])
        }
        14 => {
            check(
                "clicking the Web segment opens it",
                shell.segment == category_named(shell, "Web"),
                &format!(
                    "rows {:?}",
                    shell
                        .rows()
                        .iter()
                        .map(|row| row.tile.name.clone())
                        .collect::<Vec<_>>()
                ),
            );
            // ---- pointer: drag the first row onto the Bastions segment
            let from = layout::center(layout::row(0, true).unwrap_or_default());
            Task::batch([
                pointer(vec![
                    (PointerEvent::Moved, from, 0),
                    (PointerEvent::Down, from, 1),
                ]),
                next(400),
            ])
        }
        15 => {
            let from = layout::center(layout::row(0, true).unwrap_or_default());
            let to = layout::center(pill(shell, 3));
            let path = (1..=8)
                .map(|i| {
                    #[expect(clippy::cast_precision_loss, reason = "eight steps")]
                    let t = i as f32 / 8.0;
                    (
                        PointerEvent::Dragged,
                        (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t),
                        1,
                    )
                })
                .collect();
            Task::batch([pointer(path), next(500)])
        }
        16 => {
            check(
                "a row pressed and moved past the threshold lifts",
                shell.drag.is_some_and(|drag| drag.lifted),
                &format!("drag {:?}", shell.drag),
            );
            check(
                "the segment under the pointer is the drop target",
                shell.hovered_subgroup == category_named(shell, "Bastions"),
                &format!("hovered {:?}", shell.hovered_subgroup),
            );
            let to = layout::center(pill(shell, 3));
            Task::batch([pointer(vec![(PointerEvent::Up, to, 1)]), next(600)])
        }
        17 => {
            check(
                "releasing over Bastions moves the server there",
                names_in(shell, "Bastions") == ["App server"]
                    && names_in(shell, "Web") == ["Worker"],
                &format!(
                    "Web {:?}, Bastions {:?}",
                    names_in(shell, "Web"),
                    names_in(shell, "Bastions")
                ),
            );
            check(
                "the list followed the server to Bastions",
                shell.segment == category_named(shell, "Bastions") && shell.drag.is_none(),
                &format!("segment {:?}", shell.segment),
            );
            // ---- keyboard: the editor
            Task::batch([keys(vec![("n", 45, true)]), next(600)])
        }
        18 => {
            check(
                "Command-N opens the server editor",
                matches!(
                    shell.application.state.dialog,
                    Some(Dialog::AddLauncher { .. })
                ),
                &format!("dialog {:?}", shell.application.state.dialog),
            );
            Task::batch([
                keys(vec![
                    ("L", 37, false),
                    ("i", 34, false),
                    ("v", 9, false),
                    ("e", 14, false),
                ]),
                next(600),
            ])
        }
        19 => {
            let typed = shell
                .launcher_draft
                .as_ref()
                .map(|draft| draft.name.clone())
                .unwrap_or_default();
            check(
                "typing lands in the editor's name field",
                typed == "Live",
                &format!("name {typed:?}"),
            );
            Task::batch([apply(shell, Message::SaveLauncher), next(400)])
        }
        20 => {
            let error = shell
                .launcher_draft
                .as_ref()
                .and_then(|draft| draft.field_error(Field::Host).map(str::to_owned));
            check(
                "saving without a host keeps the editor open and names the field",
                error.is_some() && shell.application.state.dialog.is_some(),
                &format!("host error {error:?}"),
            );
            Task::batch([keys(vec![("\u{1b}", 53, false)]), next(600)])
        }
        21 => {
            check(
                "Escape closes the editor without saving",
                shell.application.state.dialog.is_none()
                    && names_in(shell, "Bastions") == ["App server"],
                &format!("dialog {:?}", shell.application.state.dialog),
            );
            // ---- a native context menu, answered from the keyboard. The keys are posted and the
            // menu opened in one main-thread call, so the menu's own tracking loop is what reads
            // them; posted any earlier they would reach the list instead.
            let Some(bastions) = category_named(shell, "Bastions") else {
                return next(0);
            };
            let items: Vec<despatch_platform_macos::ContextMenuItem> =
                context_entries(ContextTarget::Category(bastions))
                    .into_iter()
                    .map(|entry| match entry {
                        ContextEntry::Action(title, _) => {
                            despatch_platform_macos::ContextMenuItem::Action(title.to_owned())
                        }
                        ContextEntry::Separator => {
                            despatch_platform_macos::ContextMenuItem::Separator
                        }
                    })
                    .collect();
            Task::batch([
                status_item::on_view(window_id, move |view| {
                    despatch_platform_macos::post_key(view, "\u{f701}", 125, false)?;
                    despatch_platform_macos::post_key(view, "\r", 36, false)?;
                    Ok(despatch_platform_macos::show_context_menu(&items))
                })
                .map(move |chosen| Message::LiveMenuChosen(bastions, chosen.ok().flatten())),
                next(1_000),
            ])
        }
        22 => {
            check(
                "the category's right-click menu offers Rename, which opens the editor",
                matches!(
                    shell.application.state.dialog,
                    Some(Dialog::EditSubgroup { .. })
                ),
                &format!("dialog {:?}", shell.application.state.dialog),
            );
            Task::batch([apply(shell, Message::DismissDialog), next(300)])
        }
        // ---- pointer: drag Home lab to the top of the sidebar
        23 => {
            let from = layout::center(layout::group_row(2));
            Task::batch([
                pointer(vec![
                    (PointerEvent::Moved, from, 0),
                    (PointerEvent::Down, from, 1),
                ]),
                next(400),
            ])
        }
        24 => {
            let from = layout::center(layout::group_row(2));
            let to = layout::center(layout::group_row(0));
            let path = (1..=6)
                .map(|i| {
                    #[expect(clippy::cast_precision_loss, reason = "six steps")]
                    let t = i as f32 / 6.0;
                    (
                        PointerEvent::Dragged,
                        (from.0, from.1 + (to.1 - from.1) * t),
                        1,
                    )
                })
                .collect();
            Task::batch([pointer(path), next(500)])
        }
        25 => Task::batch([
            pointer(vec![(
                PointerEvent::Up,
                layout::center(layout::group_row(0)),
                1,
            )]),
            next(600),
        ]),
        26 => {
            let order: Vec<String> = presenters::sidebar_groups(&shell.application.state)
                .into_iter()
                .map(|row| row.name)
                .collect();
            check(
                "dragging Home lab onto Production moves it to the top",
                order == ["Home lab", "Production", "Staging"],
                &format!("order {order:?}"),
            );
            // ---- pointer: Staging, then a double click that launches through a real terminal
            Task::batch([click(layout::center(layout::group_row(2))), next(600)])
        }
        27 => {
            let staging = shell
                .application
                .state
                .config
                .groups
                .iter()
                .find(|group| group.name == "Staging")
                .map(|group| Destination::Group(group.id));
            check(
                "clicking Staging in the sidebar opens it",
                shell.destination.is_some() && shell.destination == staging,
                &format!("destination {:?}", shell.destination),
            );
            let _ = std::fs::remove_file(&marker);
            let at = layout::center(layout::row(0, true).unwrap_or_default());
            Task::batch([
                pointer(vec![
                    (PointerEvent::Moved, at, 0),
                    (PointerEvent::Down, at, 1),
                    (PointerEvent::Up, at, 1),
                    (PointerEvent::Down, at, 2),
                    (PointerEvent::Up, at, 2),
                ]),
                next(5_000),
            ])
        }
        28 => {
            check(
                "a double click launched the server, and the terminal ran its command",
                matches!(
                    shell.application.state.last_launch,
                    Some(despatch_application::LaunchOutcome::Success { .. })
                ) && marker.exists(),
                &format!(
                    "launch {:?}, marker {}",
                    shell.application.state.last_launch,
                    marker.exists()
                ),
            );
            check(
                "the launch was remembered in Recently used",
                shell.application.recent_launches().len() == 1,
                &format!("{} remembered", shell.application.recent_launches().len()),
            );
            check(
                "a pinned panel stays open after a launch",
                shell.panel_visible,
                &format!("visible {}", shell.panel_visible),
            );
            // ---- pointer: Recently used, at the foot of the sidebar
            Task::batch([click(layout::center(layout::footer_row(0))), next(600)])
        }
        29 => {
            check(
                "clicking Recently used at the foot of the sidebar lists the launch",
                shell.destination == Some(Destination::Recent) && shell.rows().len() == 1,
                &format!(
                    "destination {:?}, {} rows",
                    shell.destination,
                    shell.rows().len()
                ),
            );
            // ---- opening at login, through the real ServiceManagement API
            Task::batch([apply(shell, Message::OpenAtLoginToggled(true)), next(1_500)])
        }
        30 => {
            let status = shell.open_at_login;
            if status == Some(OpenAtLogin::Unavailable) {
                eprintln!("live: SKIP open at login: not running from an app bundle");
                return next(0);
            }
            check(
                "turning on Open at login registers the app",
                matches!(status, Some(OpenAtLogin::On | OpenAtLogin::NeedsApproval)),
                &format!("status {status:?}, error {:?}", shell.action_error),
            );
            Task::batch([
                apply(shell, Message::OpenAtLoginToggled(false)),
                next(1_500),
            ])
        }
        31 => {
            if shell.open_at_login != Some(OpenAtLogin::Unavailable) {
                check(
                    "turning it off unregisters the app",
                    shell.open_at_login == Some(OpenAtLogin::Off),
                    &format!("status {:?}", shell.open_at_login),
                );
            }
            check(
                "the theme follows Increase Contrast",
                crate::theme::increased_contrast(&shell.theme)
                    == crate::capture::increased_contrast_override(),
                &format!("theme {}", shell.theme),
            );
            // ---- motion: hide, then sample the window's opacity just after it is shown again
            shell.menu_bar.perform_click();
            next(1_000)
        }
        32 => {
            shell.menu_bar.perform_click();
            // Sampled out of sequence, 40 ms in: well inside the fade, long after an instant show.
            Task::batch([
                after(Duration::from_millis(40), Message::LiveStep(1_000)),
                next(1_200),
            ])
        }
        1_000 => status_item::on_view(window_id, despatch_platform_macos::panel_report)
            .map(|report| Message::LiveMotion(report.map(|report| report.alpha))),
        33 => Task::batch([report("shown again after the fade", true), next(400)]),
        // ---- accessibility: frames, and presses the way VoiceOver makes them
        34 => Task::batch([
            status_item::on_view(window_id, |view| {
                let elements = despatch_platform_macos::accessibility_report(view)?;
                let panel = despatch_platform_macos::panel_report(view)?;
                Ok((elements, panel.frame))
            })
            .map(|result| match result {
                Ok((elements, window)) => Message::LiveAccessibilityFrames(elements, window),
                Err(error) => Message::PanelCommand(Err(error)),
            }),
            next(400),
        ]),
        35 => Task::batch([
            status_item::on_view(window_id, |view| {
                despatch_platform_macos::press_accessibility_element(view, "Production, ")
            })
            .map(|pressed| Message::PanelCommand(pressed.map(|_| ()))),
            next(600),
        ]),
        36 => {
            check(
                "pressing the Production element (VO-Space) opens the group",
                group_named(shell, "Production")
                    .is_some_and(|id| shell.destination == Some(Destination::Group(id))),
                &format!("destination {:?}", shell.destination),
            );
            Task::batch([
                status_item::on_view(window_id, |view| {
                    despatch_platform_macos::press_accessibility_element(view, "Workers, ")
                })
                .map(|pressed| Message::PanelCommand(pressed.map(|_| ()))),
                next(600),
            ])
        }
        37 => {
            check(
                "pressing the Workers segment element opens that category",
                shell.segment == category_named(shell, "Workers"),
                &format!("segment {:?}", shell.segment),
            );
            next(200)
        }
        _ => iced::exit(),
    }
}

/// Prints one live check.
#[cfg(feature = "capture")]
fn check(label: &str, passed: bool, detail: &str) {
    let verdict = if passed { "PASS" } else { "FAIL" };
    eprintln!("live: {verdict} {label} ({detail})");
}

/// Translates a `DESPATCH_CAPTURE_SCENE` name into the actions that open that screen.
#[cfg(feature = "capture")]
#[expect(
    clippy::too_many_lines,
    reason = "one arm per capture scene; splitting it hides which screen each arm opens"
)]
fn capture_scene(shell: &AppShell) -> Vec<Message> {
    let Some(scene) = crate::capture::scene() else {
        return Vec::new();
    };
    let group = shell
        .application
        .state
        .config
        .groups
        .iter()
        .find(|group| group.section == Section::Servers);
    let group_id = group.map(|group| group.id);
    let first = group.and_then(|group| group.subgroups.first());
    let second = group.and_then(|group| group.subgroups.get(1));
    let first_launcher = first.and_then(|subgroup| subgroup.items.first().map(Launcher::id));
    let open_first = || {
        group_id
            .map(|group_id| {
                vec![
                    Message::SelectDestination(Destination::Group(group_id)),
                    Message::SelectSegment(first.map(|subgroup| subgroup.id)),
                ]
            })
            .unwrap_or_default()
    };
    let with = |mut messages: Vec<Message>, more: Vec<Message>| {
        messages.extend(more);
        messages
    };
    match scene.as_str() {
        "all" => group_id
            .map(|group_id| vec![Message::SelectDestination(Destination::Group(group_id))])
            .unwrap_or_default(),
        "category" => open_first(),
        "add-server" => with(open_first(), vec![Message::AddLauncher]),
        "add-server-all" => group_id
            .map(|group_id| {
                vec![
                    Message::SelectDestination(Destination::Group(group_id)),
                    Message::AddLauncher,
                ]
            })
            .unwrap_or_default(),
        "add-server-advanced" => with(
            open_first(),
            vec![Message::AddLauncher, Message::DraftAdvancedToggled],
        ),
        "reachability" => with(
            open_first(),
            vec![
                Message::AddLauncher,
                Message::DraftFieldChanged(Field::Name, "API server".to_owned()),
                Message::DraftFieldChanged(Field::Host, "localhost".to_owned()),
                Message::DraftFieldChanged(Field::User, "deploy".to_owned()),
                Message::ReachabilityChecked {
                    destination: "localhost:22".to_owned(),
                    outcome: Ok(Reachability::Open {
                        service: Some("SSH-2.0-OpenSSH_9.6".to_owned()),
                    }),
                },
            ],
        ),
        "rename-group" => group_id
            .map(|group_id| vec![Message::RenameGroup(group_id)])
            .unwrap_or_default(),
        "rename-category" => first
            .map(|subgroup| vec![Message::RenameSubgroup(subgroup.id)])
            .unwrap_or_default(),
        "recent" => vec![Message::SelectDestination(Destination::Recent)],
        "settings" => vec![Message::SelectDestination(Destination::Settings)],
        "search" => vec![Message::SearchChanged("a".to_owned())],
        "delete" => first_launcher
            .map(|launcher| {
                with(
                    open_first(),
                    vec![Message::DeleteLauncherRequested(launcher)],
                )
            })
            .unwrap_or_default(),
        "hover" => first_launcher
            .map(|launcher| {
                with(
                    open_first(),
                    vec![
                        Message::SelectTile(launcher),
                        Message::HoverTile(Some(launcher)),
                    ],
                )
            })
            .unwrap_or_default(),
        "drag" => first_launcher
            .zip(second)
            .map(|(launcher, other)| {
                with(
                    open_first(),
                    vec![
                        Message::HoverTile(Some(launcher)),
                        Message::PressTile(launcher),
                        // Over the second category, far enough from the press to count as a drag.
                        Message::DragLifted,
                        Message::HoverSubgroup(Some(other.id)),
                    ],
                )
            })
            .unwrap_or_default(),
        "pinned" => vec![Message::TogglePin],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use despatch_application::{
        Application, ConfigDocument, ConfigFingerprint, ConfigRepository, Dialog,
        EphemeralRecentLaunches, FileRevealer, LaunchRequest, LocalPreferences, PortError,
        PortErrorKind, RepositorySaveExpectation, TerminalLauncher,
    };
    use despatch_domain::{AppConfig, GroupId, Launcher, LauncherId, Section, SubgroupId};

    use crate::editor::Field;
    use crate::presenters::Destination;
    use crate::shortcuts::Shortcut;
    use crate::status_item::PanelEvent;

    use super::{
        AppShell, ContextEntry, ContextTarget, Message, context_entries, duplicate_name, update,
    };

    struct MemoryRepository;

    impl ConfigRepository for MemoryRepository {
        fn load(&self, _: &Path) -> Result<ConfigDocument, PortError> {
            Err(PortError::new(PortErrorKind::NotFound, "not found"))
        }

        fn save(
            &self,
            _: &Path,
            _: &AppConfig,
            _: RepositorySaveExpectation,
        ) -> Result<ConfigFingerprint, PortError> {
            ConfigFingerprint::new("ui-test")
        }
    }

    struct MemoryPreferences;

    impl LocalPreferences for MemoryPreferences {
        fn selected_config_path(&self) -> Result<Option<std::path::PathBuf>, PortError> {
            Ok(None)
        }

        fn set_selected_config_path(&self, _: &Path) -> Result<(), PortError> {
            Ok(())
        }
    }

    /// Records every launch, and fails them all once told to.
    #[derive(Default)]
    struct RecordingTerminal {
        launches: Mutex<usize>,
        refuse: Mutex<bool>,
    }

    impl TerminalLauncher for RecordingTerminal {
        fn launch(&self, _: &LaunchRequest) -> Result<(), PortError> {
            if *self.refuse.lock().expect("terminal lock") {
                return Err(PortError::new(PortErrorKind::Unavailable, "no terminal"));
            }
            *self.launches.lock().expect("terminal lock") += 1;
            Ok(())
        }
    }

    struct NoopRevealer;

    impl FileRevealer for NoopRevealer {
        fn reveal(&self, _: &Path) -> Result<(), PortError> {
            Ok(())
        }
    }

    /// Remembers what it was told, so an appearance can be shown to survive a restart.
    #[derive(Default)]
    struct RecordingPreferences {
        appearance: Mutex<despatch_application::AppearanceChoice>,
    }

    impl LocalPreferences for RecordingPreferences {
        fn selected_config_path(&self) -> Result<Option<std::path::PathBuf>, PortError> {
            Ok(None)
        }

        fn set_selected_config_path(&self, _: &Path) -> Result<(), PortError> {
            Ok(())
        }

        fn appearance(&self) -> Result<despatch_application::AppearanceChoice, PortError> {
            Ok(*self.appearance.lock().expect("appearance lock"))
        }

        fn set_appearance(
            &self,
            choice: despatch_application::AppearanceChoice,
        ) -> Result<(), PortError> {
            *self.appearance.lock().expect("appearance lock") = choice;
            Ok(())
        }
    }

    /// The shell tests never reach the network.
    struct OfflineProbe;

    impl despatch_application::HostProbe for OfflineProbe {
        fn probe(
            &self,
            _: &despatch_application::ReachabilityRequest,
        ) -> Result<despatch_application::Reachability, PortError> {
            Ok(despatch_application::Reachability::TimedOut)
        }
    }

    fn application_with(terminal: Arc<RecordingTerminal>) -> Application {
        let mut application = Application::new(
            Arc::new(MemoryRepository),
            Arc::new(MemoryPreferences),
            terminal,
            Arc::new(NoopRevealer),
            Arc::new(OfflineProbe),
        )
        .with_recents(Arc::new(EphemeralRecentLaunches));
        application
            .create_config(Path::new("/tmp/despatch-ui-test.json"))
            .expect("create config");
        // A created config arrives seeded with the built-in agents. These tests are about the
        // shell rather than the seeding, so they start from nothing and build what they need.
        application.state.config.groups.clear();
        application
    }

    fn application() -> Application {
        application_with(Arc::new(RecordingTerminal::default()))
    }

    fn send(shell: &mut AppShell, message: Message) {
        let _ = update(shell, message);
    }

    fn open_group(shell: &AppShell) -> Option<&despatch_domain::Group> {
        shell.open_group()
    }

    fn selected_subgroup(shell: &AppShell) -> Option<&despatch_domain::Subgroup> {
        let subgroup_id = shell.application.state.selected_subgroup?;
        shell.subgroup(subgroup_id).map(|(_, subgroup)| subgroup)
    }

    /// A shell holding one group with one category in the given section, that category open.
    fn shell_with_category(section: Section, group: &str, category: &str) -> AppShell {
        shell_with_category_and(application(), section, group, category)
    }

    fn shell_with_category_and(
        mut application: Application,
        section: Section,
        group: &str,
        category: &str,
    ) -> AppShell {
        let group_id = application.add_group(group, section).expect("add group");
        let subgroup_id = application
            .add_subgroup(group_id, category)
            .expect("add category");
        assert!(application.state.select_subgroup(subgroup_id));
        AppShell::new(application, None)
    }

    fn tiles(shell: &AppShell) -> &[Launcher] {
        &selected_subgroup(shell).expect("an open category").items
    }

    fn subgroups(shell: &AppShell) -> &[despatch_domain::Subgroup] {
        &shell.application.state.config.groups[0].subgroups
    }

    fn shown(shell: &AppShell) -> Vec<String> {
        shell.rows().into_iter().map(|row| row.tile.name).collect()
    }

    fn add_server(shell: &mut AppShell, name: &str, host: &str) {
        send(shell, Message::AddLauncher);
        send(
            shell,
            Message::DraftFieldChanged(Field::Name, name.to_owned()),
        );
        send(
            shell,
            Message::DraftFieldChanged(Field::Host, host.to_owned()),
        );
        send(
            shell,
            Message::DraftFieldChanged(Field::User, "deploy".to_owned()),
        );
        send(shell, Message::SaveLauncher);
    }

    fn add_group(shell: &mut AppShell, name: &str) -> GroupId {
        send(shell, Message::AddGroup);
        send(shell, Message::NameChanged(name.to_owned()));
        send(shell, Message::SaveName);
        open_group(shell).expect("the new group is open").id
    }

    // ------------------------------------------------------------ the panel

    #[test]
    fn the_accessibility_tree_is_sent_again_only_when_it_changes() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        shell.window_id = Some(iced::window::Id::unique());

        let first = update(&mut shell, Message::HoverTile(None));
        assert!(
            first.units() > 0,
            "the window has not been given a tree yet"
        );

        // Installing reports back through `update`; that report must not install it again, or the
        // app answers its own message forever.
        let report = update(&mut shell, Message::AccessibilityInstalled(Ok(())));
        assert_eq!(report.units(), 0, "an unchanged tree is not sent again");

        let changed = update(&mut shell, Message::SelectDestination(Destination::Recent));
        assert!(
            changed.units() > 0,
            "a different screen sends a different tree"
        );
    }

    #[test]
    fn the_panel_opens_on_the_selected_group_and_its_segment() {
        let shell = shell_with_category(Section::Servers, "Production", "Web");

        assert_eq!(
            shell.destination,
            Some(Destination::Group(open_group(&shell).expect("group").id))
        );
        assert_eq!(shell.segment, Some(subgroups(&shell)[0].id));
    }

    #[test]
    fn with_no_server_group_the_panel_points_nowhere_and_offers_to_create_one() {
        let mut shell = AppShell::new(application(), None);
        assert_eq!(shell.destination, None);

        // The footer's add action creates a group when there is none.
        send(&mut shell, Message::AddLauncher);

        assert!(matches!(
            shell.application.state.dialog,
            Some(Dialog::AddGroup { .. })
        ));
    }

    #[test]
    fn escape_closes_a_sheet_then_a_search_then_the_panel() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        send(&mut shell, Message::Panel(PanelEvent::Open));
        send(&mut shell, Message::SearchChanged("api".to_owned()));
        send(&mut shell, Message::AddLauncher);

        send(&mut shell, Message::Escape);
        assert!(shell.application.state.dialog.is_none(), "the sheet first");
        assert!(shell.panel_visible);

        send(&mut shell, Message::Escape);
        assert!(
            shell.application.state.search_query.is_empty(),
            "then the search"
        );
        assert!(shell.panel_visible);

        send(&mut shell, Message::Escape);
        assert!(!shell.panel_visible, "then the panel itself");
    }

    #[test]
    fn the_menu_bar_item_toggles_the_panel() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");

        send(&mut shell, Message::Panel(PanelEvent::Toggle));
        assert!(shell.panel_visible);
        send(&mut shell, Message::Panel(PanelEvent::Toggle));
        assert!(!shell.panel_visible);
    }

    #[test]
    fn clicking_elsewhere_hides_the_panel_unless_it_is_pinned() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        send(&mut shell, Message::Panel(PanelEvent::Open));

        send(&mut shell, Message::TogglePin);
        send(&mut shell, Message::Panel(PanelEvent::Deactivated));
        assert!(shell.panel_visible, "a pinned panel stays");

        send(&mut shell, Message::TogglePin);
        send(&mut shell, Message::Panel(PanelEvent::Deactivated));
        assert!(!shell.panel_visible);
    }

    #[test]
    fn a_successful_launch_puts_the_panel_away_unless_it_is_pinned() {
        let terminal = Arc::new(RecordingTerminal::default());
        let mut shell = shell_with_category_and(
            application_with(terminal.clone()),
            Section::Servers,
            "Production",
            "Web",
        );
        add_server(&mut shell, "API server", "api.example.com");
        let launcher_id = tiles(&shell)[0].id();

        send(&mut shell, Message::TogglePin);
        send(&mut shell, Message::Panel(PanelEvent::Open));
        send(&mut shell, Message::LaunchTile(launcher_id));
        assert!(shell.panel_visible, "pinned");

        send(&mut shell, Message::TogglePin);
        send(&mut shell, Message::LaunchTile(launcher_id));
        assert!(!shell.panel_visible, "unpinned, the terminal takes over");
        assert_eq!(*terminal.launches.lock().expect("terminal lock"), 2);
    }

    #[test]
    fn a_failed_launch_keeps_the_panel_open_to_say_why() {
        let terminal = Arc::new(RecordingTerminal::default());
        let mut shell = shell_with_category_and(
            application_with(terminal.clone()),
            Section::Servers,
            "Production",
            "Web",
        );
        add_server(&mut shell, "API server", "api.example.com");
        let launcher_id = tiles(&shell)[0].id();
        *terminal.refuse.lock().expect("terminal lock") = true;
        send(&mut shell, Message::Panel(PanelEvent::Open));

        send(&mut shell, Message::LaunchTile(launcher_id));

        assert!(shell.panel_visible);
        assert!(
            crate::presenters::status(&shell.application.state, shell.banner_error())
                .is_some_and(|status| status.is_error)
        );
    }

    #[test]
    fn settings_from_the_menu_bar_item_opens_the_panel_on_settings() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");

        send(&mut shell, Message::Panel(PanelEvent::OpenSettings));

        assert!(shell.panel_visible);
        assert_eq!(shell.destination, Some(Destination::Settings));
    }

    // ------------------------------------------------------------ destinations and segments

    #[test]
    fn selecting_the_open_group_again_keeps_it_open() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        add_server(&mut shell, "API server", "api.example.com");
        let group_id = open_group(&shell).expect("group").id;

        send(
            &mut shell,
            Message::SelectDestination(Destination::Group(group_id)),
        );
        send(
            &mut shell,
            Message::SelectDestination(Destination::Group(group_id)),
        );

        assert_eq!(shell.destination, Some(Destination::Group(group_id)));
        assert_eq!(shown(&shell), ["API server"]);
    }

    #[test]
    fn all_lists_every_category_and_a_segment_narrows_it() {
        let (mut shell, _, source, target) = shell_with_two_categories();
        send(&mut shell, Message::SelectSegment(Some(target)));
        add_server(&mut shell, "Primary", "db-1");

        send(&mut shell, Message::SelectSegment(None));
        assert_eq!(shown(&shell), ["API server", "Primary"]);

        send(&mut shell, Message::SelectSegment(Some(source)));
        assert_eq!(shown(&shell), ["API server"]);
    }

    #[test]
    fn a_group_returns_to_the_segment_it_was_left_on() {
        let (mut shell, _, _, target) = shell_with_two_categories();
        let production = open_group(&shell).expect("group").id;
        send(&mut shell, Message::SelectSegment(Some(target)));
        let staging = add_group(&mut shell, "Staging");
        assert_eq!(shell.segment, None, "a new group opens on All");

        send(
            &mut shell,
            Message::SelectDestination(Destination::Group(production)),
        );

        assert_eq!(shell.segment, Some(target));
        send(
            &mut shell,
            Message::SelectDestination(Destination::Group(staging)),
        );
        assert_eq!(shell.segment, None);
    }

    #[test]
    fn a_segment_from_another_group_is_refused() {
        let (mut shell, _, _, target) = shell_with_two_categories();
        let staging = add_group(&mut shell, "Staging");
        assert_eq!(shell.destination, Some(Destination::Group(staging)));

        send(&mut shell, Message::SelectSegment(Some(target)));

        assert_eq!(shell.segment, None);
    }

    #[test]
    fn the_arrow_keys_walk_the_segments_and_the_rows() {
        let (mut shell, launcher_id, source, target) = shell_with_two_categories();
        send(&mut shell, Message::SelectSegment(None));

        send(&mut shell, Message::Shortcut(Shortcut::NextSegment));
        assert_eq!(shell.segment, Some(source));
        send(&mut shell, Message::Shortcut(Shortcut::NextSegment));
        assert_eq!(shell.segment, Some(target));
        send(&mut shell, Message::Shortcut(Shortcut::NextSegment));
        assert_eq!(shell.segment, None, "wrapping back to All");
        send(&mut shell, Message::Shortcut(Shortcut::PreviousSegment));
        assert_eq!(shell.segment, Some(target));

        send(&mut shell, Message::SelectSegment(None));
        send(&mut shell, Message::Shortcut(Shortcut::Next));
        assert_eq!(shell.application.state.selected_launcher, Some(launcher_id));
        assert_eq!(shell.segment, None, "selecting a row in All stays in All");
    }

    #[test]
    fn command_digits_jump_to_a_group() {
        let (mut shell, _, _, _) = shell_with_two_categories();
        let production = open_group(&shell).expect("group").id;
        let staging = add_group(&mut shell, "Staging");

        send(&mut shell, Message::Shortcut(Shortcut::Group(0)));
        assert_eq!(shell.destination, Some(Destination::Group(production)));
        send(&mut shell, Message::Shortcut(Shortcut::Group(1)));
        assert_eq!(shell.destination, Some(Destination::Group(staging)));
        send(&mut shell, Message::Shortcut(Shortcut::Group(7)));
        assert_eq!(shell.destination, Some(Destination::Group(staging)));
    }

    #[test]
    fn choosing_a_destination_clears_the_search_it_would_otherwise_hide_behind() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        send(&mut shell, Message::SearchChanged("api".to_owned()));

        send(&mut shell, Message::SelectDestination(Destination::Recent));

        assert!(shell.application.state.search_query.is_empty());
    }

    #[test]
    fn launches_show_up_in_recently_used_newest_first() {
        let (mut shell, first, _, target) = shell_with_two_categories();
        send(&mut shell, Message::SelectSegment(Some(target)));
        add_server(&mut shell, "Primary", "db-1");
        let second = tiles(&shell)[0].id();
        send(&mut shell, Message::TogglePin);

        send(&mut shell, Message::LaunchTile(first));
        send(&mut shell, Message::LaunchTile(second));
        send(&mut shell, Message::SelectDestination(Destination::Recent));

        assert_eq!(shown(&shell), ["Primary", "API server"]);
        let rows = shell.rows();
        assert_eq!(rows[0].trailing.as_deref(), Some("Just now"));

        send(&mut shell, Message::ClearRecents);
        assert!(shell.rows().is_empty());
    }

    #[test]
    fn return_in_the_search_field_opens_the_first_match() {
        let terminal = Arc::new(RecordingTerminal::default());
        let mut shell = shell_with_category_and(
            application_with(terminal.clone()),
            Section::Servers,
            "Production",
            "Web",
        );
        add_server(&mut shell, "API server", "api.example.com");
        add_server(&mut shell, "Cache", "cache.example.com");
        let cache = tiles(&shell)[1].id();
        shell.application.state.selected_launcher = None;

        send(&mut shell, Message::SearchChanged("cache".to_owned()));
        send(&mut shell, Message::SearchSubmitted);

        assert_eq!(*terminal.launches.lock().expect("terminal lock"), 1);
        assert_eq!(shell.application.state.selected_launcher, Some(cache));
    }

    #[test]
    fn deleting_the_open_group_moves_the_panel_to_the_next_one() {
        let (mut shell, _, _, _) = shell_with_two_categories();
        let production = open_group(&shell).expect("group").id;
        let staging = add_group(&mut shell, "Staging");
        send(
            &mut shell,
            Message::SelectDestination(Destination::Group(production)),
        );

        send(&mut shell, Message::DeleteGroupRequested(production));
        send(&mut shell, Message::ConfirmDelete);

        assert_eq!(shell.destination, Some(Destination::Group(staging)));
        assert_eq!(shell.segment, None);
    }

    // ------------------------------------------------------------ launchers

    #[test]
    fn adding_a_tile_stores_it_in_the_open_category_and_selects_it() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");

        add_server(&mut shell, "API server", "api.example.com");

        assert_eq!(tiles(&shell).len(), 1);
        let Launcher::Ssh(ssh) = &tiles(&shell)[0] else {
            panic!("expected an SSH launcher");
        };
        assert_eq!(ssh.name, "API server");
        assert_eq!(ssh.host, "api.example.com");
        assert_eq!(
            shell.application.state.selected_launcher,
            Some(tiles(&shell)[0].id())
        );
        assert!(shell.application.state.dialog.is_none());
    }

    #[test]
    fn adding_from_all_offers_the_categories_and_saves_into_the_chosen_one() {
        let (mut shell, _, source, target) = shell_with_two_categories();
        send(&mut shell, Message::SelectSegment(None));

        send(&mut shell, Message::AddLauncher);
        let draft = shell.launcher_draft.as_ref().expect("an editor");
        assert_eq!(draft.categories.len(), 2);
        assert_eq!(
            draft.subgroup_id, source,
            "the first category is the default"
        );

        send(&mut shell, Message::DraftCategorySelected(target));
        send(
            &mut shell,
            Message::DraftFieldChanged(Field::Name, "Primary".to_owned()),
        );
        send(
            &mut shell,
            Message::DraftFieldChanged(Field::Host, "db-1".to_owned()),
        );
        send(
            &mut shell,
            Message::DraftFieldChanged(Field::User, "deploy".to_owned()),
        );
        send(&mut shell, Message::SaveLauncher);

        assert_eq!(category_names(&shell, target), ["Primary"]);
        assert_eq!(shell.segment, None, "All stays open, and shows it");
        assert_eq!(shown(&shell), ["API server", "Primary"]);
    }

    #[test]
    fn a_group_without_categories_asks_for_one_before_a_server() {
        let mut shell = AppShell::new(application(), None);
        add_group(&mut shell, "Production");

        send(&mut shell, Message::AddLauncher);

        assert!(matches!(
            shell.application.state.dialog,
            Some(Dialog::AddSubgroup { .. })
        ));
    }

    #[test]
    fn choosing_another_category_in_the_editor_moves_the_launcher_and_follows_it() {
        let (mut shell, launcher_id, source, target) = shell_with_two_categories();

        send(&mut shell, Message::EditLauncher(launcher_id));
        send(&mut shell, Message::DraftCategorySelected(target));
        send(&mut shell, Message::SaveLauncher);

        assert!(category_names(&shell, source).is_empty());
        assert_eq!(category_names(&shell, target), ["API server"]);
        assert_eq!(shell.segment, Some(target));
        assert_eq!(shell.application.state.selected_launcher, Some(launcher_id));
    }

    #[test]
    fn an_incomplete_tile_keeps_the_editor_open_and_reports_the_offending_field() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");

        send(&mut shell, Message::AddLauncher);
        send(
            &mut shell,
            Message::DraftFieldChanged(Field::Name, "API".to_owned()),
        );
        send(&mut shell, Message::SaveLauncher);

        assert!(shell.application.state.dialog.is_some());
        assert_eq!(
            shell
                .launcher_draft
                .as_ref()
                .expect("draft stays open")
                .field_error(Field::Host),
            Some("Enter a hostname or IP address.")
        );
        assert!(tiles(&shell).is_empty(), "nothing may be persisted");
    }

    #[test]
    fn a_duplicate_tile_name_is_reported_without_losing_the_entered_values() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        add_server(&mut shell, "API server", "api.example.com");

        send(&mut shell, Message::AddLauncher);
        send(
            &mut shell,
            Message::DraftFieldChanged(Field::Name, "api server".to_owned()),
        );
        send(
            &mut shell,
            Message::DraftFieldChanged(Field::Host, "second.example.com".to_owned()),
        );
        send(
            &mut shell,
            Message::DraftFieldChanged(Field::User, "deploy".to_owned()),
        );
        send(&mut shell, Message::SaveLauncher);

        let draft = shell.launcher_draft.as_ref().expect("draft stays open");
        assert!(draft.general_error().is_some());
        assert_eq!(draft.host, "second.example.com");
        assert_eq!(tiles(&shell).len(), 1);
    }

    #[test]
    fn editing_a_tile_replaces_it_in_place_and_keeps_its_identifier() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        add_server(&mut shell, "API server", "api.example.com");
        let launcher_id = tiles(&shell)[0].id();

        send(&mut shell, Message::EditLauncher(launcher_id));
        assert_eq!(
            shell.launcher_draft.as_ref().expect("draft").name,
            "API server"
        );
        send(
            &mut shell,
            Message::DraftFieldChanged(Field::Host, "api2.example.com".to_owned()),
        );
        send(
            &mut shell,
            Message::DraftFieldChanged(Field::Port, "2222".to_owned()),
        );
        send(&mut shell, Message::SaveLauncher);

        assert_eq!(tiles(&shell).len(), 1);
        assert_eq!(tiles(&shell)[0].id(), launcher_id);
        let Launcher::Ssh(ssh) = &tiles(&shell)[0] else {
            panic!("expected an SSH launcher");
        };
        assert_eq!(ssh.host, "api2.example.com");
        assert_eq!(ssh.port.get(), 2222);
        assert!(shell.application.state.dialog.is_none());
    }

    #[test]
    fn deleting_a_tile_asks_first_and_then_removes_only_that_tile() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        add_server(&mut shell, "API server", "api.example.com");
        add_server(&mut shell, "Cache", "cache.example.com");
        let doomed = tiles(&shell)[0].id();
        let survivor = tiles(&shell)[1].id();

        send(&mut shell, Message::DeleteLauncherRequested(doomed));
        assert_eq!(
            shell.application.state.dialog,
            Some(Dialog::DeleteLauncher {
                launcher_id: doomed
            })
        );
        assert_eq!(
            tiles(&shell).len(),
            2,
            "nothing is removed before confirming"
        );

        send(&mut shell, Message::ConfirmDelete);

        assert_eq!(tiles(&shell).len(), 1);
        assert_eq!(tiles(&shell)[0].id(), survivor);
        assert!(shell.application.state.dialog.is_none());
        assert_eq!(shell.application.state.selected_launcher, None);
    }

    #[test]
    fn dismissing_a_delete_confirmation_keeps_the_tile() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        add_server(&mut shell, "API server", "api.example.com");
        let launcher_id = tiles(&shell)[0].id();

        send(&mut shell, Message::DeleteLauncherRequested(launcher_id));
        send(&mut shell, Message::DismissDialog);

        assert_eq!(tiles(&shell).len(), 1);
        assert!(shell.application.state.dialog.is_none());
    }

    #[test]
    fn duplicating_a_tile_copies_its_settings_under_a_free_name() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        add_server(&mut shell, "API server", "api.example.com");
        let original = tiles(&shell)[0].id();

        send(&mut shell, Message::DuplicateLauncher(original));
        send(&mut shell, Message::DuplicateLauncher(original));

        let names: Vec<_> = tiles(&shell)
            .iter()
            .map(|launcher| launcher.name().to_owned())
            .collect();
        assert_eq!(
            names,
            ["API server", "API server copy", "API server copy 2"]
        );
        let Launcher::Ssh(copy) = &tiles(&shell)[1] else {
            panic!("expected an SSH launcher");
        };
        assert_eq!(copy.host, "api.example.com");
        assert_ne!(tiles(&shell)[1].id(), original);
    }

    #[test]
    fn duplicate_names_skip_every_taken_variant_case_insensitively() {
        let taken = vec![
            "API server".to_owned(),
            "api server copy".to_owned(),
            "API SERVER COPY 2".to_owned(),
        ];

        assert_eq!(duplicate_name(&taken, "API server"), "API server copy 3");
        assert_eq!(duplicate_name(&[], "Cache"), "Cache copy");
    }

    #[test]
    fn groups_can_be_created_renamed_and_deleted_with_everything_inside_them() {
        let mut shell = AppShell::new(application(), None);

        let group_id = add_group(&mut shell, "Prod");
        send(&mut shell, Message::AddSubgroup);
        send(&mut shell, Message::NameChanged("Web".to_owned()));
        send(&mut shell, Message::SaveName);
        add_server(&mut shell, "API server", "api.example.com");

        send(&mut shell, Message::RenameGroup(group_id));
        send(&mut shell, Message::NameChanged("Production".to_owned()));
        send(&mut shell, Message::SaveName);
        assert_eq!(open_group(&shell).expect("group").name, "Production");

        send(&mut shell, Message::DeleteGroupRequested(group_id));
        send(&mut shell, Message::ConfirmDelete);
        assert!(shell.application.state.config.groups.is_empty());
        assert_eq!(shell.destination, None);
        assert_eq!(shell.segment, None);
    }

    #[test]
    fn a_group_keeps_the_label_color_it_was_created_with_and_can_change_or_drop_it() {
        use despatch_domain::LabelColor;

        let blue = LabelColor::from_rgb(0x00, 0x7A, 0xFF);
        let green = LabelColor::from_rgb(0x34, 0xC7, 0x59);
        let mut shell = AppShell::new(application(), None);

        send(&mut shell, Message::AddGroup);
        send(&mut shell, Message::NameChanged("Prod".to_owned()));
        send(&mut shell, Message::NameColorSelected(blue));
        send(&mut shell, Message::SaveName);
        let group_id = open_group(&shell).expect("group").id;
        assert_eq!(open_group(&shell).expect("group").color, Some(blue));

        send(&mut shell, Message::RenameGroup(group_id));
        assert_eq!(
            shell.name_draft.as_ref().expect("draft").selected_color(),
            Some(blue),
            "the editor opens on the color the group already has"
        );
        send(&mut shell, Message::NameColorSelected(green));
        send(&mut shell, Message::SaveName);
        assert_eq!(open_group(&shell).expect("group").color, Some(green));
        assert_eq!(open_group(&shell).expect("group").name, "Prod");

        send(&mut shell, Message::RenameGroup(group_id));
        send(&mut shell, Message::NameColorCleared);
        send(&mut shell, Message::SaveName);
        assert_eq!(open_group(&shell).expect("group").color, None);
    }

    #[test]
    fn a_category_is_never_given_a_color_of_its_own() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        let subgroup_id = subgroups(&shell)[0].id;

        send(&mut shell, Message::RenameSubgroup(subgroup_id));
        let draft = shell.name_draft.as_ref().expect("draft");

        assert!(
            !draft.has_color(),
            "a category editor must not offer a color it cannot keep"
        );
        assert_eq!(draft.build().expect("valid draft").color, None);
    }

    #[test]
    fn a_launcher_inherits_its_groups_color_until_it_is_given_one_of_its_own() {
        use despatch_domain::LabelColor;

        let yellow = LabelColor::from_rgb(0xFF, 0xCC, 0x00);
        let green = LabelColor::from_rgb(0x34, 0xC7, 0x59);
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        let group_id = open_group(&shell).expect("group").id;

        send(&mut shell, Message::RenameGroup(group_id));
        send(&mut shell, Message::NameColorSelected(yellow));
        send(&mut shell, Message::SaveName);
        add_server(&mut shell, "API server", "api.example.com");
        let drawn = |shell: &AppShell| shell.rows()[0].tile.color;

        assert_eq!(
            tiles(&shell)[0].color(),
            None,
            "an inherited color is not copied onto the launcher"
        );
        assert_eq!(
            drawn(&shell),
            Some(yellow),
            "but it is what the row is drawn in"
        );

        let launcher_id = tiles(&shell)[0].id();
        send(&mut shell, Message::EditLauncher(launcher_id));
        send(&mut shell, Message::DraftColorSelected(green));
        send(&mut shell, Message::SaveLauncher);
        assert_eq!(tiles(&shell)[0].color(), Some(green));
        assert_eq!(
            drawn(&shell),
            Some(green),
            "an override wins over the group"
        );

        send(&mut shell, Message::EditLauncher(launcher_id));
        send(&mut shell, Message::DraftColorCleared);
        send(&mut shell, Message::SaveLauncher);
        assert_eq!(tiles(&shell)[0].color(), None);
        assert_eq!(
            drawn(&shell),
            Some(yellow),
            "clearing the override falls back to the group"
        );
    }

    #[test]
    fn the_additional_settings_of_a_server_are_a_disclosure_the_user_opens() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");

        send(&mut shell, Message::AddLauncher);
        assert!(
            !shell.launcher_draft.as_ref().expect("draft").advanced,
            "a new server starts on name, host, and username alone"
        );

        send(&mut shell, Message::DraftAdvancedToggled);
        assert!(shell.launcher_draft.as_ref().expect("draft").advanced);

        send(&mut shell, Message::DraftAdvancedToggled);
        assert!(!shell.launcher_draft.as_ref().expect("draft").advanced);
    }

    #[test]
    fn choosing_an_appearance_repaints_the_app_and_is_remembered() {
        use despatch_application::AppearanceChoice;

        let preferences = Arc::new(RecordingPreferences::default());
        let mut application = Application::new(
            Arc::new(MemoryRepository),
            preferences.clone(),
            Arc::new(RecordingTerminal::default()),
            Arc::new(NoopRevealer),
            Arc::new(OfflineProbe),
        );
        application
            .create_config(Path::new("/tmp/despatch-appearance-test.json"))
            .expect("create config");
        let mut shell = AppShell::new(application, None);

        send(
            &mut shell,
            Message::AppearanceSelected(AppearanceChoice::Dark),
        );
        assert_eq!(shell.appearance, AppearanceChoice::Dark);
        assert!(crate::theme::is_dark(&shell.theme()));

        send(
            &mut shell,
            Message::AppearanceSelected(AppearanceChoice::Light),
        );
        assert_eq!(shell.appearance, AppearanceChoice::Light);
        assert!(!crate::theme::is_dark(&shell.theme()));

        assert_eq!(
            preferences.appearance().expect("stored appearance"),
            AppearanceChoice::Light,
            "the choice outlives the panel"
        );
    }

    fn shell_with_two_categories() -> (AppShell, LauncherId, SubgroupId, SubgroupId) {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        let source = shell.segment.expect("an open category");
        let group_id = open_group(&shell).expect("a group").id;
        let target = shell
            .application
            .add_subgroup(group_id, "Databases")
            .expect("add category");
        add_server(&mut shell, "API server", "api.example.com");
        let launcher_id = tiles(&shell)[0].id();
        (shell, launcher_id, source, target)
    }

    /// Presses on a row, carries it over a segment, and lets go — the whole gesture.
    fn drag_onto(shell: &mut AppShell, launcher_id: LauncherId, target: Option<SubgroupId>) {
        send(shell, Message::HoverTile(Some(launcher_id)));
        send(shell, Message::PressTile(launcher_id));
        send(shell, Message::DragLifted);
        send(shell, Message::HoverSubgroup(target));
        send(shell, Message::PointerReleased);
    }

    fn category_names(shell: &AppShell, subgroup_id: SubgroupId) -> Vec<String> {
        shell
            .subgroup(subgroup_id)
            .expect("the category")
            .1
            .items
            .iter()
            .map(|launcher| launcher.name().to_owned())
            .collect()
    }

    #[test]
    fn dragging_a_row_onto_another_category_moves_the_launcher_into_it() {
        let (mut shell, launcher_id, source, target) = shell_with_two_categories();

        drag_onto(&mut shell, launcher_id, Some(target));

        assert!(category_names(&shell, source).is_empty(), "it left");
        assert_eq!(
            category_names(&shell, target),
            ["API server"],
            "and arrived"
        );
        assert_eq!(shell.drag, None, "the gesture is over");
        assert_eq!(
            shell.segment,
            Some(target),
            "the list follows the launcher to where it landed"
        );
        assert_eq!(shell.application.state.selected_launcher, Some(launcher_id));
    }

    #[test]
    fn a_drop_while_all_is_open_keeps_all_open() {
        let (mut shell, launcher_id, _, target) = shell_with_two_categories();
        send(&mut shell, Message::SelectSegment(None));

        drag_onto(&mut shell, launcher_id, Some(target));

        assert_eq!(category_names(&shell, target), ["API server"]);
        assert_eq!(shell.segment, None);
    }

    #[test]
    fn a_drop_on_the_category_it_already_lives_in_changes_nothing() {
        let (mut shell, launcher_id, source, _) = shell_with_two_categories();

        drag_onto(&mut shell, launcher_id, Some(source));

        assert_eq!(category_names(&shell, source), ["API server"]);
        assert_eq!(shell.action_error, None, "and it is not an error");
    }

    #[test]
    fn letting_go_away_from_every_category_abandons_the_drag() {
        let (mut shell, launcher_id, source, target) = shell_with_two_categories();

        drag_onto(&mut shell, launcher_id, None);

        assert_eq!(category_names(&shell, source), ["API server"], "it stayed");
        assert!(category_names(&shell, target).is_empty());
        assert_eq!(shell.drag, None);
    }

    #[test]
    fn a_row_only_leaves_its_place_once_it_has_been_lifted() {
        let (mut shell, launcher_id, source, target) = shell_with_two_categories();

        send(&mut shell, Message::PressTile(launcher_id));
        assert_eq!(
            shell.drag.map(|drag| drag.lifted),
            Some(false),
            "a press alone is still a click"
        );

        // How far the pointer must travel before that message is sent is the drag layer's own
        // business; see its tests.
        send(&mut shell, Message::DragLifted);
        assert_eq!(shell.drag.map(|drag| drag.lifted), Some(true), "now it is");

        send(&mut shell, Message::HoverSubgroup(Some(target)));
        send(&mut shell, Message::PointerReleased);
        assert_eq!(category_names(&shell, target), ["API server"]);
        assert!(category_names(&shell, source).is_empty());
    }

    #[test]
    fn one_click_selects_a_row_and_two_launch_it() {
        let (mut shell, launcher_id, _, _) = shell_with_two_categories();

        send(&mut shell, Message::PressTile(launcher_id));
        send(&mut shell, Message::PointerReleased);
        assert_eq!(
            shell.application.state.selected_launcher,
            Some(launcher_id),
            "one click highlights"
        );
        assert!(
            shell.application.state.last_launch.is_none(),
            "and only highlights: {:?}",
            shell.application.state.last_launch
        );

        send(&mut shell, Message::PressTile(launcher_id));
        send(&mut shell, Message::LaunchTile(launcher_id));
        send(&mut shell, Message::PointerReleased);
        assert!(
            matches!(
                shell.application.state.last_launch,
                Some(despatch_application::LaunchOutcome::Success { .. })
            ),
            "two clicks run it: {:?}",
            shell.application.state.last_launch
        );
    }

    #[test]
    fn a_press_picks_up_the_row_that_reported_it_whatever_was_last_hovered() {
        let (mut shell, launcher_id, source, target) = shell_with_two_categories();

        send(&mut shell, Message::HoverTile(Some(LauncherId::new())));
        send(&mut shell, Message::PressTile(launcher_id));

        assert_eq!(
            shell.drag.map(|drag| drag.launcher_id),
            Some(launcher_id),
            "the pressed row is the one being carried"
        );

        send(&mut shell, Message::DragLifted);
        send(&mut shell, Message::HoverSubgroup(Some(target)));
        send(&mut shell, Message::PointerReleased);
        assert_eq!(category_names(&shell, target), ["API server"]);
        assert!(category_names(&shell, source).is_empty());
    }

    #[test]
    fn a_click_that_never_moved_drops_nothing_anywhere() {
        let (mut shell, launcher_id, source, target) = shell_with_two_categories();

        send(&mut shell, Message::PressTile(launcher_id));
        send(&mut shell, Message::HoverSubgroup(Some(target)));
        send(&mut shell, Message::PointerReleased);

        assert_eq!(category_names(&shell, source), ["API server"]);
        assert!(category_names(&shell, target).is_empty());
    }

    #[test]
    fn the_pointer_leaving_the_panel_puts_the_row_back() {
        let (mut shell, launcher_id, source, target) = shell_with_two_categories();
        send(&mut shell, Message::PressTile(launcher_id));
        send(&mut shell, Message::DragLifted);
        send(&mut shell, Message::HoverSubgroup(Some(target)));

        send(&mut shell, Message::PointerCancelled);
        send(&mut shell, Message::PointerReleased);

        assert_eq!(shell.drag, None);
        assert_eq!(category_names(&shell, source), ["API server"]);
        assert!(category_names(&shell, target).is_empty());
    }

    #[test]
    fn escape_puts_a_held_row_back_before_anything_else() {
        let (mut shell, launcher_id, source, target) = shell_with_two_categories();
        send(&mut shell, Message::Panel(PanelEvent::Open));
        send(&mut shell, Message::PressTile(launcher_id));
        send(&mut shell, Message::DragLifted);
        send(&mut shell, Message::HoverSubgroup(Some(target)));

        send(&mut shell, Message::Escape);
        send(&mut shell, Message::PointerReleased);

        assert!(shell.panel_visible);
        assert_eq!(category_names(&shell, source), ["API server"]);
    }

    #[test]
    fn a_dialog_holds_off_dragging() {
        let (mut shell, launcher_id, source, target) = shell_with_two_categories();
        send(&mut shell, Message::EditLauncher(launcher_id));

        send(&mut shell, Message::PressTile(launcher_id));

        assert_eq!(shell.drag, None, "the sheet owns the pointer");
        send(&mut shell, Message::HoverSubgroup(Some(target)));
        send(&mut shell, Message::PointerReleased);
        assert_eq!(category_names(&shell, source), ["API server"]);
    }

    fn server_group_names(shell: &AppShell) -> Vec<String> {
        shell
            .application
            .state
            .config
            .groups
            .iter()
            .filter(|group| group.section == Section::Servers)
            .map(|group| group.name.clone())
            .collect()
    }

    /// Presses a group's row, carries it over another row, and lets go.
    fn drag_group_onto(shell: &mut AppShell, group: GroupId, target: Option<GroupId>) {
        send(shell, Message::PressGroup(group));
        send(shell, Message::GroupDragLifted);
        send(shell, Message::HoverGroup(target));
        send(shell, Message::PointerReleased);
    }

    #[test]
    fn dragging_a_group_onto_another_takes_its_place() {
        let mut shell = AppShell::new(application(), None);
        let production = add_group(&mut shell, "Production");
        let staging = add_group(&mut shell, "Staging");
        let lab = add_group(&mut shell, "Home lab");

        drag_group_onto(&mut shell, lab, Some(production));
        assert_eq!(
            server_group_names(&shell),
            ["Home lab", "Production", "Staging"]
        );
        assert_eq!(
            shell.destination,
            Some(Destination::Group(lab)),
            "the press opened it"
        );

        drag_group_onto(&mut shell, lab, Some(staging));
        assert_eq!(
            server_group_names(&shell),
            ["Production", "Staging", "Home lab"]
        );
        assert_eq!(shell.group_drag, None);
    }

    #[test]
    fn a_group_press_that_never_moved_only_opens_it() {
        let mut shell = AppShell::new(application(), None);
        let production = add_group(&mut shell, "Production");
        let staging = add_group(&mut shell, "Staging");

        send(&mut shell, Message::PressGroup(production));
        send(&mut shell, Message::HoverGroup(Some(staging)));
        send(&mut shell, Message::PointerReleased);

        assert_eq!(server_group_names(&shell), ["Production", "Staging"]);
        assert_eq!(shell.destination, Some(Destination::Group(production)));
    }

    #[test]
    fn a_group_dropped_on_itself_or_nowhere_stays_put() {
        let mut shell = AppShell::new(application(), None);
        let production = add_group(&mut shell, "Production");
        add_group(&mut shell, "Staging");

        drag_group_onto(&mut shell, production, Some(production));
        drag_group_onto(&mut shell, production, None);

        assert_eq!(server_group_names(&shell), ["Production", "Staging"]);
        assert_eq!(shell.action_error, None);
    }

    #[test]
    fn escape_puts_a_held_group_back() {
        let mut shell = AppShell::new(application(), None);
        let production = add_group(&mut shell, "Production");
        let staging = add_group(&mut shell, "Staging");
        send(&mut shell, Message::Panel(PanelEvent::Open));
        send(&mut shell, Message::PressGroup(production));
        send(&mut shell, Message::GroupDragLifted);
        send(&mut shell, Message::HoverGroup(Some(staging)));

        send(&mut shell, Message::Escape);
        send(&mut shell, Message::PointerReleased);

        assert_eq!(server_group_names(&shell), ["Production", "Staging"]);
        assert!(
            shell.panel_visible,
            "Escape put the group back rather than closing the panel"
        );
    }

    fn entry_titles(target: ContextTarget) -> Vec<&'static str> {
        context_entries(target)
            .into_iter()
            .map(|entry| match entry {
                ContextEntry::Action(title, _) => title,
                ContextEntry::Separator => "—",
            })
            .collect()
    }

    #[test]
    fn a_right_click_offers_the_actions_of_what_it_landed_on() {
        let launcher = LauncherId::new();
        assert_eq!(
            entry_titles(ContextTarget::Launcher(launcher)),
            ["Open", "—", "Edit…", "Duplicate", "—", "Delete…"]
        );
        assert_eq!(
            entry_titles(ContextTarget::Group(GroupId::new())),
            ["Rename…", "—", "Delete Group…"]
        );
        assert_eq!(
            entry_titles(ContextTarget::Category(SubgroupId::new())),
            ["Rename…", "—", "Delete Category…"]
        );
        // Each line sends the same message as the control it stands in for.
        let ContextEntry::Action(_, Message::DeleteLauncherRequested(target)) =
            context_entries(ContextTarget::Launcher(launcher)).remove(5)
        else {
            panic!("the last line of a launcher's menu deletes it");
        };
        assert_eq!(target, launcher);
    }

    #[test]
    fn entering_a_row_before_the_previous_one_reports_leaving_keeps_the_new_hover() {
        let (mut shell, _, source, target) = shell_with_two_categories();

        // Moving up or left, the newly entered widget is told first.
        send(&mut shell, Message::HoverSubgroup(Some(target)));
        send(&mut shell, Message::HoverSubgroup(Some(source)));
        send(&mut shell, Message::LeaveSubgroup(target));
        assert_eq!(shell.hovered_subgroup, Some(source));

        send(&mut shell, Message::LeaveSubgroup(source));
        assert_eq!(shell.hovered_subgroup, None);
    }

    #[test]
    fn dragging_a_group_upward_lands_despite_the_order_rows_report_in() {
        let mut shell = AppShell::new(application(), None);
        let production = add_group(&mut shell, "Production");
        let staging = add_group(&mut shell, "Staging");
        let lab = add_group(&mut shell, "Home lab");

        send(&mut shell, Message::PressGroup(lab));
        send(&mut shell, Message::GroupDragLifted);
        send(&mut shell, Message::HoverGroup(Some(staging)));
        send(&mut shell, Message::HoverGroup(Some(production)));
        send(&mut shell, Message::LeaveGroup(staging));
        send(&mut shell, Message::PointerReleased);

        assert_eq!(
            server_group_names(&shell),
            ["Home lab", "Production", "Staging"]
        );
    }

    #[test]
    fn a_press_from_voiceover_does_what_its_element_was_announced_as() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        shell.window_id = Some(iced::window::Id::unique());
        let _ = update(&mut shell, Message::HoverTile(None));
        let installed = shell
            .installed_accessibility
            .clone()
            .expect("a tree was sent");
        let recent = installed
            .iter()
            .position(|node| node.label.starts_with("Recently used"))
            .expect("a Recently used element");

        send(
            &mut shell,
            Message::Panel(PanelEvent::AccessibilityPress(recent)),
        );
        assert_eq!(shell.destination, Some(Destination::Recent));

        // An index from a tree that has since changed still means what was announced.
        let pin = installed
            .iter()
            .position(|node| node.label == "Keep open")
            .expect("the pin");
        shell.installed_accessibility = Some(installed);
        send(
            &mut shell,
            Message::Panel(PanelEvent::AccessibilityPress(pin)),
        );
        assert!(shell.pinned);

        // A heading, or an index past the end, does nothing.
        send(
            &mut shell,
            Message::Panel(PanelEvent::AccessibilityPress(9_999)),
        );
        assert!(shell.pinned);
    }

    // ------------------------------------------------------------ status

    #[test]
    fn a_launch_result_expires_but_only_the_one_it_was_started_for() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        add_server(&mut shell, "API server", "api.example.com");
        let launcher_id = tiles(&shell)[0].id();

        send(&mut shell, Message::LaunchTile(launcher_id));
        assert!(
            crate::presenters::status(&shell.application.state, shell.banner_error()).is_some(),
            "a launch says so"
        );
        let token = shell.status_token;

        send(&mut shell, Message::DismissStatus(token.wrapping_sub(1)));
        assert!(
            crate::presenters::status(&shell.application.state, shell.banner_error()).is_some(),
            "a stale dismissal is ignored"
        );

        send(&mut shell, Message::DismissStatus(token));
        assert!(
            crate::presenters::status(&shell.application.state, shell.banner_error()).is_none(),
            "its own dismissal clears it"
        );
    }

    #[test]
    fn a_config_state_is_not_treated_as_an_expiring_event() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        shell.application.state.config_status = despatch_application::ConfigStatus::Conflict;

        let token = shell.status_token;
        send(&mut shell, Message::DismissStatus(token));

        assert!(
            crate::presenters::status(&shell.application.state, shell.banner_error()).is_some(),
            "a conflict is still true, so it stays on screen"
        );
    }

    // ------------------------------------------------------------ names

    #[test]
    fn a_blank_name_keeps_the_editor_open() {
        let mut shell = AppShell::new(application(), None);

        send(&mut shell, Message::AddGroup);
        send(&mut shell, Message::NameChanged("   ".to_owned()));
        send(&mut shell, Message::SaveName);

        assert!(shell.application.state.config.groups.is_empty());
        assert!(shell.name_draft.is_some());
    }

    #[test]
    fn categories_can_be_created_renamed_and_deleted_inside_a_group() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        add_server(&mut shell, "API server", "api.example.com");
        let subgroup_id = subgroups(&shell)[0].id;

        send(&mut shell, Message::AddSubgroup);
        send(&mut shell, Message::NameChanged("Databases".to_owned()));
        send(&mut shell, Message::SaveName);
        assert_eq!(subgroups(&shell).len(), 2);
        assert_eq!(
            shell.segment,
            Some(subgroups(&shell)[1].id),
            "a new category opens straight away"
        );

        send(&mut shell, Message::RenameSubgroup(subgroup_id));
        send(&mut shell, Message::NameChanged("Web tier".to_owned()));
        send(&mut shell, Message::SaveName);
        assert_eq!(subgroups(&shell)[0].name, "Web tier");
        assert_eq!(
            subgroups(&shell)[0].items.len(),
            1,
            "renaming keeps the launchers"
        );

        send(&mut shell, Message::SelectSegment(Some(subgroup_id)));
        send(&mut shell, Message::DeleteSubgroupRequested(subgroup_id));
        assert_eq!(subgroups(&shell).len(), 2, "nothing goes before confirming");
        send(&mut shell, Message::ConfirmDelete);
        assert_eq!(subgroups(&shell).len(), 1);
        assert_eq!(subgroups(&shell)[0].name, "Databases");
        assert_eq!(shell.segment, None, "a deleted segment falls back to All");
    }

    #[test]
    fn a_duplicate_category_name_keeps_the_editor_open() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");

        send(&mut shell, Message::AddSubgroup);
        send(&mut shell, Message::NameChanged("web".to_owned()));
        send(&mut shell, Message::SaveName);

        assert_eq!(subgroups(&shell).len(), 1);
        let draft = shell.name_draft.as_ref().expect("draft stays open");
        assert!(draft.error.is_some());
        assert_eq!(draft.name, "web");
    }

    #[test]
    fn hovering_a_row_exposes_its_actions_without_changing_the_selection() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        add_server(&mut shell, "API server", "api.example.com");
        let launcher_id = tiles(&shell)[0].id();
        send(&mut shell, Message::SelectTile(launcher_id));

        send(&mut shell, Message::HoverTile(None));
        assert_eq!(shell.hovered_tile, None);
        assert_eq!(shell.application.state.selected_launcher, Some(launcher_id));

        send(&mut shell, Message::HoverTile(Some(launcher_id)));
        assert_eq!(shell.hovered_tile, Some(launcher_id));
    }

    #[test]
    fn search_reaches_across_groups_and_can_be_cleared() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        add_server(&mut shell, "API server", "api.example.com");
        add_server(&mut shell, "Cache", "cache.example.com");
        add_group(&mut shell, "Staging");

        send(&mut shell, Message::SearchChanged("cache".to_owned()));
        assert_eq!(shown(&shell), ["Cache"], "found from inside another group");

        send(&mut shell, Message::ClearSearch);
        assert!(shown(&shell).is_empty(), "back to the empty Staging group");
    }
}
