//! The menu-bar item that Despatch lives behind.
//!
//! Iced and winit have no notion of a status item, so this is the smallest `AppKit` bridge that
//! provides one: an `NSStatusItem` with a template SF Symbol, a target object that turns its clicks
//! and its menu into [`StatusItemEvent`]s, and an observer that reports when the app stops being
//! the active one. Nothing Objective-C leaves this module; the UI receives plain events through the
//! callback it passed in.

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationDidResignActiveNotification, NSEventMask, NSEventModifierFlags,
    NSEventType, NSImage, NSMenu, NSMenuItem, NSScreen, NSStatusBar, NSStatusItem,
    NSVariableStatusItemLength,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSPoint, NSString};

use crate::panel_window::{ScreenRect, StatusAnchor};

/// Something the user did to the status item, or to the app around it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusItemEvent {
    /// The item was clicked: show the panel if it is hidden, hide it if it is showing.
    Toggle,
    /// "Open Despatch" in the item's menu: show the panel.
    Open,
    /// "Settings…" in the item's menu.
    OpenSettings,
    /// "Reveal Config in Finder" in the item's menu.
    RevealConfig,
    /// "Quit Despatch" in the item's menu.
    Quit,
    /// Another app became active, which is how a click anywhere outside the panel arrives.
    Deactivated,
}

type Callback = Box<dyn Fn(StatusItemEvent)>;

struct TargetIvars {
    on_event: Callback,
}

define_class!(
    // SAFETY:
    // - NSObject has no subclassing requirements.
    // - `StatusTarget` does not implement `Drop`, and its only ivar is an owned Rust closure.
    // - It is only ever created and messaged on the main thread (`MainThreadOnly`), which is the
    //   thread AppKit delivers control actions, menu actions, and these notifications on.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "DespatchStatusItemTarget"]
    #[ivars = TargetIvars]
    struct StatusTarget;

    impl StatusTarget {
        #[unsafe(method(statusItemPressed:))]
        fn status_item_pressed(&self, sender: Option<&AnyObject>) {
            let _ = sender;
            if self.is_context_click() {
                self.show_menu();
            } else {
                self.send(StatusItemEvent::Toggle);
            }
        }

        #[unsafe(method(openPanel:))]
        fn open_panel(&self, sender: Option<&AnyObject>) {
            let _ = sender;
            self.send(StatusItemEvent::Open);
        }

        #[unsafe(method(openSettings:))]
        fn open_settings(&self, sender: Option<&AnyObject>) {
            let _ = sender;
            self.send(StatusItemEvent::OpenSettings);
        }

        #[unsafe(method(revealConfig:))]
        fn reveal_config(&self, sender: Option<&AnyObject>) {
            let _ = sender;
            self.send(StatusItemEvent::RevealConfig);
        }

        #[unsafe(method(quit:))]
        fn quit(&self, sender: Option<&AnyObject>) {
            let _ = sender;
            self.send(StatusItemEvent::Quit);
        }

        #[unsafe(method(applicationDidResignActive:))]
        fn application_did_resign_active(&self, notification: &NSNotification) {
            let _ = notification;
            self.send(StatusItemEvent::Deactivated);
        }
    }

    unsafe impl NSObjectProtocol for StatusTarget {}
);

impl StatusTarget {
    fn new(main_thread: MainThreadMarker, on_event: Callback) -> Retained<Self> {
        let this = Self::alloc(main_thread).set_ivars(TargetIvars { on_event });
        // SAFETY: `init` is NSObject's designated initializer and takes no arguments; the
        // allocation above has its ivars set, which is what `define_class!` requires first.
        unsafe { msg_send![super(this), init] }
    }

    fn send(&self, event: StatusItemEvent) {
        (self.ivars().on_event)(event);
    }

    /// Whether the click that is being handled asks for the menu rather than the panel: a right
    /// click, or a left click with Control held, which is the same request on a one-button mouse.
    fn is_context_click(&self) -> bool {
        let application = NSApplication::sharedApplication(self.mtm());
        let Some(event) = application.currentEvent() else {
            return false;
        };
        let kind = event.r#type();
        kind == NSEventType::RightMouseDown
            || kind == NSEventType::RightMouseUp
            || event
                .modifierFlags()
                .contains(NSEventModifierFlags::Control)
    }

    fn show_menu(&self) {
        let main_thread = self.mtm();
        let Some(item) = STATUS_ITEM_FOR_MENU.with(|cell| cell.borrow().clone()) else {
            return;
        };
        let Some(button) = item.button(main_thread) else {
            return;
        };
        let menu = NSMenu::initWithTitle(NSMenu::alloc(main_thread), &NSString::from_str(""));
        for entry in menu_entries() {
            match entry {
                MenuEntry::Separator => menu.addItem(&NSMenuItem::separatorItem(main_thread)),
                MenuEntry::Action { title, action, key } => {
                    // SAFETY: Every selector named in `menu_entries` is a method of `StatusTarget` taking one
                    // object argument, and `self` — the target — outlives the menu, which is only
                    // shown for the duration of the synchronous pop-up below.
                    let menu_item = unsafe {
                        NSMenuItem::initWithTitle_action_keyEquivalent(
                            NSMenuItem::alloc(main_thread),
                            &NSString::from_str(title),
                            Some(action),
                            &NSString::from_str(key),
                        )
                    };
                    // SAFETY: `self` is a live NSObject on the main thread; the menu item only
                    // keeps a weak reference to its target, and the target outlives the menu.
                    unsafe { menu_item.setTarget(Some(self)) };
                    menu.addItem(&menu_item);
                }
            }
        }
        // The menu hangs from the bottom edge of the item. A status bar button is a flipped view,
        // so its bottom edge is at its full height.
        let location = NSPoint::new(0.0, button.bounds().size.height + 4.0);
        menu.popUpMenuPositioningItem_atLocation_inView(None, location, Some(&button));
    }
}

/// One line of the status item's menu.
enum MenuEntry {
    Action {
        title: &'static str,
        action: Sel,
        key: &'static str,
    },
    Separator,
}

fn menu_entries() -> [MenuEntry; 6] {
    [
        MenuEntry::Action {
            title: "Open Despatch",
            action: sel!(openPanel:),
            key: "",
        },
        MenuEntry::Action {
            title: "Settings…",
            action: sel!(openSettings:),
            key: ",",
        },
        MenuEntry::Separator,
        MenuEntry::Action {
            title: "Reveal Config in Finder",
            action: sel!(revealConfig:),
            key: "",
        },
        MenuEntry::Separator,
        MenuEntry::Action {
            title: "Quit Despatch",
            action: sel!(quit:),
            key: "q",
        },
    ]
}

thread_local! {
    /// The installed item, for the target to hang its menu from.
    ///
    /// The target cannot own the item — the item owns the target through its button — so the menu
    /// finds the item here. It is main-thread state by construction: AppKit only creates and clicks
    /// status items there.
    static STATUS_ITEM_FOR_MENU: std::cell::RefCell<Option<Retained<NSStatusItem>>> =
        const { std::cell::RefCell::new(None) };
}

/// The status item, alive for as long as this value is.
pub struct StatusItem {
    item: Retained<NSStatusItem>,
    target: Retained<StatusTarget>,
}

impl StatusItem {
    /// Puts an item showing the SF Symbol `symbol` into the menu bar.
    ///
    /// `on_event` is called on the main thread for every click, menu choice, and deactivation of
    /// the app. Returns `None` off the main thread, or when the symbol does not exist on the running
    /// macOS version.
    #[must_use]
    pub fn install(
        symbol: &str,
        label: &str,
        on_event: impl Fn(StatusItemEvent) + 'static,
    ) -> Option<Self> {
        let main_thread = MainThreadMarker::new()?;
        let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(symbol),
            Some(&NSString::from_str(label)),
        )?;
        // A template image is drawn by the menu bar itself, in whatever color the bar and its
        // highlight need, so it follows light, dark, and tinted menu bars with no work here.
        image.setTemplate(true);

        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        let target = StatusTarget::new(main_thread, Box::new(on_event));
        let button = item.button(main_thread)?;
        button.setImage(Some(&image));
        button.setToolTip(Some(&NSString::from_str(label)));
        // SAFETY: `target` is kept alive by the returned `StatusItem` for as long as the item is in
        // the menu bar, and `statusItemPressed:` is a method of its class taking one object.
        unsafe {
            button.setTarget(Some(&target));
            button.setAction(Some(sel!(statusItemPressed:)));
        }
        // The panel answers the press, not the release, like every other menu-bar extra.
        button.sendActionOn(NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown);

        // SAFETY: The observer is `target`, which the returned value keeps alive and which removes
        // itself in `Drop` below; the selector takes one NSNotification; the notification name is
        // an immutable AppKit constant.
        unsafe {
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                &target,
                sel!(applicationDidResignActive:),
                Some(NSApplicationDidResignActiveNotification),
                None,
            );
        }
        STATUS_ITEM_FOR_MENU.with(|cell| *cell.borrow_mut() = Some(item.clone()));
        Some(Self { item, target })
    }

    /// Where the item is, and the room the screen it is on has for a panel below it.
    #[must_use]
    pub fn anchor(&self) -> Option<StatusAnchor> {
        let main_thread = MainThreadMarker::new()?;
        let window = self.item.button(main_thread)?.window()?;
        let screen = window
            .screen()
            .or_else(|| NSScreen::mainScreen(main_thread))?;
        let frame = ScreenRect::from(window.frame());
        // Right after it is created the item has not been laid out yet, and its window reports an
        // empty frame at the origin. A panel hung from that would open at the bottom of the screen.
        let placed = frame.width > 0.0 && frame.height > 0.0;
        Some(StatusAnchor {
            item: placed.then_some(frame),
            visible: ScreenRect::from(screen.visibleFrame()),
        })
    }

    #[cfg(feature = "self-test")]
    pub(crate) fn button(
        &self,
        main_thread: MainThreadMarker,
    ) -> Option<Retained<objc2_app_kit::NSStatusBarButton>> {
        self.item.button(main_thread)
    }

    /// Draws the item pressed while its panel is open, the way a menu-bar menu looks while open.
    pub fn set_highlighted(&self, highlighted: bool) {
        let Some(main_thread) = MainThreadMarker::new() else {
            return;
        };
        if let Some(button) = self.item.button(main_thread) {
            button.highlight(highlighted);
        }
    }
}

impl Drop for StatusItem {
    fn drop(&mut self) {
        // SAFETY: `target` was registered as an observer in `install` and is still alive here.
        unsafe { NSNotificationCenter::defaultCenter().removeObserver(&self.target) };
        NSStatusBar::systemStatusBar().removeStatusItem(&self.item);
        STATUS_ITEM_FOR_MENU.with(|cell| *cell.borrow_mut() = None);
    }
}

impl std::fmt::Debug for StatusItem {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("StatusItem")
    }
}
