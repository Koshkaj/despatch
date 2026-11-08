//! The UI side of the menu-bar item: the events it sends into Iced, and the calls that show, hide,
//! and place the panel window.
//!
//! The item itself and every `AppKit` call live in `despatch-platform-macos`. This module only
//! turns its callbacks into a [`Subscription`] and its window calls into [`Task`]s, so the rest of
//! the interface deals in plain messages.

use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, StreamExt};
use iced::{Subscription, Task, window};

/// Something that happened to the menu-bar item or around it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PanelEvent {
    /// The item was clicked.
    Toggle,
    /// "Open Despatch" was chosen from the item's menu.
    Open,
    /// "Settings…" was chosen from the item's menu.
    OpenSettings,
    /// "Reveal Config in Finder" was chosen from the item's menu.
    RevealConfig,
    /// "Quit Despatch" was chosen from the item's menu.
    Quit,
    /// Another app became active: the user clicked somewhere outside the panel.
    Deactivated,
    /// An assistive technology such as `VoiceOver` pressed the accessibility element at this
    /// index of the tree last installed.
    AccessibilityPress(usize),
}

/// The receiving end of the item's events, handed to the subscription once.
///
/// The item's callbacks arrive on the main thread while the subscription's stream is polled by
/// Iced's executor, so the two meet through a channel. The receiver is taken by the first stream
/// that asks for it; the subscription identity is constant, so Iced only ever starts one.
#[derive(Clone)]
struct Inbox(Arc<Mutex<Option<mpsc::UnboundedReceiver<PanelEvent>>>>);

impl Hash for Inbox {
    fn hash<H: Hasher>(&self, state: &mut H) {
        "despatch-menu-bar".hash(state);
    }
}

/// The menu-bar item and the channel its events travel through.
pub struct MenuBar {
    sender: mpsc::UnboundedSender<PanelEvent>,
    inbox: Inbox,
    #[cfg(target_os = "macos")]
    item: Option<despatch_platform_macos::StatusItem>,
}

impl std::fmt::Debug for MenuBar {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("MenuBar")
    }
}

impl Default for MenuBar {
    fn default() -> Self {
        let (sender, receiver) = mpsc::unbounded();
        Self {
            sender,
            inbox: Inbox(Arc::new(Mutex::new(Some(receiver)))),
            #[cfg(target_os = "macos")]
            item: None,
        }
    }
}

/// The symbol the menu-bar item shows. A template image, so the menu bar tints it.
pub const STATUS_SYMBOL: &str = "terminal";

impl MenuBar {
    /// Puts the item into the menu bar. Does nothing if it is already there.
    ///
    /// Must run on the main thread after the event loop has started, because `AppKit` only hands out
    /// status items to a running application.
    pub fn install(&mut self) {
        #[cfg(target_os = "macos")]
        {
            if self.item.is_some() {
                return;
            }
            let sender = self.sender.clone();
            self.item = despatch_platform_macos::StatusItem::install(
                STATUS_SYMBOL,
                "Despatch",
                move |event| {
                    let event = match event {
                        despatch_platform_macos::StatusItemEvent::Toggle => PanelEvent::Toggle,
                        despatch_platform_macos::StatusItemEvent::Open => PanelEvent::Open,
                        despatch_platform_macos::StatusItemEvent::OpenSettings => {
                            PanelEvent::OpenSettings
                        }
                        despatch_platform_macos::StatusItemEvent::RevealConfig => {
                            PanelEvent::RevealConfig
                        }
                        despatch_platform_macos::StatusItemEvent::Quit => PanelEvent::Quit,
                        despatch_platform_macos::StatusItemEvent::Deactivated => {
                            PanelEvent::Deactivated
                        }
                    };
                    // A closed channel means the interface is shutting down; there is no one to tell.
                    let _ = sender.unbounded_send(event);
                },
            );
            despatch_platform_macos::use_accessory_activation_policy();
        }
    }

    /// A way for other `AppKit` callbacks to reach the interface through the same channel.
    #[must_use]
    pub fn sender(&self) -> mpsc::UnboundedSender<PanelEvent> {
        self.sender.clone()
    }

    /// Whether the item is in the menu bar.
    #[must_use]
    pub fn is_installed(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.item.is_some()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    /// Where a panel of `width` should hang to sit under the item, in screen coordinates.
    #[must_use]
    pub fn panel_top_left(&self, width: f32) -> Option<(f64, f64)> {
        #[cfg(target_os = "macos")]
        {
            let anchor = self.item.as_ref()?.anchor()?;
            Some(despatch_platform_macos::panel_top_left(
                &anchor,
                f64::from(width),
            ))
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = width;
            None
        }
    }

    /// Presses the item through `AppKit`, as a click would. Development builds only.
    #[cfg(feature = "capture")]
    pub fn perform_click(&self) {
        #[cfg(target_os = "macos")]
        if let Some(item) = &self.item {
            item.perform_click();
        }
    }

    /// Draws the item pressed while the panel is open.
    pub fn set_highlighted(&self, highlighted: bool) {
        #[cfg(target_os = "macos")]
        if let Some(item) = &self.item {
            item.set_highlighted(highlighted);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = highlighted;
    }

    /// The item's events, as messages.
    pub fn events(&self) -> Subscription<PanelEvent> {
        Subscription::run_with(self.inbox.clone(), |inbox| {
            let receiver = inbox.0.lock().ok().and_then(|mut slot| slot.take());
            iced::stream::channel(16, async move |mut output| {
                let Some(mut receiver) = receiver else {
                    return;
                };
                while let Some(event) = receiver.next().await {
                    if output.send(event).await.is_err() {
                        return;
                    }
                }
            })
        })
    }
}

/// Gives the panel window its menu-bar behaviour — every Space, over full-screen apps, a shadow —
/// and the glass it stands on, in the light or dark appearance the interface is drawn in.
pub fn configure(window_id: window::Id, dark: bool) -> Task<Result<(), String>> {
    on_view(window_id, move |view| {
        #[cfg(target_os = "macos")]
        {
            despatch_platform_macos::configure_panel(view, dark)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (view, dark);
            Ok(())
        }
    })
}

/// Turns the glass behind the panel, and its native menus, light or dark to match the interface.
pub fn set_appearance(window_id: window::Id, dark: bool) -> Task<Result<(), String>> {
    on_view(window_id, move |view| {
        #[cfg(target_os = "macos")]
        {
            despatch_platform_macos::set_panel_appearance(view, dark)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (view, dark);
            Ok(())
        }
    })
}

/// Shows the panel with its top-left corner at `top_left` and gives it the keyboard, fading it in
/// when `fade` — which is off when the user has asked to reduce motion.
pub fn show(
    window_id: window::Id,
    top_left: Option<(f64, f64)>,
    fade: bool,
) -> Task<Result<(), String>> {
    on_view(window_id, move |view| {
        #[cfg(target_os = "macos")]
        {
            despatch_platform_macos::show_panel(view, top_left, fade)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (view, top_left, fade);
            Ok(())
        }
    })
}

/// Recomputes the panel's shadow from what it has now drawn.
pub fn refresh_shadow(window_id: window::Id) -> Task<Result<(), String>> {
    on_view(window_id, |view| {
        #[cfg(target_os = "macos")]
        {
            despatch_platform_macos::refresh_panel_shadow(view)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = view;
            Ok(())
        }
    })
}

/// Hides the panel; with `return_focus`, hands the keyboard back to the app the user came from.
pub fn hide(window_id: window::Id, return_focus: bool) -> Task<Result<(), String>> {
    on_view(window_id, move |view| {
        #[cfg(target_os = "macos")]
        {
            despatch_platform_macos::hide_panel(view, return_focus)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (view, return_focus);
            Ok(())
        }
    })
}

/// Runs `f` with the `NSView` behind the window, on the main thread.
pub(crate) fn on_view<T: Send + 'static>(
    window_id: window::Id,
    f: impl FnOnce(std::ptr::NonNull<std::ffi::c_void>) -> Result<T, String> + Send + 'static,
) -> Task<Result<T, String>> {
    window::run(window_id, move |window| {
        use iced::window::raw_window_handle::RawWindowHandle;
        let handle = window
            .window_handle()
            .map_err(|error| format!("native window handle is unavailable: {error}"))?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return Err("Iced did not provide an AppKit window handle".to_owned());
        };
        f(handle.ns_view)
    })
}
