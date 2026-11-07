//! The Settings destination: appearance, where launches go, the config file, the launch history,
//! the keyboard, and quitting — which a menu-bar app has nowhere else to put.

use despatch_application::AppearanceChoice;
use iced::widget::text::Wrapping;
use iced::widget::{Space, button, column, container, pick_list, row, text, toggler};
use iced::{Center, Element, Fill, Padding};

use super::editors::labelled;
use super::{BUTTON_PADDING, FIELD_PADDING, FIELD_TEXT_SIZE, SCROLLBAR_GUTTER, scroll};
use crate::app_shell::{AppShell, Message, OpenAtLogin};
use crate::presenters;
use crate::shortcuts::{self, Shortcut};
use crate::style;
use crate::typography;

#[expect(
    clippy::too_many_lines,
    reason = "one section per setting, read top to bottom like the pane itself"
)]
pub(super) fn pane(shell: &AppShell) -> Element<'_, Message> {
    let path = shell.application.state.source_path.as_ref().map_or_else(
        || "No config file is open".to_owned(),
        |path| path.display().to_string(),
    );
    let recent = shell.application.recent_launches().len();

    let shortcuts_list = column(
        [
            (Shortcut::FocusSearch, "Search"),
            (Shortcut::Launch, "Open the selected server"),
            (Shortcut::AddItem, "New server"),
            (Shortcut::AddGroup, "New group"),
            (Shortcut::Edit, "Edit the selection"),
            (Shortcut::Delete, "Delete the selection"),
            (Shortcut::Group(0), "Go to a group"),
            (Shortcut::NextSegment, "Next category"),
            (Shortcut::Close, "Hide the panel"),
            (Shortcut::Quit, "Quit Despatch"),
        ]
        .map(|(shortcut, label)| {
            row![
                text(label).size(FIELD_TEXT_SIZE),
                Space::new().width(Fill),
                text(shortcuts::equivalent(shortcut))
                    .size(12)
                    .style(style::tertiary_text),
            ]
            .align_y(Center)
            .into()
        }),
    )
    .spacing(6);

    let body = column![
        appearance_picker(shell.appearance),
        labelled(
            "Launches open in",
            column![
                pick_list(
                    shell.terminals.as_slice(),
                    shell.picked_terminal().cloned(),
                    Message::TerminalPicked,
                )
                .text_size(FIELD_TEXT_SIZE)
                .padding(FIELD_PADDING)
                .width(280)
                .style(style::popup_button)
                .menu_style(style::menu),
                text("Servers open in a new window of this terminal.")
                    .size(11)
                    .style(style::tertiary_text),
            ]
            .spacing(6)
            .into(),
            None,
        ),
        labelled(
            "Config file",
            column![
                well(&path),
                row![
                    action("Reveal in Finder", Message::RevealConfig),
                    action("Reload", Message::ReloadConfig),
                    action("Open…", Message::OpenConfig),
                    action("New…", Message::NewConfig),
                    action("Save a Copy…", Message::SaveConfigCopy),
                ]
                .spacing(6),
                text(
                    "Groups and servers live in this JSON file, which can sit in a synced or \
                     version-controlled folder. Secrets are never stored in it."
                )
                .size(11)
                .line_height(1.35)
                .style(style::tertiary_text),
            ]
            .spacing(8)
            .into(),
            None,
        ),
        open_at_login(shell),
        labelled(
            "Recently used",
            row![
                text(match recent {
                    0 => "Nothing remembered yet.".to_owned(),
                    1 => "1 launch remembered on this Mac.".to_owned(),
                    count => format!("{count} launches remembered on this Mac."),
                })
                .size(FIELD_TEXT_SIZE)
                .width(Fill),
                button(text("Clear History").size(12))
                    .on_press_maybe((recent > 0).then_some(Message::ClearRecents))
                    .padding(BUTTON_PADDING)
                    .style(style::secondary_button),
            ]
            .spacing(8)
            .align_y(Center)
            .into(),
            None,
        ),
        labelled("Keyboard", shortcuts_list.into(), None),
        row![
            Space::new().width(Fill),
            button(
                row![
                    text("Quit Despatch")
                        .size(FIELD_TEXT_SIZE)
                        .font(typography::MEDIUM),
                    text(shortcuts::equivalent(Shortcut::Quit))
                        .size(12)
                        .style(style::tertiary_text),
                ]
                .spacing(10)
                .align_y(Center),
            )
            .on_press(Message::Quit)
            .padding(BUTTON_PADDING)
            .style(style::secondary_button),
        ],
    ]
    .spacing(18);

    scroll(container(body).padding(Padding {
        top: 2.0,
        right: SCROLLBAR_GUTTER,
        bottom: 14.0,
        left: 0.0,
    }))
    .height(Fill)
    .into()
}

/// The switch that opens Despatch at login, with what the system says about it.
fn open_at_login(shell: &AppShell) -> Element<'_, Message> {
    let status = shell.open_at_login;
    let note = match status {
        Some(OpenAtLogin::NeedsApproval) => {
            Some("Allow Despatch in System Settings → General → Login Items to finish.")
        }
        Some(OpenAtLogin::Unavailable) => {
            Some("Available when Despatch runs from its app bundle, as installed.")
        }
        _ => None,
    };
    let enabled = matches!(status, Some(OpenAtLogin::On | OpenAtLogin::NeedsApproval));
    let mut switch = toggler(enabled)
        .label("Open Despatch at login")
        .text_size(FIELD_TEXT_SIZE)
        .size(18);
    if !matches!(status, None | Some(OpenAtLogin::Unavailable)) {
        switch = switch.on_toggle(Message::OpenAtLoginToggled);
    }
    let mut block = column![switch].spacing(5);
    if let Some(note) = note {
        block = block.push(text(note).size(11).style(style::tertiary_text));
    }
    labelled("Startup", block.into(), None)
}

/// A read-only value in a field-shaped well.
fn well(value: &str) -> Element<'static, Message> {
    container(
        text(presenters::ellipsize(value, 440.0, FIELD_TEXT_SIZE))
            .size(FIELD_TEXT_SIZE)
            .wrapping(Wrapping::None),
    )
    .width(Fill)
    .padding(FIELD_PADDING)
    .clip(true)
    .style(style::value_well)
    .into()
}

fn action(label: &'static str, message: Message) -> Element<'static, Message> {
    button(text(label).size(12))
        .on_press(message)
        .padding(Padding::from([4.0, 10.0]))
        .style(style::secondary_button)
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
                    .size(13)
                    .font(if is_selected {
                        typography::MEDIUM
                    } else {
                        typography::UI
                    })
                    .width(Fill)
                    .align_x(Center),
            )
            .on_press(Message::AppearanceSelected(choice))
            .width(Fill)
            .padding(Padding::from([4.0, 6.0]))
            .style(move |theme, status| style::segment(theme, status, is_selected)),
        );
    }

    labelled(
        "Appearance",
        container(segments)
            .width(280)
            .padding(2)
            .style(style::segment_track)
            .into(),
        None,
    )
}
