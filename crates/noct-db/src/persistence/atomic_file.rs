//! Replaces a regular file without exposing a partly written payload. Port of
//! `electron/persistence/AtomicFile.ts`.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use uuid::Uuid;

/// Writes `content` to `path` through a temporary file in the same directory. The temporary
/// file is synced before the rename, so an interruption cannot leave the official file empty
/// or truncated. On failure the original file is kept and the temporary file is removed.
/// `mode` is the permission of the new file, where the platform has POSIX modes.
pub fn write_atomic_file(path: &Path, content: &[u8], mode: u32) -> io::Result<()> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(format!(".tmp-{}-{}", std::process::id(), Uuid::new_v4()));
    let temporary = PathBuf::from(temporary);
    let result = write_and_replace(path, &temporary, content, mode);
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn write_and_replace(path: &Path, temporary: &Path, content: &[u8], mode: u32) -> io::Result<()> {
    let mut file = create_new_file(temporary, mode)?;
    file.write_all(content)?;
    file.sync_all()?;
    drop(file);
    set_mode(temporary, mode)?;
    fs::rename(temporary, path)
}

/// Creates a file that must not exist yet, with `mode` before the umask applies.
fn create_new_file(path: &Path, mode: u32) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode);
    }
    #[cfg(not(unix))]
    let _ = mode;
    options.open(path)
}

/// Applies `mode` exactly, which the umask may have narrowed when the file was created.
fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        Ok(())
    }
}
