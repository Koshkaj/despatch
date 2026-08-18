use iced::Font;
use iced::font::{Family, Weight};

/// The app's typeface, bundled with the binary rather than taken from the system, so every machine
/// draws the same interface. All three weights are registered in [`crate::run`]; nothing else loads
/// them.
///
/// The files share one typographic family (`Lilex`) and differ only in weight, which is how the
/// shaper picks between them: [`UI`], [`MEDIUM`], and [`SEMIBOLD`] name the same family and let the
/// weight select the face. Without a real face for a weight the shaper synthesizes one, which looks
/// smeared rather than bold.
pub const BUNDLED: [&[u8]; 3] = [
    include_bytes!("../assets/fonts/Lilex-Regular.otf"),
    include_bytes!("../assets/fonts/Lilex-Medium.otf"),
    include_bytes!("../assets/fonts/Lilex-SemiBold.otf"),
];

/// The family name inside every file in [`BUNDLED`]. It must match the font's own name table,
/// because that is what the text shaper looks up.
pub const FAMILY: &str = "Lilex";

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
    use super::{BUNDLED, FAMILY};

    /// The family the font database will file a face under.
    ///
    /// This mirrors `fontdb`, which prefers the typographic family (name 16) and falls back to the
    /// legacy family (name 1). The distinction matters here: Lilex's weighted files call themselves
    /// `Lilex Medium` and `Lilex SemiBold` in the legacy field, and only the typographic field
    /// gathers all three under one family that a weight can select within.
    fn families(face: &ttf_parser::Face) -> Vec<String> {
        for id in [
            ttf_parser::name_id::TYPOGRAPHIC_FAMILY,
            ttf_parser::name_id::FAMILY,
        ] {
            let names: Vec<String> = face
                .names()
                .into_iter()
                .filter(|name| name.name_id == id)
                .filter_map(|name| name.to_string())
                .collect();
            if !names.is_empty() {
                return names;
            }
        }
        Vec::new()
    }

    /// Every bundled file has to be a real font, and all of them have to carry the one family name
    /// the app asks for. A mismatch is silent: the shaper falls back to some system face and the
    /// app merely looks a little off.
    #[test]
    fn every_bundled_file_is_the_family_the_app_asks_for() {
        for bytes in BUNDLED {
            let face = ttf_parser::Face::parse(bytes, 0).expect("a parsable font");
            let families = families(&face);
            assert!(
                families.iter().any(|name| name == FAMILY),
                "the font calls itself {families:?}, the app asks for {FAMILY}"
            );
        }
    }

    /// The weights the app names have to be the weights it ships, or the shaper picks the nearest
    /// face it has and synthesizes the difference.
    #[test]
    fn the_bundled_weights_are_the_ones_the_app_names() {
        let named = [
            (super::UI, iced::font::Weight::Normal, 400),
            (super::MEDIUM, iced::font::Weight::Medium, 500),
            (super::SEMIBOLD, iced::font::Weight::Semibold, 600),
        ];

        for (index, (font, weight, class)) in named.into_iter().enumerate() {
            assert_eq!(font.weight, weight, "the app asks for the wrong weight");
            let face = ttf_parser::Face::parse(BUNDLED[index], 0).expect("a parsable font");
            assert_eq!(
                face.weight().to_number(),
                class,
                "the file at index {index} is not the weight the app asks for"
            );
        }
    }

    /// Every layout width in the app is derived from one advance, which only holds because the
    /// bundled fonts are monospaced — and because the weights share that advance, so a heading does
    /// not outgrow the box measured for it. A proportional replacement would break every card
    /// width, so the property is asserted rather than assumed.
    #[test]
    fn every_bundled_weight_is_monospaced_at_the_same_advance() {
        for bytes in BUNDLED {
            let face = ttf_parser::Face::parse(bytes, 0).expect("a parsable font");
            let units_per_em = f32::from(face.units_per_em());
            let advances: Vec<f32> = "aWm0.@-_"
                .chars()
                .filter_map(|character| face.glyph_index(character))
                .filter_map(|glyph| face.glyph_hor_advance(glyph))
                .map(|advance| f32::from(advance) / units_per_em)
                .collect();

            assert_eq!(advances.len(), 8, "every sampled character has a glyph");
            for advance in &advances {
                assert!(
                    (advance - crate::presenters::ADVANCE).abs() < 0.001,
                    "advance {advance} does not match the constant the layout uses"
                );
            }
        }
    }
}
