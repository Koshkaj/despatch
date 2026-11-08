use iced::Font;
use iced::font::{Family, Weight};

/// The family name of San Francisco, the macOS system font, as the font database files it.
///
/// It is the system's own variable font (`/System/Library/Fonts/SFNS.ttf`), so the panel is set in
/// the same face as every `AppKit` control around it. Its English family name is "System Font";
/// the weights below select positions on its weight axis rather than separate files.
pub const FAMILY: &str = "System Font";

/// Where that font is installed, read to measure labels for an ellipsis.
pub(crate) const SYSTEM_FONT_PATH: &str = "/System/Library/Fonts/SFNS.ttf";

pub const UI: Font = Font {
    family: Family::Name(FAMILY),
    ..Font::DEFAULT
};

pub const MEDIUM: Font = Font {
    weight: Weight::Medium,
    ..UI
};

pub const SEMIBOLD: Font = Font {
    weight: Weight::Semibold,
    ..UI
};

#[cfg(test)]
mod tests {
    use super::FAMILY;

    fn system_font() -> Option<Vec<u8>> {
        std::fs::read(super::SYSTEM_FONT_PATH).ok()
    }

    /// The family the font database will file the system font under: the typographic family when
    /// there is one, else the legacy family, in US English.
    fn english_families(face: &ttf_parser::Face) -> Vec<String> {
        face.names()
            .into_iter()
            .filter(|name| {
                name.name_id == ttf_parser::name_id::FAMILY
                    || name.name_id == ttf_parser::name_id::TYPOGRAPHIC_FAMILY
            })
            .filter(|name| name.language() == ttf_parser::Language::English_UnitedStates)
            .filter_map(|name| name.to_string())
            .collect()
    }

    /// A family name that matches nothing is silent: the shaper falls back to some other face and
    /// the panel merely looks a little off. The name is checked against the installed file.
    #[test]
    fn the_family_the_app_asks_for_is_the_installed_system_font() {
        let Some(bytes) = system_font() else {
            return;
        };
        let face = ttf_parser::Face::parse(&bytes, 0).expect("a parsable system font");

        assert!(
            english_families(&face).iter().any(|name| name == FAMILY),
            "the system font calls itself {:?}",
            english_families(&face)
        );
        // The three weights the interface uses are positions on one axis, so the axis must exist.
        assert!(
            face.variation_axes()
                .into_iter()
                .any(|axis| axis.tag == ttf_parser::Tag::from_bytes(b"wght")),
            "the system font has no weight axis"
        );
    }

    /// Labels are shortened with an ellipsis from an estimated advance, because Iced has no
    /// ellipsis of its own. The estimate has to be at least as wide as the text it shortens, or a
    /// shortened label still runs past its box.
    #[test]
    fn the_ellipsis_estimate_is_never_narrower_than_typical_text() {
        let Some(bytes) = system_font() else {
            return;
        };
        let face = ttf_parser::Face::parse(&bytes, 0).expect("a parsable system font");
        let units_per_em = f32::from(face.units_per_em());
        for sample in [
            "deploy@db-primary.internal:2222",
            "Production database replica",
            "codex --model gpt-5 --quiet",
            "Grafana dashboards",
        ] {
            let width: f32 = sample
                .chars()
                .filter_map(|character| face.glyph_index(character))
                .filter_map(|glyph| face.glyph_hor_advance(glyph))
                .map(|advance| f32::from(advance) / units_per_em)
                .sum();
            #[expect(
                clippy::cast_precision_loss,
                reason = "a sample is far shorter than f32's exact integer range"
            )]
            let estimate = sample.chars().count() as f32 * crate::presenters::ADVANCE;
            assert!(
                estimate >= width,
                "\"{sample}\" is {width} em wide, the estimate is {estimate} em"
            );
        }
    }
}
