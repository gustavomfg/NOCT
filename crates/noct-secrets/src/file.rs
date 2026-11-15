//! Private file primitives shared by the vault files: a no-follow bounded read, and an atomic
//! write with mode 0600 in a directory created with mode 0700 on Unix.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

use uuid::Uuid;

/// Reads a regular file of at most `limit` bytes. A missing file is `Ok(None)`. A symlink, a
/// directory, or a file over the limit is an error, so the caller fails closed.
pub(crate) fn read_private(path: &Path, limit: u64) -> io::Result<Option<Vec<u8>>> {
    let file = match open_no_follow(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("the vault file is not a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::other("the vault file is too large"));
    }
    Ok(Some(bytes))
}

/// Replaces `path` with `bytes`. The bytes go to a temporary file in the same directory, which is
/// synced and renamed over the target. The directory is created private if it does not exist.
pub(crate) fn write_private_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("the vault file has no directory"))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::other("the vault file has no name"))?
        .to_string_lossy();
    create_private_directory(directory)?;
    let temporary = directory.join(format!(".{file_name}.{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut file = create_private_file(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        sync_directory(directory);
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Removes `path`, and makes the removal durable on Unix. A missing file is not an error. A
/// symbolic link is removed itself, never its target.
pub(crate) fn remove_private(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => {
            if let Some(directory) = path.parent() {
                sync_directory(directory);
            }
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn open_no_follow(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(not(unix))]
fn open_no_follow(path: &Path) -> io::Result<File> {
    OpenOptions::new().read(true).open(path)
}

#[cfg(unix)]
fn create_private_directory(directory: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)
}

#[cfg(not(unix))]
fn create_private_directory(directory: &Path) -> io::Result<()> {
    std::fs::create_dir_all(directory)
}

#[cfg(unix)]
fn create_private_file(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn create_private_file(path: &Path) -> io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

/// Makes the rename durable on Unix. Other platforms cannot fsync a directory, so the step is
/// skipped there.
#[cfg(unix)]
fn sync_directory(directory: &Path) {
    if let Ok(handle) = File::open(directory) {
        let _ = handle.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Path) {}
