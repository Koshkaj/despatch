//! A development-only screenshot harness.
//!
//! Enabled by the `capture` cargo feature, it lets a build render one frame of the real GPU
//! interface to a file so the layout can be reviewed without a human at the keyboard. It is never
//! compiled into a release build of the app.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use iced::window::Screenshot;
use iced::{Task, window};

const SETTLE: Duration = Duration::from_millis(700);
const FILE_HEADER_LEN: u32 = 14;
const INFO_HEADER_LEN: u32 = 40;

/// The file a capture run should be written to, taken from `DESPATCH_CAPTURE`.
#[must_use]
pub fn requested() -> Option<PathBuf> {
    std::env::var_os("DESPATCH_CAPTURE").map(PathBuf::from)
}

/// The directory a live self-test writes its photographs to, taken from `DESPATCH_LIVE_TEST`.
///
/// A live run is the real menu-bar app: it installs its item, opens its panel under it, and then
/// drives it through the item the way a click would, reporting what the window server says after
/// each step. Unlike a capture, it photographs the composited window, so the panel's transparent
/// corners and its shadow are in the picture.
#[must_use]
pub fn live() -> Option<PathBuf> {
    std::env::var_os("DESPATCH_LIVE_TEST").map(PathBuf::from)
}

/// The interface state to open before capturing, taken from `DESPATCH_CAPTURE_SCENE`.
#[must_use]
pub fn scene() -> Option<String> {
    std::env::var("DESPATCH_CAPTURE_SCENE").ok()
}

/// Forces an appearance for a capture run, taken from `DESPATCH_CAPTURE_APPEARANCE`.
#[must_use]
pub fn appearance_override() -> Option<bool> {
    match std::env::var("DESPATCH_CAPTURE_APPEARANCE").ok()?.as_str() {
        "dark" => Some(true),
        "light" => Some(false),
        _ => None,
    }
}

/// Forces Increase Contrast for a capture run, taken from `DESPATCH_CAPTURE_CONTRAST=increased`.
#[must_use]
pub fn increased_contrast_override() -> bool {
    std::env::var("DESPATCH_CAPTURE_CONTRAST").is_ok_and(|value| value == "increased")
}

/// Forces Reduce Motion for a live run, taken from `DESPATCH_LIVE_REDUCE_MOTION=1`.
#[must_use]
pub fn reduce_motion_override() -> bool {
    std::env::var("DESPATCH_LIVE_REDUCE_MOTION").is_ok_and(|value| value == "1")
}

/// Waits for the first frame to render, then grabs the window contents.
///
/// A capture run photographs the first frame on purpose: a window that never becomes frontmost is
/// not asked to redraw again, so the scene is opened before the interface is built.
pub fn schedule(window_id: window::Id) -> Task<Screenshot> {
    Task::future(async {
        std::thread::sleep(SETTLE);
    })
    .then(move |()| window::screenshot(window_id))
}

/// Writes the capture and quits.
pub fn finish<Message: Send + 'static>(screenshot: &Screenshot) -> Task<Message> {
    if let Some(path) = requested() {
        let written = write_bmp(
            &path,
            &screenshot.rgba,
            screenshot.size.width,
            screenshot.size.height,
        );
        match written {
            Ok(()) => eprintln!("capture: wrote {}", path.display()),
            Err(error) => eprintln!("capture: {error}"),
        }
    }
    iced::exit()
}

/// Writes a top-down 32-bit BMP, which macOS `sips` can convert without extra dependencies.
fn write_bmp(path: &PathBuf, rgba: &[u8], width: u32, height: u32) -> io::Result<()> {
    let pixel_bytes = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| io::Error::other("capture dimensions overflowed"))?;
    if rgba.len() < pixel_bytes as usize {
        return Err(io::Error::other("capture buffer is smaller than its size"));
    }

    let mut file = Vec::with_capacity((FILE_HEADER_LEN + INFO_HEADER_LEN + pixel_bytes) as usize);
    file.extend_from_slice(b"BM");
    file.extend_from_slice(&(FILE_HEADER_LEN + INFO_HEADER_LEN + pixel_bytes).to_le_bytes());
    file.extend_from_slice(&0u16.to_le_bytes());
    file.extend_from_slice(&0u16.to_le_bytes());
    file.extend_from_slice(&(FILE_HEADER_LEN + INFO_HEADER_LEN).to_le_bytes());

    file.extend_from_slice(&INFO_HEADER_LEN.to_le_bytes());
    file.extend_from_slice(&i32::try_from(width).unwrap_or(i32::MAX).to_le_bytes());
    // A negative height stores the rows top-down, matching the screenshot buffer.
    file.extend_from_slice(
        &i32::try_from(height)
            .unwrap_or(i32::MAX)
            .saturating_neg()
            .to_le_bytes(),
    );
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&32u16.to_le_bytes());
    file.extend_from_slice(&0u32.to_le_bytes());
    file.extend_from_slice(&pixel_bytes.to_le_bytes());
    file.extend_from_slice(&2835i32.to_le_bytes());
    file.extend_from_slice(&2835i32.to_le_bytes());
    file.extend_from_slice(&0u32.to_le_bytes());
    file.extend_from_slice(&0u32.to_le_bytes());

    for pixel in rgba[..pixel_bytes as usize].chunks_exact(4) {
        file.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
    }
    fs::write(path, file)
}
