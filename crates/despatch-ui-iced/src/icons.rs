use std::collections::HashMap;
use std::sync::Arc;

use iced::widget::image;
use iced::{Color, Theme};

use crate::theme;

/// The SF Symbols Despatch draws.
///
/// Every symbol is an `AppKit` system symbol, so the app inherits the platform icon set instead of
/// shipping hand-drawn substitutes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Symbol {
    Servers,
    Agents,
    Category,
    Server,
    Agent,
    Claude,
    Codex,
    Gemini,
    OpenCode,
    Custom,
    Add,
    Search,
    Settings,
    Launch,
    Edit,
    Duplicate,
    Delete,
    Reveal,
    Error,
    Success,
    Close,
    Expand,
    Collapse,
}

impl Symbol {
    /// The `AppKit` system symbol name.
    const fn name(self) -> &'static str {
        match self {
            Self::Servers => "square.stack.3d.up.fill",
            Self::Agents => "sparkles",
            Self::Category => "folder.fill",
            Self::Server => "server.rack",
            Self::Agent => "terminal.fill",
            // Stand-in marks, not brand logos: shipping the real ones means shipping someone
            // else's trademark, which is a licensing decision rather than a drawing decision.
            Self::Claude => "asterisk",
            Self::Codex => "chevron.left.slash.chevron.right",
            Self::Gemini => "sparkle",
            Self::OpenCode => "curlybraces",
            Self::Custom => "chevron.left.forwardslash.chevron.right",
            Self::Add => "plus",
            Self::Search => "magnifyingglass",
            Self::Settings => "gearshape.fill",
            Self::Launch => "arrow.up.forward",
            Self::Edit => "pencil",
            Self::Duplicate => "plus.square.on.square",
            Self::Delete => "trash",
            Self::Reveal => "folder",
            Self::Error => "exclamationmark.triangle.fill",
            Self::Success => "checkmark.circle.fill",
            Self::Close => "xmark",
            Self::Expand => "chevron.down",
            Self::Collapse => "chevron.up",
        }
    }

    /// The `VoiceOver` description for the symbol.
    const fn accessibility_label(self) -> &'static str {
        match self {
            Self::Servers => "servers",
            Self::Agents => "agents",
            Self::Category => "category",
            Self::Server => "server",
            Self::Agent => "command-line agent",
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Gemini => "Gemini",
            Self::OpenCode => "OpenCode",
            Self::Custom => "custom command",
            Self::Add => "add",
            Self::Search => "search",
            Self::Settings => "settings",
            Self::Launch => "launch",
            Self::Edit => "edit",
            Self::Duplicate => "duplicate",
            Self::Delete => "delete",
            Self::Reveal => "reveal in Finder",
            Self::Error => "error",
            Self::Success => "success",
            Self::Close => "close",
            Self::Expand => "show additional settings",
            Self::Collapse => "hide additional settings",
        }
    }

    const ALL: [Self; 23] = [
        Self::Servers,
        Self::Agents,
        Self::Category,
        Self::Server,
        Self::Agent,
        Self::Claude,
        Self::Codex,
        Self::Gemini,
        Self::OpenCode,
        Self::Custom,
        Self::Add,
        Self::Search,
        Self::Settings,
        Self::Launch,
        Self::Edit,
        Self::Duplicate,
        Self::Delete,
        Self::Reveal,
        Self::Error,
        Self::Success,
        Self::Close,
        Self::Expand,
        Self::Collapse,
    ];
}

/// The color roles a symbol can be drawn in.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Tint {
    /// White, for symbols sitting on a saturated backplate.
    OnColor,
    /// Whichever of black or white stays legible on the accent, for symbols sitting on an
    /// accent-filled control. The accent is not always dark, so white is not always right.
    OnAccent,
    /// The primary label color.
    Label,
    /// The secondary label color, for quiet controls.
    Secondary,
    /// The accent color.
    Accent,
    /// The destructive color.
    Danger,
    /// A user-chosen label color, as `[red, green, blue]`.
    ///
    /// These are not known when the set is loaded, so they are rendered the first time a group or
    /// category asks for one and then cached like every other tint.
    Custom([u8; 3]),
}

impl Tint {
    /// The tints every symbol is pre-rendered in, because the interface always needs them.
    const ROLES: [Self; 6] = [
        Self::OnColor,
        Self::OnAccent,
        Self::Label,
        Self::Secondary,
        Self::Accent,
        Self::Danger,
    ];

    fn color(self, theme: &Theme) -> Color {
        match self {
            Self::OnColor => Color::WHITE,
            Self::OnAccent => crate::style::on_accent(theme::accent(theme)),
            Self::Label => theme::label(theme),
            Self::Secondary => theme::label_secondary(theme),
            Self::Accent => theme::accent(theme),
            Self::Danger => theme.palette().danger,
            Self::Custom([red, green, blue]) => Color::from_rgb8(red, green, blue),
        }
    }
}

/// Decoded SF Symbols, pre-tinted for every color role the interface uses.
///
/// `AppKit` symbol rendering only works on the main thread, so the set is decoded once while the
/// app shell is built and then cloned cheaply into views. The undecorated masks are kept so a
/// label color the user picks later can be tinted without touching `AppKit` again.
#[derive(Clone, Debug)]
pub struct IconSet {
    masks: HashMap<Symbol, Arc<Mask>>,
    handles: HashMap<(Symbol, Tint), image::Handle>,
    theme: Theme,
}

impl IconSet {
    /// Renders every symbol in every color role required by `theme`.
    #[must_use]
    pub fn load(theme: &Theme) -> Self {
        let mut set = Self {
            masks: HashMap::new(),
            handles: HashMap::new(),
            theme: theme.clone(),
        };
        for symbol in Symbol::ALL {
            let Some(mask) = load_mask(symbol) else {
                continue;
            };
            set.masks.insert(symbol, Arc::new(mask));
            for tint in Tint::ROLES {
                set.render(symbol, tint);
            }
        }
        set
    }

    /// Returns the handle for `symbol` in `tint`, if the running macOS version provides it.
    ///
    /// A custom tint that was never [`prepared`](Self::prepare) yields `None` rather than a fresh
    /// texture per frame, so the renderer keeps reusing one upload per color.
    #[must_use]
    pub fn get(&self, symbol: Symbol, tint: Tint) -> Option<image::Handle> {
        self.handles.get(&(symbol, tint)).cloned()
    }

    /// Renders `symbol` in `tint` unless that combination is already cached.
    pub fn prepare(&mut self, symbol: Symbol, tint: Tint) {
        if !self.handles.contains_key(&(symbol, tint)) {
            self.render(symbol, tint);
        }
    }

    fn render(&mut self, symbol: Symbol, tint: Tint) {
        let Some(mask) = self.masks.get(&symbol) else {
            return;
        };
        let handle = image::Handle::from_rgba(
            mask.width,
            mask.height,
            tinted(&mask.rgba, tint.color(&self.theme)),
        );
        self.handles.insert((symbol, tint), handle);
    }
}

#[derive(Debug)]
struct Mask {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// Replaces the color channels of a white coverage mask, leaving its antialiased alpha intact.
fn tinted(mask: &[u8], color: Color) -> Vec<u8> {
    let rgba = color.into_rgba8();
    mask.chunks_exact(4)
        .flat_map(|pixel| [rgba[0], rgba[1], rgba[2], scale_alpha(pixel[3], color.a)])
        .collect()
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the product of two 0..=1 factors and u8::MAX always fits an unsigned byte"
)]
fn scale_alpha(coverage: u8, alpha: f32) -> u8 {
    (f32::from(coverage) * alpha.clamp(0.0, 1.0)).round() as u8
}

/// The provider marks that ship with the app rather than coming from the system symbol set.
///
/// These are the providers' own brand glyphs, in the single-color form published by the
/// [Simple Icons](https://simpleicons.org) project (CC0). Each trademark belongs to its owner and
/// is used here only to name the tool the launcher runs. They are monochrome by design, which is
/// what lets them go through the same coverage-mask tinting as every other symbol.
const fn bundled_logo(symbol: Symbol) -> Option<&'static [u8]> {
    match symbol {
        Symbol::Claude => Some(include_bytes!("../assets/logos/claude.png")),
        Symbol::Codex => Some(include_bytes!("../assets/logos/codex.png")),
        Symbol::Gemini => Some(include_bytes!("../assets/logos/gemini.png")),
        Symbol::OpenCode => Some(include_bytes!("../assets/logos/opencode.png")),
        _ => None,
    }
}

/// Turns a bundled logo into a coverage mask.
///
/// Only the alpha channel survives. The glyph's own color is discarded so the mark can be tinted
/// to whatever the interface needs, exactly like a system symbol.
fn decode_logo(bytes: &[u8]) -> Option<Mask> {
    let mut reader = png::Decoder::new(bytes).read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).ok()?;
    if info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale => 1,
        png::ColorType::Indexed => return None,
    };
    let rgba = buffer[..info.buffer_size()]
        .chunks_exact(channels)
        .flat_map(|pixel| [u8::MAX, u8::MAX, u8::MAX, coverage(pixel, channels)])
        .collect();
    Some(Mask {
        width: info.width,
        height: info.height,
        rgba,
    })
}

/// How much of a pixel the glyph covers.
///
/// A logo may arrive either as ink on transparency or as ink on white, depending on what
/// rasterized it, and the two are indistinguishable without looking at both channels: a glyph
/// flattened onto white is fully opaque everywhere, so alpha alone would report a solid block.
/// Darkness is what carries the shape in that case, so coverage is alpha scaled by how dark the
/// pixel is, which reads both forms correctly.
fn coverage(pixel: &[u8], channels: usize) -> u8 {
    let luminance = |pixel: &[u8]| {
        (u32::from(pixel[0]) * 299 + u32::from(pixel[1]) * 587 + u32::from(pixel[2]) * 114) / 1000
    };
    let (light, alpha) = match channels {
        4 => (luminance(pixel), u32::from(pixel[3])),
        3 => (luminance(pixel), 255),
        2 => (u32::from(pixel[0]), u32::from(pixel[1])),
        _ => (u32::from(pixel[0]), 255),
    };
    let darkness = 255_u32.saturating_sub(light);
    u8::try_from(alpha * darkness / 255).unwrap_or(u8::MAX)
}

#[cfg(target_os = "macos")]
fn load_mask(symbol: Symbol) -> Option<Mask> {
    if let Some(logo) = bundled_logo(symbol) {
        return decode_logo(logo);
    }
    let pixels =
        despatch_platform_macos::render_system_symbol(symbol.name(), symbol.accessibility_label())?;
    Some(Mask {
        width: pixels.width,
        height: pixels.height,
        rgba: pixels.rgba,
    })
}

#[cfg(not(target_os = "macos"))]
fn load_mask(symbol: Symbol) -> Option<Mask> {
    bundled_logo(symbol).and_then(decode_logo)
}

#[cfg(test)]
mod tests {
    use iced::Color;

    use super::{Symbol, Tint, scale_alpha, tinted};

    #[test]
    fn every_symbol_declares_a_name_and_a_voiceover_label() {
        for symbol in Symbol::ALL {
            assert!(!symbol.name().trim().is_empty());
            assert!(!symbol.accessibility_label().trim().is_empty());
        }
    }

    #[test]
    fn symbol_names_are_unique_so_no_two_roles_share_an_icon_by_accident() {
        let mut names: Vec<_> = Symbol::ALL.iter().map(|symbol| symbol.name()).collect();
        names.sort_unstable();
        let total = names.len();
        names.dedup();

        assert_eq!(names.len(), total);
    }

    #[test]
    fn tinting_replaces_color_and_preserves_antialiased_coverage() {
        let mask = [255, 255, 255, 0, 255, 255, 255, 128, 255, 255, 255, 255];

        let tinted = tinted(&mask, Color::from_rgb(1.0, 0.0, 0.0));

        assert_eq!(tinted[..4], [255, 0, 0, 0]);
        assert_eq!(tinted[4..8], [255, 0, 0, 128]);
        assert_eq!(tinted[8..], [255, 0, 0, 255]);
    }

    #[test]
    fn a_translucent_tint_scales_the_coverage() {
        assert_eq!(scale_alpha(255, 0.5), 128);
        assert_eq!(scale_alpha(0, 1.0), 0);
        assert_eq!(scale_alpha(200, 2.0), 200);
    }

    #[test]
    fn every_bundled_provider_logo_decodes_to_a_square_coverage_mask() {
        for symbol in [
            Symbol::Claude,
            Symbol::Codex,
            Symbol::Gemini,
            Symbol::OpenCode,
        ] {
            let logo = super::bundled_logo(symbol).expect("a bundled logo");
            let mask = super::decode_logo(logo).expect("a decodable logo");

            assert_eq!(mask.width, mask.height, "{symbol:?} must be square");
            assert_eq!(mask.rgba.len(), (mask.width * mask.height * 4) as usize);
            assert!(
                mask.rgba.chunks_exact(4).any(|pixel| pixel[3] > 200),
                "{symbol:?} decoded to nothing"
            );
            assert!(
                mask.rgba.chunks_exact(4).any(|pixel| pixel[3] < 40),
                "{symbol:?} decoded to a solid block rather than a glyph"
            );
            assert!(
                mask.rgba
                    .chunks_exact(4)
                    .all(|pixel| pixel[..3] == [u8::MAX; 3]),
                "{symbol:?} kept its own color instead of becoming a mask"
            );
        }
    }

    #[test]
    fn every_tint_role_is_covered_by_the_generated_set() {
        assert_eq!(Tint::ROLES.len(), 6);
    }

    #[test]
    fn a_custom_tint_is_keyed_by_its_channels_so_one_color_is_rendered_once() {
        let color = Tint::Custom([0x4F, 0x9D, 0xFF]);

        assert_eq!(color, Tint::Custom([0x4F, 0x9D, 0xFF]));
        assert_ne!(color, Tint::Custom([0x4F, 0x9D, 0xFE]));
        assert!(!Tint::ROLES.contains(&color));
    }
}
