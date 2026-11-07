//! Native right-click menus.
//!
//! Iced draws no menus of its own, and a menu drawn by the interface would not behave like the ones
//! around it — no keyboard navigation, no type-select, no system look. This pops up a real `NSMenu`
//! at the pointer and reports which item was chosen.

use std::cell::Cell;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSEvent, NSMenu, NSMenuItem};
use objc2_foundation::NSString;

/// One line of a context menu.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContextMenuItem {
    Action(String),
    Separator,
}

struct ChoiceIvars {
    chosen: Cell<Option<isize>>,
}

define_class!(
    // SAFETY:
    // - NSObject has no subclassing requirements.
    // - `ChoiceTarget` does not implement `Drop`; its only ivar is a plain `Cell`.
    // - It is created and messaged only on the main thread, where AppKit sends menu actions.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "DespatchContextMenuTarget"]
    #[ivars = ChoiceIvars]
    struct ChoiceTarget;

    impl ChoiceTarget {
        #[unsafe(method(choose:))]
        fn choose(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else {
                return;
            };
            // SAFETY: Only `NSMenuItem`s created below send `choose:`, and `tag` is an
            // `NSInteger` property of every menu item.
            let tag: isize = unsafe { msg_send![sender, tag] };
            self.ivars().chosen.set(Some(tag));
        }
    }

    unsafe impl NSObjectProtocol for ChoiceTarget {}
);

/// Shows `items` as a native menu at the pointer and returns the index of the action chosen, or
/// `None` when the menu was dismissed. Separators are not counted in the index.
///
/// The menu runs its own tracking loop, so this returns only once it has closed. Must run on the
/// main thread.
#[must_use]
pub fn show_context_menu(items: &[ContextMenuItem]) -> Option<usize> {
    let main_thread = MainThreadMarker::new()?;
    let target: Retained<ChoiceTarget> = {
        let this = ChoiceTarget::alloc(main_thread).set_ivars(ChoiceIvars {
            chosen: Cell::new(None),
        });
        // SAFETY: NSObject's designated initializer; the ivars are set as `define_class!`
        // requires.
        unsafe { msg_send![super(this), init] }
    };
    let menu = NSMenu::initWithTitle(NSMenu::alloc(main_thread), &NSString::from_str(""));
    let mut index: isize = 0;
    for item in items {
        match item {
            ContextMenuItem::Separator => menu.addItem(&NSMenuItem::separatorItem(main_thread)),
            ContextMenuItem::Action(title) => {
                // SAFETY: `choose:` is a method of `ChoiceTarget` taking one object, and the
                // target outlives the menu, which is only shown inside this function.
                let menu_item = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(main_thread),
                        &NSString::from_str(title),
                        Some(sel!(choose:)),
                        &NSString::from_str(""),
                    )
                };
                menu_item.setTag(index);
                // SAFETY: The item keeps a weak reference to `target`, which lives until this
                // function returns, after the menu has closed.
                unsafe { menu_item.setTarget(Some(&target)) };
                menu.addItem(&menu_item);
                index += 1;
            }
        }
    }
    // With no view, the location is in screen coordinates: the menu opens at the pointer.
    menu.popUpMenuPositioningItem_atLocation_inView(None, NSEvent::mouseLocation(), None);
    target
        .ivars()
        .chosen
        .get()
        .and_then(|chosen| usize::try_from(chosen).ok())
}
