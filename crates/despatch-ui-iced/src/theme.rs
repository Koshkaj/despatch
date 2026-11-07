#![expect(
    clippy::unreadable_literal,
    reason = "six-digit hex is the canonical, reviewable way to write a color"
)]

use despatch_application::AppearanceChoice;
use iced::theme::Palette;
use iced::{Color, Theme, color};

/// The macOS appearance the theme was built from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Appearance {
    pub dark: bool,
    /// The user's system accent color, which selection and focus are drawn in.
    pub accent: Color,
    /// Accessibility → Display → Increase contrast: stronger hairlines, borders, and secondary text.
    pub increase_contrast: bool,
    /// Accessibility → Display → Reduce motion: the panel appears without fading in.
    pub reduce_motion: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            dark: false,
            accent: SYSTEM_BLUE,
            increase_contrast: false,
            reduce_motion: false,
        }
    }
}

/// The accent used when the system one cannot be read: the default macOS blue.
const SYSTEM_BLUE: Color = color!(0x007AFF);

/// The panel's ground in light mode: chrome, sidebar, and footer, a hair off white like an
/// `AppKit` popover.
const LIGHT_GROUND: Color = color!(0xF4F5F7);
/// The list pane in light mode, a step brighter than the chrome around it.
const LIGHT_PANE: Color = color!(0xFAFAFB);
const LIGHT_LABEL: Color = color!(0x1D1D1F);
const DARK_GROUND: Color = color!(0x2A2B2F);
const DARK_PANE: Color = color!(0x232427);
const DARK_LABEL: Color = color!(0xF2F2F4);

impl Appearance {
    /// Resolves the user's choice against the system setting.
    #[must_use]
    pub fn resolve(choice: AppearanceChoice) -> Self {
        let system = Self::system();
        match choice {
            AppearanceChoice::System => system,
            AppearanceChoice::Light => Self {
                dark: false,
                ..system
            },
            AppearanceChoice::Dark => Self {
                dark: true,
                ..system
            },
        }
    }

    /// Reads the current system interface style and accent color.
    #[must_use]
    pub fn system() -> Self {
        #[cfg(target_os = "macos")]
        {
            let system = despatch_platform_macos::system_appearance();
            let [red, green, blue] = system.accent;
            Self {
                dark: system.dark,
                accent: Color::from_rgb(red, green, blue),
                increase_contrast: system.increase_contrast,
                reduce_motion: system.reduce_motion,
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
        let (background, text) = if self.dark {
            (DARK_GROUND, DARK_LABEL)
        } else {
            (LIGHT_GROUND, LIGHT_LABEL)
        };
        let palette = Palette {
            background,
            text,
            primary: self.accent,
            success: if self.dark {
                color!(0x30D158)
            } else {
                color!(0x248A3D)
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
        let name = match (self.dark, self.increase_contrast) {
            (false, false) => "Despatch Light",
            (true, false) => "Despatch Dark",
            (false, true) => LIGHT_INCREASED_CONTRAST,
            (true, true) => DARK_INCREASED_CONTRAST,
        };
        Theme::custom(name, palette)
    }
}

const LIGHT_INCREASED_CONTRAST: &str = "Despatch Light, Increased Contrast";
const DARK_INCREASED_CONTRAST: &str = "Despatch Dark, Increased Contrast";

/// Reports whether `theme` was built for Increase Contrast.
///
/// An Iced theme carries a palette and a name and nothing else, so the setting travels in the
/// name: the two contrast themes are distinct themes, which is also what makes Iced redraw when the
/// setting changes.
#[must_use]
pub fn increased_contrast(theme: &Theme) -> bool {
    let name = theme.to_string();
    name == LIGHT_INCREASED_CONTRAST || name == DARK_INCREASED_CONTRAST
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

/// The list pane: the content surface, solid, so rows read against it rather than the material.
#[must_use]
pub fn pane(theme: &Theme) -> Color {
    if is_dark(theme) {
        DARK_PANE
    } else {
        LIGHT_PANE
    }
}

/// Raises a surface above the panel ground, used for sheets and the selected segment.
#[must_use]
pub fn raised(theme: &Theme, dark_amount: f32, light_amount: f32) -> Color {
    let background = theme.palette().background;
    if is_dark(theme) {
        mix(background, Color::WHITE, dark_amount)
    } else {
        mix(background, Color::WHITE, light_amount)
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
    if increased_contrast(theme) {
        return with_alpha(theme.palette().text, 0.82);
    }
    with_alpha(
        theme.palette().text,
        if is_dark(theme) { 0.62 } else { 0.60 },
    )
}

/// The tertiary label color used for counts, hints, and placeholder text.
#[must_use]
pub fn label_tertiary(theme: &Theme) -> Color {
    if increased_contrast(theme) {
        return with_alpha(theme.palette().text, 0.68);
    }
    with_alpha(theme.palette().text, 0.42)
}

/// The hairline separator color.
#[must_use]
///
/// Dark mode uses an opaque gray rather than translucent white: the renderer blends in linear
/// light, where a faint white overlay comes out several times brighter than the same overlay in
/// `AppKit`, and every hairline would read as a bright rule.
pub fn separator(theme: &Theme) -> Color {
    if increased_contrast(theme) {
        return if is_dark(theme) {
            color!(0x8E8F94)
        } else {
            with_alpha(Color::BLACK, 0.38)
        };
    }
    if is_dark(theme) {
        color!(0x3A3B3F)
    } else {
        with_alpha(Color::BLACK, 0.09)
    }
}

/// The accent color: the user's system accent.
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

    use super::{
        Appearance, accent, increased_contrast, is_dark, label_secondary, label_tertiary, pane,
        raised, separator,
    };

    fn appearance(dark: bool) -> Appearance {
        Appearance {
            dark,
            ..Appearance::default()
        }
    }

    #[test]
    fn dark_and_light_themes_are_detected_from_their_background() {
        assert!(is_dark(&appearance(true).theme()));
        assert!(!is_dark(&appearance(false).theme()));
    }

    #[test]
    fn the_list_pane_is_a_distinct_surface_from_the_chrome_in_both_appearances() {
        for dark in [true, false] {
            let theme = appearance(dark).theme();
            assert_ne!(pane(&theme), theme.palette().background);
            assert!(
                luminance(raised(&theme, 0.1, 0.9)) > luminance(theme.palette().background),
                "a sheet must read above the panel"
            );
        }
    }

    #[test]
    fn the_accent_is_the_system_accent_rather_than_a_fixed_color() {
        let orange = Color::from_rgb(1.0, 0.58, 0.0);
        let theme = Appearance {
            accent: orange,
            ..Appearance::default()
        }
        .theme();

        assert_eq!(accent(&theme), orange);
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

    #[test]
    fn increase_contrast_strengthens_hairlines_and_quiet_text_in_both_appearances() {
        for dark in [false, true] {
            let normal = appearance(dark).theme();
            let strong = Appearance {
                increase_contrast: true,
                ..appearance(dark)
            }
            .theme();

            assert!(!increased_contrast(&normal));
            assert!(increased_contrast(&strong));
            assert_eq!(is_dark(&normal), is_dark(&strong));
            assert!(label_secondary(&strong).a > label_secondary(&normal).a);
            assert!(label_tertiary(&strong).a > label_tertiary(&normal).a);
            assert!(
                contrast_against(separator(&strong), pane(&strong))
                    > contrast_against(separator(&normal), pane(&normal)),
                "a hairline must stand out more against the pane"
            );
        }
    }

    /// How far a hairline drawn over `ground` lands from the ground itself, in luminance.
    fn contrast_against(line: Color, ground: Color) -> f32 {
        let blended = Color {
            r: ground.r + (line.r - ground.r) * line.a,
            g: ground.g + (line.g - ground.g) * line.a,
            b: ground.b + (line.b - ground.b) * line.a,
            a: 1.0,
        };
        (luminance(blended) - luminance(ground)).abs()
    }
}
