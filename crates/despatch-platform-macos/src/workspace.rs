use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use despatch_application::{FileRevealer, PortError, PortErrorKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenRequest {
    pub arguments: Vec<PathBuf>,
    pub reveal: bool,
    /// The bundle identifier to open the arguments with. `None` uses the system association.
    pub application_bundle_id: Option<String>,
    /// Arguments handed to the opened application itself rather than treated as files to open.
    ///
    /// Present only for terminals that accept a command on their own command line; a non-empty
    /// value also forces a new instance, so the command lands in a new window rather than being
    /// swallowed by one already running.
    pub application_arguments: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenResult {
    pub success: bool,
    pub status_code: Option<i32>,
    pub stderr: String,
}

pub trait OpenInvoker: Send + Sync {
    fn invoke(&self, request: &OpenRequest) -> Result<OpenResult, PortError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemOpenInvoker;

impl OpenInvoker for SystemOpenInvoker {
    fn invoke(&self, request: &OpenRequest) -> Result<OpenResult, PortError> {
        let mut command = Command::new("/usr/bin/open");
        if request.reveal {
            command.arg("-R");
        }
        if !request.application_arguments.is_empty() {
            // A terminal already running would otherwise ignore the command entirely.
            command.arg("-n");
        }
        if let Some(bundle_id) = &request.application_bundle_id {
            // Passed as a separate process argument, never interpolated into a shell string.
            command.arg("-b").arg(bundle_id);
        }
        command.args(&request.arguments);
        if !request.application_arguments.is_empty() {
            command.arg("--args");
            command.args(&request.application_arguments);
        }
        let output = command.output().map_err(|error| {
            PortError::new(
                PortErrorKind::Unavailable,
                format!("could not invoke /usr/bin/open: {error}"),
            )
        })?;
        Ok(OpenResult {
            success: output.status.success(),
            status_code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}

#[derive(Clone)]
pub struct MacOsFileRevealer {
    invoker: Arc<dyn OpenInvoker>,
}

impl Default for MacOsFileRevealer {
    fn default() -> Self {
        Self {
            invoker: Arc::new(SystemOpenInvoker),
        }
    }
}

impl MacOsFileRevealer {
    #[must_use]
    pub fn new(invoker: Arc<dyn OpenInvoker>) -> Self {
        Self { invoker }
    }
}

impl FileRevealer for MacOsFileRevealer {
    fn reveal(&self, path: &Path) -> Result<(), PortError> {
        let result = self.invoker.invoke(&OpenRequest {
            arguments: vec![path.to_owned()],
            reveal: true,
            application_bundle_id: None,
            application_arguments: Vec::new(),
        })?;
        if result.success {
            Ok(())
        } else {
            Err(open_failure("reveal config in Finder", &result))
        }
    }
}

pub(crate) fn open_failure(action: &str, result: &OpenResult) -> PortError {
    PortError::new(
        PortErrorKind::Unavailable,
        format!(
            "could not {action}: /usr/bin/open exited with {}{}",
            result
                .status_code
                .map_or_else(|| "no status".to_owned(), |code| code.to_string()),
            if result.stderr.is_empty() {
                String::new()
            } else {
                format!(" ({})", result.stderr)
            }
        ),
    )
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use despatch_application::FileRevealer;

    use super::{MacOsFileRevealer, OpenInvoker, OpenRequest, OpenResult};

    #[derive(Default)]
    struct FakeOpen {
        requests: Mutex<Vec<OpenRequest>>,
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
            Ok(OpenResult {
                success: true,
                status_code: Some(0),
                stderr: String::new(),
            })
        }
    }

    #[test]
    fn finder_reveal_passes_the_config_as_one_path_argument() {
        let invoker = Arc::new(FakeOpen::default());
        let revealer = MacOsFileRevealer::new(invoker.clone());
        let path = Path::new("/tmp/shared config/config.json");

        revealer.reveal(path).expect("reveal config");

        assert_eq!(
            *invoker.requests.lock().expect("open request lock"),
            vec![OpenRequest {
                arguments: vec![PathBuf::from(path)],
                reveal: true,
                application_bundle_id: None,
                application_arguments: Vec::new(),
            }]
        );
    }
}
