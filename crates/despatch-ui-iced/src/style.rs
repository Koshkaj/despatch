#![expect(
    clippy::unreadable_literal,
    reason = "six-digit hex is the canonical, reviewable way to write a color"
)]

use despatch_domain::LabelColor;
use iced::widget::{button, container, rule, text, text_editor, text_input};
use iced::{Background, Border, Color, Shadow, Theme, Vector, color};

use crate::theme::{self, is_dark, label, label_secondary, label_tertiary, mix, with_alpha};

/// The saturated backplate colors tiles use to tell launcher kinds apart at a glance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Accent {
    Blue,
    Indigo,
    Teal,
    Green,
    Orange,
    Gray,
}

impl Accent {
    pub fn color(self, theme: &Theme) -> Color {
        let dark = is_dark(theme);
        match self {
            Self::Blue if dark => color!(0x0A84FF),
            Self::Blue => color!(0x007AFF),
            Self::Indigo if dark => color!(0x5E5CE6),
            Self::Indigo => color!(0x5856D6),
            Self::Teal if dark => color!(0x40C8E0),
            Self::Teal => color!(0x00A2C7),
            Self::Green if dark => color!(0x30D158),
            Self::Green => color!(0x28A745),
            Self::Orange if dark => color!(0xFF9F0A),
            Self::Orange => color!(0xE8790B),
            Self::Gray if dark => color!(0x8E8E93),
            Self::Gray => color!(0x6E6E73),
        }
    }
}

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

/// The window background behind the sidebar and the content column.
pub fn window(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(theme.palette().background)),
        text_color: Some(label(theme)),
        ..container::Style::default()
    }
}

/// The translucent sidebar chrome, separated from content by a hairline.
pub fn sidebar(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(theme::lowered(theme, 0.30, 0.045))),
        border: Border::default(),
        ..container::Style::default()
    }
}

/// The content column: the palette's own ground, flat.
///
/// It carries no gradient and no lift. This is the largest surface in the window, so anything
/// mixed into it is read as the app's color rather than as depth, and a wash toward white turned
/// the ground grey next to the true dark of the rail.
pub fn content(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(theme.palette().background)),
        ..container::Style::default()
    }
}

/// The hairline that separates the sidebar from the content column.
pub fn separator(theme: &Theme) -> rule::Style {
    rule::Style {
        color: theme::separator(theme),
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    }
}

/// A modal scrim that pushes the app behind the active sheet.
pub fn modal_scrim(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(with_alpha(
            Color::BLACK,
            if is_dark(theme) { 0.52 } else { 0.28 },
        ))),
        ..container::Style::default()
    }
}

/// A sheet: the heaviest surface in the app, so it reads as arriving above everything else.
pub fn sheet(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(theme::raised(theme, 0.10, 0.9))),
        text_color: Some(label(theme)),
        border: Border {
            radius: 14.0.into(),
            width: 1.0,
            color: with_alpha(Color::WHITE, if is_dark(theme) { 0.10 } else { 0.0 }),
        },
        shadow: Shadow {
            color: with_alpha(Color::BLACK, if is_dark(theme) { 0.55 } else { 0.24 }),
            offset: Vector::new(0.0, 18.0),
            blur_radius: 48.0,
        },
        ..container::Style::default()
    }
}

/// An inline status banner for launch results and persistence errors.
pub fn status(theme: &Theme, is_error: bool) -> container::Style {
    let palette = theme.extended_palette();
    let accent = if is_error {
        palette.danger.base.color
    } else {
        palette.success.base.color
    };
    container::Style {
        text_color: Some(accent),
        background: Some(Background::Color(with_alpha(accent, 0.12))),
        border: Border {
            radius: 9.0.into(),
            width: 1.0,
            color: with_alpha(accent, 0.30),
        },
        ..container::Style::default()
    }
}

// ---------------------------------------------------------------- sidebar

/// A group row in the sidebar; the selected row uses the system accent like a native source list.
pub fn sidebar_row(theme: &Theme, status: button::Status, selected: bool) -> button::Style {
    let accent = theme::accent(theme);
    let (background, text_color) = if selected {
        (accent, on_accent(accent))
    } else {
        let fill = match status {
            button::Status::Hovered => with_alpha(theme.palette().text, 0.07),
            button::Status::Pressed => with_alpha(theme.palette().text, 0.12),
            button::Status::Active | button::Status::Disabled => Color::TRANSPARENT,
        };
        (fill, label(theme))
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border {
            radius: 8.0.into(),
            ..Border::default()
        },
        shadow: Shadow::default(),
        ..button::Style::default()
    }
}

// ---------------------------------------------------------------- buttons

/// The filled accent button used for the single primary action on a surface.
pub fn primary_button(theme: &Theme, status: button::Status) -> button::Style {
    let accent = theme::accent(theme);
    let background = match status {
        button::Status::Hovered => mix(accent, Color::WHITE, 0.12),
        button::Status::Pressed => mix(accent, Color::BLACK, 0.16),
        button::Status::Disabled => with_alpha(accent, 0.35),
        button::Status::Active => accent,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: on_accent(accent),
        border: Border {
            radius: 8.0.into(),
            ..Border::default()
        },
        shadow: Shadow::default(),
        ..button::Style::default()
    }
}

/// The bordered button used for cancel and other secondary actions.
pub fn secondary_button(theme: &Theme, status: button::Status) -> button::Style {
    let text = theme.palette().text;
    let background = match status {
        button::Status::Hovered => with_alpha(text, 0.10),
        button::Status::Pressed => with_alpha(text, 0.16),
        button::Status::Active | button::Status::Disabled => with_alpha(text, 0.05),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: label(theme),
        border: Border {
            radius: 8.0.into(),
            width: 1.0,
            color: theme::separator(theme),
        },
        shadow: Shadow::default(),
        ..button::Style::default()
    }
}

/// The filled red button that confirms a destructive action.
pub fn destructive_button(theme: &Theme, status: button::Status) -> button::Style {
    let danger = theme.palette().danger;
    let background = match status {
        button::Status::Hovered => mix(danger, Color::WHITE, 0.12),
        button::Status::Pressed => mix(danger, Color::BLACK, 0.16),
        button::Status::Disabled => with_alpha(danger, 0.35),
        button::Status::Active => danger,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: on_accent(danger),
        border: Border {
            radius: 8.0.into(),
            ..Border::default()
        },
        shadow: Shadow::default(),
        ..button::Style::default()
    }
}

/// A borderless icon control that only paints a backplate while it is hovered or pressed.
pub fn icon_button(theme: &Theme, status: button::Status) -> button::Style {
    let text = theme.palette().text;
    let background = match status {
        button::Status::Hovered => with_alpha(text, 0.10),
        button::Status::Pressed => with_alpha(text, 0.17),
        button::Status::Active | button::Status::Disabled => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: label_secondary(theme),
        border: Border {
            radius: 8.0.into(),
            ..Border::default()
        },
        shadow: Shadow::default(),
        ..button::Style::default()
    }
}

/// A segment of the provider picker, styled as one cell of a macOS segmented control.
pub fn segment(theme: &Theme, status: button::Status, selected: bool) -> button::Style {
    let accent = theme::accent(theme);
    if selected {
        return button::Style {
            background: Some(Background::Color(accent)),
            text_color: on_accent(accent),
            border: Border {
                radius: 7.0.into(),
                ..Border::default()
            },
            shadow: Shadow {
                color: with_alpha(Color::BLACK, 0.18),
                offset: Vector::new(0.0, 1.0),
                blur_radius: 3.0,
            },
            ..button::Style::default()
        };
    }
    let text = theme.palette().text;
    let background = match status {
        button::Status::Hovered => with_alpha(text, 0.08),
        button::Status::Pressed => with_alpha(text, 0.14),
        button::Status::Active | button::Status::Disabled => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: label(theme),
        border: Border {
            radius: 7.0.into(),
            ..Border::default()
        },
        shadow: Shadow::default(),
        ..button::Style::default()
    }
}

/// The recessed track a segmented control sits in.
pub fn segment_track(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(field_fill(theme))),
        border: Border {
            radius: 9.0.into(),
            width: 1.0,
            color: theme::separator(theme),
        },
        ..container::Style::default()
    }
}

/// A category chip: a compact, content-sized control that sits one level below a group card.
///
/// A category with a label color is tinted rather than flooded with it: a translucent wash of the
/// color, a border in the same color, and the ordinary label color for the text. Opening the
/// category deepens the wash and thickens the border instead of inverting the chip, which keeps
/// the name readable and the color recognizable in every state.
pub fn chip(
    theme: &Theme,
    status: button::Status,
    selected: bool,
    color: Option<Color>,
) -> button::Style {
    if let Some(color) = color {
        let resting = if selected { 0.26 } else { 0.11 };
        let wash = match status {
            button::Status::Hovered => resting + 0.09,
            button::Status::Pressed => resting + 0.16,
            button::Status::Active | button::Status::Disabled => resting,
        };
        return button::Style {
            background: Some(Background::Color(with_alpha(color, wash))),
            text_color: label(theme),
            border: Border {
                radius: 9.0.into(),
                width: if selected { 2.0 } else { 1.0 },
                color: with_alpha(color, if selected { 0.9 } else { 0.4 }),
            },
            shadow: Shadow::default(),
            ..button::Style::default()
        };
    }
    let accent = theme::accent(theme);
    if selected {
        let background = match status {
            button::Status::Hovered => mix(accent, Color::WHITE, 0.10),
            button::Status::Pressed => mix(accent, Color::BLACK, 0.14),
            button::Status::Active | button::Status::Disabled => accent,
        };
        return button::Style {
            background: Some(Background::Color(background)),
            text_color: on_accent(accent),
            border: Border {
                radius: 9.0.into(),
                ..Border::default()
            },
            shadow: Shadow::default(),
            ..button::Style::default()
        };
    }
    let text = theme.palette().text;
    let background = match status {
        button::Status::Hovered => with_alpha(text, 0.10),
        button::Status::Pressed => with_alpha(text, 0.15),
        button::Status::Active | button::Status::Disabled => with_alpha(text, 0.05),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: label(theme),
        border: Border {
            radius: 9.0.into(),
            width: 1.0,
            color: theme::separator(theme),
        },
        shadow: Shadow::default(),
        ..button::Style::default()
    }
}

/// Marks a chip as somewhere a dragged launcher can land.
///
/// Available targets are outlined; the one under the pointer is also filled, so the answer to
/// "where would this land if I let go now" is a single obvious chip rather than a row of equals.
pub fn drop_target(theme: &Theme, base: &button::Style, engaged: bool) -> button::Style {
    let accent = theme::accent(theme);
    button::Style {
        background: if engaged {
            Some(Background::Color(with_alpha(accent, 0.22)))
        } else {
            base.background
        },
        border: Border {
            width: if engaged { 2.0 } else { 1.5 },
            color: with_alpha(accent, if engaged { 1.0 } else { 0.55 }),
            ..base.border
        },
        ..*base
    }
}

/// The quiet chip that adds a category, kept lighter than the real ones.
pub fn add_chip(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = chip(theme, status, false, None);
    style.background = match status {
        button::Status::Hovered | button::Status::Pressed => style.background,
        button::Status::Active | button::Status::Disabled => None,
    };
    style.text_color = label_secondary(theme);
    style
}

// ---------------------------------------------------------------- tiles

/// A launcher tile: a solid, readable surface with a soft elevation shadow.
///
/// A card carrying a label color is washed with it. The wash is mixed into the surface rather than
/// laid over it with alpha, so the card stays opaque above the content gradient and its text keeps
/// the contrast it has on every other card.
pub fn tile(
    theme: &Theme,
    status: button::Status,
    selected: bool,
    color: Option<Color>,
) -> button::Style {
    let accent = color.unwrap_or_else(|| theme::accent(theme));
    let base = match color {
        Some(color) => mix(
            theme::raised(theme, 0.09, 1.0),
            color,
            if selected { 0.24 } else { 0.14 },
        ),
        None => theme::raised(theme, 0.09, 1.0),
    };
    let background = match (status, color) {
        (button::Status::Hovered, None) => theme::raised(theme, 0.13, 0.94),
        (button::Status::Hovered, Some(color)) => mix(base, color, 0.10),
        (button::Status::Pressed, _) => mix(base, accent, 0.14),
        (button::Status::Active | button::Status::Disabled, _) => base,
    };
    let shadow = match status {
        button::Status::Pressed => Shadow {
            color: with_alpha(Color::BLACK, shadow_strength(theme, 0.5)),
            offset: Vector::new(0.0, 1.0),
            blur_radius: 3.0,
        },
        button::Status::Hovered => Shadow {
            color: with_alpha(Color::BLACK, shadow_strength(theme, 1.4)),
            offset: Vector::new(0.0, 6.0),
            blur_radius: 16.0,
        },
        button::Status::Active | button::Status::Disabled => Shadow {
            color: with_alpha(Color::BLACK, shadow_strength(theme, 1.0)),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: label(theme),
        border: Border {
            radius: 14.0.into(),
            width: if selected { 2.0 } else { 1.0 },
            color: match (selected, color) {
                (true, _) => accent,
                (false, Some(color)) => with_alpha(color, 0.45),
                (false, None) => with_alpha(
                    theme.palette().text,
                    if is_dark(theme) { 0.09 } else { 0.07 },
                ),
            },
        },
        shadow,
        ..button::Style::default()
    }
}

/// A launcher card's surface.
///
/// The card is not a button — it has to tell a click, a double click, and the start of a drag
/// apart, and a button consumes the press before any of that can be decided — so it draws itself
/// through a container. The appearance is still [`tile`]'s: one definition of what a card looks
/// like, whichever widget happens to be carrying it.
pub fn tile_surface(
    theme: &Theme,
    status: button::Status,
    selected: bool,
    color: Option<Color>,
) -> container::Style {
    let surface = tile(theme, status, selected, color);
    container::Style {
        background: surface.background,
        text_color: Some(label(theme)),
        border: surface.border,
        shadow: surface.shadow,
        snap: true,
    }
}

/// The card that follows the pointer during a drag.
///
/// It is the hovered card, lifted: the same surface under a deeper shadow, so it reads as being
/// held above the grid rather than as a second copy sitting in it.
pub fn dragged_tile(theme: &Theme, color: Option<Color>) -> container::Style {
    let surface = tile(theme, button::Status::Hovered, true, color);
    container::Style {
        background: surface.background,
        text_color: Some(label(theme)),
        border: surface.border,
        shadow: Shadow {
            color: with_alpha(Color::BLACK, shadow_strength(theme, 2.2)),
            offset: Vector::new(0.0, 10.0),
            blur_radius: 24.0,
        },
        snap: true,
    }
}

/// The add tile: deliberately quieter than a real launcher, but still keyboard reachable.
pub fn add_tile(theme: &Theme, status: button::Status) -> button::Style {
    let text = theme.palette().text;
    let background = match status {
        button::Status::Hovered => with_alpha(text, 0.06),
        button::Status::Pressed => with_alpha(text, 0.11),
        button::Status::Active | button::Status::Disabled => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: label_secondary(theme),
        border: Border {
            radius: 14.0.into(),
            width: 1.0,
            color: with_alpha(text, if is_dark(theme) { 0.17 } else { 0.14 }),
        },
        shadow: Shadow::default(),
        ..button::Style::default()
    }
}

/// The quiet backplate on the add tile, aligned with the symbol backplates next to it.
pub fn add_badge(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(with_alpha(theme.palette().text, 0.06))),
        border: Border {
            radius: 12.0.into(),
            width: 1.0,
            color: theme::separator(theme),
        },
        ..container::Style::default()
    }
}

/// The rounded backplate behind a tile's symbol.
pub fn badge(color: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(color)),
        border: Border {
            radius: 12.0.into(),
            ..Border::default()
        },
        shadow: Shadow {
            color: with_alpha(color, 0.22),
            offset: Vector::new(0.0, 1.0),
            blur_radius: 4.0,
        },
        ..container::Style::default()
    }
}

// ---------------------------------------------------------------- fields

/// The container that holds the search symbol and its borderless input.
pub fn search_field(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(field_fill(theme))),
        border: Border {
            radius: 8.0.into(),
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

/// The de-emphasized text on a chip filled with the system accent, such as its launcher count.
pub fn on_chip_text(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(with_alpha(on_accent(theme::accent(theme)), 0.75)),
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
        with_alpha(Color::BLACK, 0.22)
    } else {
        with_alpha(Color::WHITE, 0.85)
    }
}

fn shadow_strength(theme: &Theme, scale: f32) -> f32 {
    (if is_dark(theme) { 0.30 } else { 0.09 }) * scale
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

    use super::{Accent, on_accent, sidebar_row, tile};

    fn themes() -> [Theme; 2] {
        [
            Appearance { dark: true }.theme(),
            Appearance { dark: false }.theme(),
        ]
    }

    #[test]
    fn light_and_dark_tiles_never_share_a_hard_coded_surface_color() {
        let [dark, light] = themes();

        assert_ne!(
            tile(&dark, button::Status::Active, false, None).background,
            tile(&light, button::Status::Active, false, None).background
        );
    }

    #[test]
    fn a_selected_sidebar_row_is_filled_with_the_system_accent() {
        for theme in themes() {
            let selected = sidebar_row(&theme, button::Status::Active, true);
            let hovered = sidebar_row(&theme, button::Status::Hovered, false);

            assert_eq!(
                selected.background,
                Some(iced::Background::Color(crate::theme::accent(&theme)))
            );
            assert_ne!(selected.background, hovered.background);
            assert_eq!(selected.text_color, on_accent(crate::theme::accent(&theme)));
        }
    }

    #[test]
    fn a_selected_tile_gains_a_visible_focus_ring() {
        for theme in themes() {
            assert!(
                tile(&theme, button::Status::Active, true, None)
                    .border
                    .width
                    > tile(&theme, button::Status::Active, false, None)
                        .border
                        .width
            );
        }
    }

    #[test]
    fn every_launcher_kind_gets_a_distinct_backplate_in_both_appearances() {
        for theme in themes() {
            let mut colors = Vec::new();
            for accent in [
                Accent::Blue,
                Accent::Indigo,
                Accent::Teal,
                Accent::Green,
                Accent::Orange,
                Accent::Gray,
            ] {
                let color = accent.color(&theme);
                assert!(
                    !colors.contains(&color.into_rgba8()),
                    "two launcher kinds share the backplate {color:?}"
                );
                colors.push(color.into_rgba8());
            }
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
