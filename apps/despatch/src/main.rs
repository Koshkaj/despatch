use anyhow::Context;
use despatch_application::Application;
use despatch_config::JsonConfigRepository;
use despatch_platform_macos::{
    LocalPreferences, MacOsFileRevealer, MacOsTerminalLauncher, SystemExecutableResolver,
    default_config_path,
};
use despatch_probe::TcpHostProbe;
use std::sync::Arc;

fn main() -> anyhow::Result<()> {
    let mut application = Application::new(
        Arc::new(JsonConfigRepository),
        Arc::new(LocalPreferences::standard()),
        Arc::new(MacOsTerminalLauncher::default()),
        Arc::new(MacOsFileRevealer::default()),
        Arc::new(TcpHostProbe),
        Arc::new(SystemExecutableResolver),
    );
    let startup_error = default_config_path()
        .map_err(anyhow::Error::from)
        .and_then(|path| application.load_startup(&path).map_err(anyhow::Error::from))
        .err()
        .map(|error| error.to_string());

    despatch_ui_iced::run(application, startup_error).context("the Despatch UI event loop failed")
}
