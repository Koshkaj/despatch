use despatch_platform_macos::{
    NativeAccessibilityNode, NativeAccessibilityRole, install_accessibility_root,
};
use iced::window::Window;
use iced::window::raw_window_handle::RawWindowHandle;

use crate::accessibility::{Node, Role};

pub fn install(window: &dyn Window, nodes: &[Node]) -> Result<(), String> {
    let handle = window
        .window_handle()
        .map_err(|error| format!("native window handle is unavailable: {error}"))?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err("Iced did not provide an AppKit window handle".to_owned());
    };
    let nodes: Vec<_> = nodes
        .iter()
        .map(|node| NativeAccessibilityNode {
            role: match node.role {
                Role::Group => NativeAccessibilityRole::Group,
                Role::Button => NativeAccessibilityRole::Button,
                Role::SearchField => NativeAccessibilityRole::SearchField,
            },
            label: node.label.clone(),
            selected: node.selected,
        })
        .collect();
    install_accessibility_root(handle.ns_view, &nodes)
}
