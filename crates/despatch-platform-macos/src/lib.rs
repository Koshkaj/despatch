#![doc = "Isolated macOS platform adapters for Despatch."]
#![deny(unsafe_code)]
#![allow(clippy::missing_errors_doc)]

mod default_paths;

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod accessibility;
#[cfg(target_os = "macos")]
mod appearance;
#[cfg(target_os = "macos")]
mod command;
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod context_menu;
#[cfg(target_os = "macos")]
mod default_terminal;
#[cfg(target_os = "macos")]
mod file_panel;
#[cfg(target_os = "macos")]
mod ghostty;
#[cfg(target_os = "macos")]
mod local_preferences;
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod login_item;
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod panel_window;
#[cfg(target_os = "macos")]
mod script_file;
#[cfg(all(target_os = "macos", feature = "self-test"))]
#[allow(unsafe_code)]
mod self_test;
#[cfg(target_os = "macos")]
mod shell_script;
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod status_item;
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod system_symbol;
#[cfg(target_os = "macos")]
mod terminal_argv;
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod terminal_association;
#[cfg(target_os = "macos")]
mod workspace;

pub use default_paths::{DefaultPathError, default_config_path};

#[cfg(target_os = "macos")]
pub use accessibility::{
    NativeAccessibilityNode, NativeAccessibilityRole, PressHandler, install_accessibility_root,
};
#[cfg(target_os = "macos")]
pub use appearance::{FALLBACK_ACCENT, SystemAppearance, system_appearance};
#[cfg(target_os = "macos")]
pub use command::{
    EnvironmentVariables, ExecutableResolver, LaunchBuildError, RequestResolver, ResolvedCommand,
    ResolvedPayload, SystemExecutableResolver,
};
#[cfg(target_os = "macos")]
pub use context_menu::{ContextMenuItem, show_context_menu};
#[cfg(target_os = "macos")]
pub use default_terminal::{
    MacOsTerminalLauncher, TerminalChoice, terminal_code, terminal_for_code,
};
#[cfg(target_os = "macos")]
pub use file_panel::{
    FileChoiceRequest, SavePathRequest, choose_file, choose_save_path, portable_path,
};
#[cfg(target_os = "macos")]
pub use local_preferences::{LocalPreferences, LocalPreferencesError};
#[cfg(target_os = "macos")]
pub use login_item::{LoginItemStatus, login_item_status, set_open_at_login};
#[cfg(target_os = "macos")]
pub use panel_window::{
    FADE_SECONDS, PANEL_GAP, ScreenRect, StatusAnchor, configure_panel, hide_panel, panel_top_left,
    refresh_panel_shadow, set_panel_appearance, show_panel, use_accessory_activation_policy,
};
#[cfg(all(target_os = "macos", feature = "self-test"))]
pub use self_test::{
    AccessibilityReport, PanelOutline, PanelReport, PointerEvent, accessibility_labels,
    accessibility_report, capture_panel, panel_report, post_key, post_pointer,
    press_accessibility_element,
};
#[cfg(target_os = "macos")]
pub use status_item::{StatusItem, StatusItemEvent};
#[cfg(target_os = "macos")]
pub use system_symbol::{SystemSymbolPixels, render_system_symbol};
#[cfg(target_os = "macos")]
pub use terminal_association::preferred_terminal_bundle_id;
#[cfg(target_os = "macos")]
pub use workspace::{MacOsFileRevealer, OpenInvoker, OpenRequest, OpenResult, SystemOpenInvoker};
