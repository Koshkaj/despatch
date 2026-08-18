use std::ffi::c_void;
use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSAccessibility, NSAccessibilityButtonRole, NSAccessibilityElement, NSAccessibilityGroupRole,
    NSAccessibilityRole, NSAccessibilityTextFieldRole, NSView,
};
use objc2_foundation::{NSArray, NSRect, NSString};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeAccessibilityRole {
    Group,
    Button,
    SearchField,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeAccessibilityNode {
    pub role: NativeAccessibilityRole,
    pub label: String,
    pub selected: bool,
}

pub fn install_accessibility_root(
    ns_view: NonNull<c_void>,
    nodes: &[NativeAccessibilityNode],
) -> Result<(), String> {
    // SAFETY: Iced/winit supplied `ns_view` through the AppKit raw-window-handle contract, so it
    // is a non-null pointer to an NSView owned by the live window. The UI invokes this adapter from
    // Iced's window callback on the macOS main thread, the reference does not escape this call, and
    // every selector below accepts a non-null NSView receiver for this lifetime.
    let view = unsafe { ns_view.cast::<NSView>().as_ref() };
    view.setAccessibilityElement(true);
    view.setAccessibilityRole(Some(accessibility_role(NativeAccessibilityRole::Group)));
    view.setAccessibilityLabel(Some(&NSString::from_str(
        "Despatch server and agent launcher",
    )));

    let children: Vec<Retained<AnyObject>> = nodes
        .iter()
        .map(|node| {
            let label = if node.selected {
                format!("{}, selected", node.label)
            } else {
                node.label.clone()
            };
            let label = NSString::from_str(&label);
            // SAFETY: The role constants are valid AppKit NSAccessibilityRole instances on the
            // supported macOS 13+ target. `view` is a live NSAccessibility-conforming parent on
            // the main thread, the frame is intentionally null because Iced owns layout geometry,
            // and AppKit returns a retained, non-null accessibility object that owns its label.
            unsafe {
                NSAccessibilityElement::accessibilityElementWithRole_frame_label_parent(
                    accessibility_role(node.role),
                    NSRect::ZERO,
                    Some(&label),
                    Some(view),
                )
            }
        })
        .collect();
    let children = NSArray::from_retained_slice(&children);
    // SAFETY: Every array member was created immediately above by NSAccessibilityElement's
    // designated factory and therefore conforms to the accessibility element protocol. AppKit
    // retains/copies the array for the live NSView; no borrowed Rust storage or nullable element
    // pointer is exposed after this setter returns. The call remains on the main thread.
    unsafe { view.setAccessibilityChildren(Some(&children)) };
    Ok(())
}

fn accessibility_role(role: NativeAccessibilityRole) -> &'static NSAccessibilityRole {
    // SAFETY: These are immutable, process-lifetime AppKit constants available on every supported
    // macOS version. The extern statics are declared non-null by objc2-app-kit.
    unsafe {
        match role {
            NativeAccessibilityRole::Group => NSAccessibilityGroupRole,
            NativeAccessibilityRole::Button => NSAccessibilityButtonRole,
            NativeAccessibilityRole::SearchField => NSAccessibilityTextFieldRole,
        }
    }
}
