use std::path::{Path, PathBuf};

use despatch_application::{
    AppearanceChoice, LocalPreferences as LocalPreferencesPort, PortError, PortErrorKind,
};
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_foundation::{NSString, NSURL, NSUserDefaults};
use thiserror::Error;

const SELECTED_CONFIG_PATH_KEY: &str = "selectedConfigURL";
const APPEARANCE_KEY: &str = "appearance";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LocalPreferences {
    suite_name: Option<String>,
}

impl LocalPreferences {
    #[must_use]
    pub const fn standard() -> Self {
        Self { suite_name: None }
    }

    pub fn with_suite(suite_name: impl Into<String>) -> Result<Self, LocalPreferencesError> {
        let suite_name = suite_name.into();
        if suite_name.trim().is_empty() {
            return Err(LocalPreferencesError::InvalidSuiteName);
        }
        Ok(Self {
            suite_name: Some(suite_name),
        })
    }

    pub fn selected_config_path(&self) -> Result<Option<PathBuf>, LocalPreferencesError> {
        let defaults = self.defaults()?;
        let key = NSString::from_str(SELECTED_CONFIG_PATH_KEY);
        let Some(url) = defaults.URLForKey(&key) else {
            return Ok(None);
        };
        let path = url
            .path()
            .ok_or(LocalPreferencesError::StoredUrlHasNoFilePath)?;
        Ok(Some(PathBuf::from(path.to_string())))
    }

    pub fn set_selected_config_path(&self, path: &Path) -> Result<(), LocalPreferencesError> {
        let path = path
            .to_str()
            .ok_or(LocalPreferencesError::PathIsNotUnicode)?;
        let defaults = self.defaults()?;
        let key = NSString::from_str(SELECTED_CONFIG_PATH_KEY);
        let path = NSString::from_str(path);
        let url = NSURL::fileURLWithPath(&path);
        defaults.setURL_forKey(Some(&url), &key);
        Ok(())
    }

    pub fn clear_selected_config_path(&self) -> Result<(), LocalPreferencesError> {
        let defaults = self.defaults()?;
        defaults.removeObjectForKey(&NSString::from_str(SELECTED_CONFIG_PATH_KEY));
        Ok(())
    }

    /// The appearance is stored as its code rather than a name because `NSUserDefaults` only
    /// exposes a safe setter for numbers, and writing a preference is not worth an `unsafe` block.
    pub fn appearance(&self) -> Result<AppearanceChoice, LocalPreferencesError> {
        let defaults = self.defaults()?;
        let stored = defaults.integerForKey(&NSString::from_str(APPEARANCE_KEY));
        Ok(AppearanceChoice::from_code(
            i64::try_from(stored).unwrap_or(0),
        ))
    }

    pub fn set_appearance(&self, choice: AppearanceChoice) -> Result<(), LocalPreferencesError> {
        let defaults = self.defaults()?;
        let code = isize::try_from(choice.code()).unwrap_or(0);
        defaults.setInteger_forKey(code, &NSString::from_str(APPEARANCE_KEY));
        Ok(())
    }

    fn defaults(&self) -> Result<Retained<NSUserDefaults>, LocalPreferencesError> {
        if let Some(suite_name) = &self.suite_name {
            let suite_name = NSString::from_str(suite_name);
            NSUserDefaults::initWithSuiteName(NSUserDefaults::alloc(), Some(&suite_name))
                .ok_or(LocalPreferencesError::SuiteUnavailable)
        } else {
            Ok(NSUserDefaults::standardUserDefaults())
        }
    }

    #[cfg(test)]
    fn clear_suite(&self) -> Result<(), LocalPreferencesError> {
        let Some(suite_name) = &self.suite_name else {
            return Ok(());
        };
        self.defaults()?
            .removePersistentDomainForName(&NSString::from_str(suite_name));
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum LocalPreferencesError {
    #[error("the preference suite name must not be empty")]
    InvalidSuiteName,
    #[error("the requested NSUserDefaults suite could not be created")]
    SuiteUnavailable,
    #[error("the selected config path is not valid Unicode")]
    PathIsNotUnicode,
    #[error("the stored config URL does not contain a local file path")]
    StoredUrlHasNoFilePath,
}

impl LocalPreferencesPort for LocalPreferences {
    fn selected_config_path(&self) -> Result<Option<PathBuf>, PortError> {
        LocalPreferences::selected_config_path(self).map_err(preference_port_error)
    }

    fn set_selected_config_path(&self, path: &Path) -> Result<(), PortError> {
        LocalPreferences::set_selected_config_path(self, path).map_err(preference_port_error)
    }

    fn appearance(&self) -> Result<AppearanceChoice, PortError> {
        LocalPreferences::appearance(self).map_err(preference_port_error)
    }

    fn set_appearance(&self, choice: AppearanceChoice) -> Result<(), PortError> {
        LocalPreferences::set_appearance(self, choice).map_err(preference_port_error)
    }
}

fn preference_port_error(error: LocalPreferencesError) -> PortError {
    let kind = match error {
        LocalPreferencesError::InvalidSuiteName | LocalPreferencesError::PathIsNotUnicode => {
            PortErrorKind::InvalidData
        }
        LocalPreferencesError::SuiteUnavailable | LocalPreferencesError::StoredUrlHasNoFilePath => {
            PortErrorKind::Unavailable
        }
    };
    PortError::new(kind, error.to_string())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use despatch_domain::AppConfig;
    use uuid::Uuid;

    use super::LocalPreferences;

    #[test]
    fn selected_config_path_round_trips_in_an_isolated_suite() {
        let suite_name = format!("com.despatch.tests.{}", Uuid::now_v7());
        let preferences = LocalPreferences::with_suite(suite_name).expect("valid suite");
        let selected = Path::new("/tmp/Shared Config/despatch.json");

        assert_eq!(
            preferences
                .selected_config_path()
                .expect("read empty preferences"),
            None
        );
        preferences
            .set_selected_config_path(selected)
            .expect("store selected path");
        assert_eq!(
            preferences
                .selected_config_path()
                .expect("read selected path"),
            Some(PathBuf::from(selected))
        );

        let shared_json = serde_json::to_string(&AppConfig::default()).expect("serialize config");
        assert!(!shared_json.contains(selected.to_string_lossy().as_ref()));

        preferences
            .clear_selected_config_path()
            .expect("clear selected path");
        assert_eq!(
            preferences
                .selected_config_path()
                .expect("read cleared preferences"),
            None
        );
        preferences.clear_suite().expect("remove test suite");
    }

    #[test]
    fn empty_suite_name_is_rejected() {
        assert!(LocalPreferences::with_suite("  ").is_err());
    }
}
