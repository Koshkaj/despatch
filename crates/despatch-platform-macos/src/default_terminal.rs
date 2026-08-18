use std::fs;
use std::sync::Arc;

use despatch_application::{LaunchRequest, PortError, PortErrorKind, TerminalLauncher};

use crate::script_file;
use crate::shell_script;
use crate::terminal_argv;
use crate::terminal_association;
use crate::workspace::{OpenInvoker, OpenRequest, SystemOpenInvoker, open_failure};
use crate::{RequestResolver, ResolvedCommand, ghostty};

/// Where the terminal that runs a launcher comes from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminalChoice {
    /// The application the user picked in Launch Services, by bundle identifier.
    Chosen(String),
    /// No explicit choice; Launch Services picks the handler for the script itself.
    SystemAssociation,
}

/// Terminals that take a command on their own command line, as `-e <argv...>`.
///
/// A terminal outside this list is handed a script file instead: there is no portable way to give
/// one an argument vector, and guessing a flag would silently open an empty window.
const ARGV_TERMINALS: [&str; 5] = [
    "net.kovidgoyal.kitty",
    "org.alacritty",
    "io.alacritty",
    "com.github.wez.wezterm",
    "dev.warp.Warp-Stable",
];

const GHOSTTY_BUNDLE_ID: &str = "com.mitchellh.ghostty";

/// What a terminal can be asked to do for itself, rather than through a shell wrapped around the
/// launcher's command.
///
/// Whatever a terminal cannot do, [`crate::terminal_argv::WRAPPER`] does — at the price of a shell
/// process sitting between the terminal and the command the user actually asked for. Everything a
/// terminal *can* do is one less process in the session and one less thing to explain.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TerminalTraits {
    /// Arguments handed over before the command, so a launch opens exactly one window.
    ///
    /// A launch starts a **new instance** of the terminal (see `launch_with_arguments`), and a
    /// starting instance does two things nobody asked for: it opens its own default window, and it
    /// reopens whatever windows the app had when it was last quit. Either shows up as a second tab
    /// running a login shell.
    pub leading: &'static [&'static str],
    /// How this terminal is told where to start, if it can be told at all.
    pub working_directory_flag: Option<&'static str>,
    /// Whether the window survives the command exiting, holding the error where it can be read.
    ///
    /// A terminal that closes the window regardless needs the wrapper for it, or a launch that
    /// fails on the spot vanishes with its own explanation. Measured, not assumed: a Ghostty asked
    /// to run a command that exits non-zero closes its window and quits the instance, and only
    /// `--wait-after-command` in [`Self::leading`] changes that.
    pub holds_window_after_failure: bool,
}

fn traits(_bundle_id: &str) -> TerminalTraits {
    // Ghostty used to be the only entry with native traits. It now launches through its scripting
    // API before this table is consulted: every executable supplied with `-e` gets a confirmation
    // dialog from Ghostty. Keep the table boundary for argv terminals that gain reviewed switches.
    TerminalTraits::default()
}

impl TerminalChoice {
    /// What this terminal can do for itself.
    #[must_use]
    pub fn traits(&self) -> TerminalTraits {
        self.bundle_id()
            .map_or_else(TerminalTraits::default, traits)
    }

    /// Whether this terminal can be handed the command directly rather than a staged file.
    #[must_use]
    pub fn accepts_command_arguments(&self) -> bool {
        self.bundle_id()
            .is_some_and(|bundle_id| ARGV_TERMINALS.contains(&bundle_id))
    }

    /// Reads the user's current choice.
    #[must_use]
    pub fn detect() -> Self {
        terminal_association::preferred_terminal_bundle_id()
            .map_or(Self::SystemAssociation, Self::Chosen)
    }

    #[must_use]
    pub fn bundle_id(&self) -> Option<&str> {
        match self {
            Self::Chosen(bundle_id) => Some(bundle_id),
            Self::SystemAssociation => None,
        }
    }

    /// How the choice reads in the settings sheet.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Chosen(bundle_id) => {
                application_name(bundle_id).unwrap_or_else(|| bundle_id.clone())
            }
            Self::SystemAssociation => "The app associated with .command files".to_owned(),
        }
    }
}

/// The display name of an installed application, found through Launch Services' own lookup.
fn application_name(bundle_id: &str) -> Option<String> {
    let output = std::process::Command::new("/usr/bin/mdfind")
        .arg(format!("kMDItemCFBundleIdentifier == '{bundle_id}'"))
        .output()
        .ok()?;
    let path = String::from_utf8_lossy(&output.stdout);
    let path = path.lines().next()?.trim();
    std::path::Path::new(path)
        .file_stem()?
        .to_str()
        .map(ToOwned::to_owned)
}

#[derive(Clone)]
pub struct MacOsTerminalLauncher {
    resolver: RequestResolver,
    invoker: Arc<dyn OpenInvoker>,
    ghostty_invoker: Arc<dyn ghostty::GhosttyInvoker>,
    terminal: TerminalChoice,
}

impl Default for MacOsTerminalLauncher {
    fn default() -> Self {
        Self {
            resolver: RequestResolver::default(),
            invoker: Arc::new(SystemOpenInvoker),
            ghostty_invoker: Arc::new(ghostty::SystemGhosttyInvoker),
            terminal: TerminalChoice::detect(),
        }
    }
}

impl MacOsTerminalLauncher {
    #[must_use]
    pub fn new(
        resolver: RequestResolver,
        invoker: Arc<dyn OpenInvoker>,
        terminal: TerminalChoice,
    ) -> Self {
        Self {
            resolver,
            invoker,
            ghostty_invoker: Arc::new(ghostty::SystemGhosttyInvoker),
            terminal,
        }
    }

    /// The terminal this launcher will hand scripts to.
    #[must_use]
    pub const fn terminal(&self) -> &TerminalChoice {
        &self.terminal
    }

    /// Opens a new window of the terminal with the command on its own command line.
    ///
    /// Nothing is written to disk. The older path staged a `.command` file, which terminals run by
    /// appending `; exit` to a shell line — that is where "press any key to close the terminal"
    /// came from, and it is not something the file's own contents can prevent.
    fn launch_with_arguments(&self, command: &ResolvedCommand) -> Result<(), PortError> {
        let traits = self.terminal.traits();
        let mut application_arguments: Vec<String> = traits
            .leading
            .iter()
            .map(|argument| (*argument).to_owned())
            .collect();

        // Whatever the terminal can do for itself, it does. What is left over — a working directory
        // a terminal cannot be told about, or holding a window open on a command that failed — is
        // what the wrapper is for, and the wrapper costs a shell process in every session.
        let directory = command.working_directory.as_ref().and_then(|directory| {
            traits
                .working_directory_flag
                .map(|flag| format!("{flag}{}", directory.display()))
        });
        let wrapped = (command.working_directory.is_some() && directory.is_none())
            || !traits.holds_window_after_failure;
        application_arguments.extend(directory);

        application_arguments.push("-e".to_owned());
        application_arguments.extend(terminal_argv::render(command, wrapped));
        let result = self.invoker.invoke(&OpenRequest {
            arguments: Vec::new(),
            reveal: false,
            application_bundle_id: self.terminal.bundle_id().map(ToOwned::to_owned),
            application_arguments,
        })?;
        if result.success {
            Ok(())
        } else {
            Err(open_failure("open a terminal for the command", &result))
        }
    }

    /// Opens a Ghostty window through its macOS scripting API.
    ///
    /// Supplying a program with Ghostty's `-e` startup option intentionally causes Ghostty to ask
    /// the user to approve that executable on every new instance, and `open -n` starts a whole
    /// second copy of Ghostty to receive it. The scripting API is protected by macOS Automation
    /// permission instead, which is granted once to Despatch, and talks to the copy already
    /// running.
    ///
    /// Nothing is written to disk and no shell of ours is involved: the command, the directory it
    /// starts in, and the variables it needs are properties of the surface Ghostty creates.
    fn launch_ghostty(&self, command: &ResolvedCommand) -> Result<(), PortError> {
        let result = self
            .ghostty_invoker
            .invoke(&ghostty::GhosttySurface::new(command));
        match result {
            Ok(result) if result.success => Ok(()),
            Ok(result) => Err(PortError::new(
                PortErrorKind::Unavailable,
                format!(
                    "could not open command in Ghostty: its scripting API exited with {}{}; Ghostty 1.3 or later must be installed and macos-applescript must be enabled{}",
                    result
                        .status_code
                        .map_or_else(|| "no status".to_owned(), |code| code.to_string()),
                    if result.stderr.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", result.stderr)
                    },
                    if result.stderr.contains("Not authorized")
                        || result.stderr.contains("not allowed assistive access")
                    {
                        "; allow Despatch to automate Ghostty in System Settings"
                    } else {
                        ""
                    }
                ),
            )),
            Err(error) => Err(error),
        }
    }

    #[cfg(test)]
    fn with_ghostty_invoker(mut self, invoker: Arc<dyn ghostty::GhosttyInvoker>) -> Self {
        self.ghostty_invoker = invoker;
        self
    }
}

fn create_script(contents: &str) -> Result<std::path::PathBuf, PortError> {
    script_file::create(contents).map_err(|error| {
        PortError::new(
            PortErrorKind::PermissionDenied,
            format!("could not create temporary command file: {error}"),
        )
    })
}

impl TerminalLauncher for MacOsTerminalLauncher {
    fn destination(&self) -> String {
        self.terminal.describe()
    }

    fn launch(&self, request: &LaunchRequest) -> Result<(), PortError> {
        let command = self.resolver.resolve(request).map_err(PortError::from)?;
        if self.terminal.bundle_id() == Some(GHOSTTY_BUNDLE_ID) {
            return self.launch_ghostty(&command);
        }
        if self.terminal.accepts_command_arguments() {
            return self.launch_with_arguments(&command);
        }
        let script = shell_script::render(&command);
        let path = create_script(&script)?;
        let result = self.invoker.invoke(&OpenRequest {
            arguments: vec![path.clone()],
            reveal: false,
            application_bundle_id: self.terminal.bundle_id().map(ToOwned::to_owned),
            application_arguments: Vec::new(),
        });
        match result {
            Ok(result) if result.success => Ok(()),
            Ok(result) => {
                let _ = fs::remove_file(&path);
                Err(open_failure(
                    "open command in the associated terminal",
                    &result,
                ))
            }
            Err(error) => {
                let _ = fs::remove_file(path);
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use despatch_application::{
        CommandLaunchRequest, LaunchRequest, SshLaunchRequest, TerminalLauncher,
    };
    use despatch_domain::SshPort;

    use crate::ghostty::{GhosttyInvoker, GhosttySurface};
    use crate::{
        EnvironmentVariables, ExecutableResolver, LaunchBuildError, OpenInvoker, OpenRequest,
        OpenResult, RequestResolver,
    };

    use despatch_domain::PathTemplate;

    use super::{MacOsTerminalLauncher, TerminalChoice, TerminalTraits};

    fn launcher(invoker: Arc<dyn OpenInvoker>, terminal: TerminalChoice) -> MacOsTerminalLauncher {
        MacOsTerminalLauncher::new(
            RequestResolver::new(Arc::new(FakeResolver), EnvironmentVariables::default()),
            invoker,
            terminal,
        )
    }

    struct FakeResolver;

    impl ExecutableResolver for FakeResolver {
        fn resolve(
            &self,
            requested: &str,
            _config_directory: &Path,
            _environment: &EnvironmentVariables,
        ) -> Result<PathBuf, LaunchBuildError> {
            Ok(PathBuf::from("/resolved").join(requested))
        }
    }

    struct FakeOpen {
        requests: Mutex<Vec<OpenRequest>>,
        result: OpenResult,
    }

    struct FakeGhostty {
        surfaces: Mutex<Vec<GhosttySurface>>,
        result: OpenResult,
    }

    impl FakeGhostty {
        fn successful() -> Self {
            Self {
                surfaces: Mutex::new(Vec::new()),
                result: OpenResult {
                    success: true,
                    status_code: Some(0),
                    stderr: String::new(),
                },
            }
        }
    }

    impl GhosttyInvoker for FakeGhostty {
        fn invoke(
            &self,
            surface: &GhosttySurface,
        ) -> Result<OpenResult, despatch_application::PortError> {
            self.surfaces
                .lock()
                .expect("Ghostty surface lock")
                .push(surface.clone());
            Ok(self.result.clone())
        }
    }

    impl FakeOpen {
        fn successful() -> Self {
            Self {
                requests: Mutex::new(Vec::new()),
                result: OpenResult {
                    success: true,
                    status_code: Some(0),
                    stderr: String::new(),
                },
            }
        }
    }

    impl OpenInvoker for FakeOpen {
        fn invoke(
            &self,
            request: &OpenRequest,
        ) -> Result<OpenResult, despatch_application::PortError> {
            self.requests
                .lock()
                .expect("open request lock")
                .push(request.clone());
            Ok(self.result.clone())
        }
    }

    fn request() -> LaunchRequest {
        LaunchRequest::Command(CommandLaunchRequest {
            env: BTreeMap::new(),
            title: "Codex\u{7} title".to_owned(),
            executable: "codex".to_owned(),
            working_directory: None,
            args: vec!["a b; $(id); 'quoted'".to_owned()],
            config_directory: PathBuf::from("/shared/config"),
        })
    }

    #[test]
    fn terminal_launch_opens_one_path_argument_and_writes_a_self_deleting_script() {
        let invoker = Arc::new(FakeOpen::successful());
        let launcher = launcher(invoker.clone(), TerminalChoice::SystemAssociation);

        launcher.launch(&request()).expect("launch command file");

        let requests = invoker.requests.lock().expect("open request lock");
        assert_eq!(requests.len(), 1);
        assert!(!requests[0].reveal);
        assert_eq!(
            requests[0].application_bundle_id, None,
            "without a chosen terminal the system association stays in charge"
        );
        assert_eq!(requests[0].arguments.len(), 1);
        let script_path = &requests[0].arguments[0];
        let script = fs::read_to_string(script_path).expect("read staged script");
        assert!(script.contains("rm -f -- \"$script_path\""));
        assert!(script.contains("'/resolved/codex' 'a b; $(id); '\\''quoted'\\'''"));
        assert!(!script.contains('\u{7}'));
        fs::remove_file(script_path).expect("clean up staged script");
    }

    #[test]
    fn open_failure_removes_the_script_and_reports_status_and_stderr() {
        let invoker = Arc::new(FakeOpen {
            requests: Mutex::new(Vec::new()),
            result: OpenResult {
                success: false,
                status_code: Some(23),
                stderr: "no associated application".to_owned(),
            },
        });
        let launcher = launcher(invoker.clone(), TerminalChoice::SystemAssociation);

        let error = launcher.launch(&request()).expect_err("open must fail");

        assert!(error.message().contains("23"));
        assert!(error.message().contains("no associated application"));
        let path = invoker
            .requests
            .lock()
            .expect("open request lock")
            .requests_path();
        assert!(!path.exists());
    }

    #[test]
    fn ghostty_uses_its_scripting_api_instead_of_the_confirmed_executable_path() {
        let invoker = Arc::new(FakeOpen::successful());
        let ghostty = Arc::new(FakeGhostty::successful());
        let launcher = launcher(
            invoker.clone(),
            TerminalChoice::Chosen("com.mitchellh.ghostty".to_owned()),
        )
        .with_ghostty_invoker(ghostty.clone());

        launcher
            .launch(&LaunchRequest::Ssh(SshLaunchRequest {
                title: "Server".to_owned(),
                host: "example.com".to_owned(),
                user: "deploy".to_owned(),
                port: SshPort::DEFAULT,
                identity_file: None,
                extra_args: Vec::new(),
                config_directory: PathBuf::from("/shared/config"),
            }))
            .expect("launch SSH through Ghostty");

        let requests = invoker.requests.lock().expect("open request lock");
        assert!(
            requests.is_empty(),
            "Ghostty must not receive `open --args -e`, which triggers its executable prompt"
        );
        let surfaces = ghostty.surfaces.lock().expect("Ghostty surface lock");
        assert_eq!(surfaces.len(), 1);
        assert_eq!(surfaces[0].command, "'/usr/bin/ssh' 'deploy@example.com'");
        assert_eq!(surfaces[0].environment, ["TERM=xterm-256color"]);
        assert!(
            !surfaces[0].command.contains("/usr/bin/env"),
            "Ghostty exports the variables itself: {}",
            surfaces[0].command
        );
    }

    /// The launch that broke: a staged script reached Ghostty as `direct:<path>`, Ghostty passed the
    /// whole string to `bash -c exec -l`, and bash looked for a file called `direct:/var/folders/…`
    /// relative to the working directory. Nothing is staged now, and no prefix is invented.
    #[test]
    fn ghostty_is_handed_the_command_itself_and_stages_no_file() {
        let invoker = Arc::new(FakeOpen::successful());
        let ghostty = Arc::new(FakeGhostty::successful());
        let launcher = launcher(
            invoker,
            TerminalChoice::Chosen("com.mitchellh.ghostty".to_owned()),
        )
        .with_ghostty_invoker(ghostty.clone());

        launcher.launch(&request()).expect("launch the command");

        let surfaces = ghostty.surfaces.lock().expect("Ghostty surface lock");
        assert_eq!(
            surfaces[0].command, "'/resolved/codex' 'a b; $(id); '\\''quoted'\\'''",
            "the command is the program, quoted word by word"
        );
        assert!(!surfaces[0].command.contains("direct:"));
        assert!(!surfaces[0].command.contains(".command"));
    }

    /// A terminal that cannot hold a window open on a failure gets the wrapper, which can. The
    /// command still arrives intact, with a shell in front of it.
    #[test]
    fn a_terminal_that_cannot_hold_its_window_still_gets_the_wrapper() {
        let invoker = Arc::new(FakeOpen::successful());
        let launcher = launcher(
            invoker.clone(),
            TerminalChoice::Chosen("org.alacritty".to_owned()),
        );

        launcher.launch(&request()).expect("launch the command");

        let requests = invoker.requests.lock().expect("open request lock");
        let arguments = &requests[0].application_arguments;
        assert_eq!(arguments[0], "-e");
        assert_eq!(arguments[2], "-c");
        assert_eq!(
            arguments[arguments.len() - 2..],
            ["/resolved/codex", "a b; $(id); 'quoted'"]
        );
    }

    /// A working directory the terminal can set itself is set that way, rather than by wrapping the
    /// command in a shell that does nothing else.
    #[test]
    fn an_argv_terminal_without_a_native_directory_switch_gets_the_wrapper() {
        let invoker = Arc::new(FakeOpen::successful());
        let launcher = launcher(
            invoker.clone(),
            TerminalChoice::Chosen("org.alacritty".to_owned()),
        );
        let LaunchRequest::Command(mut command) = request() else {
            unreachable!("the fixture is a command launcher")
        };
        command.working_directory = Some(PathTemplate::new("/tmp/work").expect("a path"));

        launcher
            .launch(&LaunchRequest::Command(command))
            .expect("launch the command");

        let requests = invoker.requests.lock().expect("open request lock");
        let arguments = &requests[0].application_arguments;
        assert!(arguments.contains(&"/tmp/work".to_owned()), "{arguments:?}");
        assert_eq!(
            arguments[arguments.len() - 2..],
            ["/resolved/codex", "a b; $(id); 'quoted'"],
            "the command stays intact behind the directory wrapper"
        );
    }

    /// The switches are Ghostty's; a terminal with no such settings is handed nothing rather than a
    /// flag it would have to guess the meaning of — and it gets the wrapper instead.
    #[test]
    fn ghostty_is_not_given_the_old_command_line_switches() {
        let ghostty = TerminalChoice::Chosen("com.mitchellh.ghostty".to_owned()).traits();
        assert_eq!(ghostty, TerminalTraits::default());

        for other in [
            TerminalChoice::Chosen("org.alacritty".to_owned()),
            TerminalChoice::SystemAssociation,
        ] {
            assert_eq!(other.traits(), TerminalTraits::default());
        }
    }

    #[test]
    fn a_terminal_without_a_command_line_still_gets_a_staged_file() {
        let invoker = Arc::new(FakeOpen::successful());
        let launcher = launcher(
            invoker.clone(),
            TerminalChoice::Chosen("com.apple.Terminal".to_owned()),
        );

        launcher.launch(&request()).expect("launch command file");

        let requests = invoker.requests.lock().expect("open request lock");
        assert!(requests[0].application_arguments.is_empty());
        assert_eq!(requests[0].arguments.len(), 1);
        fs::remove_file(&requests[0].arguments[0]).expect("clean up staged script");
    }

    #[test]
    fn only_terminals_known_to_accept_a_command_are_given_one() {
        assert!(TerminalChoice::Chosen("org.alacritty".to_owned()).accepts_command_arguments());
        assert!(
            !TerminalChoice::Chosen("com.mitchellh.ghostty".to_owned()).accepts_command_arguments(),
            "Ghostty has a separate scripting path because `-e` causes an executable prompt"
        );
        assert!(
            !TerminalChoice::Chosen("com.apple.Terminal".to_owned()).accepts_command_arguments(),
            "guessing a flag would open an empty window"
        );
        assert!(!TerminalChoice::SystemAssociation.accepts_command_arguments());
    }

    #[test]
    fn a_detected_choice_reports_the_bundle_id_it_will_open_with() {
        assert_eq!(TerminalChoice::SystemAssociation.bundle_id(), None);
        assert_eq!(
            TerminalChoice::Chosen("com.googlecode.iterm2".to_owned()).bundle_id(),
            Some("com.googlecode.iterm2")
        );
    }

    #[test]
    fn every_choice_describes_itself_for_the_settings_sheet() {
        assert!(!TerminalChoice::SystemAssociation.describe().is_empty());
        let unknown = TerminalChoice::Chosen("com.example.not-installed".to_owned());
        assert_eq!(unknown.describe(), "com.example.not-installed");
    }

    trait RequestPath {
        fn requests_path(&self) -> PathBuf;
    }

    impl RequestPath for Vec<OpenRequest> {
        fn requests_path(&self) -> PathBuf {
            self.first()
                .and_then(|request| request.arguments.first())
                .expect("captured script path")
                .clone()
        }
    }
}
