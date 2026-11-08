//! Opening a session in Ghostty through Ghostty's own macOS scripting API.
//!
//! A Ghostty that is already running with a window open gets a new **tab** in its front window —
//! what Cmd-T does there. A Ghostty that is not running, or has no window, gets a new window: one
//! that is only being started opens its own default window, and a tab beside that would leave a
//! plain shell next to the session the user asked for.
//!
//! The alternative is `open -n -b … --args -e <argv…>`, which starts a *second copy of the whole
//! application* for every launch — Ghostty gains an entry in the Dock and in Cmd-Tab each time — and
//! hands the executable to a startup option Ghostty confirms with the user. The scripting API talks
//! to the copy already running: one instance, one new window, and macOS authorizes Despatch to
//! automate Ghostty once rather than per command.
//!
//! Everything the wrapper shell used to do, Ghostty does here as a property of the surface: where to
//! start, what to export, and holding the window open once the command ends.

use std::process::Command;

use despatch_application::{PortError, PortErrorKind};

use crate::shell_script::quote;
use crate::workspace::OpenResult;
use crate::{ResolvedCommand, ResolvedPayload};

/// The fixed `AppleScript` that creates the window.
///
/// Nothing is interpolated into this source. Every launcher value arrives as an `osascript`
/// argument, so a host or argument carrying a quote cannot close a string and become script.
///
/// `command` is *shell text*: Ghostty runs it as `/bin/bash --noprofile --norc -c 'exec -l <text>'`.
/// Ghostty's `direct:` prefix, which would skip that shell, is a **configuration-file** feature and
/// is not parsed by this property — a surface asked to run `direct:/path` tries to execute a file
/// literally named `direct:/path` and fails. So the text is assembled from single-quoted words by
/// [`command_line`], and a value carrying `;` or `$(` stays one inert argument.
const LAUNCH_SCRIPT: &str = r#"on run argv
set launch_command to item 1 of argv
set start_directory to item 2 of argv
if (count of argv) > 2 then
  set launch_environment to items 3 thru -1 of argv
else
  set launch_environment to {}
end if
set already_running to application id "com.mitchellh.ghostty" is running
tell application id "com.mitchellh.ghostty"
  set cfg to new surface configuration
  set command of cfg to launch_command
  if start_directory is not "" then
    set initial working directory of cfg to start_directory
  end if
  if (count of launch_environment) > 0 then
    set environment variables of cfg to launch_environment
  end if
  set wait after command of cfg to true
  if already_running and (count of windows) > 0 then
    new tab in front window with configuration cfg
  else
    new window with configuration cfg
  end if
  activate
end tell
end run"#;

/// One window's worth of instructions, in the order [`LAUNCH_SCRIPT`] reads them.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct GhosttySurface {
    /// The shell text Ghostty runs, already quoted word by word.
    pub(crate) command: String,
    /// Where the session starts, or empty for Ghostty's own default.
    pub(crate) working_directory: String,
    /// `KEY=VALUE` entries, exported by Ghostty rather than by a shell of ours.
    pub(crate) environment: Vec<String>,
}

impl GhosttySurface {
    pub(crate) fn new(command: &ResolvedCommand) -> Self {
        Self {
            command: command_line(command),
            working_directory: command
                .working_directory
                .as_ref()
                .map(|directory| directory.to_string_lossy().into_owned())
                .unwrap_or_default(),
            environment: command
                .env
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect(),
        }
    }

    fn arguments(&self) -> Vec<String> {
        let mut arguments = vec![self.command.clone(), self.working_directory.clone()];
        arguments.extend(self.environment.iter().cloned());
        arguments
    }
}

/// The command as one line of shell text, with every word quoted.
///
/// A raw shell launcher is shell text the user authored, and goes through as written; that is what
/// the launcher *is*.
fn command_line(command: &ResolvedCommand) -> String {
    match &command.payload {
        ResolvedPayload::Structured { executable, args } => {
            std::iter::once(quote(&executable.to_string_lossy()))
                .chain(args.iter().map(|argument| quote(argument)))
                .collect::<Vec<_>>()
                .join(" ")
        }
        ResolvedPayload::RawShell { command } => command.clone(),
    }
}

pub(crate) trait GhosttyInvoker: Send + Sync {
    fn invoke(&self, surface: &GhosttySurface) -> Result<OpenResult, PortError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SystemGhosttyInvoker;

impl GhosttyInvoker for SystemGhosttyInvoker {
    fn invoke(&self, surface: &GhosttySurface) -> Result<OpenResult, PortError> {
        let output = Command::new("/usr/bin/osascript")
            .arg("-e")
            .arg(LAUNCH_SCRIPT)
            .arg("--")
            .args(surface.arguments())
            .output()
            .map_err(|error| {
                PortError::new(
                    PortErrorKind::Unavailable,
                    format!("could not invoke Ghostty's scripting API: {error}"),
                )
            })?;
        Ok(OpenResult {
            success: output.status.success(),
            status_code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use crate::{ResolvedCommand, ResolvedPayload};

    use super::{GhosttySurface, LAUNCH_SCRIPT};

    fn command(payload: ResolvedPayload) -> ResolvedCommand {
        ResolvedCommand {
            env: BTreeMap::new(),
            title: "Server".to_owned(),
            working_directory: None,
            shell: PathBuf::from("/bin/zsh"),
            payload,
        }
    }

    /// A running Ghostty is given a tab in the window in front, like Cmd-T; only a Ghostty with no
    /// window to put it in — including one this launch starts — is given a window.
    #[test]
    fn a_running_ghostty_gets_a_tab_in_its_front_window() {
        assert!(LAUNCH_SCRIPT.contains("is running"));
        assert!(LAUNCH_SCRIPT.contains("new tab in front window with configuration cfg"));
        assert!(LAUNCH_SCRIPT.contains("new window with configuration cfg"));
    }

    #[test]
    fn launcher_values_are_arguments_not_applescript_source() {
        assert!(LAUNCH_SCRIPT.contains("item 1 of argv"));
        assert!(!LAUNCH_SCRIPT.contains("/tmp/"));
    }

    /// The prefix is a configuration-file feature. Sent through the scripting API it becomes part of
    /// the filename Ghostty tries to execute, which is how the launch failed with
    /// `exec -l direct:/var/folders/…: No such file or directory`.
    #[test]
    fn the_command_never_carries_a_configuration_only_prefix() {
        let surface = GhosttySurface::new(&command(ResolvedPayload::Structured {
            executable: PathBuf::from("/usr/bin/ssh"),
            args: vec!["deploy@example.com".to_owned()],
        }));

        assert_eq!(surface.command, "'/usr/bin/ssh' 'deploy@example.com'");
        assert!(!surface.command.contains("direct:"));
        assert!(!LAUNCH_SCRIPT.contains("direct:"));
    }

    /// Ghostty runs the command through `bash -c`, so a host or argument carrying shell syntax has
    /// to arrive as one inert word.
    #[test]
    fn every_word_of_a_structured_command_is_quoted() {
        let surface = GhosttySurface::new(&command(ResolvedPayload::Structured {
            executable: PathBuf::from("/resolved/codex"),
            args: vec!["a b; $(id); 'quoted'".to_owned()],
        }));

        assert_eq!(
            surface.command,
            "'/resolved/codex' 'a b; $(id); '\\''quoted'\\'''"
        );
    }

    #[test]
    fn a_raw_shell_launcher_is_the_shell_text_the_user_wrote() {
        let surface = GhosttySurface::new(&command(ResolvedPayload::RawShell {
            command: "cd /srv && ./run.sh".to_owned(),
        }));

        assert_eq!(surface.command, "cd /srv && ./run.sh");
    }

    /// Ghostty exports these itself, so no `env` process and no shell stands between the terminal
    /// and the program.
    #[test]
    fn environment_and_directory_travel_as_their_own_arguments() {
        let mut resolved = command(ResolvedPayload::Structured {
            executable: PathBuf::from("/usr/bin/ssh"),
            args: Vec::new(),
        });
        resolved.working_directory = Some(PathBuf::from("/tmp/work"));
        resolved
            .env
            .insert("TERM".to_owned(), "xterm-256color".to_owned());

        let surface = GhosttySurface::new(&resolved);

        assert_eq!(surface.working_directory, "/tmp/work");
        assert_eq!(surface.environment, ["TERM=xterm-256color"]);
        assert_eq!(
            surface.arguments(),
            [
                "'/usr/bin/ssh'".to_owned(),
                "/tmp/work".to_owned(),
                "TERM=xterm-256color".to_owned()
            ]
        );
    }

    /// A launcher with no directory of its own still has to hand the script an argument, or every
    /// following argument shifts one place and the first environment entry is read as a directory.
    #[test]
    fn a_surface_without_a_directory_still_fills_its_place() {
        let surface = GhosttySurface::new(&command(ResolvedPayload::Structured {
            executable: PathBuf::from("/usr/bin/ssh"),
            args: Vec::new(),
        }));

        assert_eq!(surface.arguments().len(), 2);
        assert_eq!(surface.arguments()[1], "");
    }

    #[test]
    fn the_window_outlives_the_command_so_a_failure_can_be_read() {
        assert!(LAUNCH_SCRIPT.contains("set wait after command of cfg to true"));
        assert!(LAUNCH_SCRIPT.contains("new window with configuration cfg"));
    }
}
