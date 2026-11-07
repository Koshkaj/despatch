#![expect(
    clippy::unreadable_literal,
    reason = "six-digit hex is the canonical, reviewable way to write a color"
)]

use despatch_domain::LabelColor;
use iced::widget::{
    button, container, overlay, pick_list, rule, scrollable, text, text_editor, text_input,
};
use iced::{Background, Border, Color, Shadow, Theme, Vector, color};

use crate::theme::{self, is_dark, label, label_secondary, label_tertiary, mix, with_alpha};

/// The user-chosen label color of a group or category, ready for the renderer.
#[must_use]
pub fn label_color(color: LabelColor) -> Color {
    let [red, green, blue] = color.rgb();
    Color::from_rgb8(red, green, blue)
}

/// A label swatch in the color picker, filled with the color it applies.
///
/// The selected swatch is ringed by a translucent halo of its own color, and carries a check mark
/// so the selection is never signalled by color alone.
pub fn swatch(status: button::Status, color: Color, selected: bool) -> button::Style {
    let fill = match status {
        button::Status::Hovered => mix(color, Color::WHITE, 0.14),
        button::Status::Pressed => mix(color, Color::BLACK, 0.14),
        button::Status::Active | button::Status::Disabled => color,
    };
    button::Style {
        background: Some(Background::Color(fill)),
        text_color: on_accent(color),
        border: Border {
            radius: 7.0.into(),
            width: 1.0,
            color: with_alpha(Color::BLACK, 0.18),
        },
        shadow: if selected {
            Shadow {
                color: with_alpha(color, 0.55),
                offset: Vector::ZERO,
                blur_radius: 8.0,
            }
        } else {
            Shadow::default()
        },
        ..button::Style::default()
    }
}

/// The swatch that clears the label color, drawn as an empty well rather than a fill.
pub fn clear_swatch(theme: &Theme, status: button::Status, selected: bool) -> button::Style {
    let text = theme.palette().text;
    let fill = match status {
        button::Status::Hovered => with_alpha(text, 0.10),
        button::Status::Pressed => with_alpha(text, 0.16),
        button::Status::Active | button::Status::Disabled => with_alpha(text, 0.05),
    };
    button::Style {
        background: Some(Background::Color(fill)),
        text_color: label_secondary(theme),
        border: Border {
            radius: 7.0.into(),
            width: if selected { 2.0 } else { 1.0 },
            color: if selected {
                label(theme)
            } else {
                theme::separator(theme)
            },
        },
        shadow: Shadow::default(),
        ..button::Style::default()
    }
}

/// The header of a disclosure group, such as the additional settings of a server.
pub fn disclosure(theme: &Theme, status: button::Status) -> button::Style {
    let text = theme.palette().text;
    let background = match status {
        button::Status::Hovered => with_alpha(text, 0.07),
        button::Status::Pressed => with_alpha(text, 0.12),
        button::Status::Active | button::Status::Disabled => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: label(theme),
        border: Border {
            radius: 8.0.into(),
            ..Border::default()
        },
        shadow: Shadow::default(),
        ..button::Style::default()
    }
}

// ---------------------------------------------------------------- structural surfaces

/// The panel itself: one rounded surface hanging from the menu bar.
///
/// The window around it is transparent, so these corners are the panel's real shape, and the system
/// shadow is computed from them.
pub fn panel(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(theme.palette().background)),
        text_color: Some(label(theme)),
        border: Border {
            radius: PANEL_RADIUS.into(),
            width: 1.0,
            color: if is_dark(theme) {
                color!(0x4A4B50)
            } else {
                with_alpha(Color::BLACK, 0.12)
            },
        },
        ..container::Style::default()
    }
}

/// The corner radius of the panel, shared by everything that has to follow its outline.
pub const PANEL_RADIUS: f32 = 12.0;

/// The list pane: a solid content surface, a step off the chrome around it.
pub fn pane(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(theme::pane(theme))),
        text_color: Some(label(theme)),
        ..container::Style::default()
    }
}

/// The hairlines that divide the header, sidebar, pane, and footer.
pub fn separator(theme: &Theme) -> rule::Style {
    rule::Style {
        color: theme::separator(theme),
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    }
}

/// A modal scrim that pushes the panel behind the active sheet, following the panel's corners.
pub fn modal_scrim(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(with_alpha(
            Color::BLACK,
            if is_dark(theme) { 0.45 } else { 0.20 },
        ))),
        border: Border {
            radius: PANEL_RADIUS.into(),
            ..Border::default()
        },
        ..container::Style::default()
    }
}

/// A sheet: the heaviest surface in the panel, so it reads as arriving above everything else.
pub fn sheet(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(theme::raised(theme, 0.06, 0.75))),
        text_color: Some(label(theme)),
        border: Border {
            radius: PANEL_RADIUS.into(),
            width: 1.0,
            color: theme::separator(theme),
        },
        shadow: Shadow {
            color: with_alpha(Color::BLACK, if is_dark(theme) { 0.50 } else { 0.20 }),
            offset: Vector::new(0.0, 12.0),
            blur_radius: 36.0,
        },
        ..container::Style::default()
    }
}

/// A tooltip bubble.
pub fn tooltip(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(theme::raised(theme, 0.10, 0.9))),
        text_color: Some(label(theme)),
        border: Border {
            radius: 6.0.into(),
            width: 1.0,
            color: theme::separator(theme),
        },
        shadow: Shadow {
            color: with_alpha(Color::BLACK, if is_dark(theme) { 0.4 } else { 0.14 }),
            offset: Vector::new(0.0, 3.0),
            blur_radius: 10.0,
        },
        ..container::Style::default()
    }
}

// ---------------------------------------------------------------- sidebar

/// A row of the source list. The selected row is filled with the accent, like a native sidebar;
/// the others only paint a faint wash while hovered or pressed.
pub fn source_list_row(theme: &Theme, status: button::Status, selected: bool) -> button::Style {
    let accent = theme::accent(theme);
    let (background, text_color) = if selected {
        let fill = match status {
            button::Status::Pressed => mix(accent, Color::BLACK, 0.12),
            _ => accent,
        };
        (fill, on_accent(accent))
    } else {
        let fill = match status {
            button::Status::Hovered => with_alpha(theme.palette().text, 0.06),
            button::Status::Pressed => with_alpha(theme.palette().text, 0.11),
            button::Status::Active | button::Status::Disabled => Color::TRANSPARENT,
        };
        (fill, label(theme))
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border {
            radius: 6.0.into(),
            ..Border::default()
        },
        shadow: Shadow::default(),
        ..button::Style::default()
    }
}

/// A server group's row: the same look as [`source_list_row`], drawn by a container because the
/// row is pressed and dragged rather than clicked. A row a dragged group would land on is outlined
/// in the accent.
pub fn source_list_surface(
    theme: &Theme,
    status: button::Status,
    selected: bool,
    drop_target: bool,
) -> container::Style {
    let row = source_list_row(theme, status, selected);
    container::Style {
        background: row.background,
        text_color: Some(row.text_color),
        border: if drop_target {
            Border {
                width: 2.0,
                color: theme::accent(theme),
                ..row.border
            }
        } else {
            row.border
        },
        ..container::Style::default()
    }
}

// ---------------------------------------------------------------- buttons

/// The filled accent button used for the single primary action on a surface.
pub fn primary_button(theme: &Theme, status: button::Status) -> button::Style {
    let accent = theme::accent(theme);
    let background = match status {
        button::Status::Hovered => mix(accent, Color::WHITE, 0.10),
        button::Status::Pressed => mix(accent, Color::BLACK, 0.16),
        button::Status::Disabled => with_alpha(accent, 0.35),
        button::Status::Active => accent,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: on_accent(accent),
        border: Border {
            radius: 7.0.into(),
            ..Border::default()
        },
        shadow: Shadow {
            color: with_alpha(Color::BLACK, 0.12),
            offset: Vector::new(0.0, 1.0),
            blur_radius: 1.5,
        },
        ..button::Style::default()
    }
}

/// The bordered push button used for cancel and other secondary actions.
pub fn secondary_button(theme: &Theme, status: button::Status) -> button::Style {
    let base = field_fill(theme);
    let text = theme.palette().text;
    let background = match status {
        button::Status::Hovered => mix(base, text, 0.05),
        button::Status::Pressed => mix(base, text, 0.11),
        button::Status::Active | button::Status::Disabled => base,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: label(theme),
        border: Border {
            radius: 7.0.into(),
            width: 1.0,
            color: if is_dark(theme) {
                color!(0x4A4B50)
            } else {
                with_alpha(Color::BLACK, 0.13)
            },
        },
        shadow: Shadow {
            color: with_alpha(Color::BLACK, if is_dark(theme) { 0.25 } else { 0.06 }),
            offset: Vector::new(0.0, 1.0),
            blur_radius: 1.0,
        },
        ..button::Style::default()
    }
}

/// The filled red button that confirms a destructive action.
pub fn destructive_button(theme: &Theme, status: button::Status) -> button::Style {
    let danger = theme.palette().danger;
    let background = match status {
        button::Status::Hovered => mix(danger, Color::WHITE, 0.10),
        button::Status::Pressed => mix(danger, Color::BLACK, 0.16),
        button::Status::Disabled => with_alpha(danger, 0.35),
        button::Status::Active => danger,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: on_accent(danger),
        border: Border {
            radius: 7.0.into(),
            ..Border::default()
        },
        shadow: Shadow::default(),
        ..button::Style::default()
    }
}

/// A borderless control that only paints a backplate while it is hovered or pressed: icon
/// buttons, and text actions such as the footer's add button.
pub fn icon_button(theme: &Theme, status: button::Status) -> button::Style {
    let text = theme.palette().text;
    let background = match status {
        button::Status::Hovered => with_alpha(text, 0.08),
        button::Status::Pressed => with_alpha(text, 0.14),
        button::Status::Active | button::Status::Disabled => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: label_secondary(theme),
        border: Border {
            radius: 6.0.into(),
            ..Border::default()
        },
        shadow: Shadow::default(),
        ..button::Style::default()
    }
}

/// The pin button, which stays visibly pressed while the panel is pinned open.
pub fn toggle_button(theme: &Theme, status: button::Status, on: bool) -> button::Style {
    if !on {
        return icon_button(theme, status);
    }
    let accent = theme::accent(theme);
    button::Style {
        background: Some(Background::Color(with_alpha(
            accent,
            if matches!(status, button::Status::Hovered) {
                0.22
            } else {
                0.16
            },
        ))),
        text_color: accent,
        border: Border {
            radius: 6.0.into(),
            ..Border::default()
        },
        shadow: Shadow::default(),
        ..button::Style::default()
    }
}

/// One segment of a segmented control. The selected segment is a raised plate on the recessed
/// track, with its label in the accent, as in `NSSegmentedControl`'s capsule style.
pub fn segment(theme: &Theme, status: button::Status, selected: bool) -> button::Style {
    if selected {
        return button::Style {
            background: Some(Background::Color(if is_dark(theme) {
                color!(0x46474C)
            } else {
                Color::WHITE
            })),
            text_color: if is_dark(theme) {
                label(theme)
            } else {
                theme::accent(theme)
            },
            border: Border {
                radius: 6.0.into(),
                width: if is_dark(theme) { 0.0 } else { 0.5 },
                color: with_alpha(Color::BLACK, 0.06),
            },
            shadow: Shadow {
                color: with_alpha(Color::BLACK, if is_dark(theme) { 0.35 } else { 0.10 }),
                offset: Vector::new(0.0, 1.0),
                blur_radius: 2.5,
            },
            ..button::Style::default()
        };
    }
    let text = theme.palette().text;
    let background = match status {
        button::Status::Hovered => with_alpha(text, 0.05),
        button::Status::Pressed => with_alpha(text, 0.10),
        button::Status::Active | button::Status::Disabled => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: label_secondary(theme),
        border: Border {
            radius: 6.0.into(),
            ..Border::default()
        },
        shadow: Shadow::default(),
        ..button::Style::default()
    }
}

/// A category pill: a capsule outlined in a quiet gray, or, when selected, outlined in the accent
/// over a faint wash of it with its label in the accent.
pub fn pill(theme: &Theme, status: button::Status, selected: bool) -> button::Style {
    let accent = theme::accent(theme);
    let dark = is_dark(theme);
    let high_contrast = theme::increased_contrast(theme);
    let radius = 14.0.into();
    if selected {
        return button::Style {
            background: Some(Background::Color(with_alpha(
                accent,
                if dark { 0.20 } else { 0.08 },
            ))),
            // On a dark ground the accent itself is too deep to read against its own wash.
            text_color: if dark {
                theme::mix(accent, Color::WHITE, 0.45)
            } else {
                accent
            },
            border: Border {
                radius,
                width: 1.5,
                color: accent,
            },
            ..button::Style::default()
        };
    }
    let text = theme.palette().text;
    let background = match status {
        button::Status::Hovered => with_alpha(text, 0.05),
        button::Status::Pressed => with_alpha(text, 0.10),
        button::Status::Active | button::Status::Disabled => Color::TRANSPARENT,
    };
    let outline = match (dark, high_contrast) {
        (_, true) => theme::separator(theme),
        (true, false) => color!(0x4A4B50),
        (false, false) => with_alpha(Color::BLACK, 0.16),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: label_secondary(theme),
        border: Border {
            radius,
            width: 1.0,
            color: outline,
        },
        ..button::Style::default()
    }
}

/// The recessed track a segmented control sits in.
pub fn segment_track(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(if is_dark(theme) {
            color!(0x1C1D20)
        } else {
            with_alpha(Color::BLACK, 0.055)
        })),
        // With Increase Contrast the control is outlined, so its extent never depends on a faint
        // fill alone.
        border: Border {
            radius: 8.0.into(),
            width: if theme::increased_contrast(theme) {
                1.0
            } else {
                0.0
            },
            color: theme::separator(theme),
        },
        ..container::Style::default()
    }
}

/// Marks a segment as somewhere a dragged launcher can land.
///
/// Available targets are outlined; the one under the pointer is also filled, so the answer to
/// "where would this land if I let go now" is a single obvious segment rather than a row of equals.
pub fn drop_target(theme: &Theme, base: &button::Style, engaged: bool) -> button::Style {
    let accent = theme::accent(theme);
    button::Style {
        background: if engaged {
            Some(Background::Color(with_alpha(accent, 0.22)))
        } else {
            base.background
        },
        text_color: if engaged { accent } else { base.text_color },
        border: Border {
            width: if engaged { 2.0 } else { 1.5 },
            color: with_alpha(accent, if engaged { 1.0 } else { 0.55 }),
            ..base.border
        },
        ..*base
    }
}

// ---------------------------------------------------------------- list rows

/// A launcher row. Rows carry no card of their own; the selected one is washed with the accent and
/// a hovered one with the label color, both inset with a small radius like a source-list row.
pub fn list_row(theme: &Theme, selected: bool, hovered: bool, held: bool) -> container::Style {
    let accent = theme::accent(theme);
    let background = if selected || held {
        with_alpha(accent, if is_dark(theme) { 0.26 } else { 0.13 })
    } else if hovered {
        with_alpha(theme.palette().text, 0.045)
    } else {
        Color::TRANSPARENT
    };
    container::Style {
        background: Some(Background::Color(background)),
        text_color: Some(label(theme)),
        // With Increase Contrast the selection is also outlined, so it never rests on a wash alone.
        border: Border {
            radius: 7.0.into(),
            width: if theme::increased_contrast(theme) && (selected || held) {
                1.5
            } else {
                0.0
            },
            color: accent,
        },
        ..container::Style::default()
    }
}

/// The row that follows the pointer during a drag: the same row lifted onto a plate of its own,
/// under a shadow, so it reads as held above the list rather than as a second copy in it.
pub fn dragged_row(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(theme::raised(theme, 0.08, 0.85))),
        text_color: Some(label(theme)),
        border: Border {
            radius: 8.0.into(),
            width: 1.5,
            color: theme::accent(theme),
        },
        shadow: Shadow {
            color: with_alpha(Color::BLACK, if is_dark(theme) { 0.55 } else { 0.22 }),
            offset: Vector::new(0.0, 8.0),
            blur_radius: 20.0,
        },
        snap: true,
    }
}

/// The rounded square a row's symbol sits on.
///
/// A launcher with a label color is tinted with it — a border in the color around the ordinary
/// plate, and the symbol in the color — rather than flooded, so the row stays readable.
pub fn icon_tile(theme: &Theme, color: Option<Color>) -> container::Style {
    container::Style {
        background: Some(Background::Color(match color {
            Some(color) => mix(field_fill(theme), color, 0.10),
            None => field_fill(theme),
        })),
        border: Border {
            radius: 7.0.into(),
            width: 1.0,
            color: match color {
                Some(color) => with_alpha(color, 0.55),
                None => theme::separator(theme),
            },
        },
        shadow: Shadow {
            color: with_alpha(Color::BLACK, if is_dark(theme) { 0.25 } else { 0.05 }),
            offset: Vector::new(0.0, 1.0),
            blur_radius: 1.5,
        },
        ..container::Style::default()
    }
}

/// An inline status line for launch results and persistence errors.
pub fn status_text(theme: &Theme, is_error: bool) -> text::Style {
    let palette = theme.palette();
    text::Style {
        color: Some(if is_error {
            palette.danger
        } else {
            label_secondary(theme)
        }),
    }
}

// ---------------------------------------------------------------- scrolling and menus

/// An overlay scrollbar: no rail, and a thin scroller that darkens while it is used, the way a
/// macOS scroller looks rather than a block of color down the side of the list.
pub fn scroller(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let text = theme.palette().text;
    let engaged = matches!(
        status,
        scrollable::Status::Hovered {
            is_vertical_scrollbar_hovered: true,
            ..
        } | scrollable::Status::Dragged { .. }
    );
    let rail = scrollable::Rail {
        background: None,
        border: Border::default(),
        scroller: scrollable::Scroller {
            background: Background::Color(with_alpha(text, if engaged { 0.45 } else { 0.22 })),
            border: Border {
                radius: 3.0.into(),
                ..Border::default()
            },
        },
    };
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        ..scrollable::default(theme, status)
    }
}

/// A pop-up button, such as the category chooser in the launcher editor.
pub fn popup_button(theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let hovered = matches!(
        status,
        pick_list::Status::Hovered | pick_list::Status::Opened { .. }
    );
    pick_list::Style {
        text_color: label(theme),
        placeholder_color: label_tertiary(theme),
        handle_color: label_secondary(theme),
        background: Background::Color(if hovered {
            mix(field_fill(theme), theme.palette().text, 0.03)
        } else {
            field_fill(theme)
        }),
        border: Border {
            radius: 7.0.into(),
            width: 1.0,
            color: theme::separator(theme),
        },
    }
}

/// The menu a pop-up button opens.
pub fn menu(theme: &Theme) -> overlay::menu::Style {
    let accent = theme::accent(theme);
    overlay::menu::Style {
        background: Background::Color(theme::raised(theme, 0.08, 0.9)),
        border: Border {
            radius: 7.0.into(),
            width: 1.0,
            color: theme::separator(theme),
        },
        text_color: label(theme),
        selected_text_color: on_accent(accent),
        selected_background: Background::Color(accent),
        shadow: Shadow {
            color: with_alpha(Color::BLACK, if is_dark(theme) { 0.45 } else { 0.16 }),
            offset: Vector::new(0.0, 6.0),
            blur_radius: 16.0,
        },
    }
}

// ---------------------------------------------------------------- fields

/// The container that holds the search symbol and its borderless input.
pub fn search_field(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(field_fill(theme))),
        border: Border {
            radius: 7.0.into(),
            width: 1.0,
            color: theme::separator(theme),
        },
        shadow: Shadow {
            color: with_alpha(Color::BLACK, if is_dark(theme) { 0.2 } else { 0.03 }),
            offset: Vector::new(0.0, 1.0),
            blur_radius: 1.0,
        },
        ..container::Style::default()
    }
}

/// A read-only value shown in a field-shaped well, such as the config path in Settings.
pub fn value_well(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(field_fill(theme))),
        border: Border {
            radius: 7.0.into(),
            width: 1.0,
            color: theme::separator(theme),
        },
        ..container::Style::default()
    }
}

/// A text input with no chrome of its own, for use inside a styled field container.
pub fn borderless_input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut style = form_input(theme, status, false);
    style.background = Background::Color(Color::TRANSPARENT);
    style.border = Border::default();
    style
}

/// A labelled editor field, with a red border while its value is invalid.
pub fn form_input(theme: &Theme, status: text_input::Status, has_error: bool) -> text_input::Style {
    let accent = theme::accent(theme);
    let focused = matches!(status, text_input::Status::Focused { .. });
    let border_color = if has_error {
        theme.palette().danger
    } else if focused {
        accent
    } else {
        match status {
            text_input::Status::Hovered => with_alpha(theme.palette().text, 0.24),
            _ => theme::separator(theme),
        }
    };
    text_input::Style {
        background: Background::Color(field_fill(theme)),
        border: Border {
            radius: 7.0.into(),
            width: if has_error || focused { 2.0 } else { 1.0 },
            color: border_color,
        },
        icon: label_tertiary(theme),
        placeholder: label_tertiary(theme),
        value: label(theme),
        selection: with_alpha(accent, 0.35),
    }
}

/// The multi-line argument editor.
pub fn form_editor(
    theme: &Theme,
    status: text_editor::Status,
    has_error: bool,
) -> text_editor::Style {
    let accent = theme::accent(theme);
    let focused = matches!(status, text_editor::Status::Focused { .. });
    let border_color = if has_error {
        theme.palette().danger
    } else if focused {
        accent
    } else {
        match status {
            text_editor::Status::Hovered => with_alpha(theme.palette().text, 0.24),
            _ => theme::separator(theme),
        }
    };
    text_editor::Style {
        background: Background::Color(field_fill(theme)),
        border: Border {
            radius: 7.0.into(),
            width: if has_error || focused { 2.0 } else { 1.0 },
            color: border_color,
        },
        placeholder: label_tertiary(theme),
        value: label(theme),
        selection: with_alpha(accent, 0.35),
    }
}

// ---------------------------------------------------------------- text roles

/// Supporting detail text.
pub fn secondary_text(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(label_secondary(theme)),
    }
}

/// Counts, hints, and other de-emphasized text.
pub fn tertiary_text(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(label_tertiary(theme)),
    }
}

/// The de-emphasized text on a row filled with the system accent, such as its launcher count.
pub fn on_accent_secondary_text(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(with_alpha(on_accent(theme::accent(theme)), 0.78)),
    }
}

/// A heading in the source list, such as "Groups".
pub fn section_heading_text(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(with_alpha(theme.palette().text, 0.50)),
    }
}

/// Confirmation that something checked out, such as a reachable host.
pub fn success_text(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.palette().success),
    }
}

/// Inline validation messages.
pub fn error_text(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.palette().danger),
    }
}

// ---------------------------------------------------------------- helpers

fn field_fill(theme: &Theme) -> Color {
    if is_dark(theme) {
        color!(0x37383C)
    } else {
        Color::WHITE
    }
}

/// Picks black or white text for a filled control, so accent colors stay legible.
pub fn on_accent(accent: Color) -> Color {
    if accent
        .r
        .mul_add(0.299, accent.g.mul_add(0.587, accent.b * 0.114))
        > 0.65
    {
        Color::BLACK
    } else {
        Color::WHITE
    }
}

#[cfg(test)]
mod tests {
    use iced::widget::button;
    use iced::{Color, Theme};

    use crate::theme::Appearance;

    use super::{list_row, on_accent, pane, segment, source_list_row};

    fn themes() -> [Theme; 2] {
        [
            Appearance {
                dark: true,
                ..Appearance::default()
            }
            .theme(),
            Appearance::default().theme(),
        ]
    }

    #[test]
    fn light_and_dark_panes_never_share_a_hard_coded_surface_color() {
        let [dark, light] = themes();

        assert_ne!(pane(&dark).background, pane(&light).background);
    }

    #[test]
    fn a_selected_source_list_row_is_filled_with_the_system_accent() {
        for theme in themes() {
            let selected = source_list_row(&theme, button::Status::Active, true);
            let hovered = source_list_row(&theme, button::Status::Hovered, false);

            assert_eq!(
                selected.background,
                Some(iced::Background::Color(crate::theme::accent(&theme)))
            );
            assert_ne!(selected.background, hovered.background);
            assert_eq!(selected.text_color, on_accent(crate::theme::accent(&theme)));
        }
    }

    #[test]
    fn the_selected_segment_is_raised_off_its_track() {
        for theme in themes() {
            let selected = segment(&theme, button::Status::Active, true);
            let resting = segment(&theme, button::Status::Active, false);

            assert_ne!(selected.background, resting.background);
            assert!(selected.shadow.blur_radius > resting.shadow.blur_radius);
        }
    }

    #[test]
    fn a_selected_row_is_marked_by_more_than_its_hover_wash() {
        for theme in themes() {
            assert_ne!(
                list_row(&theme, true, false, false).background,
                list_row(&theme, false, true, false).background
            );
            assert_eq!(
                list_row(&theme, false, false, false).background,
                Some(iced::Background::Color(Color::TRANSPARENT))
            );
        }
    }

    #[test]
    fn filled_controls_pick_the_legible_label_color_for_their_fill() {
        assert_eq!(on_accent(Color::BLACK), Color::WHITE);
        assert_eq!(on_accent(Color::from_rgb(0.0, 0.48, 1.0)), Color::WHITE);
        assert_eq!(on_accent(Color::WHITE), Color::BLACK);
        assert_eq!(on_accent(Color::from_rgb(1.0, 0.85, 0.2)), Color::BLACK);
    }
}
