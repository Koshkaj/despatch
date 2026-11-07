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
    Category,
    Group,
    Server,
    App,
    Add,
    Search,
    Settings,
    Recent,
    Click,
    Edit,
    Duplicate,
    Delete,
    Pin,
    Pinned,
    Error,
    Success,
    Clear,
    Expand,
    Collapse,
}

impl Symbol {
    /// The `AppKit` system symbol name.
    const fn name(self) -> &'static str {
        match self {
            Self::Category => "folder.fill",
            Self::Group => "folder",
            Self::Server => "server.rack",
            Self::App => "terminal",
            Self::Add => "plus",
            Self::Search => "magnifyingglass",
            Self::Settings => "gearshape",
            Self::Recent => "clock",
            Self::Click => "cursorarrow.click.2",
            Self::Edit => "pencil",
            Self::Duplicate => "plus.square.on.square",
            Self::Delete => "trash",
            Self::Pin => "pin",
            Self::Pinned => "pin.fill",
            Self::Error => "exclamationmark.triangle.fill",
            Self::Success => "checkmark.circle.fill",
            Self::Clear => "xmark.circle.fill",
            Self::Expand => "chevron.down",
            Self::Collapse => "chevron.up",
        }
    }

    /// The `VoiceOver` description for the symbol.
    const fn accessibility_label(self) -> &'static str {
        match self {
            Self::Category => "category",
            Self::Group => "group",
            Self::Server => "server",
            Self::App => "Despatch",
            Self::Add => "add",
            Self::Search => "search",
            Self::Settings => "settings",
            Self::Recent => "recently used",
            Self::Click => "double-click",
            Self::Edit => "edit",
            Self::Duplicate => "duplicate",
            Self::Delete => "delete",
            Self::Pin => "keep open",
            Self::Pinned => "pinned open",
            Self::Error => "error",
            Self::Success => "success",
            Self::Clear => "clear",
            Self::Expand => "show additional settings",
            Self::Collapse => "hide additional settings",
        }
    }

    const ALL: [Self; 19] = [
        Self::Category,
        Self::Group,
        Self::Server,
        Self::App,
        Self::Add,
        Self::Search,
        Self::Settings,
        Self::Recent,
        Self::Click,
        Self::Edit,
        Self::Duplicate,
        Self::Delete,
        Self::Pin,
        Self::Pinned,
        Self::Error,
        Self::Success,
        Self::Clear,
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

#[cfg(target_os = "macos")]
fn load_mask(symbol: Symbol) -> Option<Mask> {
    let pixels =
        despatch_platform_macos::render_system_symbol(symbol.name(), symbol.accessibility_label())?;
    Some(Mask {
        width: pixels.width,
        height: pixels.height,
        rgba: pixels.rgba,
    })
}

#[cfg(not(target_os = "macos"))]
fn load_mask(_symbol: Symbol) -> Option<Mask> {
    None
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
