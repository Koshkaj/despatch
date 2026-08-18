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
mod style;
mod theme;
mod typography;
mod views;

use despatch_application::Application;
use iced::Size;
use iced::window::settings::PlatformSpecific;

pub use app_shell::Message;

/// Failure to initialize or run the GPU-backed UI event loop.
#[derive(Debug, thiserror::Error)]
pub enum UiError {
    #[error("Iced platform initialization or event loop failed: {0}")]
    Iced(#[from] iced::Error),
}

/// Starts the Iced event loop using the wgpu renderer.
///
/// # Errors
///
/// Returns [`UiError`] if the platform window, GPU renderer, or event loop cannot initialize or
/// run.
pub fn run(application: Application, startup_error: Option<String>) -> Result<(), UiError> {
    let mut app = iced::application(
        move || app_shell::AppShell::new(application.clone(), startup_error.clone()),
        app_shell::update,
        views::view,
    )
    .title("Despatch")
    .subscription(app_shell::subscription)
    .theme(app_shell::AppShell::theme);

    // Every weight is registered, not just the default one: a weight with no face behind it is
    // synthesized by the shaper, which reads as a smeared Regular rather than a heading.
    for font in typography::BUNDLED {
        app = app.font(font);
    }

    app.default_font(typography::UI)
        .window(iced::window::Settings {
            size: Size::new(940.0, 620.0),
            min_size: Some(Size::new(680.0, 470.0)),
            position: iced::window::Position::Centered,
            // The sidebar runs to the top of the window and the traffic lights float over it, the way
            // a native macOS source-list app looks.
            platform_specific: PlatformSpecific {
                title_hidden: true,
                titlebar_transparent: true,
                fullsize_content_view: true,
            },
            ..iced::window::Settings::default()
        })
        .antialiasing(true)
        .run()?;
    Ok(())
}
