use std::path::{Path, PathBuf};

use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
use objc2_foundation::{MainThreadMarker, NSString, NSURL};

/// What a file panel asks the user to pick.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileChoiceRequest {
    /// The panel title, shown above the browser.
    pub title: String,
    /// The sentence explaining what the chosen file is used for.
    pub message: String,
    /// The label of the confirm button.
    pub prompt: String,
    /// The directory the panel opens in, when it still exists.
    pub directory: Option<PathBuf>,
    /// Whether files starting with a dot, and dot directories such as `~/.ssh`, are listed.
    pub shows_hidden_files: bool,
}

/// Runs the system open panel and returns the file the user chose.
///
/// The panel is application-modal, so the call blocks until the user picks a file or cancels, and
/// returns `None` for a cancelled panel. Iced offers no file chooser, so this is the smallest
/// `AppKit` bridge that gets a native one; it must run on the main thread, which is where the Iced
/// update loop already lives.
#[must_use]
pub fn choose_file(request: &FileChoiceRequest) -> Option<PathBuf> {
    let main_thread = MainThreadMarker::new()?;
    let panel = NSOpenPanel::openPanel(main_thread);
    panel.setTitle(Some(&NSString::from_str(&request.title)));
    panel.setMessage(Some(&NSString::from_str(&request.message)));
    panel.setPrompt(Some(&NSString::from_str(&request.prompt)));
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(false);
    panel.setAllowsMultipleSelection(false);
    panel.setShowsHiddenFiles(request.shows_hidden_files);
    if let Some(directory) = request.directory.as_deref().filter(|path| path.is_dir()) {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&directory.to_string_lossy()));
        panel.setDirectoryURL(Some(&url));
    }

    if panel.runModal() != NSModalResponseOK {
        return None;
    }
    let path = panel.URL()?.path()?;
    Some(PathBuf::from(path.to_string()))
}

/// Rewrites a path inside `home` to start with `$HOME`, so a chosen key stays portable in a config
/// file shared between machines. Any other path is returned unchanged.
#[must_use]
pub fn portable_path(path: &Path, home: &Path) -> String {
    path.strip_prefix(home).map_or_else(
        |_| path.to_string_lossy().into_owned(),
        |relative| format!("$HOME/{}", relative.to_string_lossy()),
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::portable_path;

    #[test]
    fn a_key_inside_the_home_directory_is_stored_as_a_home_relative_template() {
        assert_eq!(
            portable_path(
                Path::new("/Users/alice/.ssh/id_ed25519"),
                Path::new("/Users/alice")
            ),
            "$HOME/.ssh/id_ed25519"
        );
    }

    #[test]
    fn a_key_outside_the_home_directory_keeps_its_absolute_path() {
        assert_eq!(
            portable_path(
                Path::new("/Volumes/keys/id_ed25519"),
                Path::new("/Users/alice")
            ),
            "/Volumes/keys/id_ed25519"
        );
    }
}
