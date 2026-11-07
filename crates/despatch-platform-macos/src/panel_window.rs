//! The window behind the menu-bar panel.
//!
//! Iced opens an ordinary borderless winit window. A menu-bar panel needs a few things Iced 0.14
//! does not expose: showing and hiding a window that keeps its state, opening on whichever Space is
//! active (including over a full-screen app), a system shadow, and an app with no Dock icon. These
//! are applied here to the `NSWindow` behind Iced's `NSView`.
//!
//! Geometry is in `AppKit` screen coordinates: points, with the origin at the bottom-left of the
//! primary screen and y growing upward.

use std::ffi::c_void;
use std::ptr::NonNull;

use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSAnimatablePropertyContainer, NSAnimationContext, NSApplication,
    NSApplicationActivationPolicy, NSView, NSWindow, NSWindowCollectionBehavior,
};
use objc2_foundation::{NSPoint, NSRect};

/// A rectangle on screen, in `AppKit` screen coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl ScreenRect {
    #[must_use]
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    const fn max_x(self) -> f64 {
        self.x + self.width
    }

    const fn max_y(self) -> f64 {
        self.y + self.height
    }
}

impl From<NSRect> for ScreenRect {
    fn from(rect: NSRect) -> Self {
        Self::new(
            rect.origin.x,
            rect.origin.y,
            rect.size.width,
            rect.size.height,
        )
    }
}

/// Where the status item sits, and the part of its screen a panel may cover.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StatusAnchor {
    /// The status item's own frame, or `None` while the menu bar has not placed it yet.
    pub item: Option<ScreenRect>,
    /// The screen's visible frame: everything except the menu bar and the Dock.
    pub visible: ScreenRect,
}

/// The gap between the bottom of the menu bar and the top of the panel.
pub const PANEL_GAP: f64 = 6.0;
/// The closest a panel comes to the side edges of the screen.
const SCREEN_MARGIN: f64 = 8.0;

/// The top-left corner a panel of `width` should be placed at to hang under `anchor`.
///
/// The panel is centered on the item and then pushed back inside the screen, so an item near the
/// right edge — where most of them are — still gets a panel that is fully visible. It hangs from the
/// bottom of the item, never from the top of the visible frame, so a hidden menu bar does not pull
/// the panel up under the notch.
///
/// An item the menu bar has not placed yet has no position to hang from; the panel then opens at
/// the right end of the menu bar, where status items live.
#[must_use]
pub fn panel_top_left(anchor: &StatusAnchor, width: f64) -> (f64, f64) {
    let leftmost = anchor.visible.x + SCREEN_MARGIN;
    let rightmost = anchor.visible.max_x() - SCREEN_MARGIN - width;
    let Some(item) = anchor.item else {
        return (rightmost.max(leftmost), anchor.visible.max_y() - PANEL_GAP);
    };
    let centered = item.x + item.width / 2.0 - width / 2.0;
    let x = if rightmost < leftmost {
        leftmost
    } else {
        centered.clamp(leftmost, rightmost)
    };
    let top = item.y.min(anchor.visible.max_y()) - PANEL_GAP;
    (x, top)
}

/// Makes the app an accessory: no Dock icon and no menu bar of its own.
///
/// A bundled app gets this from `LSUIElement`; this covers a binary started outside a bundle, such
/// as `cargo run`.
pub fn use_accessory_activation_policy() {
    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    let application = NSApplication::sharedApplication(main_thread);
    let _ = application.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
}

/// Gives the panel window the behaviour of a menu-bar panel.
///
/// It joins every Space and floats over full-screen apps, so it opens where the user is rather than
/// switching Spaces to wherever it was last shown; it stays out of the window cycle and Exposé; and
/// it carries the system shadow, which is computed from the panel's own rounded shape.
pub fn configure_panel(ns_view: NonNull<c_void>) -> Result<(), String> {
    let window = window_of(ns_view)?;
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Transient
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    window.setHasShadow(true);
    Ok(())
}

/// Places the panel with its top-left corner at `top_left`, brings it to the front, and gives it
/// the keyboard.
///
/// An accessory app is not active while its panel is hidden, so it is activated here: without that
/// the panel would appear behind the frontmost app's windows and typing would go to that app.
///
/// With `fade`, the panel fades in over [`FADE_SECONDS`]; without it — when the user has asked to
/// reduce motion — it simply appears.
pub fn show_panel(
    ns_view: NonNull<c_void>,
    top_left: Option<(f64, f64)>,
    fade: bool,
) -> Result<(), String> {
    let window = window_of(ns_view)?;
    let main_thread = MainThreadMarker::new().ok_or("not on the main thread")?;
    if let Some((x, y)) = top_left {
        window.setFrameTopLeftPoint(NSPoint::new(x, y));
    }
    let application = NSApplication::sharedApplication(main_thread);
    // `activate` replaces this from macOS 14, and Despatch supports macOS 13.
    #[allow(deprecated)]
    application.activateIgnoringOtherApps(true);
    if fade {
        window.setAlphaValue(0.0);
        window.makeKeyAndOrderFront(None);
        NSAnimationContext::beginGrouping();
        NSAnimationContext::currentContext().setDuration(FADE_SECONDS);
        window.animator().setAlphaValue(1.0);
        NSAnimationContext::endGrouping();
    } else {
        window.setAlphaValue(1.0);
        window.makeKeyAndOrderFront(None);
    }
    window.invalidateShadow();
    Ok(())
}

/// How long the panel takes to fade in: long enough to read as arriving, short enough never to be
/// waited for.
pub const FADE_SECONDS: f64 = 0.12;

/// Recomputes the shadow from what the panel has drawn.
///
/// The window is transparent around the panel's rounded corners, and the shadow follows whatever is
/// opaque. Right after a show the GPU may not have presented the new frame yet, so this is asked
/// for again once it has.
pub fn refresh_panel_shadow(ns_view: NonNull<c_void>) -> Result<(), String> {
    window_of(ns_view)?.invalidateShadow();
    Ok(())
}

/// Hides the panel. With `return_focus`, the app also steps back so the app the user was in before
/// opening the panel gets the keyboard again — what pressing Escape on a menu should do.
pub fn hide_panel(ns_view: NonNull<c_void>, return_focus: bool) -> Result<(), String> {
    let window = window_of(ns_view)?;
    window.orderOut(None);
    if return_focus {
        let main_thread = MainThreadMarker::new().ok_or("not on the main thread")?;
        NSApplication::sharedApplication(main_thread).hide(None);
    }
    Ok(())
}

fn window_of(ns_view: NonNull<c_void>) -> Result<objc2::rc::Retained<NSWindow>, String> {
    MainThreadMarker::new().ok_or("not on the main thread")?;
    // SAFETY: Iced/winit supplied `ns_view` through the AppKit raw-window-handle contract, so it is
    // a non-null pointer to the live NSView of an open window. This is only reached from Iced's
    // window callback on the main thread (checked above), and the borrow ends within this call.
    let view = unsafe { ns_view.cast::<NSView>().as_ref() };
    view.window()
        .ok_or_else(|| "the panel view is not in a window".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{PANEL_GAP, SCREEN_MARGIN, ScreenRect, StatusAnchor, panel_top_left};

    /// A 1512 × 982 laptop screen with a 37-point menu bar and no Dock on screen.
    fn laptop(item_x: f64) -> StatusAnchor {
        StatusAnchor {
            item: Some(ScreenRect::new(item_x, 945.0, 32.0, 37.0)),
            visible: ScreenRect::new(0.0, 0.0, 1512.0, 945.0),
        }
    }

    #[test]
    fn a_panel_hangs_centered_under_its_item() {
        let (x, top) = panel_top_left(&laptop(700.0), 720.0);

        assert!((x + 360.0 - 716.0).abs() < f64::EPSILON);
        assert!((top - (945.0 - PANEL_GAP)).abs() < f64::EPSILON);
    }

    #[test]
    fn an_item_near_the_right_edge_keeps_its_panel_on_screen() {
        let (x, _) = panel_top_left(&laptop(1460.0), 720.0);

        assert!((x + 720.0 - (1512.0 - SCREEN_MARGIN)).abs() < f64::EPSILON);
    }

    #[test]
    fn an_item_near_the_left_edge_keeps_its_panel_on_screen() {
        let (x, _) = panel_top_left(&laptop(10.0), 720.0);

        assert!((x - SCREEN_MARGIN).abs() < f64::EPSILON);
    }

    #[test]
    fn a_secondary_screen_to_the_left_uses_its_own_coordinates() {
        let anchor = StatusAnchor {
            item: Some(ScreenRect::new(-300.0, 1055.0, 30.0, 25.0)),
            visible: ScreenRect::new(-1920.0, 0.0, 1920.0, 1055.0),
        };

        let (x, top) = panel_top_left(&anchor, 720.0);

        assert!(x >= -1920.0 + SCREEN_MARGIN && x + 720.0 <= -SCREEN_MARGIN);
        assert!((top - (1055.0 - PANEL_GAP)).abs() < f64::EPSILON);
    }

    #[test]
    fn a_screen_narrower_than_the_panel_pins_it_to_the_left_margin() {
        let anchor = StatusAnchor {
            item: Some(ScreenRect::new(400.0, 600.0, 30.0, 24.0)),
            visible: ScreenRect::new(0.0, 0.0, 640.0, 600.0),
        };

        let (x, _) = panel_top_left(&anchor, 720.0);

        assert!((x - SCREEN_MARGIN).abs() < f64::EPSILON);
    }

    #[test]
    fn an_item_not_placed_yet_opens_the_panel_at_the_right_of_the_menu_bar() {
        let anchor = StatusAnchor {
            item: None,
            visible: ScreenRect::new(0.0, 0.0, 1512.0, 949.0),
        };

        let (x, top) = panel_top_left(&anchor, 720.0);

        assert!((x + 720.0 - (1512.0 - SCREEN_MARGIN)).abs() < f64::EPSILON);
        assert!((top - (949.0 - PANEL_GAP)).abs() < f64::EPSILON);
    }
}
