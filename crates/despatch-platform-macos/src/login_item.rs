//! Opening Despatch at login, through `SMAppService` (macOS 13 and later).
//!
//! A menu-bar app has no Dock icon to remind anyone it exists, so without this it has to be
//! started by hand after every login. The `ServiceManagement` framework is called through the
//! Objective-C runtime rather than a binding crate: it is three messages to one class, and the
//! class is looked up at run time so a missing one is an error rather than a launch failure.

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2_foundation::{NSError, NSInteger};

#[link(name = "ServiceManagement", kind = "framework")]
unsafe extern "C" {}

/// Whether Despatch opens at login, as the system sees it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoginItemStatus {
    /// Not registered.
    Off,
    /// Registered and allowed: Despatch opens at login.
    On,
    /// Registered, but the user has to allow it in System Settings → General → Login Items.
    NeedsApproval,
    /// The system cannot find an app to register — the binary is not running from a bundle.
    Unavailable,
}

impl LoginItemStatus {
    const fn from_code(code: NSInteger) -> Self {
        // `SMAppServiceStatus`: notRegistered, enabled, requiresApproval, notFound.
        match code {
            1 => Self::On,
            2 => Self::NeedsApproval,
            3 => Self::Unavailable,
            _ => Self::Off,
        }
    }
}

fn main_app_service() -> Option<Retained<AnyObject>> {
    let class = AnyClass::get(c"SMAppService")?;
    // SAFETY: `+[SMAppService mainAppService]` takes no arguments and returns a non-null,
    // autoreleased service object for the running app, which `msg_send!` retains.
    unsafe { msg_send![class, mainAppService] }
}

/// Reads whether Despatch opens at login.
#[must_use]
pub fn login_item_status() -> LoginItemStatus {
    let Some(service) = main_app_service() else {
        return LoginItemStatus::Unavailable;
    };
    // SAFETY: `status` is a read-only `SMAppServiceStatus` (an `NSInteger`) property of the live
    // service object obtained above.
    let code: NSInteger = unsafe { msg_send![&service, status] };
    LoginItemStatus::from_code(code)
}

/// Turns opening at login on or off, and returns the status the system reports afterwards.
pub fn set_open_at_login(enabled: bool) -> Result<LoginItemStatus, String> {
    let service = main_app_service().ok_or("this macOS has no SMAppService")?;
    // SAFETY: `registerAndReturnError:` and `unregisterAndReturnError:` take one `NSError **` out
    // parameter and return a `BOOL`, which `msg_send!`'s `_` error form maps to a `Result`. The
    // service object is live for the duration of the call.
    let outcome: Result<(), Retained<NSError>> = unsafe {
        if enabled {
            msg_send![&service, registerAndReturnError: _]
        } else {
            msg_send![&service, unregisterAndReturnError: _]
        }
    };
    outcome.map_err(|error| error.localizedDescription().to_string())?;
    Ok(login_item_status())
}

#[cfg(test)]
mod tests {
    use super::{LoginItemStatus, login_item_status};

    #[test]
    fn status_codes_map_to_what_the_user_is_told() {
        assert_eq!(LoginItemStatus::from_code(0), LoginItemStatus::Off);
        assert_eq!(LoginItemStatus::from_code(1), LoginItemStatus::On);
        assert_eq!(
            LoginItemStatus::from_code(2),
            LoginItemStatus::NeedsApproval
        );
        assert_eq!(LoginItemStatus::from_code(3), LoginItemStatus::Unavailable);
    }

    #[test]
    fn the_status_of_a_binary_outside_a_bundle_can_be_read_without_changing_anything() {
        // A test binary is not a registered app, so this only reads; it never registers.
        let _ = login_item_status();
    }
}
