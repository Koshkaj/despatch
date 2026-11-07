//! The sheets that open above the panel: the group and category name editor, the launcher
//! editor, and delete confirmations.

use despatch_application::{Dialog, Reachability};
use despatch_domain::LabelColor;
use iced::widget::{
    Column, Space, button, column, container, opaque, pick_list, row, text, text_editor, text_input,
};
use iced::{Center, Element, Fill, Left, Padding, Theme};

use super::{
    BUTTON_PADDING, BUTTON_TEXT_SIZE, FIELD_PADDING, FIELD_TEXT_SIZE, SCROLLBAR_GUTTER, scroll,
    symbol,
};
use crate::app_shell::{AppShell, GROUP_NAME_ID, LAUNCHER_NAME_ID, Message};
use crate::editor::{Field, LauncherDraft, NameDraft, ReachabilityState};
use crate::icons::{IconSet, Symbol, Tint};
use crate::presenters;
use crate::style;
use crate::typography;

/// The label-color swatches in the group and launcher editors.
const SWATCH_SIZE: f32 = 22.0;
const SWATCH_SPACING: f32 = 6.0;
/// Keeps the palette inside the sheet instead of running off its right edge.
const SWATCHES_PER_ROW: usize = 13;
/// The vertical rhythm inside an editor sheet.
const SHEET_PADDING: f32 = 18.0;
const SHEET_SPACING: f32 = 12.0;

pub(super) fn sheet_overlay<'a>(shell: &'a AppShell, dialog: &'a Dialog) -> Element<'a, Message> {
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
    };
    let Some(content) = content else {
        return container(Space::new()).into();
    };

    opaque(
        container(content)
            .width(Fill)
            .height(Fill)
            .center(Fill)
            .padding(16)
            .style(style::modal_scrim),
    )
}

pub(super) fn sheet_header(title: &'static str, detail: String) -> Element<'static, Message> {
    column![
        text(title)
            .size(15)
            .font(typography::SEMIBOLD)
            .line_height(1.2),
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
        .width(400)
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
    let mut form = column![sheet_header(
        draft.title(),
        "Structured SSH connection. Key paths are stored, key contents never are.".to_owned()
    )]
    .spacing(SHEET_SPACING);
    if !draft.categories.is_empty() {
        form = form.push(category_picker(draft));
    }
    form = form
        .push(draft_field(
            draft,
            "Name",
            Field::Name,
            "API server",
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
    form = ssh_fields(form, draft, icons);
    form = form
        .push(general_error(draft.general_error()))
        .push(sheet_actions(draft.action(), Message::SaveLauncher, false));

    // The scrollbar is drawn inside the scrollable, so the fields keep a gutter clear of it. That
    // gutter is taken out of the sheet's own right padding instead of being added to it, which
    // leaves the visible margins on both sides equal whether or not the editor scrolls.
    container(scroll(container(form).padding(Padding {
        top: 0.0,
        right: SCROLLBAR_GUTTER,
        bottom: 0.0,
        left: 0.0,
    })))
    .width(500)
    .max_height(470.0)
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

/// Which category the launcher is saved into. Choosing another one for an existing launcher moves
/// it there when the editor is saved.
fn category_picker(draft: &LauncherDraft) -> Element<'_, Message> {
    labelled(
        "Category",
        pick_list(
            draft.categories.as_slice(),
            draft.selected_category().cloned(),
            |choice| Message::DraftCategorySelected(choice.id),
        )
        .text_size(FIELD_TEXT_SIZE)
        .padding(FIELD_PADDING)
        .width(Fill)
        .style(style::popup_button)
        .menu_style(style::menu)
        .into(),
        None,
    )
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

pub(super) fn labelled<'a>(
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
        .spacing(18),
    )
    .width(420)
    .padding(22)
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
