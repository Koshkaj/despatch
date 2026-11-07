use objc2_app_kit::{NSColor, NSColorSpace, NSWorkspace};
use objc2_foundation::{NSString, NSUserDefaults};

/// The system-wide appearance settings the Despatch theme derives from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SystemAppearance {
    /// Whether macOS is currently using the dark interface style.
    pub dark: bool,
    /// The user's accent color as linear-free sRGB components in `0.0..=1.0`.
    pub accent: [f32; 3],
    /// Accessibility → Display → Increase contrast.
    pub increase_contrast: bool,
    /// Accessibility → Display → Reduce motion.
    pub reduce_motion: bool,
}

/// The macOS default blue accent, used when the accent color cannot be resolved.
pub const FALLBACK_ACCENT: [f32; 3] = [0.0, 0.478_431_4, 1.0];

impl Default for SystemAppearance {
    fn default() -> Self {
        Self {
            dark: false,
            accent: FALLBACK_ACCENT,
            increase_contrast: false,
            reduce_motion: false,
        }
    }
}

/// Reads the current interface style, accent color, and the display accessibility settings.
///
/// Falls back to the light appearance and the default blue accent when either value is
/// unavailable, so the UI always has a usable theme.
#[must_use]
pub fn system_appearance() -> SystemAppearance {
    let workspace = NSWorkspace::sharedWorkspace();
    SystemAppearance {
        dark: prefers_dark_interface(),
        accent: control_accent_color().unwrap_or(FALLBACK_ACCENT),
        increase_contrast: workspace.accessibilityDisplayShouldIncreaseContrast(),
        reduce_motion: workspace.accessibilityDisplayShouldReduceMotion(),
    }
}

fn prefers_dark_interface() -> bool {
    NSUserDefaults::standardUserDefaults()
        .stringForKey(&NSString::from_str("AppleInterfaceStyle"))
        .is_some_and(|style| style.to_string().eq_ignore_ascii_case("dark"))
}

fn control_accent_color() -> Option<[f32; 3]> {
    let accent =
        NSColor::controlAccentColor().colorUsingColorSpace(&NSColorSpace::sRGBColorSpace());
    let accent = accent?;
    Some([
        narrow(accent.redComponent()),
        narrow(accent.greenComponent()),
        narrow(accent.blueComponent()),
    ])
}

/// Narrows an `AppKit` color component to the precision the renderer uses.
#[expect(
    clippy::cast_possible_truncation,
    reason = "color components are clamped to 0.0..=1.0, where f32 is exact enough for rendering"
)]
fn narrow(component: f64) -> f32 {
    component.clamp(0.0, 1.0) as f32
}

#[cfg(test)]
mod tests {
    use super::{FALLBACK_ACCENT, system_appearance};

    #[test]
    fn appearance_components_stay_inside_the_unit_range() {
        let appearance = system_appearance();

        assert!(
            appearance
                .accent
                .iter()
                .all(|component| (0.0..=1.0).contains(component)),
            "accent {:?} left the unit range",
            appearance.accent
        );
        assert!(FALLBACK_ACCENT.iter().all(|c| (0.0..=1.0).contains(c)));
    }
}
