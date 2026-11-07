#![doc = "GPU-rendered Iced user-interface adapter for Despatch."]
#![forbid(unsafe_code)]

mod accessibility;
#[cfg(target_os = "macos")]
mod accessibility_bridge;
mod app_shell;
#[cfg(feature = "capture")]
mod capture;
mod drag_layer;
mod editor;
mod icons;
mod presenters;
mod shortcuts;
mod status_item;
mod style;
mod theme;
mod typography;
mod views;

use despatch_application::Application;
use iced::window::settings::PlatformSpecific;
use iced::{Size, window};

pub use app_shell::Message;

/// Failure to initialize or run the GPU-backed UI event loop.
#[derive(Debug, thiserror::Error)]
pub enum UiError {
    #[error("Iced platform initialization or event loop failed: {0}")]
    Iced(#[from] iced::Error),
}

/// Starts the Iced event loop using the wgpu renderer.
///
/// Despatch runs as a menu-bar app: a daemon with no window of its own at start, which opens one
/// borderless panel window and keeps it for as long as it runs, showing and hiding it under the
/// menu-bar item.
///
/// # Errors
///
/// Returns [`UiError`] if the platform window, GPU renderer, or event loop cannot initialize or
/// run.
pub fn run(application: Application, startup_error: Option<String>) -> Result<(), UiError> {
    iced::daemon(
        move || {
            let shell = app_shell::AppShell::new(application.clone(), startup_error.clone());
            let (_, open) = window::open(panel_settings());
            (shell, open.map(app_shell::Message::WindowOpened))
        },
        app_shell::update,
        views::view,
    )
    .title(|_: &app_shell::AppShell, _| "Despatch".to_owned())
    .subscription(app_shell::subscription)
    .theme(|shell: &app_shell::AppShell, _| shell.theme())
    // The window is transparent outside the panel's rounded corners, so nothing is painted behind
    // the panel itself.
    .style(|_, theme| iced::theme::Style {
        background_color: iced::Color::TRANSPARENT,
        text_color: theme.palette().text,
    })
    .default_font(typography::UI)
    .antialiasing(true)
    .run()?;
    Ok(())
}

/// The panel window: borderless, transparent around the panel's corners, floating above other
/// windows, and hidden until the menu-bar item shows it.
fn panel_settings() -> window::Settings {
    #[cfg(feature = "capture")]
    let capturing = capture::requested().is_some();
    #[cfg(not(feature = "capture"))]
    let capturing = false;
    window::Settings {
        size: Size::new(app_shell::PANEL_WIDTH, app_shell::PANEL_HEIGHT),
        // A capture run never shows the panel under the menu bar, so it is placed where a screenshot
        // can find it.
        position: if capturing {
            window::Position::Centered
        } else {
            window::Position::Default
        },
        visible: capturing,
        resizable: false,
        closeable: false,
        minimizable: false,
        decorations: false,
        transparent: true,
        level: window::Level::AlwaysOnTop,
        exit_on_close_request: false,
        platform_specific: PlatformSpecific {
            title_hidden: true,
            titlebar_transparent: true,
            fullsize_content_view: true,
        },
        ..window::Settings::default()
    }
}
