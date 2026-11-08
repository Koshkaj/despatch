//! Resolves which terminal application should run Despatch's generated `.command` scripts.
//!
//! Launch Services routes a `.command` file by its content type, `com.apple.terminal.shell-script`,
//! whose stock handler is Terminal.app. Third-party terminals such as Ghostty, iTerm2, `WezTerm`, and
//! Kitty are instead made "the default terminal" by claiming `public.unix-executable`, which is what
//! `duti`-style setup instructions and the terminals' own docs configure. Opening a script without
//! naming an application therefore lands in Terminal.app even when the user has clearly chosen
//! something else.
//!
//! This module reads the user's own Launch Services choices and reports the bundle identifier to
//! hand to `open -b`. When the user has chosen nothing, it reports `None` and the caller falls back
//! to the plain system association.

use std::path::{Path, PathBuf};

use objc2::runtime::AnyObject;
use objc2_foundation::{NSArray, NSDictionary, NSString, NSURL};

/// The content type a user configures when picking a default terminal: Launch Services asks its
/// handler to *run* executables rather than edit them.
const EXECUTABLE_TYPE: &str = "public.unix-executable";
/// The content type of the `.command` scripts Despatch writes.
const SHELL_SCRIPT_TYPE: &str = "com.apple.terminal.shell-script";
/// Content types are consulted in this order; the first user-chosen handler wins.
const PREFERRED_TYPES: [&str; 2] = [EXECUTABLE_TYPE, SHELL_SCRIPT_TYPE];

const HANDLERS_KEY: &str = "LSHandlers";
const CONTENT_TYPE_KEY: &str = "LSHandlerContentType";
/// Roles are read in this order: an explicit shell role beats a catch-all role.
const ROLE_KEYS: [&str; 2] = ["LSHandlerRoleShell", "LSHandlerRoleAll"];

/// The bundle identifier of the terminal the user chose, if they chose one.
#[must_use]
pub fn preferred_terminal_bundle_id() -> Option<String> {
    let handlers = read_handlers(&launch_services_preferences()?)?;
    PREFERRED_TYPES
        .iter()
        .find_map(|content_type| handler_for(&handlers, content_type))
}

/// `~/Library/Preferences/com.apple.LaunchServices/com.apple.launchservices.secure.plist`.
fn launch_services_preferences() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join("Library/Preferences/com.apple.LaunchServices")
            .join("com.apple.launchservices.secure.plist"),
    )
}

fn read_handlers(path: &Path) -> Option<Vec<(String, String)>> {
    let path = NSString::from_str(path.to_str()?);
    let url = NSURL::fileURLWithPath(&path);
    // SAFETY: `dictionaryWithContentsOfURL:` reads a property list from a local file URL and
    // returns nil for anything it cannot parse, including a file that is not a dictionary. The
    // dictionary is typed with `AnyObject` values, so every value is downcast to a concrete class
    // before it is used and a plist with an unexpected shape yields `None` rather than a mistyped
    // object. Apple deprecated the selector in favour of an error-reporting variant that objc2
    // does not expose; the reason a preference file is unreadable is not actionable here, so the
    // deprecated form is the only complete API available.
    #[expect(
        deprecated,
        reason = "objc2 exposes no error-reporting replacement, and the reason is not actionable"
    )]
    let plist: objc2::rc::Retained<NSDictionary<NSString, AnyObject>> =
        unsafe { NSDictionary::dictionaryWithContentsOfURL(&url) }?;

    let handlers = plist.objectForKey(&NSString::from_str(HANDLERS_KEY))?;
    let handlers = handlers.downcast_ref::<NSArray>()?;

    let mut entries = Vec::new();
    for index in 0..handlers.count() {
        let entry = handlers.objectAtIndex(index);
        let Some(entry) = entry.downcast_ref::<NSDictionary>() else {
            continue;
        };
        let Some(content_type) = string_value(entry, CONTENT_TYPE_KEY) else {
            continue;
        };
        let Some(bundle_id) = ROLE_KEYS
            .iter()
            .find_map(|role| string_value(entry, role))
            .filter(|bundle_id| !bundle_id.trim().is_empty())
        else {
            continue;
        };
        entries.push((content_type, bundle_id));
    }
    Some(entries)
}

fn string_value(entry: &NSDictionary, key: &str) -> Option<String> {
    let value = entry.objectForKey(&*NSString::from_str(key))?;
    Some(value.downcast_ref::<NSString>()?.to_string())
}

/// The handler a set of Launch Services entries declares for one content type.
fn handler_for(handlers: &[(String, String)], content_type: &str) -> Option<String> {
    handlers
        .iter()
        .find(|(declared, _)| declared == content_type)
        .map(|(_, bundle_id)| bundle_id.clone())
}

#[cfg(test)]
mod tests {
    use super::{EXECUTABLE_TYPE, PREFERRED_TYPES, SHELL_SCRIPT_TYPE, handler_for};

    fn handlers() -> Vec<(String, String)> {
        vec![
            ("public.html".to_owned(), "com.brave.browser".to_owned()),
            (
                EXECUTABLE_TYPE.to_owned(),
                "com.mitchellh.ghostty".to_owned(),
            ),
            (
                SHELL_SCRIPT_TYPE.to_owned(),
                "com.apple.Terminal".to_owned(),
            ),
        ]
    }

    #[test]
    fn a_chosen_executable_handler_wins_over_the_shell_script_handler() {
        let chosen = PREFERRED_TYPES
            .iter()
            .find_map(|content_type| handler_for(&handlers(), content_type));

        assert_eq!(chosen.as_deref(), Some("com.mitchellh.ghostty"));
    }

    #[test]
    fn the_shell_script_handler_is_used_when_no_executable_handler_is_set() {
        let handlers: Vec<_> = handlers()
            .into_iter()
            .filter(|(content_type, _)| content_type != EXECUTABLE_TYPE)
            .collect();

        let chosen = PREFERRED_TYPES
            .iter()
            .find_map(|content_type| handler_for(&handlers, content_type));

        assert_eq!(chosen.as_deref(), Some("com.apple.Terminal"));
    }

    #[test]
    fn no_chosen_handler_leaves_the_system_association_in_charge() {
        let chosen = PREFERRED_TYPES
            .iter()
            .find_map(|content_type| handler_for(&[], content_type));

        assert_eq!(chosen, None);
    }

    #[test]
    fn reading_the_real_preferences_never_panics_and_yields_a_plausible_bundle_id() {
        if let Some(bundle_id) = super::preferred_terminal_bundle_id() {
            assert_ne!(bundle_id.trim(), "");
            assert!(bundle_id.contains('.'), "{bundle_id} is not a bundle id");
        }
    }
}
