//! Development-only hooks that let the app check its own menu-bar panel without a person at the
//! keyboard: pressing the status item through `AppKit`, and photographing the panel window as the
//! window server composites it, corners and shadow included.
//!
//! Synthetic mouse events need Accessibility permission and screenshots of the screen need Screen
//! Recording permission, but a process may always message its own controls and capture its own
//! windows. Compiled only with the `self-test` feature, which no release build enables.

use std::ffi::c_void;
use std::path::Path;
use std::ptr::NonNull;

use objc2::AnyThread;
use objc2::msg_send;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSAccessibility, NSApplication, NSBitmapImageFileType, NSBitmapImageRep, NSEvent,
    NSEventModifierFlags, NSEventType, NSView,
};
use objc2_core_graphics::{CGWindowImageOption, CGWindowListOption};
use objc2_foundation::{MainThreadMarker, NSDictionary, NSPoint, NSRect, NSSize, NSString};

use crate::StatusItem;

/// What the window server reports about the panel window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelReport {
    pub visible: bool,
    pub key: bool,
    /// The window's opacity: below 1 while it is fading in.
    pub alpha: f64,
    /// The window frame in `AppKit` screen coordinates: x, y, width, height.
    pub frame: (f64, f64, f64, f64),
}

impl StatusItem {
    /// Presses the menu-bar item through `AppKit`, exactly as a click would reach its target.
    pub fn perform_click(&self) {
        let Some(main_thread) = MainThreadMarker::new() else {
            return;
        };
        if let Some(button) = self.button(main_thread) {
            // SAFETY: `performClick:` takes an optional sender; the button is live and this runs
            // on the main thread, where AppKit dispatches the resulting action synchronously.
            unsafe { button.performClick(None) };
        }
    }
}

/// Reports whether the panel window is on screen, whether it has the keyboard, and where it is.
pub fn panel_report(ns_view: NonNull<c_void>) -> Result<PanelReport, String> {
    let window = view(ns_view)?
        .window()
        .ok_or("the panel view is not in a window")?;
    let frame = window.frame();
    Ok(PanelReport {
        visible: window.isVisible(),
        key: window.isKeyWindow(),
        alpha: window.alphaValue(),
        frame: (
            frame.origin.x,
            frame.origin.y,
            frame.size.width,
            frame.size.height,
        ),
    })
}

/// What the photographed panel shows about its outline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelOutline {
    /// The highest alpha found at the four corners of the panel's bounding box. A rounded panel in
    /// a transparent window leaves those corners all but empty — only the shadow reaches them.
    pub corner_alpha: f64,
    /// The lowest alpha a few points inside each corner, where the panel itself is.
    pub inside_alpha: f64,
    /// The alpha just below the panel's bottom edge: the window shadow.
    pub shadow_alpha: f64,
}

/// Writes the panel window, as the window server composites it, to a PNG at `path`, and measures
/// its outline in the same image.
///
/// The image keeps the window's alpha and includes its shadow, so the rounded corners and the
/// shadow can be checked in the file.
pub fn capture_panel(ns_view: NonNull<c_void>, path: &Path) -> Result<PanelOutline, String> {
    let window = view(ns_view)?
        .window()
        .ok_or("the panel view is not in a window")?;
    let window_id = u32::try_from(window.windowNumber()).map_err(|error| error.to_string())?;
    // A null rectangle asks for the window's own bounds, shadow included.
    let null = NSRect::new(
        NSPoint::new(f64::INFINITY, f64::INFINITY),
        NSSize::new(0.0, 0.0),
    );
    #[expect(
        deprecated,
        reason = "ScreenCaptureKit needs Screen Recording permission even for the app's own window"
    )]
    let image = objc2_core_graphics::CGWindowListCreateImage(
        null,
        CGWindowListOption::OptionIncludingWindow,
        window_id,
        CGWindowImageOption::BestResolution,
    )
    .ok_or("the window server returned no image")?;
    let bitmap = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &image);
    // SAFETY: An empty dictionary is a valid property list for PNG encoding.
    let data = unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }
    .ok_or("the image could not be encoded")?;
    if !data.writeToFile_atomically(&NSString::from_str(&path.to_string_lossy()), true) {
        return Err(format!("could not write {}", path.display()));
    }
    outline(&bitmap)
}

/// Finds the panel's opaque bounding box along the middle row and column of the image, then reads
/// the alpha at its corners, just inside them, and just below it.
fn outline(bitmap: &NSBitmapImageRep) -> Result<PanelOutline, String> {
    let width = bitmap.pixelsWide();
    let height = bitmap.pixelsHigh();
    let alpha = |x: isize, y: isize| {
        bitmap
            .colorAtX_y(x.clamp(0, width - 1), y.clamp(0, height - 1))
            .map_or(0.0, |color| color.alphaComponent())
    };
    let opaque = |x: isize, y: isize| alpha(x, y) > 0.999;
    let (mid_x, mid_y) = (width / 2, height / 2);
    let left = (0..width)
        .find(|&x| opaque(x, mid_y))
        .ok_or("no opaque panel")?;
    let right = (0..width)
        .rev()
        .find(|&x| opaque(x, mid_y))
        .ok_or("no opaque panel")?;
    let top = (0..height)
        .find(|&y| opaque(mid_x, y))
        .ok_or("no opaque panel")?;
    let bottom = (0..height)
        .rev()
        .find(|&y| opaque(mid_x, y))
        .ok_or("no opaque panel")?;
    let inset = 16;
    Ok(PanelOutline {
        corner_alpha: [(left, top), (right, top), (left, bottom), (right, bottom)]
            .into_iter()
            .map(|(x, y)| alpha(x, y))
            .fold(0.0, f64::max),
        inside_alpha: [
            (left + inset, top + inset),
            (right - inset, top + inset),
            (left + inset, bottom - inset),
            (right - inset, bottom - inset),
        ]
        .into_iter()
        .map(|(x, y)| alpha(x, y))
        .fold(1.0, f64::min),
        shadow_alpha: alpha(mid_x, bottom + 12),
    })
}

/// A pointer event the self-test can post to the app's own event queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerEvent {
    Moved,
    Down,
    Dragged,
    Up,
    RightDown,
    RightUp,
}

/// Posts a pointer event at `(x, y)` — in the window's own points, measured from its top-left
/// corner, as Iced lays it out — to the app's event queue, where winit reads it as real input.
///
/// An app may post events to itself without Accessibility permission, which posting to the
/// system's event stream would need.
pub fn post_pointer(
    ns_view: NonNull<c_void>,
    event: PointerEvent,
    x: f64,
    y: f64,
    click_count: isize,
) -> Result<(), String> {
    let main_thread = MainThreadMarker::new().ok_or("not on the main thread")?;
    let window = view(ns_view)?
        .window()
        .ok_or("the panel view is not in a window")?;
    let height = window.frame().size.height;
    let kind = match event {
        PointerEvent::Moved => NSEventType::MouseMoved,
        PointerEvent::Down => NSEventType::LeftMouseDown,
        PointerEvent::Dragged => NSEventType::LeftMouseDragged,
        PointerEvent::Up => NSEventType::LeftMouseUp,
        PointerEvent::RightDown => NSEventType::RightMouseDown,
        PointerEvent::RightUp => NSEventType::RightMouseUp,
    };
    let pressure = if matches!(event, PointerEvent::Down | PointerEvent::Dragged) {
        1.0
    } else {
        0.0
    };
    let posted = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
        kind,
        NSPoint::new(x, height - y),
        NSEventModifierFlags::empty(),
        0.0,
        window.windowNumber(),
        None,
        0,
        click_count,
        pressure,
    )
    .ok_or("AppKit refused to create the pointer event")?;
    NSApplication::sharedApplication(main_thread).postEvent_atStart(&posted, false);
    Ok(())
}

/// Posts a key press — down then up — typing `characters`, with Command held when `command`.
pub fn post_key(
    ns_view: NonNull<c_void>,
    characters: &str,
    key_code: u16,
    command: bool,
) -> Result<(), String> {
    let main_thread = MainThreadMarker::new().ok_or("not on the main thread")?;
    let window = view(ns_view)?
        .window()
        .ok_or("the panel view is not in a window")?;
    let flags = if command {
        NSEventModifierFlags::Command
    } else {
        NSEventModifierFlags::empty()
    };
    let text = NSString::from_str(characters);
    for kind in [NSEventType::KeyDown, NSEventType::KeyUp] {
        let posted = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            kind,
            NSPoint::new(0.0, 0.0),
            flags,
            0.0,
            window.windowNumber(),
            None,
            &text,
            &text,
            false,
            key_code,
        )
        .ok_or("AppKit refused to create the key event")?;
        NSApplication::sharedApplication(main_thread).postEvent_atStart(&posted, false);
    }
    Ok(())
}

/// One accessibility element as `AppKit` reports it to assistive technologies.
#[derive(Clone, Debug, PartialEq)]
pub struct AccessibilityReport {
    pub label: String,
    /// The frame in the view's own coordinates, as the element was given it.
    pub frame_in_view: (f64, f64, f64, f64),
    /// The frame on screen, which `AppKit` derives and `VoiceOver` outlines.
    pub frame_on_screen: (f64, f64, f64, f64),
    /// Whether the element answers a press.
    pub pressable: bool,
}

/// Every accessibility element the panel's view publishes, with its frames.
pub fn accessibility_report(ns_view: NonNull<c_void>) -> Result<Vec<AccessibilityReport>, String> {
    let view = view(ns_view)?;
    let children = view.accessibilityChildren().unwrap_or_default();
    Ok((0..children.count())
        .map(|index| children.objectAtIndex(index))
        .map(|child| {
            let child: &AnyObject = &child;
            // SAFETY: Every child was created by the accessibility bridge as an
            // `NSAccessibilityElement` (or a subclass), which answers each of these: a nullable
            // label, two rectangles, and `respondsToSelector:`.
            unsafe {
                let label: Option<objc2::rc::Retained<NSString>> =
                    msg_send![child, accessibilityLabel];
                let in_view: NSRect = msg_send![child, accessibilityFrameInParentSpace];
                let on_screen: NSRect = msg_send![child, accessibilityFrame];
                let pressable: bool = msg_send![
                    child,
                    respondsToSelector: objc2::sel!(accessibilityPerformPress)
                ];
                AccessibilityReport {
                    label: label.map(|label| label.to_string()).unwrap_or_default(),
                    frame_in_view: (
                        in_view.origin.x,
                        in_view.origin.y,
                        in_view.size.width,
                        in_view.size.height,
                    ),
                    frame_on_screen: (
                        on_screen.origin.x,
                        on_screen.origin.y,
                        on_screen.size.width,
                        on_screen.size.height,
                    ),
                    pressable,
                }
            }
        })
        .collect())
}

/// Presses the first accessibility element whose label starts with `prefix`, the way `VoiceOver`
/// does for VO-Space: by sending it `accessibilityPerformPress`. Reports whether it was handled.
pub fn press_accessibility_element(ns_view: NonNull<c_void>, prefix: &str) -> Result<bool, String> {
    let view = view(ns_view)?;
    let children = view.accessibilityChildren().unwrap_or_default();
    for index in 0..children.count() {
        let child = children.objectAtIndex(index);
        let child: &AnyObject = &child;
        // SAFETY: As above; `accessibilityPerformPress` is sent only to elements that answer it.
        unsafe {
            let label: Option<objc2::rc::Retained<NSString>> = msg_send![child, accessibilityLabel];
            let matches = label.is_some_and(|label| label.to_string().starts_with(prefix));
            let pressable: bool = msg_send![
                child,
                respondsToSelector: objc2::sel!(accessibilityPerformPress)
            ];
            if matches && pressable {
                let handled: bool = msg_send![child, accessibilityPerformPress];
                return Ok(handled);
            }
        }
    }
    Err(format!("no pressable element labelled {prefix:?}"))
}

/// The accessibility labels the panel's view publishes, in order: what `VoiceOver` reads.
pub fn accessibility_labels(ns_view: NonNull<c_void>) -> Result<Vec<String>, String> {
    let view = view(ns_view)?;
    let children = view.accessibilityChildren().unwrap_or_default();
    Ok((0..children.count())
        .map(|index| children.objectAtIndex(index))
        .filter_map(|child| {
            let child: &AnyObject = &child;
            // SAFETY: Every child was created by the accessibility bridge as an
            // `NSAccessibilityElement`, whose `accessibilityLabel` is a nullable `NSString`.
            let label: Option<objc2::rc::Retained<NSString>> =
                unsafe { msg_send![child, accessibilityLabel] };
            label.map(|label| label.to_string())
        })
        .collect())
}

fn view<'a>(ns_view: NonNull<c_void>) -> Result<&'a NSView, String> {
    MainThreadMarker::new().ok_or("not on the main thread")?;
    // SAFETY: Iced/winit supplied `ns_view` through the AppKit raw-window-handle contract, so it is
    // a non-null pointer to the live NSView of an open window. This is reached only from Iced's
    // window callback on the main thread (checked above), and callers do not keep the borrow.
    Ok(unsafe { ns_view.cast::<NSView>().as_ref() })
}
