use std::fs::{self, File};
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use tempfile::NamedTempFile;

pub(crate) fn stage_sibling(path: &Path, contents: &[u8]) -> io::Result<NamedTempFile> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let mut staged = NamedTempFile::new_in(parent)?;
    staged
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    staged.write_all(contents)?;
    staged.flush()?;
    staged.as_file().sync_all()?;
    Ok(staged)
}

pub(crate) fn commit_sibling(staged: NamedTempFile, path: &Path) -> io::Result<()> {
    staged.persist(path).map_err(|error| error.error)?;
    sync_parent(path)
}

pub(crate) fn commit_sibling_noclobber(staged: NamedTempFile, path: &Path) -> io::Result<()> {
    staged
        .persist_noclobber(path)
        .map_err(|error| error.error)?;
    sync_parent(path)
}

fn sync_parent(path: &Path) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    File::open(parent)?.sync_all()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::stage_sibling;

    #[test]
    fn dropping_an_uncommitted_stage_preserves_the_previous_file() {
        let directory = tempdir().expect("temporary directory");
        let path = directory.path().join("config.json");
        fs::write(&path, b"previous").expect("write initial file");

        let staged = stage_sibling(&path, b"replacement").expect("stage replacement");
        drop(staged);

        assert_eq!(fs::read(path).expect("read original"), b"previous");
    }
}
