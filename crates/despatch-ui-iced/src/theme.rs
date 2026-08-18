#![expect(
    clippy::unreadable_literal,
    reason = "six-digit hex is the canonical, reviewable way to write a color"
)]

use despatch_application::AppearanceChoice;
use iced::theme::Palette;
use iced::{Color, Theme, color};

/// The two colors the whole interface is built from.
///
/// The appearance decides which is the ground and which is the mark: dark mode paints [`INK`] and
/// highlights in [`PARCHMENT`], and light mode swaps them. There is no third color and no system
/// accent — an arbitrary accent dropped onto this pair reads as an unrelated color.
const INK: Color = color!(0x03191E);
const PARCHMENT: Color = color!(0xF2F5EA);

/// The macOS appearance the theme was built from.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Appearance {
    pub dark: bool,
}

impl Appearance {
    /// Resolves the user's choice against the system setting.
    #[must_use]
    pub fn resolve(choice: AppearanceChoice) -> Self {
        match choice {
            AppearanceChoice::System => Self::system(),
            AppearanceChoice::Light => Self { dark: false },
            AppearanceChoice::Dark => Self { dark: true },
        }
    }

    /// Reads the current system interface style.
    #[must_use]
    pub fn system() -> Self {
        #[cfg(target_os = "macos")]
        {
            Self {
                dark: despatch_platform_macos::system_appearance().dark,
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            Self::default()
        }
    }

    /// Builds the Despatch theme for this appearance.
    #[must_use]
    pub fn theme(self) -> Theme {
        let (background, foreground) = if self.dark {
            (INK, PARCHMENT)
        } else {
            (PARCHMENT, INK)
        };
        let palette = Palette {
            background,
            text: foreground,
            primary: foreground,
            success: if self.dark {
                color!(0x30D158)
            } else {
                color!(0x28A745)
            },
            warning: if self.dark {
                color!(0xFF9F0A)
            } else {
                color!(0xC77700)
            },
            danger: if self.dark {
                color!(0xFF453A)
            } else {
                color!(0xD70015)
            },
        };
        Theme::custom(
            if self.dark {
                "Despatch Dark"
            } else {
                "Despatch Light"
            },
            palette,
        )
    }
}

/// Reports whether `theme` uses a dark background, so surfaces know which way to elevate.
#[must_use]
pub fn is_dark(theme: &Theme) -> bool {
    let background = theme.palette().background;
    background
        .r
        .mul_add(0.2126, background.g.mul_add(0.7152, background.b * 0.0722))
        < 0.5
}

/// Raises a surface above the window background.
///
/// A dark surface is lifted toward the parchment it is paired with rather than toward pure white,
/// which would wash the teal out to grey and make the app look like it had a third, colder color
/// in it. A light surface is lifted toward white, which is already the direction of its own paper.
#[must_use]
pub fn raised(theme: &Theme, dark_amount: f32, light_amount: f32) -> Color {
    let background = theme.palette().background;
    if is_dark(theme) {
        mix(background, PARCHMENT, dark_amount)
    } else {
        mix(background, Color::WHITE, light_amount)
    }
}

/// Lowers a surface below the window background, used for structural chrome.
#[must_use]
pub fn lowered(theme: &Theme, dark_amount: f32, light_amount: f32) -> Color {
    let background = theme.palette().background;
    if is_dark(theme) {
        mix(background, Color::BLACK, dark_amount)
    } else {
        mix(background, Color::BLACK, light_amount)
    }
}

/// The primary label color.
#[must_use]
pub fn label(theme: &Theme) -> Color {
    theme.palette().text
}

/// The secondary label color used for supporting detail text.
#[must_use]
pub fn label_secondary(theme: &Theme) -> Color {
    with_alpha(
        theme.palette().text,
        if is_dark(theme) { 0.62 } else { 0.58 },
    )
}

/// The tertiary label color used for counts, hints, and placeholder text.
#[must_use]
pub fn label_tertiary(theme: &Theme) -> Color {
    with_alpha(
        theme.palette().text,
        if is_dark(theme) { 0.42 } else { 0.40 },
    )
}

/// The hairline separator color.
#[must_use]
pub fn separator(theme: &Theme) -> Color {
    with_alpha(
        theme.palette().text,
        if is_dark(theme) { 0.14 } else { 0.10 },
    )
}

/// The accent color.
#[must_use]
pub fn accent(theme: &Theme) -> Color {
    theme.palette().primary
}

/// Returns `color` with the given alpha.
#[must_use]
pub const fn with_alpha(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

/// Blends `amount` of `second` into `first`, keeping the alpha of `first`.
#[must_use]
pub fn mix(first: Color, second: Color, amount: f32) -> Color {
    Color {
        r: first.r + (second.r - first.r) * amount,
        g: first.g + (second.g - first.g) * amount,
        b: first.b + (second.b - first.b) * amount,
        a: first.a,
    }
}

#[cfg(test)]
mod tests {
    use iced::Color;

    use super::{Appearance, is_dark, label_secondary, lowered, raised};

    fn appearance(dark: bool) -> Appearance {
        Appearance { dark }
    }

    #[test]
    fn dark_and_light_themes_are_detected_from_their_background() {
        assert!(is_dark(&appearance(true).theme()));
        assert!(!is_dark(&appearance(false).theme()));
    }

    #[test]
    fn raised_surfaces_stay_brighter_than_lowered_chrome_in_both_appearances() {
        for dark in [true, false] {
            let theme = appearance(dark).theme();
            let surface = raised(&theme, 0.07, 0.6);
            let chrome = lowered(&theme, 0.35, 0.04);

            assert!(
                luminance(surface) > luminance(chrome),
                "raised surface must read above chrome in {} mode",
                if dark { "dark" } else { "light" }
            );
        }
    }

    #[test]
    fn secondary_labels_never_hard_code_an_appearance_specific_color() {
        let dark = label_secondary(&appearance(true).theme());
        let light = label_secondary(&appearance(false).theme());

        assert_ne!(dark, light);
        assert!(dark.a < 1.0 && light.a < 1.0);
    }

    fn luminance(color: Color) -> f32 {
        color
            .r
            .mul_add(0.2126, color.g.mul_add(0.7152, color.b * 0.0722))
    }
}
