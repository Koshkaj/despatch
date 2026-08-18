use despatch_application::{AppearanceChoice, Application, Dialog, Reachability};
use despatch_domain::{
    AgentProvider, Group, GroupId, LabelColor, Launcher, LauncherId, Section, Subgroup, SubgroupId,
};
use iced::keyboard;
use iced::widget::text_editor;
use iced::{Subscription, Task, Theme};

use crate::editor::{Field, LauncherDraft, NameDraft, NameTarget, ReachabilityState, Target};
use crate::icons::{IconSet, Tint};
use crate::shortcuts::{self, Shortcut};
use crate::theme::Appearance;
use crate::{accessibility, accessibility::Node};

pub const SEARCH_ID: &str = "launcher-search";
pub const GROUP_NAME_ID: &str = "group-name";
pub const LAUNCHER_NAME_ID: &str = "launcher-name";

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

pub struct AppShell {
    pub application: Application,
    pub startup_error: Option<String>,
    pub action_error: Option<String>,
    pub icons: IconSet,
    pub name_draft: Option<NameDraft>,
    pub launcher_draft: Option<LauncherDraft>,
    pub hovered_tile: Option<LauncherId>,
    pub hovered_group: Option<GroupId>,
    /// The category under the pointer, which is the drop target while a launcher is being dragged.
    pub hovered_subgroup: Option<SubgroupId>,
    /// The launcher currently held by the pointer, if the press landed on a card.
    pub drag: Option<Drag>,
    pub appearance: AppearanceChoice,
    /// Identifies the status banner currently on screen, so a dismissal that arrives after the
    /// banner was replaced is recognised as stale and ignored.
    status_token: u64,
    /// What the banner is showing, used only to notice when it becomes something else.
    status_fingerprint: Option<String>,
    theme: Theme,
    accessibility_nodes: Vec<Node>,
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
        let theme = appearance.theme();
        let accessibility_nodes = accessibility::model(&application.state);
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
            appearance: choice,
            status_token: 0,
            status_fingerprint: None,
            theme,
            accessibility_nodes,
            window_id: None,
        };
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

    /// The banner text shown under the toolbar, if anything needs the user's attention.
    #[must_use]
    pub fn banner_error(&self) -> Option<&str> {
        self.action_error
            .as_deref()
            .or(self.startup_error.as_deref())
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

#[derive(Clone, Debug)]
pub enum Message {
    SelectSection(Section),
    SelectGroup(GroupId),
    SelectSubgroup(SubgroupId),
    SelectTile(LauncherId),
    /// A card was pressed: it becomes the selection, and the press is the start of a possible drag.
    PressTile(LauncherId),
    /// The held card has travelled far enough to become a drag. Sent once per gesture.
    DragLifted,
    HoverTile(Option<LauncherId>),
    HoverGroup(Option<GroupId>),
    HoverSubgroup(Option<SubgroupId>),
    PointerReleased,
    PointerCancelled,
    LaunchTile(LauncherId),
    SearchChanged(String),
    ClearSearch,

    AddGroup(Section),
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
    DraftEnvEdited(text_editor::Action),
    DraftProviderSelected(Option<AgentProvider>),
    DraftRawShellToggled(bool),
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
    OpenSettings,
    RevealConfig,
    ReloadConfig,
    DismissDialog,

    Shortcut(Shortcut),
    WindowOpened(iced::window::Id),
    AccessibilityInstalled(Result<(), String>),
    #[cfg(feature = "capture")]
    CaptureReady(iced::window::Screenshot),
}

/// How long a launch result or an action error stays on screen before it clears itself.
const STATUS_LIFETIME: std::time::Duration = std::time::Duration::from_secs(10);

pub fn update(shell: &mut AppShell, message: Message) -> Task<Message> {
    let task = apply(shell, message);
    shell.accessibility_nodes = accessibility::model(&shell.application.state);
    prepare_label_icons(shell);
    let dismissal = schedule_status_dismissal(shell);
    Task::batch([task, refresh_accessibility(shell), dismissal])
}

/// Starts the clock on a status banner the moment it appears, and only then.
///
/// Only *events* expire — a launch result or a failed action, which describe something that
/// happened. A conflicted or unsaved config is a state rather than an event, and a banner that
/// hides itself while the condition is still true would be lying.
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
    dismiss_after(STATUS_LIFETIME, shell.status_token)
}

/// What the banner is saying, or `None` when it is saying nothing that expires.
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

/// Sends [`Message::DismissStatus`] once `delay` has passed.
///
/// The wait happens on a thread of its own rather than on the task executor: this build runs Iced
/// on a thread pool with no timer, and parking one of its threads for ten seconds would take it
/// away from the work it exists for.
fn dismiss_after(delay: std::time::Duration, token: u64) -> Task<Message> {
    let (sender, receiver) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        // A closed receiver means the window is gone; nothing to tell.
        let _ = sender.send(());
    });
    Task::future(async move {
        let _ = receiver.await;
        Message::DismissStatus(token)
    })
}

/// Renders the symbols that carry a label color in every color the config uses.
///
/// The tinted masks are cached, so a color is rendered the first time it appears and then reused
/// instead of being uploaded again on every frame.
fn prepare_label_icons(shell: &mut AppShell) {
    for color in crate::presenters::label_colors(&shell.application.state) {
        shell
            .icons
            .prepare(crate::icons::Symbol::Category, Tint::Custom(color.rgb()));
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "one arm per user action reads best flat"
)]
fn apply(shell: &mut AppShell, message: Message) -> Task<Message> {
    match message {
        Message::SelectSection(section) => {
            shell.application.state.select_section(section);
        }
        Message::SelectGroup(group_id) => {
            // Pressing the open group closes it, which hides its categories and launchers.
            if shell.application.state.selected_group == Some(group_id) {
                shell.application.state.clear_group_selection();
            } else {
                let _ = shell.application.state.select_group(group_id);
            }
        }
        Message::SelectSubgroup(subgroup_id) => {
            if shell.application.state.selected_subgroup == Some(subgroup_id) {
                shell.application.state.clear_subgroup_selection();
            } else {
                let _ = shell.application.state.select_subgroup(subgroup_id);
            }
        }
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
        Message::PointerReleased => return finish_drag(shell),
        Message::PointerCancelled => {
            // The pointer left the window, so its release will never arrive here. Holding the drag
            // open would leave a card stuck to a cursor that is somewhere else entirely.
            shell.drag = None;
            shell.hovered_subgroup = None;
        }
        Message::LaunchTile(launcher_id) => {
            if shell.application.state.select_launcher(launcher_id) {
                let outcome = shell.application.launch_selected();
                report(shell, outcome);
            }
        }
        Message::SearchChanged(query) => shell.application.state.set_search_query(query),
        Message::ClearSearch => shell.application.state.set_search_query(String::new()),

        Message::AddGroup(section) => {
            return open_name_editor(
                shell,
                NameDraft::new(NameTarget::NewGroup(section)),
                Dialog::AddGroup { section },
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
            let Some(group_id) = shell.application.state.selected_group else {
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

        Message::AddLauncher => {
            let section = shell.application.state.selected_section;
            // Agents are a flat list, so nothing has been picked to add into: the container they
            // all live in is resolved here, and created if the config has none.
            let subgroup_id = if section == Section::Agents {
                match shell.application.agent_container() {
                    Ok(subgroup_id) => subgroup_id,
                    Err(error) => {
                        shell.action_error = Some(error.to_string());
                        return Task::none();
                    }
                }
            } else if let Some(subgroup_id) = shell.application.state.selected_subgroup {
                subgroup_id
            } else {
                return Task::none();
            };
            shell.launcher_draft = Some(LauncherDraft::new(subgroup_id, section));
            shell.name_draft = None;
            shell.application.state.dialog = Some(Dialog::AddLauncher { subgroup_id });
            return focus(LAUNCHER_NAME_ID);
        }
        Message::EditLauncher(launcher_id) => {
            let section = shell.application.state.selected_section;
            let Some(draft) = shell
                .launcher(launcher_id)
                .map(|(subgroup, launcher)| LauncherDraft::edit(subgroup.id, section, launcher))
            else {
                return Task::none();
            };
            shell.launcher_draft = Some(draft);
            shell.name_draft = None;
            let _ = shell.application.state.select_launcher(launcher_id);
            shell.application.state.dialog = Some(Dialog::EditLauncher { launcher_id });
            return focus(LAUNCHER_NAME_ID);
        }
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
        Message::DraftEnvEdited(action) => {
            if let Some(draft) = &mut shell.launcher_draft {
                draft.edit_env(action);
            }
        }
        Message::DraftProviderSelected(provider) => {
            if let Some(draft) = &mut shell.launcher_draft {
                draft.select_provider(provider);
            }
        }
        Message::DraftRawShellToggled(raw_shell) => {
            if let Some(draft) = &mut shell.launcher_draft {
                draft.set_raw_shell(raw_shell);
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
            // A banner that has already been replaced keeps its successor on screen.
            if token == shell.status_token {
                shell.action_error = None;
                shell.application.state.last_error = None;
                shell.application.state.last_launch = None;
            }
        }
        Message::ConfirmDelete => confirm_delete(shell),
        Message::OpenSettings => shell.application.state.dialog = Some(Dialog::Settings),
        Message::RevealConfig => {
            let outcome = shell.application.reveal_config();
            report(shell, outcome);
        }
        Message::ReloadConfig => {
            let outcome = shell.application.reload();
            report(shell, outcome);
        }
        Message::DismissDialog => dismiss_dialog(shell),

        Message::Shortcut(shortcut) => return apply_shortcut(shell, shortcut),
        Message::WindowOpened(window_id) => {
            shell.window_id = Some(window_id);
            #[cfg(feature = "capture")]
            if crate::capture::requested().is_some() {
                return crate::capture::schedule(window_id).map(Message::CaptureReady);
            }
        }
        Message::AccessibilityInstalled(result) => {
            if let Err(error) = result {
                shell.startup_error = Some(format!("Accessibility bridge: {error}"));
            }
        }
        #[cfg(feature = "capture")]
        Message::CaptureReady(screenshot) => return crate::capture::finish(&screenshot),
    }
    Task::none()
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
    let group = |section: Section| {
        shell
            .application
            .state
            .config
            .groups
            .iter()
            .find(|group| group.section == section)
            .map(|group| group.id)
    };
    let open = |section: Section| {
        let group = group(section)?;
        let subgroup = shell.group(group)?.subgroups.first()?.id;
        Some((group, subgroup))
    };
    let first_launcher = |subgroup_id: SubgroupId| {
        shell
            .subgroup(subgroup_id)
            .and_then(|(_, subgroup)| subgroup.items.first())
            .map(Launcher::id)
    };
    match scene.as_str() {
        "add-server" => open(Section::Servers)
            .map(|(_, subgroup)| vec![Message::SelectSubgroup(subgroup), Message::AddLauncher])
            .unwrap_or_default(),
        "reachability" => open(Section::Servers)
            .map(|(_, subgroup)| {
                vec![
                    Message::SelectSubgroup(subgroup),
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
                ]
            })
            .unwrap_or_default(),
        "add-server-advanced" => open(Section::Servers)
            .map(|(_, subgroup)| {
                vec![
                    Message::SelectSubgroup(subgroup),
                    Message::AddLauncher,
                    Message::DraftAdvancedToggled,
                ]
            })
            .unwrap_or_default(),
        "rename-group" => group(Section::Servers)
            .map(|group| vec![Message::RenameGroup(group)])
            .unwrap_or_default(),
        "rename-category" => open(Section::Servers)
            .map(|(_, subgroup)| vec![Message::RenameSubgroup(subgroup)])
            .unwrap_or_default(),
        "add-agent" => open(Section::Agents)
            .map(|(_, subgroup)| vec![Message::SelectSubgroup(subgroup), Message::AddLauncher])
            .unwrap_or_default(),
        "agents" => open(Section::Agents)
            .map(|(_, subgroup)| vec![Message::SelectSubgroup(subgroup)])
            .unwrap_or_default(),
        "groups" => group(Section::Servers)
            .map(|group| vec![Message::SelectGroup(group)])
            .unwrap_or_default(),
        "delete" => open(Section::Servers)
            .and_then(|(_, subgroup)| {
                let launcher = first_launcher(subgroup)?;
                Some(vec![
                    Message::SelectSubgroup(subgroup),
                    Message::DeleteLauncherRequested(launcher),
                ])
            })
            .unwrap_or_default(),
        "hover" => open(Section::Servers)
            .and_then(|(_, subgroup)| {
                let launcher = first_launcher(subgroup)?;
                Some(vec![
                    Message::SelectSubgroup(subgroup),
                    Message::HoverTile(Some(launcher)),
                ])
            })
            .unwrap_or_default(),
        "drag" => open(Section::Servers)
            .and_then(|(group, subgroup)| {
                let launcher = first_launcher(subgroup)?;
                let other = shell
                    .group(group)?
                    .subgroups
                    .iter()
                    .find(|candidate| candidate.id != subgroup)?
                    .id;
                Some(vec![
                    Message::SelectSubgroup(subgroup),
                    Message::HoverTile(Some(launcher)),
                    Message::PressTile(launcher),
                    // Over the second category chip, far enough from the press to count as a drag.
                    Message::DragLifted,
                    Message::HoverSubgroup(Some(other)),
                ])
            })
            .unwrap_or_default(),
        "settings" => vec![Message::OpenSettings],
        _ => Vec::new(),
    }
}

pub fn subscription(shell: &AppShell) -> Subscription<Message> {
    let keyboard = iced::event::listen_with(|event, _status, _window| {
        let iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            modifiers,
            repeat: false,
            ..
        }) = event
        else {
            return None;
        };
        if key == keyboard::Key::Named(keyboard::key::Named::Escape) {
            return Some(Message::DismissDialog);
        }
        shortcuts::interpret(&key, modifiers).map(Message::Shortcut)
    });
    let mut subscriptions = vec![
        keyboard,
        iced::window::open_events().map(Message::WindowOpened),
    ];
    // The rest of a drag — the travel and the release, which happen over chips and empty space —
    // comes from the event stream, which a widget's capture cannot hide. It is listened to only
    // while a card is actually being carried: every message rebuilds and re-lays-out the whole
    // window, and pointer movement arrives far faster than that, so subscribing to it while the app
    // holds nothing spends a full frame on every twitch of a cursor merely passing over.
    if shell.drag.is_some() {
        subscriptions.push(iced::event::listen_with(carried_pointer));
    }
    Subscription::batch(subscriptions)
}

/// The pointer events that matter while a card is being carried.
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

// ---------------------------------------------------------------- editing

/// Opens the shared name editor for a group or a category.
fn open_name_editor(shell: &mut AppShell, draft: NameDraft, dialog: Dialog) -> Task<Message> {
    shell.name_draft = Some(draft);
    shell.launcher_draft = None;
    shell.application.state.dialog = Some(dialog);
    focus(GROUP_NAME_ID)
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
        NameTarget::NewGroup(section) => shell
            .application
            .add_group(name, section)
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
            let _ = shell.application.state.select_group(group_id);
            dismiss_dialog(shell);
        }
        Ok(Selection::Subgroup(subgroup_id)) => {
            let _ = shell.application.state.select_subgroup(subgroup_id);
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
    // A launcher naming a program that is not there would fail at the worst moment: when it is
    // pressed. The check happens here, while the editor is still open and the name can be fixed.
    if let Some(error) = missing_executable(shell, &launcher) {
        if let Some(draft) = &mut shell.launcher_draft {
            draft.error = Some(error);
        }
        return;
    }
    let launcher_id = launcher.id();
    let subgroup_id = draft.subgroup_id;
    let outcome = match draft.target {
        Target::New => shell
            .application
            .add_launcher(subgroup_id, launcher)
            .map(|_| ()),
        Target::Existing(existing) => shell.application.edit_launcher(existing, launcher),
    };
    match outcome {
        Ok(()) => {
            // A flat section has nothing to open, so only the launcher itself is selected there.
            if shell.application.state.selected_section != Section::Agents {
                let _ = shell.application.state.select_subgroup(subgroup_id);
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

/// Reports the program a launcher names when it cannot be found.
///
/// SSH launchers are exempt: `ssh` is resolved to an absolute system path when the command is
/// built, and a raw-shell command is a script rather than a program.
fn missing_executable(shell: &AppShell, launcher: &Launcher) -> Option<crate::editor::FormError> {
    let executable = match launcher {
        Launcher::Ssh(_) => return None,
        Launcher::Agent(agent) => agent.executable.as_str(),
        Launcher::Custom(custom) => match &custom.command {
            despatch_domain::CustomCommand::Structured { executable, .. } => executable.as_str(),
            despatch_domain::CustomCommand::RawShell { .. } => return None,
        },
    };
    shell
        .application
        .locate_executable(executable)
        .err()
        .map(|error| crate::editor::FormError {
            field: Some(Field::Executable),
            message: format!(
                "'{executable}' was not found on your PATH. {}. A shell alias cannot be launched \
                 directly — give the full path and set what the alias sets under Environment.",
                error.message()
            ),
        })
}

/// Switches the interface between light, dark, and following the system.
///
/// Every symbol is rasterized once per theme, so the icon set is rebuilt here rather than tinted
/// on the fly; without that the old tints would survive the switch and disappear against the new
/// ground.
fn select_appearance(shell: &mut AppShell, choice: AppearanceChoice) {
    shell.appearance = choice;
    shell.theme = Appearance::resolve(choice).theme();
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

// ---------------------------------------------------------------- dragging

/// Takes a press on a card as both a selection and the start of a possible drag.
///
/// The card reports its own press, so the launcher picked up is the one under the pointer by
/// construction rather than whichever card last reported a hover — a hover that can go stale
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

/// Drops the dragged launcher into the category under the pointer, if there is one.
///
/// A release that lands anywhere else ends the drag and changes nothing, which is what makes the
/// gesture abortable: drag away from every category and let go.
fn finish_drag(shell: &mut AppShell) -> Task<Message> {
    let Some(drag) = shell.drag.take() else {
        return Task::none();
    };
    // A press that never travelled is a click, and a click on a card launches it.
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
        // The card is no longer where the pointer left it, so the hover it reported is stale.
        shell.hovered_tile = None;
        let _ = shell.application.state.select_launcher(drag.launcher_id);
    }
    Task::none()
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
/// existing launcher name in the same group.
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
    // One group cannot hold more launchers than `usize` can count, so a free suffix always exists.
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

fn apply_shortcut(shell: &mut AppShell, shortcut: Shortcut) -> Task<Message> {
    match shortcut {
        Shortcut::AddItem => return apply(shell, Message::AddLauncher),
        Shortcut::AddGroup => {
            let section = shell.application.state.selected_section;
            return apply(shell, Message::AddGroup(section));
        }
        Shortcut::FocusSearch => return focus(SEARCH_ID),
        Shortcut::OpenSettings => shell.application.state.dialog = Some(Dialog::Settings),
        Shortcut::Launch => {
            if shell.application.state.dialog.is_none() {
                let outcome = shell.application.launch_selected();
                report(shell, outcome);
            }
        }
        Shortcut::Edit => {
            if let Some(launcher_id) = shell.application.state.selected_launcher {
                return apply(shell, Message::EditLauncher(launcher_id));
            }
        }
        Shortcut::Delete => {
            if shell.application.state.dialog.is_some() {
                return Task::none();
            }
            let state = &shell.application.state;
            let dialog = state
                .selected_launcher
                .map(|launcher_id| Dialog::DeleteLauncher { launcher_id })
                .or_else(|| {
                    state
                        .selected_subgroup
                        .map(|subgroup_id| Dialog::DeleteSubgroup { subgroup_id })
                })
                .or_else(|| {
                    state
                        .selected_group
                        .map(|group_id| Dialog::DeleteGroup { group_id })
                });
            shell.application.state.dialog = dialog;
        }
        Shortcut::Previous => navigate_launcher(shell, -1),
        Shortcut::Next => navigate_launcher(shell, 1),
        Shortcut::PreviousGroup => navigate_group(shell, -1),
        Shortcut::NextGroup => navigate_group(shell, 1),
    }
    Task::none()
}

fn navigate_launcher(shell: &mut AppShell, direction: isize) {
    let state = &mut shell.application.state;
    let Some(items) = state
        .selected_subgroup
        .and_then(|selected| state.config.locate_subgroup(selected))
        .map(|(_, subgroup)| &subgroup.items)
    else {
        return;
    };
    if items.is_empty() {
        return;
    }
    let current = state
        .selected_launcher
        .and_then(|selected| items.iter().position(|item| item.id() == selected));
    let next = match current {
        Some(0) | None if direction < 0 => items.len() - 1,
        Some(index) if direction < 0 => index - 1,
        Some(index) => (index + 1) % items.len(),
        None => 0,
    };
    state.selected_launcher = Some(items[next].id());
}

fn navigate_group(shell: &mut AppShell, direction: isize) {
    let state = &mut shell.application.state;
    if state.config.groups.is_empty() {
        return;
    }
    let current = state.selected_group.and_then(|selected| {
        state
            .config
            .groups
            .iter()
            .position(|group| group.id == selected)
    });
    let next = match current {
        Some(0) | None if direction < 0 => state.config.groups.len() - 1,
        Some(index) if direction < 0 => index - 1,
        Some(index) => (index + 1) % state.config.groups.len(),
        None => 0,
    };
    let group_id = state.config.groups[next].id;
    let _ = state.select_group(group_id);
}

// ---------------------------------------------------------------- plumbing

fn focus(id: &'static str) -> Task<Message> {
    iced::widget::operation::focus(iced::widget::Id::new(id))
}

fn refresh_accessibility(shell: &AppShell) -> Task<Message> {
    let Some(window_id) = shell.window_id else {
        return Task::none();
    };
    let nodes = shell.accessibility_nodes.clone();
    #[cfg(target_os = "macos")]
    {
        iced::window::run(window_id, move |window| {
            crate::accessibility_bridge::install(window, &nodes)
        })
        .map(Message::AccessibilityInstalled)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = nodes;
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use despatch_application::{
        Application, ConfigDocument, ConfigFingerprint, ConfigRepository, Dialog, FileRevealer,
        LaunchRequest, LocalPreferences, PortError, PortErrorKind, RepositorySaveExpectation,
        TerminalLauncher,
    };
    use despatch_domain::{
        AgentProvider, AppConfig, CustomCommand, Launcher, LauncherId, Section, SshLauncher,
        SubgroupId,
    };

    use crate::editor::Field;

    use super::{AppShell, Message, duplicate_name, update};

    fn sample_ssh(name: &str, host: &str) -> Launcher {
        Launcher::Ssh(SshLauncher::new(name, host, "deploy").expect("valid SSH launcher"))
    }

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

    struct NoopTerminal;

    impl TerminalLauncher for NoopTerminal {
        fn launch(&self, _: &LaunchRequest) -> Result<(), PortError> {
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
        appearance: std::sync::Mutex<despatch_application::AppearanceChoice>,
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

    /// The shell tests accept any program name.
    struct AnyExecutable;

    impl despatch_application::ExecutableLocator for AnyExecutable {
        fn locate(&self, executable: &str) -> Result<std::path::PathBuf, PortError> {
            Ok(std::path::PathBuf::from("/usr/bin").join(executable))
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

    fn application() -> Application {
        let mut application = Application::new(
            Arc::new(MemoryRepository),
            Arc::new(MemoryPreferences),
            Arc::new(NoopTerminal),
            Arc::new(NoopRevealer),
            Arc::new(OfflineProbe),
            Arc::new(AnyExecutable),
        );
        application
            .create_config(Path::new("/tmp/despatch-ui-test.json"))
            .expect("create config");
        // A created config arrives seeded with the built-in agents. These tests are about the
        // shell rather than the seeding, so they start from nothing and build what they need.
        application.state.config.groups.clear();
        application
    }

    fn send(shell: &mut AppShell, message: Message) {
        let _ = update(shell, message);
    }

    fn selected_group(shell: &AppShell) -> Option<&despatch_domain::Group> {
        let group_id = shell.application.state.selected_group?;
        shell.group(group_id)
    }

    fn selected_subgroup(shell: &AppShell) -> Option<&despatch_domain::Subgroup> {
        let subgroup_id = shell.application.state.selected_subgroup?;
        shell.subgroup(subgroup_id).map(|(_, subgroup)| subgroup)
    }

    /// A shell holding one group with one open category in the given section.
    fn shell_with_category(section: Section, group: &str, category: &str) -> AppShell {
        let mut application = application();
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
    fn an_agent_group_adds_a_preset_agent_launcher() {
        let mut shell = shell_with_category(Section::Agents, "Project tools", "Assistants");

        send(&mut shell, Message::AddLauncher);
        send(
            &mut shell,
            Message::DraftProviderSelected(Some(AgentProvider::Claude)),
        );
        send(&mut shell, Message::SaveLauncher);

        let Launcher::Agent(agent) = &tiles(&shell)[0] else {
            panic!("expected an agent launcher");
        };
        assert_eq!(agent.provider, AgentProvider::Claude);
        assert_eq!(agent.executable, "claude");
        assert_eq!(agent.name, "Claude");
    }

    #[test]
    fn an_agent_group_adds_a_custom_cli_launcher() {
        let mut shell = shell_with_category(Section::Agents, "Project tools", "Assistants");

        send(&mut shell, Message::AddLauncher);
        send(&mut shell, Message::DraftProviderSelected(None));
        send(
            &mut shell,
            Message::DraftFieldChanged(Field::Name, "Repo shell".to_owned()),
        );
        send(
            &mut shell,
            Message::DraftFieldChanged(Field::Executable, "zsh".to_owned()),
        );
        send(&mut shell, Message::SaveLauncher);

        let Launcher::Custom(custom) = &tiles(&shell)[0] else {
            panic!("expected a custom launcher");
        };
        assert_eq!(custom.name, "Repo shell");
        assert!(matches!(custom.command, CustomCommand::Structured { .. }));
    }

    #[test]
    fn a_server_launcher_can_never_be_added_to_an_agent_category() {
        let mut shell = shell_with_category(Section::Agents, "Project tools", "Assistants");
        let subgroup_id = selected_subgroup(&shell).expect("category").id;

        let rejected = shell
            .application
            .add_launcher(subgroup_id, sample_ssh("API", "api.example.com"));

        assert!(rejected.is_err());
        assert!(tiles(&shell).is_empty());
    }

    #[test]
    fn groups_can_be_created_renamed_and_deleted_with_everything_inside_them() {
        let mut shell = AppShell::new(application(), None);

        send(&mut shell, Message::AddGroup(Section::Servers));
        send(&mut shell, Message::NameChanged("Prod".to_owned()));
        send(&mut shell, Message::SaveName);
        let group_id = selected_group(&shell).expect("group").id;

        send(&mut shell, Message::AddSubgroup);
        send(&mut shell, Message::NameChanged("Web".to_owned()));
        send(&mut shell, Message::SaveName);
        add_server(&mut shell, "API server", "api.example.com");

        send(&mut shell, Message::RenameGroup(group_id));
        send(&mut shell, Message::NameChanged("Production".to_owned()));
        send(&mut shell, Message::SaveName);
        assert_eq!(selected_group(&shell).expect("group").name, "Production");

        send(&mut shell, Message::DeleteGroupRequested(group_id));
        send(&mut shell, Message::ConfirmDelete);
        assert!(shell.application.state.config.groups.is_empty());
        assert_eq!(shell.application.state.selected_group, None);
        assert_eq!(shell.application.state.selected_subgroup, None);
    }

    #[test]
    fn a_group_keeps_the_label_color_it_was_created_with_and_can_change_or_drop_it() {
        use despatch_domain::LabelColor;

        let blue = LabelColor::from_rgb(0x00, 0x7A, 0xFF);
        let green = LabelColor::from_rgb(0x34, 0xC7, 0x59);
        let mut shell = AppShell::new(application(), None);

        send(&mut shell, Message::AddGroup(Section::Servers));
        send(&mut shell, Message::NameChanged("Prod".to_owned()));
        send(&mut shell, Message::NameColorSelected(blue));
        send(&mut shell, Message::SaveName);
        let group_id = selected_group(&shell).expect("group").id;
        assert_eq!(selected_group(&shell).expect("group").color, Some(blue));

        send(&mut shell, Message::RenameGroup(group_id));
        assert_eq!(
            shell.name_draft.as_ref().expect("draft").selected_color(),
            Some(blue),
            "the editor opens on the color the group already has"
        );
        send(&mut shell, Message::NameColorSelected(green));
        send(&mut shell, Message::SaveName);
        assert_eq!(selected_group(&shell).expect("group").color, Some(green));
        assert_eq!(selected_group(&shell).expect("group").name, "Prod");

        send(&mut shell, Message::RenameGroup(group_id));
        send(&mut shell, Message::NameColorCleared);
        send(&mut shell, Message::SaveName);
        assert_eq!(selected_group(&shell).expect("group").color, None);
    }

    #[test]
    fn a_category_is_never_given_a_color_of_its_own_and_wears_its_groups() {
        use despatch_domain::LabelColor;

        let yellow = LabelColor::from_rgb(0xFF, 0xCC, 0x00);
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        let group_id = selected_group(&shell).expect("group").id;
        let subgroup_id = subgroups(&shell)[0].id;

        send(&mut shell, Message::RenameGroup(group_id));
        send(&mut shell, Message::NameColorSelected(yellow));
        send(&mut shell, Message::SaveName);
        let _ = shell.application.state.select_subgroup(subgroup_id);

        send(&mut shell, Message::RenameSubgroup(subgroup_id));
        let draft = shell.name_draft.as_ref().expect("draft");
        assert!(
            !draft.has_color(),
            "a category editor must not offer a color it cannot keep"
        );
        assert_eq!(draft.build().expect("valid draft").color, None);
        send(&mut shell, Message::DismissDialog);

        assert_eq!(
            crate::presenters::subgroups(&shell.application.state)[0].color,
            Some(yellow),
            "the category is drawn in the color of its group"
        );
    }

    #[test]
    fn a_launcher_inherits_its_groups_color_until_it_is_given_one_of_its_own() {
        use despatch_domain::LabelColor;

        let yellow = LabelColor::from_rgb(0xFF, 0xCC, 0x00);
        let green = LabelColor::from_rgb(0x34, 0xC7, 0x59);
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        let group_id = selected_group(&shell).expect("group").id;

        send(&mut shell, Message::RenameGroup(group_id));
        send(&mut shell, Message::NameColorSelected(yellow));
        send(&mut shell, Message::SaveName);
        let subgroup_id = subgroups(&shell)[0].id;
        let _ = shell.application.state.select_subgroup(subgroup_id);
        add_server(&mut shell, "API server", "api.example.com");

        assert_eq!(
            tiles(&shell)[0].color(),
            None,
            "an inherited color is not copied onto the launcher"
        );
        assert_eq!(
            crate::presenters::selected_tiles(&shell.application.state)[0].color,
            Some(yellow),
            "but it is what the launcher is drawn in"
        );

        let launcher_id = tiles(&shell)[0].id();
        send(&mut shell, Message::EditLauncher(launcher_id));
        send(&mut shell, Message::DraftColorSelected(green));
        send(&mut shell, Message::SaveLauncher);
        assert_eq!(tiles(&shell)[0].color(), Some(green));
        assert_eq!(
            crate::presenters::selected_tiles(&shell.application.state)[0].color,
            Some(green),
            "an override wins over the group"
        );

        send(&mut shell, Message::EditLauncher(launcher_id));
        send(&mut shell, Message::DraftColorCleared);
        send(&mut shell, Message::SaveLauncher);
        assert_eq!(tiles(&shell)[0].color(), None);
        assert_eq!(
            crate::presenters::selected_tiles(&shell.application.state)[0].color,
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
            Arc::new(NoopTerminal),
            Arc::new(NoopRevealer),
            Arc::new(OfflineProbe),
            Arc::new(AnyExecutable),
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
            "the choice outlives the window"
        );
    }

    #[test]
    fn a_launcher_naming_a_program_that_is_not_there_is_refused_with_the_name_in_hand() {
        struct NoExecutables;

        impl despatch_application::ExecutableLocator for NoExecutables {
            fn locate(&self, executable: &str) -> Result<std::path::PathBuf, PortError> {
                Err(PortError::new(
                    despatch_application::PortErrorKind::NotFound,
                    format!("'{executable}' is not on the PATH"),
                ))
            }
        }

        let mut application = Application::new(
            Arc::new(MemoryRepository),
            Arc::new(MemoryPreferences),
            Arc::new(NoopTerminal),
            Arc::new(NoopRevealer),
            Arc::new(OfflineProbe),
            Arc::new(NoExecutables),
        );
        application
            .create_config(Path::new("/tmp/despatch-executable-test.json"))
            .expect("create config");
        let group_id = application
            .add_group("Project tools", Section::Agents)
            .expect("add group");
        let subgroup_id = application
            .add_subgroup(group_id, "Assistants")
            .expect("add category");
        assert!(application.state.select_subgroup(subgroup_id));
        let mut shell = AppShell::new(application, None);

        send(&mut shell, Message::AddLauncher);
        send(&mut shell, Message::SaveLauncher);

        let draft = shell.launcher_draft.as_ref().expect("draft stays open");
        let message = draft.field_error(Field::Executable).expect("blamed field");
        assert!(message.contains("codex"), "{message}");
        assert!(message.contains("alias"), "the way out is named: {message}");
        assert!(
            selected_subgroup(&shell)
                .expect("category")
                .items
                .is_empty(),
            "nothing unlaunchable is written down"
        );
    }

    #[test]
    fn an_agent_carries_the_environment_its_command_needs() {
        let mut shell = shell_with_category(Section::Agents, "Project tools", "Assistants");

        send(&mut shell, Message::AddLauncher);
        send(
            &mut shell,
            Message::DraftEnvEdited(iced::widget::text_editor::Action::Edit(
                iced::widget::text_editor::Edit::Paste(std::sync::Arc::new(
                    "ANTHROPIC_MODEL=opus\n\n# a comment\nFLAGS=--a=b".to_owned(),
                )),
            )),
        );
        send(&mut shell, Message::SaveLauncher);

        let Launcher::Agent(agent) = &tiles(&shell)[0] else {
            panic!("expected an agent launcher");
        };
        assert_eq!(agent.env.len(), 2);
        assert_eq!(agent.env["ANTHROPIC_MODEL"], "opus");
        assert_eq!(
            agent.env["FLAGS"], "--a=b",
            "only the first = separates a name from its value"
        );
    }

    /// A shell holding one group with two categories, the first of them open and holding a server.
    fn shell_with_two_categories() -> (AppShell, LauncherId, SubgroupId, SubgroupId) {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        let source = shell
            .application
            .state
            .selected_subgroup
            .expect("an open category");
        let group_id = shell.application.state.selected_group.expect("a group");
        let target = shell
            .application
            .add_subgroup(group_id, "Databases")
            .expect("add category");
        assert!(shell.application.state.select_subgroup(source));
        add_server(&mut shell, "API server", "api.example.com");
        let launcher_id = tiles(&shell)[0].id();
        (shell, launcher_id, source, target)
    }

    /// Presses on a card, carries it over a category, and lets go — the whole gesture.
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
    fn dragging_a_card_onto_another_category_moves_the_launcher_into_it() {
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
            shell.application.state.selected_subgroup,
            Some(target),
            "the view follows the launcher to where it landed"
        );
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
    fn a_card_only_leaves_its_place_once_it_has_been_lifted() {
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

        // A click that never travelled leaves everything where it was, launch included.
        send(&mut shell, Message::HoverSubgroup(Some(target)));
        send(&mut shell, Message::PointerReleased);
        assert_eq!(category_names(&shell, target), ["API server"]);
        assert!(category_names(&shell, source).is_empty());
    }

    #[test]
    fn one_click_selects_a_card_and_two_launch_it() {
        let (mut shell, launcher_id, _, _) = shell_with_two_categories();

        // The first click of the pair.
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

        // The second click of the pair, which the card reports as a double.
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
    fn a_press_picks_up_the_card_that_reported_it_whatever_was_last_hovered() {
        let (mut shell, launcher_id, source, target) = shell_with_two_categories();

        // A hover left over from a card that is no longer under the pointer. The press names its
        // own card, so the stale hover cannot make the gesture carry the wrong launcher — or, when
        // the press lands on no card at all, carry one at all.
        send(&mut shell, Message::HoverTile(Some(LauncherId::new())));
        send(&mut shell, Message::PressTile(launcher_id));

        assert_eq!(
            shell.drag.map(|drag| drag.launcher_id),
            Some(launcher_id),
            "the pressed card is the one being carried"
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
        // The pointer is over a chip, but nothing was ever carried there.
        send(&mut shell, Message::HoverSubgroup(Some(target)));
        send(&mut shell, Message::PointerReleased);

        assert_eq!(category_names(&shell, source), ["API server"]);
        assert!(category_names(&shell, target).is_empty());
    }

    #[test]
    fn the_pointer_leaving_the_window_puts_the_card_back() {
        let (mut shell, launcher_id, source, target) = shell_with_two_categories();
        send(&mut shell, Message::PressTile(launcher_id));
        send(&mut shell, Message::DragLifted);
        send(&mut shell, Message::HoverSubgroup(Some(target)));

        // The release happens over another app, where it is never seen here.
        send(&mut shell, Message::PointerCancelled);
        send(&mut shell, Message::PointerReleased);

        assert_eq!(shell.drag, None);
        assert_eq!(category_names(&shell, source), ["API server"]);
        assert!(category_names(&shell, target).is_empty());
    }

    #[test]
    fn a_press_that_lands_on_no_card_picks_nothing_up() {
        let (mut shell, _, source, target) = shell_with_two_categories();

        // The press landed on a chip, not a card, so no card ever reported one.
        send(&mut shell, Message::HoverTile(None));
        assert_eq!(shell.drag, None);

        send(&mut shell, Message::HoverSubgroup(Some(target)));
        send(&mut shell, Message::PointerReleased);
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

    #[test]
    fn an_agent_card_launches_even_though_no_category_is_open() {
        let mut shell = shell_with_category(Section::Agents, "Agents", "Assistants");
        let subgroup_id = shell
            .application
            .state
            .selected_subgroup
            .expect("an open category");
        let launcher_id = shell
            .application
            .add_launcher(
                subgroup_id,
                Launcher::Agent(
                    despatch_domain::AgentLauncher::preset(AgentProvider::Claude, "Claude")
                        .expect("agent"),
                ),
            )
            .expect("add agent");
        // The Agents section lists its launchers flat, so nothing is open when a card is clicked.
        shell.application.state.clear_group_selection();

        send(&mut shell, Message::LaunchTile(launcher_id));

        assert!(
            matches!(
                shell.application.state.last_launch,
                Some(despatch_application::LaunchOutcome::Success { .. })
            ),
            "clicking an agent runs it: {:?}",
            shell.application.state.last_launch
        );
    }

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

        // A dismissal for a banner that has since been replaced must not clear its successor.
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

    #[test]
    fn a_blank_name_keeps_the_editor_open() {
        let mut shell = AppShell::new(application(), None);

        send(&mut shell, Message::AddGroup(Section::Servers));
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
            shell.application.state.selected_subgroup,
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

        send(&mut shell, Message::DeleteSubgroupRequested(subgroup_id));
        assert_eq!(subgroups(&shell).len(), 2, "nothing goes before confirming");
        send(&mut shell, Message::ConfirmDelete);
        assert_eq!(subgroups(&shell).len(), 1);
        assert_eq!(subgroups(&shell)[0].name, "Databases");
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
    fn pressing_the_open_group_closes_it_and_hides_everything_below() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        add_server(&mut shell, "API server", "api.example.com");
        let group_id = selected_group(&shell).expect("group").id;

        send(&mut shell, Message::SelectGroup(group_id));

        assert_eq!(shell.application.state.selected_group, None);
        assert_eq!(shell.application.state.selected_subgroup, None);
        assert!(crate::presenters::subgroups(&shell.application.state).is_empty());
        assert!(crate::presenters::selected_tiles(&shell.application.state).is_empty());
    }

    #[test]
    fn pressing_the_open_category_closes_it_but_leaves_the_group_open() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        add_server(&mut shell, "API server", "api.example.com");
        let group_id = selected_group(&shell).expect("group").id;
        let subgroup_id = subgroups(&shell)[0].id;

        send(&mut shell, Message::SelectSubgroup(subgroup_id));

        assert_eq!(shell.application.state.selected_group, Some(group_id));
        assert_eq!(shell.application.state.selected_subgroup, None);
        assert_eq!(
            crate::presenters::subgroups(&shell.application.state).len(),
            1,
            "the categories of the open group stay visible"
        );
        assert!(crate::presenters::selected_tiles(&shell.application.state).is_empty());
    }

    #[test]
    fn a_launcher_cannot_be_added_while_no_category_is_open() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        let subgroup_id = subgroups(&shell)[0].id;
        send(&mut shell, Message::SelectSubgroup(subgroup_id));

        send(&mut shell, Message::AddLauncher);

        assert!(shell.launcher_draft.is_none());
        assert!(shell.application.state.dialog.is_none());
    }

    #[test]
    fn hovering_a_tile_exposes_its_actions_without_changing_the_selection() {
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
    fn search_filters_tiles_and_can_be_cleared() {
        let mut shell = shell_with_category(Section::Servers, "Production", "Web");
        add_server(&mut shell, "API server", "api.example.com");
        add_server(&mut shell, "Cache", "cache.example.com");

        send(&mut shell, Message::SearchChanged("cache".to_owned()));
        assert_eq!(
            crate::presenters::selected_tiles(&shell.application.state).len(),
            1
        );

        send(&mut shell, Message::ClearSearch);
        assert_eq!(
            crate::presenters::selected_tiles(&shell.application.state).len(),
            2
        );
    }
}
