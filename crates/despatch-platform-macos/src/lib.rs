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
mod default_terminal;
#[cfg(target_os = "macos")]
mod file_panel;
#[cfg(target_os = "macos")]
mod ghostty;
#[cfg(target_os = "macos")]
mod local_preferences;
#[cfg(target_os = "macos")]
mod script_file;
#[cfg(target_os = "macos")]
mod shell_script;
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
    NativeAccessibilityNode, NativeAccessibilityRole, install_accessibility_root,
};
#[cfg(target_os = "macos")]
pub use appearance::{FALLBACK_ACCENT, SystemAppearance, system_appearance};
#[cfg(target_os = "macos")]
pub use command::{
    EnvironmentVariables, ExecutableResolver, LaunchBuildError, RequestResolver, ResolvedCommand,
    ResolvedPayload, SystemExecutableResolver,
};
#[cfg(target_os = "macos")]
pub use default_terminal::{MacOsTerminalLauncher, TerminalChoice};
#[cfg(target_os = "macos")]
pub use file_panel::{FileChoiceRequest, choose_file, portable_path};
#[cfg(target_os = "macos")]
pub use local_preferences::{LocalPreferences, LocalPreferencesError};
#[cfg(target_os = "macos")]
pub use system_symbol::{SystemSymbolPixels, render_system_symbol};
#[cfg(target_os = "macos")]
pub use terminal_association::preferred_terminal_bundle_id;
#[cfg(target_os = "macos")]
pub use workspace::{MacOsFileRevealer, OpenInvoker, OpenRequest, OpenResult, SystemOpenInvoker};
