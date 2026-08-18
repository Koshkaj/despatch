use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::PathBuf;

use tempfile::Builder;

pub(crate) fn create(contents: &str) -> io::Result<PathBuf> {
    let mut file = Builder::new()
        .prefix("despatch-")
        .suffix(".command")
        .tempfile()?;
    file.as_file()
        .set_permissions(fs::Permissions::from_mode(0o700))?;
    let metadata = file.as_file().metadata()?;
    if metadata.uid() != rustix::process::getuid().as_raw() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "temporary command file is not owned by the current user",
        ));
    }
    file.write_all(contents.as_bytes())?;
    file.flush()?;
    file.as_file().sync_all()?;
    file.keep()
        .map(|(_, path)| path)
        .map_err(|error| error.error)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use super::create;

    #[test]
    fn command_file_is_private_executable_and_has_a_generated_name() {
        let path = create("#!/bin/sh\nexit 0\n").expect("create command file");
        let metadata = fs::metadata(&path).expect("script metadata");

        assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
        let name = path.file_name().expect("filename").to_string_lossy();
        assert!(name.starts_with("despatch-"));
        assert!(name.ends_with(".command"));
        assert!(!name.contains('/'));
        fs::remove_file(path).expect("clean up command file");
    }
}
