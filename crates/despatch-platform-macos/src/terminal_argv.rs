use crate::{ResolvedCommand, ResolvedPayload};

/// The wrapper every argv launch runs under.
///
/// It is a **constant**: no launcher value is ever concatenated into it. Everything the user
/// configured arrives as positional parameters, so a host, an argument, or an environment value
/// carrying `;` or `$(` is data to this script and can never become another command.
///
/// `$1` is the working directory; everything after it is the command to run.
///
/// A launcher without one sends [`NO_DIRECTORY`] rather than an empty string, because the argument
/// vector does not survive an empty one: `/usr/bin/open --args` drops it, every later parameter
/// shifts down by one, and the wrapper then tries to `cd` into the program it was supposed to run.
/// The launch fails in a way that looks nothing like its cause.
///
/// A command that fails, or that ends before the user could have used it, hands over to an
/// interactive login shell rather than exiting: the window stays, holding the reason above a usable
/// prompt. Otherwise the wrapper simply ends and the terminal closes the window it owned.
///
/// The elapsed-time rule is what makes a launch debuggable. A terminal closes its window the moment
/// the command exits, so a session that dies on the spot — a refused connection, a host that hangs
/// up, a shell that is not there — reads as "the terminal opened and vanished", with the message
/// that would have explained it destroyed along with the window. A session the user actually used
/// and then ended themselves takes longer than [`MINIMUM_VISIBLE_SECONDS`], and closes as it
/// should.
pub(crate) const MINIMUM_VISIBLE_SECONDS: u32 = 2;

/// What a launcher with no working directory of its own sends, which the wrapper reads as "the
/// user's home directory".
///
/// It cannot be `.`, and the session cannot simply be left where it started. A terminal opened
/// through `/usr/bin/open` is started by launchd, whose working directory is `/` — so a session
/// that inherits its directory opens at the root of the disk, which is a place no one asked for and
/// nothing useful is relative to. Home is where a terminal opens when a person opens one.
pub(crate) const NO_DIRECTORY: &str = "-";

pub(crate) const WRAPPER: &str = r#"dir=$1
shift
if [ "$dir" = "-" ] || [ -z "$dir" ]; then
  dir=${HOME:-/}
fi
cd -- "$dir" || {
  status=$?
  printf '\nDespatch: could not enter working directory (status %s).\n' "$status" >&2
  exec "${SHELL:-/bin/sh}" -l
}
started=$(date +%s)
"$@"
status=$?
elapsed=$(($(date +%s) - started))
if [ "$status" -ne 0 ] || [ "$elapsed" -lt 2 ]; then
  printf '\nDespatch: command exited with status %s after %ss.\n' "$status" "$elapsed" >&2
  exec "${SHELL:-/bin/sh}" -l
fi
"#;

/// Builds the argument vector handed to a terminal that accepts `-e`.
///
/// A terminal that can start where it is told, and that holds a window open when a command fails,
/// is handed the command itself: `[program, args...]`, with `/usr/bin/env` in front when the
/// launcher carries an environment. One process runs, and it is the one the user asked for — the
/// terminal names it honestly, and its "allow this app to run X" prompt names the right X.
///
/// A terminal that can do neither is handed [`WRAPPER`] instead:
/// `[<shell>, -c, WRAPPER, despatch, <dir>, <command...>]`. The wrapper is a constant and every
/// launcher value stays a positional parameter, so a host or argument carrying `;` or `$(` is data.
/// It costs a shell process sitting between the terminal and the command, which is why it is only
/// used where the terminal leaves something undone.
#[must_use]
pub(crate) fn render(command: &ResolvedCommand, wrapped: bool) -> Vec<String> {
    let payload = payload_argv(command);
    if !wrapped {
        return payload;
    }
    let mut argv = vec![
        command.shell.to_string_lossy().into_owned(),
        "-c".to_owned(),
        WRAPPER.to_owned(),
        // argv[0] for the wrapper, so a shell error names this app rather than `sh`.
        "despatch".to_owned(),
        command.working_directory.as_ref().map_or_else(
            || NO_DIRECTORY.to_owned(),
            |path| path.to_string_lossy().into_owned(),
        ),
    ];
    argv.extend(payload);
    argv
}

/// The command itself, environment included, as one argument vector.
fn payload_argv(command: &ResolvedCommand) -> Vec<String> {
    let shell = command.shell.to_string_lossy().into_owned();
    let mut argv = Vec::new();
    // `env` carries the variables as arguments, so a value is never parsed as shell syntax.
    if !command.env.is_empty() {
        argv.push("/usr/bin/env".to_owned());
        for (name, value) in &command.env {
            argv.push(format!("{name}={value}"));
        }
    }
    match &command.payload {
        ResolvedPayload::Structured { executable, args } => {
            argv.push(executable.to_string_lossy().into_owned());
            argv.extend(args.iter().cloned());
        }
        // Raw shell is the explicitly trusted mode, and is the only place a command is text.
        ResolvedPayload::RawShell { command } => {
            argv.push(shell);
            argv.push("-c".to_owned());
            argv.push(command.clone());
        }
    }
    argv
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::process::Command;

    use crate::{ResolvedCommand, ResolvedPayload};

    use super::{MINIMUM_VISIBLE_SECONDS, NO_DIRECTORY, WRAPPER, render};

    fn structured(executable: &str, args: &[&str]) -> ResolvedCommand {
        ResolvedCommand {
            title: "Test".to_owned(),
            working_directory: None,
            env: BTreeMap::new(),
            shell: PathBuf::from("/bin/sh"),
            payload: ResolvedPayload::Structured {
                executable: PathBuf::from(executable),
                args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            },
        }
    }

    /// The session runs under the shell the user chose, not a hardcoded one: it is what a terminal
    /// they opened themselves would have given them.
    #[test]
    fn the_wrapper_runs_under_the_configured_shell() {
        let mut command = structured("/usr/bin/true", &[]);
        command.shell = PathBuf::from("/bin/zsh");

        assert_eq!(render(&command, true)[0], "/bin/zsh");
    }

    #[test]
    fn a_plain_command_still_runs_under_the_wrapper() {
        let argv = render(
            &structured("/usr/bin/ssh", &["-p", "2222", "deploy@example.com"]),
            true,
        );

        assert_eq!(argv[0], "/bin/sh");
        assert_eq!(argv[2], WRAPPER);
        assert_eq!(
            &argv[5..],
            ["/usr/bin/ssh", "-p", "2222", "deploy@example.com"],
            "the command itself is untouched"
        );
        assert!(
            !argv.iter().any(|part| part.ends_with(".command")),
            "nothing is staged on disk"
        );
    }

    /// `/usr/bin/open --args` drops an empty argument, and every parameter after it shifts down
    /// one. The wrapper would then read the program as its working directory, `cd` into it, fail,
    /// and drop the user into a shell that never ran their launcher.
    #[test]
    fn no_argument_is_ever_empty() {
        let plain = render(&structured("/usr/bin/ssh", &["deploy@example.com"]), true);
        assert_eq!(plain[4], NO_DIRECTORY, "the placeholder stands in for none");
        for argv in [
            plain,
            render(&structured("/usr/bin/true", &[]), true),
            render(
                &{
                    let mut command = structured("/usr/bin/true", &[]);
                    command.working_directory = Some(PathBuf::from("/tmp"));
                    command
                },
                true,
            ),
        ] {
            assert!(argv.iter().all(|argument| !argument.is_empty()), "{argv:?}");
        }
    }

    /// A launcher with no directory of its own starts at home. The terminal is started by launchd,
    /// whose working directory is `/`, so a session that inherits one opens at the root of the disk.
    #[test]
    fn a_launcher_without_a_directory_starts_at_home() {
        let argv = render(&structured("/bin/pwd", &[]), true);

        let output = Command::new(&argv[0])
            .args(&argv[1..])
            .current_dir("/")
            .env("HOME", "/usr/share")
            .env("SHELL", "/usr/bin/true")
            .output()
            .expect("run the wrapper");

        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim_end(),
            "/usr/share",
            "the session opens where a person's terminal would, not where launchd left it"
        );
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains("working directory"),
            "{:?}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn a_launcher_with_a_directory_starts_in_it() {
        let mut command = structured("/bin/pwd", &[]);
        command.working_directory = Some(PathBuf::from("/usr/lib"));
        let argv = render(&command, true);

        let output = Command::new(&argv[0])
            .args(&argv[1..])
            .current_dir("/")
            .env("HOME", "/usr/share")
            .env("SHELL", "/usr/bin/true")
            .output()
            .expect("run the wrapper");

        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim_end(),
            "/usr/lib"
        );
    }

    /// The wrapper's own clock and the constant that documents it must not drift apart.
    #[test]
    fn the_wrapper_holds_the_window_for_the_documented_time() {
        assert!(
            WRAPPER.contains(&format!("-lt {MINIMUM_VISIBLE_SECONDS}")),
            "{WRAPPER}"
        );
    }

    #[test]
    fn a_command_that_ends_at_once_keeps_the_window_even_though_it_succeeded() {
        let argv = render(&structured("/usr/bin/true", &[]), true);

        let output = Command::new(&argv[0])
            .args(&argv[1..])
            .env("SHELL", "/usr/bin/true")
            .output()
            .expect("run the wrapper");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("exited with status 0 after 0s"),
            "a session nobody could have used says so rather than vanishing: {stderr}"
        );
    }

    #[test]
    fn a_command_the_user_actually_used_lets_the_window_close() {
        let argv = render(&structured("/bin/sleep", &["3"]), true);

        let output = Command::new(&argv[0])
            .args(&argv[1..])
            .env("SHELL", "/usr/bin/true")
            .output()
            .expect("run the wrapper");

        assert!(
            String::from_utf8_lossy(&output.stderr).is_empty(),
            "nothing is said about a session that ran its course"
        );
    }

    #[test]
    fn environment_entries_ride_along_as_arguments_to_env() {
        let mut command = structured("/usr/bin/true", &[]);
        command
            .env
            .insert("B_TOKEN".to_owned(), "b; $(id)".to_owned());
        command.env.insert("A_MODEL".to_owned(), "opus".to_owned());

        let argv = render(&command, true);

        assert_eq!(
            &argv[5..],
            [
                "/usr/bin/env",
                "A_MODEL=opus",
                "B_TOKEN=b; $(id)",
                "/usr/bin/true"
            ],
            "sorted, and the value stays one argument"
        );
    }

    #[test]
    fn a_working_directory_reaches_the_wrapper_as_its_own_parameter() {
        let mut command = structured("/usr/bin/true", &[]);
        command.working_directory = Some(PathBuf::from("/tmp/project with spaces"));

        let argv = render(&command, true);

        assert_eq!(argv[0], "/bin/sh");
        assert_eq!(argv[1], "-c");
        assert_eq!(argv[2], WRAPPER);
        assert_eq!(argv[3], "despatch");
        assert_eq!(argv[4], "/tmp/project with spaces");
        assert_eq!(&argv[5..], ["/usr/bin/true"]);
    }

    #[test]
    fn an_argument_full_of_shell_syntax_runs_as_one_inert_string() {
        let dangerous = "spaces 'quotes' $HOME; `id`; $(id)";
        let argv = render(&structured("/bin/echo", &[dangerous]), true);

        let output = Command::new(&argv[0])
            .args(&argv[1..])
            .output()
            .expect("run the wrapper");

        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim_end(),
            dangerous,
            "the argument reached the program untouched"
        );
    }

    #[test]
    fn a_failing_command_reports_its_status_and_keeps_a_shell() {
        let mut command = structured("/usr/bin/false", &[]);
        command.working_directory = Some(PathBuf::from("/"));
        let argv = render(&command, true);

        // `SHELL` is pointed at a command that exits immediately, standing in for the interactive
        // shell a real terminal would hold open.
        let output = Command::new(&argv[0])
            .args(&argv[1..])
            .env("SHELL", "/usr/bin/true")
            .output()
            .expect("run the wrapper");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("exited with status 1"), "{stderr}");
        assert!(
            output.status.success(),
            "the wrapper hands over to a shell instead of failing out"
        );
    }
}
