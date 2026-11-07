use anyhow::Context;
use despatch_application::Application;
use despatch_config::{JsonConfigRepository, JsonRecentLaunches};
use despatch_platform_macos::{
    LocalPreferences, MacOsFileRevealer, MacOsTerminalLauncher, default_config_path,
};
use despatch_probe::TcpHostProbe;
use std::sync::Arc;

/// The launcher that opens terminals: the user's own terminal, or — in a development build
/// running the live self-test — the one named by `DESPATCH_LIVE_TERMINAL`, so the test can launch
/// through a terminal that needs no Automation permission.
fn terminal_launcher() -> MacOsTerminalLauncher {
    #[cfg(feature = "capture")]
    if let Ok(bundle_id) = std::env::var("DESPATCH_LIVE_TERMINAL") {
        return MacOsTerminalLauncher::new(
            despatch_platform_macos::RequestResolver::default(),
            Arc::new(despatch_platform_macos::SystemOpenInvoker),
            despatch_platform_macos::TerminalChoice::Chosen(bundle_id),
        );
    }
    MacOsTerminalLauncher::default()
}

fn main() -> anyhow::Result<()> {
    let default_path = default_config_path();
    let mut application = Application::new(
        Arc::new(JsonConfigRepository),
        Arc::new(LocalPreferences::standard()),
        Arc::new(terminal_launcher()),
        Arc::new(MacOsFileRevealer::default()),
        Arc::new(TcpHostProbe),
    );
    // The launch history is machine-local, so it lives beside the default config in Application
    // Support even when the config itself has been moved to a shared folder.
    if let Ok(path) = &default_path {
        application = application.with_recents(Arc::new(JsonRecentLaunches::beside(path)));
    }
    let startup_error = default_path
        .map_err(anyhow::Error::from)
        .and_then(|path| application.load_startup(&path).map_err(anyhow::Error::from))
        .err()
        .map(|error| error.to_string());

    despatch_ui_iced::run(application, startup_error).context("the Despatch UI event loop failed")
}
