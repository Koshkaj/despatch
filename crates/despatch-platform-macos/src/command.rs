use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, OnceLock};

use despatch_application::{LaunchRequest, PortError, PortErrorKind};
use despatch_domain::{ExpansionContext, PathExpansionError, PathTemplate, SshPort};
use thiserror::Error;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EnvironmentVariables(BTreeMap<String, String>);

impl EnvironmentVariables {
    #[must_use]
    pub fn system() -> Self {
        Self(env::vars().collect())
    }

    #[must_use]
    pub fn with(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.0.insert(name.into(), value.into());
        self
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    fn expansion_context(&self, base_directory: &Path) -> ExpansionContext {
        self.0.iter().fold(
            ExpansionContext::new(base_directory),
            |context, (name, value)| context.with_variable(name, value),
        )
    }
}

/// The terminal type an SSH session announces to the host it connects to.
///
/// `ssh` copies the local `TERM` into the remote session, and the remote then looks it up in its own
/// terminfo database. Modern terminals name themselves — `xterm-ghostty`, `xterm-kitty`,
/// `alacritty` — and a server has none of those, so the remote shell falls back to a terminal that
/// can do nothing: `clear`, `Ctrl-L`, and every other capability stop working. Terminals paper over
/// this with shell integration that only loads in an interactive shell, which a launched command is
/// not.
///
/// So a launched session announces a type every host has known for decades. It costs nothing a
/// remote shell would miss: 256 colours are in it, and true colour rides on `COLORTERM`.
const PORTABLE_TERM: &str = "xterm-256color";

pub trait ExecutableResolver: Send + Sync {
    fn resolve(
        &self,
        requested: &str,
        config_directory: &Path,
        environment: &EnvironmentVariables,
    ) -> Result<PathBuf, LaunchBuildError>;

    /// The `PATH` a session should run with, when it differs from this process's own.
    ///
    /// Despatch is started by Launch Services, which gives it `/usr/bin:/bin:/usr/sbin:/sbin` and
    /// nothing else — none of the version managers, package managers, or `~/.local/bin` entries the
    /// user's shell sets up. A launched program inherits that same short list, and a program is not
    /// only the file that was resolved: `codex` and `gemini` are `#!/usr/bin/env node` scripts, so
    /// the launch dies at the shebang with `env: node: No such file or directory` even though the
    /// script itself was found. `claude` and `opencode` are compiled binaries and survive, which is
    /// exactly why some agents worked and some did not.
    ///
    /// The default is `None`: a resolver that does not consult a shell has nothing to add.
    fn session_path(&self, _environment: &EnvironmentVariables) -> Option<String> {
        None
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemExecutableResolver;

impl despatch_application::ExecutableLocator for SystemExecutableResolver {
    fn locate(&self, executable: &str) -> Result<PathBuf, despatch_application::PortError> {
        // Checked against the same environment a launch would use, so the editor cannot accept a
        // name that the launcher would then fail to find.
        self.resolve(executable, Path::new("/"), &EnvironmentVariables::system())
            .map_err(|error| {
                despatch_application::PortError::new(
                    despatch_application::PortErrorKind::NotFound,
                    error.to_string(),
                )
            })
    }
}

impl ExecutableResolver for SystemExecutableResolver {
    fn resolve(
        &self,
        requested: &str,
        config_directory: &Path,
        environment: &EnvironmentVariables,
    ) -> Result<PathBuf, LaunchBuildError> {
        if requested.contains('/') || requested.contains('$') {
            let template = PathTemplate::new(requested).map_err(|violation| {
                LaunchBuildError::InvalidExecutable {
                    requested: requested.to_owned(),
                    reason: violation.to_string(),
                }
            })?;
            let path = template
                .expand(&environment.expansion_context(config_directory))
                .map_err(LaunchBuildError::PathExpansion)?;
            return verify_executable(path, requested);
        }

        if let Some(path) = environment
            .get("PATH")
            .and_then(|path| find_on_path(requested, path))
        {
            return Ok(path);
        }

        resolve_with_login_shell(requested, environment)
    }

    /// Asked once per run: a login shell costs a fork and whatever the user's profile does, and the
    /// answer cannot change while the app is running.
    fn session_path(&self, environment: &EnvironmentVariables) -> Option<String> {
        static SESSION_PATH: OnceLock<Option<String>> = OnceLock::new();
        SESSION_PATH
            .get_or_init(|| login_shell_path(environment))
            .clone()
    }
}

/// The `PATH` the user's own login shell would hand a program.
///
/// The **interactive** login shell is asked first, and that ordering is the whole point: version
/// managers install themselves in `~/.zshrc`, which a non-interactive `zsh -l` never reads. Asked
/// the quiet way, this machine answers without `~/.nvm/versions/node/v20.18.1/bin` — and that is
/// the directory `node` lives in, so `codex` and `gemini` still die at their shebang. The
/// non-interactive shell is the fallback for a profile that refuses to run interactively at all.
fn login_shell_path(environment: &EnvironmentVariables) -> Option<String> {
    let shell = environment.get("SHELL").unwrap_or("/bin/zsh");
    for flags in [["-il", "-c"], ["-l", "-c"]] {
        let output = Command::new(shell)
            .args(flags)
            .args([r#"printf '%s\n' "$PATH""#])
            .output()
            .ok()?;
        if !output.status.success() {
            continue;
        }
        // An interactive shell prints its own greetings first, so the answer is the last line that
        // looks like a search path rather than everything the shell wrote.
        if let Some(path) = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .rfind(|line| line.starts_with('/'))
            .map(ToOwned::to_owned)
        {
            return Some(path);
        }
    }
    None
}

#[derive(Clone)]
pub struct RequestResolver {
    executable_resolver: Arc<dyn ExecutableResolver>,
    environment: EnvironmentVariables,
}

impl Default for RequestResolver {
    fn default() -> Self {
        Self {
            executable_resolver: Arc::new(SystemExecutableResolver),
            environment: EnvironmentVariables::system(),
        }
    }
}

impl RequestResolver {
    #[must_use]
    pub fn new(
        executable_resolver: Arc<dyn ExecutableResolver>,
        environment: EnvironmentVariables,
    ) -> Self {
        Self {
            executable_resolver,
            environment,
        }
    }

    /// The shell a launched session should run under.
    ///
    /// `SHELL` is what the user chose for themselves, and it is what a terminal they opened by hand
    /// would give them. A shell that cannot read the wrapper is no use for running it, though, so
    /// anything outside the POSIX family falls back to `/bin/sh` — the wrapper is a POSIX script,
    /// and `fish`, for one, would fail on its very first line. The session it hands over to is the
    /// user's shell either way.
    fn shell(&self) -> PathBuf {
        const POSIX_SHELLS: [&str; 5] = ["sh", "bash", "zsh", "dash", "ksh"];
        let shell = self.environment.get("SHELL").unwrap_or_default();
        let is_posix = Path::new(shell)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| POSIX_SHELLS.contains(&name));
        if is_posix {
            PathBuf::from(shell)
        } else {
            PathBuf::from("/bin/sh")
        }
    }

    /// A launcher's own variables, with the shell's `PATH` underneath them.
    ///
    /// Only a local program needs this; an SSH session's environment belongs to the remote host.
    /// A launcher that sets `PATH` itself keeps it — the user said what they wanted.
    fn session_environment(&self, mut env: BTreeMap<String, String>) -> BTreeMap<String, String> {
        if !env.contains_key("PATH")
            && let Some(path) = self.executable_resolver.session_path(&self.environment)
        {
            env.insert("PATH".to_owned(), path);
        }
        env
    }

    pub fn resolve(&self, request: &LaunchRequest) -> Result<ResolvedCommand, LaunchBuildError> {
        match request {
            LaunchRequest::Ssh(request) => {
                let mut args = Vec::new();
                if let Some(identity_file) = &request.identity_file {
                    args.push("-i".to_owned());
                    args.push(self.expand_path(identity_file, &request.config_directory)?);
                }
                if request.port != SshPort::DEFAULT {
                    args.push("-p".to_owned());
                    args.push(request.port.get().to_string());
                }
                args.extend(request.extra_args.clone());
                args.push(format!("{}@{}", request.user, request.host));
                Ok(ResolvedCommand {
                    title: request.title.clone(),
                    working_directory: None,
                    // An SSH session's environment is the remote host's, not this machine's — with
                    // one exception, which is the terminal type. See [`PORTABLE_TERM`].
                    env: std::iter::once(("TERM".to_owned(), PORTABLE_TERM.to_owned())).collect(),
                    shell: self.shell(),
                    payload: ResolvedPayload::Structured {
                        executable: PathBuf::from("/usr/bin/ssh"),
                        args,
                    },
                })
            }
            LaunchRequest::Command(request) => {
                let executable = self.executable_resolver.resolve(
                    &request.executable,
                    &request.config_directory,
                    &self.environment,
                )?;
                let working_directory = request
                    .working_directory
                    .as_ref()
                    .map(|path| self.expand_path_buf(path, &request.config_directory))
                    .transpose()?;
                Ok(ResolvedCommand {
                    title: request.title.clone(),
                    working_directory,
                    env: self.session_environment(request.env.clone()),
                    shell: self.shell(),
                    payload: ResolvedPayload::Structured {
                        executable,
                        args: request.args.clone(),
                    },
                })
            }
            LaunchRequest::RawShell(request) => {
                let working_directory = request
                    .working_directory
                    .as_ref()
                    .map(|path| self.expand_path_buf(path, &request.config_directory))
                    .transpose()?;
                Ok(ResolvedCommand {
                    title: request.title.clone(),
                    working_directory,
                    env: self.session_environment(request.env.clone()),
                    shell: self.shell(),
                    payload: ResolvedPayload::RawShell {
                        command: request.command.clone(),
                    },
                })
            }
        }
    }

    fn expand_path(
        &self,
        template: &PathTemplate,
        config_directory: &Path,
    ) -> Result<String, LaunchBuildError> {
        let path = self.expand_path_buf(template, config_directory)?;
        path.into_os_string()
            .into_string()
            .map_err(|_| LaunchBuildError::NonUnicodePath)
    }

    fn expand_path_buf(
        &self,
        template: &PathTemplate,
        config_directory: &Path,
    ) -> Result<PathBuf, LaunchBuildError> {
        template
            .expand(&self.environment.expansion_context(config_directory))
            .map_err(LaunchBuildError::PathExpansion)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedCommand {
    pub title: String,
    pub working_directory: Option<PathBuf>,
    /// Exported before the command runs, in sorted order so the script is reproducible.
    pub env: std::collections::BTreeMap<String, String>,
    /// The shell the session runs under: the user's own, from `SHELL`, falling back to `/bin/sh`.
    ///
    /// It is resolved here rather than left to the script, because the shell is named in an
    /// argument vector, and an argument vector is never expanded by anything.
    pub shell: PathBuf,
    pub payload: ResolvedPayload,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolvedPayload {
    Structured {
        executable: PathBuf,
        args: Vec<String>,
    },
    RawShell {
        command: String,
    },
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum LaunchBuildError {
    #[error("portable path could not be expanded: {0}")]
    PathExpansion(PathExpansionError),
    #[error("expanded path is not valid Unicode")]
    NonUnicodePath,
    #[error("executable '{requested}' is invalid: {reason}")]
    InvalidExecutable { requested: String, reason: String },
    #[error("executable '{0}' could not be found in GUI or login-shell PATH")]
    MissingExecutable(String),
    #[error("login shell could not be queried for executable '{requested}': {reason}")]
    LoginShell { requested: String, reason: String },
}

impl From<LaunchBuildError> for PortError {
    fn from(error: LaunchBuildError) -> Self {
        let kind = match &error {
            LaunchBuildError::MissingExecutable(_) => PortErrorKind::NotFound,
            LaunchBuildError::LoginShell { .. } => PortErrorKind::Unavailable,
            LaunchBuildError::PathExpansion(_)
            | LaunchBuildError::NonUnicodePath
            | LaunchBuildError::InvalidExecutable { .. } => PortErrorKind::InvalidData,
        };
        Self::new(kind, error.to_string())
    }
}

fn find_on_path(requested: &str, path: &str) -> Option<PathBuf> {
    env::split_paths(path)
        .map(|directory| directory.join(requested))
        .find(|candidate| is_executable(candidate))
}

/// Asks the user's shell where a program lives, the way the user's own terminal would answer.
///
/// A login shell is asked first, and an *interactive* login shell second. Tool installers write
/// their `PATH` line into `.zshrc`, which a non-interactive login shell never reads: `claude`,
/// `codex` and anything installed under a node version manager are invisible to `zsh -l` yet
/// present in every terminal the user opens. Asking twice is what makes "it works in my terminal"
/// and "Despatch can find it" the same statement.
fn resolve_with_login_shell(
    requested: &str,
    environment: &EnvironmentVariables,
) -> Result<PathBuf, LaunchBuildError> {
    let shell = environment.get("SHELL").unwrap_or("/bin/zsh");
    let mut failure = None;
    for flags in [["-l", "-c"], ["-il", "-c"]] {
        match ask_shell(shell, flags, requested) {
            Ok(Some(path)) => return verify_executable(path, requested),
            Ok(None) => {}
            Err(error) => failure = Some(error),
        }
    }
    Err(failure.unwrap_or_else(|| LaunchBuildError::MissingExecutable(requested.to_owned())))
}

/// Runs one `command -v` in the given shell. `Ok(None)` means that shell simply did not find it.
fn ask_shell(
    shell: &str,
    flags: [&str; 2],
    requested: &str,
) -> Result<Option<PathBuf>, LaunchBuildError> {
    let output = Command::new(shell)
        .args(flags)
        .args(["command -v -- \"$1\"", "despatch-resolve", requested])
        .output()
        .map_err(|error| LaunchBuildError::LoginShell {
            requested: requested.to_owned(),
            reason: error.to_string(),
        })?;
    if !output.status.success() {
        return Ok(None);
    }
    let resolved =
        String::from_utf8(output.stdout).map_err(|error| LaunchBuildError::LoginShell {
            requested: requested.to_owned(),
            reason: error.to_string(),
        })?;
    // An interactive shell prints its own greetings and prompts, so the answer is the last
    // absolute path it wrote rather than everything it wrote.
    Ok(resolved
        .lines()
        .map(str::trim)
        .rfind(|line| line.starts_with('/'))
        .map(PathBuf::from))
}

fn verify_executable(path: PathBuf, requested: &str) -> Result<PathBuf, LaunchBuildError> {
    if is_executable(&path) {
        Ok(path)
    } else {
        Err(LaunchBuildError::MissingExecutable(requested.to_owned()))
    }
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use despatch_application::{
        CommandLaunchRequest, LaunchRequest, RawShellLaunchRequest, SshLaunchRequest,
    };
    use despatch_domain::{PathTemplate, SshPort};

    use super::{
        EnvironmentVariables, ExecutableResolver, LaunchBuildError, RequestResolver,
        ResolvedPayload, SystemExecutableResolver,
    };
    use tempfile::tempdir;

    struct FakeExecutableResolver;

    impl ExecutableResolver for FakeExecutableResolver {
        fn resolve(
            &self,
            requested: &str,
            _config_directory: &Path,
            _environment: &EnvironmentVariables,
        ) -> Result<PathBuf, LaunchBuildError> {
            Ok(PathBuf::from("/resolved").join(requested))
        }
    }

    /// A resolver that knows where the user's shell would look, standing in for a login shell.
    struct ShellPathResolver;

    impl ExecutableResolver for ShellPathResolver {
        fn resolve(
            &self,
            requested: &str,
            _config_directory: &Path,
            _environment: &EnvironmentVariables,
        ) -> Result<PathBuf, LaunchBuildError> {
            Ok(PathBuf::from("/Users/alice/.nvm/bin").join(requested))
        }

        fn session_path(&self, _environment: &EnvironmentVariables) -> Option<String> {
            Some("/Users/alice/.nvm/bin:/usr/bin:/bin".to_owned())
        }
    }

    fn shell_path_resolver() -> RequestResolver {
        RequestResolver::new(Arc::new(ShellPathResolver), EnvironmentVariables::default())
    }

    fn command_request(env: BTreeMap<String, String>) -> LaunchRequest {
        LaunchRequest::Command(CommandLaunchRequest {
            env,
            title: "Codex".to_owned(),
            executable: "codex".to_owned(),
            working_directory: None,
            args: Vec::new(),
            config_directory: PathBuf::from("/shared/config"),
        })
    }

    /// Finding the file is not enough. `codex` and `gemini` are `#!/usr/bin/env node` scripts, and
    /// Launch Services hands Despatch a `PATH` of `/usr/bin:/bin:/usr/sbin:/sbin`, so without the
    /// shell's own `PATH` the launch dies at the shebang with `env: node: No such file or
    /// directory` — while a compiled `claude` or `opencode` in the same list works.
    #[test]
    fn a_local_command_carries_the_shells_path_so_a_shebang_can_be_read() {
        let resolved = shell_path_resolver()
            .resolve(&command_request(BTreeMap::new()))
            .expect("resolve the command");

        assert_eq!(
            resolved.env.get("PATH").map(String::as_str),
            Some("/Users/alice/.nvm/bin:/usr/bin:/bin")
        );
    }

    #[test]
    fn a_launchers_own_path_is_never_overwritten() {
        let mut env = BTreeMap::new();
        env.insert("PATH".to_owned(), "/only/this".to_owned());

        let resolved = shell_path_resolver()
            .resolve(&command_request(env))
            .expect("resolve the command");

        assert_eq!(
            resolved.env.get("PATH").map(String::as_str),
            Some("/only/this")
        );
    }

    /// An SSH session's environment is the remote host's. Sending this machine's `PATH` would
    /// point a remote shell at directories that do not exist there.
    #[test]
    fn an_ssh_session_is_not_given_this_machines_path() {
        let resolved = shell_path_resolver()
            .resolve(&LaunchRequest::Ssh(SshLaunchRequest {
                title: "Server".to_owned(),
                host: "example.com".to_owned(),
                user: "deploy".to_owned(),
                port: SshPort::DEFAULT,
                identity_file: None,
                extra_args: Vec::new(),
                config_directory: PathBuf::from("/shared/config"),
            }))
            .expect("resolve the SSH request");

        assert!(!resolved.env.contains_key("PATH"), "{:?}", resolved.env);
        assert_eq!(
            resolved.env.get("TERM").map(String::as_str),
            Some("xterm-256color")
        );
    }

    /// The real resolver answers with a real shell's `PATH`, and that answer has to be usable: a
    /// list of absolute directories, including one the login shell adds beyond Launch Services'.
    #[test]
    fn the_system_resolver_reports_a_usable_search_path() {
        let Some(path) = SystemExecutableResolver.session_path(&EnvironmentVariables::system())
        else {
            return; // No login shell on this machine; nothing to assert.
        };

        assert!(path.starts_with('/'), "{path}");
        assert!(!path.contains('\n'), "one line only: {path}");
        assert!(
            path.split(':').all(|entry| entry.starts_with('/')),
            "every entry is absolute: {path}"
        );
    }

    /// A launch runs under the user's own shell. Anything that cannot read the POSIX wrapper it is
    /// asked to run — `fish` is the common one — falls back to `/bin/sh`.
    #[test]
    fn the_session_runs_under_the_users_shell_when_it_can_read_the_wrapper() {
        let with_shell = |shell: &str| {
            RequestResolver::new(
                Arc::new(FakeExecutableResolver),
                EnvironmentVariables::default().with("SHELL", shell),
            )
            .shell()
        };

        assert_eq!(with_shell("/bin/zsh"), PathBuf::from("/bin/zsh"));
        assert_eq!(
            with_shell("/opt/homebrew/bin/bash"),
            PathBuf::from("/opt/homebrew/bin/bash")
        );
        assert_eq!(
            with_shell("/opt/homebrew/bin/fish"),
            PathBuf::from("/bin/sh")
        );
        assert_eq!(with_shell(""), PathBuf::from("/bin/sh"));
    }

    fn resolver() -> RequestResolver {
        RequestResolver::new(
            Arc::new(FakeExecutableResolver),
            EnvironmentVariables::default()
                .with("HOME", "/Users/alice")
                .with("PROJECT", "project with spaces"),
        )
    }

    fn ssh_request() -> SshLaunchRequest {
        SshLaunchRequest {
            title: "API".to_owned(),
            host: "api.example.com".to_owned(),
            user: "deploy".to_owned(),
            port: SshPort::DEFAULT,
            identity_file: None,
            extra_args: Vec::new(),
            config_directory: PathBuf::from("/shared/config"),
        }
    }

    #[test]
    fn basic_ssh_has_only_the_destination_argument() {
        let command = resolver()
            .resolve(&LaunchRequest::Ssh(ssh_request()))
            .expect("resolve SSH");

        assert_eq!(
            command.payload,
            ResolvedPayload::Structured {
                executable: PathBuf::from("/usr/bin/ssh"),
                args: vec!["deploy@api.example.com".to_owned()],
            }
        );
    }

    /// A remote host has never heard of `xterm-ghostty` and its kin, and a session that announces
    /// one loses `clear`, `Ctrl-L`, and everything else its terminfo would have described.
    #[test]
    fn an_ssh_session_announces_a_terminal_every_host_knows() {
        let command = resolver()
            .resolve(&LaunchRequest::Ssh(ssh_request()))
            .expect("resolve SSH");

        assert_eq!(
            command.env.get("TERM").map(String::as_str),
            Some("xterm-256color")
        );
        assert_eq!(command.env.len(), 1, "and nothing else is carried over");
    }

    /// Only SSH sends one: a local command runs in the terminal the user is looking at, and telling
    /// it that it is something else would cost the very capabilities it does have.
    #[test]
    fn a_local_command_keeps_the_terminals_own_type() {
        let command = resolver()
            .resolve(&LaunchRequest::Command(CommandLaunchRequest {
                title: "Codex".to_owned(),
                executable: "codex".to_owned(),
                working_directory: None,
                env: std::collections::BTreeMap::new(),
                args: Vec::new(),
                config_directory: PathBuf::from("/tmp"),
            }))
            .expect("resolve a command");

        assert!(!command.env.contains_key("TERM"), "{:?}", command.env);
    }

    #[test]
    fn ssh_argument_order_is_identity_port_extra_args_then_destination() {
        let mut request = ssh_request();
        request.identity_file =
            Some(PathTemplate::new("$HOME/.ssh/work key").expect("valid identity template"));
        request.port = SshPort::new(2222).expect("nonzero port");
        request.extra_args = vec![
            "-o".to_owned(),
            "ProxyCommand=$(touch /tmp/pwned); `id`; $HOME; a'b".to_owned(),
        ];
        let command = resolver()
            .resolve(&LaunchRequest::Ssh(request))
            .expect("resolve SSH");

        let ResolvedPayload::Structured { args, .. } = command.payload else {
            panic!("SSH must be structured");
        };
        assert_eq!(
            args,
            vec![
                "-i",
                "/Users/alice/.ssh/work key",
                "-p",
                "2222",
                "-o",
                "ProxyCommand=$(touch /tmp/pwned); `id`; $HOME; a'b",
                "deploy@api.example.com",
            ]
        );
    }

    #[test]
    fn ipv6_destination_stays_one_argument() {
        let mut request = ssh_request();
        request.host = "2001:db8::42".to_owned();
        let command = resolver()
            .resolve(&LaunchRequest::Ssh(request))
            .expect("resolve SSH");

        let ResolvedPayload::Structured { args, .. } = command.payload else {
            panic!("SSH must be structured");
        };
        assert_eq!(args, vec!["deploy@2001:db8::42"]);
    }

    #[test]
    fn all_built_in_agent_executables_are_resolved_without_argument_joining() {
        for executable in ["codex", "claude", "gemini", "opencode"] {
            let command = resolver()
                .resolve(&LaunchRequest::Command(CommandLaunchRequest {
                    env: BTreeMap::new(),
                    title: executable.to_owned(),
                    executable: executable.to_owned(),
                    working_directory: None,
                    args: vec!["--profile".to_owned(), "project one".to_owned()],
                    config_directory: PathBuf::from("/shared/config"),
                }))
                .expect("resolve built-in executable");

            assert_eq!(
                command.payload,
                ResolvedPayload::Structured {
                    executable: PathBuf::from("/resolved").join(executable),
                    args: vec!["--profile".to_owned(), "project one".to_owned()],
                }
            );
        }
    }

    #[test]
    fn working_directories_expand_relative_to_the_config_without_shell_evaluation() {
        let command = resolver()
            .resolve(&LaunchRequest::Command(CommandLaunchRequest {
                env: BTreeMap::new(),
                title: "Codex".to_owned(),
                executable: "codex".to_owned(),
                working_directory: Some(
                    PathTemplate::new("../${PROJECT}/$(touch pwned)").expect("valid path template"),
                ),
                args: Vec::new(),
                config_directory: PathBuf::from("/shared/config"),
            }))
            .expect("resolve command");

        assert_eq!(
            command.working_directory,
            Some(PathBuf::from(
                "/shared/config/../project with spaces/$(touch pwned)"
            ))
        );
    }

    #[test]
    fn raw_shell_mode_preserves_the_authored_command() {
        let authored = "echo first\nprintf '%s' '$HOME; $(id)'";
        let command = resolver()
            .resolve(&LaunchRequest::RawShell(RawShellLaunchRequest {
                env: BTreeMap::new(),
                title: "Trusted raw".to_owned(),
                command: authored.to_owned(),
                working_directory: None,
                config_directory: PathBuf::from("/shared/config"),
            }))
            .expect("resolve raw command");

        assert_eq!(
            command.payload,
            ResolvedPayload::RawShell {
                command: authored.to_owned()
            }
        );
    }

    #[test]
    fn system_resolver_uses_the_supplied_gui_path_for_bare_executables() {
        let directory = tempdir().expect("temporary directory");
        let executable = directory.path().join("custom-agent");
        fs::write(&executable, "#!/bin/sh\nexit 0\n").expect("write executable");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
            .expect("make executable");
        let environment = EnvironmentVariables::default()
            .with("PATH", directory.path().to_string_lossy().into_owned());

        assert_eq!(
            SystemExecutableResolver
                .resolve("custom-agent", Path::new("/shared/config"), &environment)
                .expect("resolve from GUI PATH"),
            executable
        );
    }

    #[test]
    fn missing_explicit_executable_error_names_the_requested_path() {
        let requested = "/definitely/missing/despatch-agent";
        let error = SystemExecutableResolver
            .resolve(
                requested,
                Path::new("/shared/config"),
                &EnvironmentVariables::default(),
            )
            .expect_err("path must be missing");

        assert_eq!(
            error,
            LaunchBuildError::MissingExecutable(requested.to_owned())
        );
        assert!(error.to_string().contains(requested));
    }
}
