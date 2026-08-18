//! Manual macOS integration checks.
//!
//! These open a real terminal window, so they are ignored by default. Run them deliberately with:
//!
//! ```bash
//! cargo test -p despatch-platform-macos --test manual_terminal_launch -- --ignored --nocapture
//! ```

#![cfg(target_os = "macos")]

use std::collections::BTreeMap;
use std::path::PathBuf;

use despatch_application::{CommandLaunchRequest, LaunchRequest, TerminalLauncher};
use despatch_platform_macos::{MacOsTerminalLauncher, TerminalChoice};

#[test]
#[ignore = "opens a real terminal window"]
fn a_launch_opens_in_the_terminal_the_user_chose() {
    let launcher = MacOsTerminalLauncher::default();
    let choice = TerminalChoice::detect();
    println!("resolved terminal: {choice:?} ({})", choice.describe());

    launcher
        .launch(&LaunchRequest::Command(CommandLaunchRequest {
            env: BTreeMap::new(),
            title: "Despatch launch check".to_owned(),
            executable: "echo".to_owned(),
            working_directory: None,
            args: vec!["Despatch opened this window".to_owned()],
            config_directory: PathBuf::from("/tmp"),
        }))
        .expect("launch the check command");
}
