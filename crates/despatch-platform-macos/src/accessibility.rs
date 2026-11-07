use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol};
use objc2::{ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSAccessibility, NSAccessibilityButtonRole, NSAccessibilityElement, NSAccessibilityGroupRole,
    NSAccessibilityRole, NSAccessibilityTextFieldRole, NSView,
};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize, NSString};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeAccessibilityRole {
    Group,
    Button,
    SearchField,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativeAccessibilityNode {
    pub role: NativeAccessibilityRole,
    pub label: String,
    pub selected: bool,
    /// Where the element is drawn, in the view's own points from its top-left corner: x, y,
    /// width, height. `VoiceOver` outlines this rectangle and moves its cursor through it.
    pub frame: Option<[f64; 4]>,
    /// Whether the element does something when pressed (VO-Space).
    pub pressable: bool,
}

/// Called with the index of the element `VoiceOver` pressed.
pub type PressHandler = Arc<dyn Fn(usize) + Send + Sync>;

struct PressableIvars {
    index: usize,
    on_press: PressHandler,
}

define_class!(
    // SAFETY:
    // - NSAccessibilityElement may be subclassed; this adds one method and no state AppKit sees.
    // - `PressableElement` does not implement `Drop`; its ivars are an index and an `Arc`.
    // - It is created on the main thread, and AppKit's accessibility runtime calls
    //   `accessibilityPerformPress` there.
    #[unsafe(super(NSAccessibilityElement))]
    #[thread_kind = MainThreadOnly]
    #[name = "DespatchPressableAccessibilityElement"]
    #[ivars = PressableIvars]
    struct PressableElement;

    impl PressableElement {
        /// What `VoiceOver` sends for VO-Space, and Switch Control for a press.
        #[unsafe(method(accessibilityPerformPress))]
        fn accessibility_perform_press(&self) -> bool {
            (self.ivars().on_press)(self.ivars().index);
            true
        }
    }

    unsafe impl NSObjectProtocol for PressableElement {}
);

pub fn install_accessibility_root(
    ns_view: NonNull<c_void>,
    nodes: &[NativeAccessibilityNode],
    on_press: &PressHandler,
) -> Result<(), String> {
    let main_thread = MainThreadMarker::new().ok_or("not on the main thread")?;
    // SAFETY: Iced/winit supplied `ns_view` through the AppKit raw-window-handle contract, so it
    // is a non-null pointer to an NSView owned by the live window. The UI invokes this adapter from
    // Iced's window callback on the macOS main thread (checked above), the reference does not
    // escape this call, and every selector below accepts a non-null NSView receiver.
    let view = unsafe { ns_view.cast::<NSView>().as_ref() };
    view.setAccessibilityElement(true);
    view.setAccessibilityRole(Some(accessibility_role(NativeAccessibilityRole::Group)));
    view.setAccessibilityLabel(Some(&NSString::from_str(
        "Despatch server and agent launcher",
    )));

    let children: Vec<Retained<AnyObject>> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let label = if node.selected {
                format!("{}, selected", node.label)
            } else {
                node.label.clone()
            };
            let label = NSString::from_str(&label);
            let element: Retained<NSAccessibilityElement> = if node.pressable {
                let this = PressableElement::alloc(main_thread).set_ivars(PressableIvars {
                    index,
                    on_press: Arc::clone(on_press),
                });
                // SAFETY: `init` is NSAccessibilityElement's initializer, inherited from NSObject;
                // the ivars are set as `define_class!` requires.
                let element: Retained<PressableElement> = unsafe { msg_send![super(this), init] };
                element.into_super()
            } else {
                // SAFETY: NSAccessibilityElement is created through its plain initializer; every
                // property is set below.
                unsafe { msg_send![NSAccessibilityElement::class(), new] }
            };
            element.setAccessibilityRole(Some(accessibility_role(node.role)));
            element.setAccessibilityLabel(Some(&label));
            element.setAccessibilitySelected(node.selected);
            // SAFETY: The parent is the live view above, an NSAccessibility-conforming object that
            // owns these children for as long as it shows them; AppKit keeps a weak reference.
            unsafe { element.setAccessibilityParent(Some(view)) };
            if let Some([x, y, width, height]) = node.frame {
                // The view is flipped (winit puts the origin at the top left), so the frame is
                // given exactly as the interface lays it out.
                element.setAccessibilityFrameInParentSpace(NSRect::new(
                    NSPoint::new(x, y),
                    NSSize::new(width, height),
                ));
            }
            Retained::into_super(Retained::into_super(element))
        })
        .collect();
    let children = NSArray::from_retained_slice(&children);
    // SAFETY: Every array member was created immediately above as an NSAccessibilityElement (or a
    // subclass of it) and therefore conforms to the accessibility element protocol. AppKit
    // retains/copies the array for the live NSView; no borrowed Rust storage is exposed after this
    // setter returns. The call remains on the main thread.
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
