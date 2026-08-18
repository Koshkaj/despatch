use std::fmt::Write;

use crate::terminal_argv::MINIMUM_VISIBLE_SECONDS;
use crate::{ResolvedCommand, ResolvedPayload};

#[must_use]
pub(crate) fn render(command: &ResolvedCommand) -> String {
    // The script runs under the user's own shell, so the session a launcher opens is the one they
    // would have opened themselves.
    let mut script = format!("#!{}\n", command.shell.display());
    script.push_str("script_path=$0\nrm -f -- \"$script_path\"\n");
    writeln!(
        script,
        "printf '\\033]0;%s\\007' {}",
        quote(&sanitize_title(&command.title))
    )
    .expect("writing to a String cannot fail");
    // Exported one assignment at a time, each value quoted, so a value carrying `;` or `$(` is a
    // string and not another command. Names were validated as identifiers before they got here.
    for (name, value) in &command.env {
        writeln!(script, "export {name}={}", quote(value))
            .expect("writing to a String cannot fail");
    }
    // A launcher with no directory of its own starts at home, the same as the argv path: a staged
    // file is run by a terminal started through Launch Services, and inheriting that start leaves
    // the session somewhere nobody chose.
    let directory = command.working_directory.as_ref().map_or_else(
        || "\"${HOME:-/}\"".to_owned(),
        |directory| quote(&directory.to_string_lossy()),
    );
    writeln!(
        script,
        "cd -- {directory} || {{ status=$?; printf '\\nDespatch: could not enter working directory (status %s).\\n' \"$status\" >&2; exit \"$status\"; }}"
    )
    .expect("writing to a String cannot fail");
    script.push_str("started=$(date +%s)\n");
    match &command.payload {
        ResolvedPayload::Structured { executable, args } => {
            script.push_str(&quote(&executable.to_string_lossy()));
            for arg in args {
                script.push(' ');
                script.push_str(&quote(arg));
            }
            script.push('\n');
        }
        ResolvedPayload::RawShell { command } => {
            script.push_str(command);
            if !command.ends_with('\n') {
                script.push('\n');
            }
        }
    }
    script.push_str("status=$?\nelapsed=$(($(date +%s) - started))\n");
    // A command that failed, or that ended before the user could have used it, hands over to an
    // interactive shell so the window stays with the reason above a usable prompt. Anything else
    // just ends, and the terminal closes it. See `terminal_argv::MINIMUM_VISIBLE_SECONDS` for why
    // the clock is part of the rule.
    writeln!(
        script,
        "if [ \"$status\" -ne 0 ] || [ \"$elapsed\" -lt {MINIMUM_VISIBLE_SECONDS} ]; then printf '\\nDespatch: command exited with status %s after %ss.\\n' \"$status\" \"$elapsed\" >&2; exec \"${{SHELL:-/bin/sh}}\" -l; fi"
    )
    .expect("writing to a String cannot fail");
    script.push_str("exit 0\n");
    script
}

pub(crate) fn quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    for character in value.chars() {
        if character == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(character);
        }
    }
    quoted.push('\'');
    quoted
}

fn sanitize_title(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::process::Command;

    use std::path::PathBuf;

    use tempfile::tempdir;

    use crate::{ResolvedCommand, ResolvedPayload};

    use super::{quote, render};

    #[test]
    fn quote_keeps_shell_metacharacters_in_one_literal_argument() {
        assert_eq!(quote(""), "''");
        assert_eq!(quote("a b'$;`$(id)"), "'a b'\\''$;`$(id)'");
    }

    #[test]
    fn structured_scripts_never_emit_eval_or_shell_c() {
        let script = render(&ResolvedCommand {
            env: BTreeMap::new(),
            title: "Injection test".to_owned(),
            working_directory: Some(PathBuf::from("/tmp/project with spaces")),
            shell: PathBuf::from("/bin/sh"),
            payload: ResolvedPayload::Structured {
                executable: PathBuf::from("/usr/bin/printf"),
                args: vec!["$(touch /tmp/pwned); `id`; $HOME; a'b".to_owned()],
            },
        });

        assert!(!script.contains("eval"));
        assert!(!script.contains("sh -c"));
        assert!(script.contains("'$(touch /tmp/pwned); `id`; $HOME; a'\\''b'"));
        assert!(script.contains("rm -f -- \"$script_path\""));
        assert!(script.contains("command exited with status"));
    }

    #[test]
    fn environment_entries_are_exported_as_quoted_assignments() {
        let mut env = BTreeMap::new();
        env.insert("API_TOKEN".to_owned(), "a b; $(id) `whoami`".to_owned());
        env.insert("PATH_EXTRA".to_owned(), "/opt/bin".to_owned());
        let script = render(&ResolvedCommand {
            env,
            title: "Env".to_owned(),
            working_directory: None,
            shell: PathBuf::from("/bin/sh"),
            payload: ResolvedPayload::Structured {
                executable: PathBuf::from("/usr/bin/printf"),
                args: Vec::new(),
            },
        });

        assert!(
            script.contains("export API_TOKEN='a b; $(id) `whoami`'"),
            "{script}"
        );
        assert!(script.contains("export PATH_EXTRA='/opt/bin'"));
        // Sorted, so the same launcher always renders the same script.
        let token = script.find("API_TOKEN").expect("token export");
        let extra = script.find("PATH_EXTRA").expect("path export");
        assert!(token < extra);
    }

    #[test]
    fn a_failed_command_hands_over_to_a_shell_instead_of_exiting() {
        let script = render(&ResolvedCommand {
            env: BTreeMap::new(),
            title: "Close".to_owned(),
            working_directory: None,
            shell: PathBuf::from("/bin/sh"),
            payload: ResolvedPayload::Structured {
                executable: PathBuf::from("/usr/bin/true"),
                args: Vec::new(),
            },
        });

        assert!(script.contains("command exited with status"));
        assert!(
            script.contains("exec \"${SHELL:-/bin/sh}\" -l"),
            "a failure keeps the window: {script}"
        );
        assert!(script.trim_end().ends_with("exit 0"));
    }

    /// The staged path has to agree with the argv path: a launcher with no directory of its own
    /// starts at home rather than wherever Launch Services started the terminal.
    #[test]
    fn a_script_without_a_working_directory_starts_at_home() {
        let script = render(&ResolvedCommand {
            env: BTreeMap::new(),
            title: "Home".to_owned(),
            working_directory: None,
            shell: PathBuf::from("/bin/sh"),
            payload: ResolvedPayload::Structured {
                executable: PathBuf::from("/bin/pwd"),
                args: Vec::new(),
            },
        });

        assert!(script.contains(r#"cd -- "${HOME:-/}""#), "{script}");
    }

    #[test]
    fn raw_shell_command_is_inserted_verbatim() {
        let authored = "printf '%s\\n' raw && echo done";
        let script = render(&ResolvedCommand {
            env: BTreeMap::new(),
            title: "Raw".to_owned(),
            working_directory: None,
            shell: PathBuf::from("/bin/sh"),
            payload: ResolvedPayload::RawShell {
                command: authored.to_owned(),
            },
        });

        assert!(script.contains(authored));
    }

    #[test]
    fn injection_corpus_executes_as_inert_single_arguments() {
        let directory = tempdir().expect("temporary directory");
        let sentinel = directory.path().join("must-not-exist");
        let script_path = directory.path().join("injection.command");
        let dangerous = format!(
            "spaces 'quotes' $HOME; `touch {0}`; $(touch {0}); \\$(touch {0})",
            sentinel.display()
        );
        let script = render(&ResolvedCommand {
            env: BTreeMap::new(),
            title: "Injection".to_owned(),
            working_directory: None,
            shell: PathBuf::from("/bin/sh"),
            payload: ResolvedPayload::Structured {
                executable: PathBuf::from("/usr/bin/printf"),
                args: vec!["%s".to_owned(), dangerous.clone()],
            },
        });
        fs::write(&script_path, script).expect("write test script");

        let output = Command::new("/bin/sh")
            .arg(&script_path)
            .output()
            .expect("execute test script");

        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).ends_with(&dangerous));
        assert!(!sentinel.exists());
        assert!(!script_path.exists());
    }
}
