use std::path::PathBuf;

use directories::BaseDirs;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum DefaultPathError {
    #[error("the current user's home and application-support directories are unavailable")]
    UserDirectoriesUnavailable,
}

pub fn default_config_path() -> Result<PathBuf, DefaultPathError> {
    let base = BaseDirs::new().ok_or(DefaultPathError::UserDirectoriesUnavailable)?;
    Ok(base.data_dir().join("Despatch").join("config.json"))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::default_config_path;

    #[test]
    fn default_config_lives_in_despatch_application_support() {
        let path = default_config_path().expect("user directories are available");

        assert!(path.is_absolute());
        assert!(path.ends_with(Path::new("Despatch/config.json")));
        #[cfg(target_os = "macos")]
        assert!(
            path.to_string_lossy()
                .contains("Library/Application Support")
        );
    }
}
