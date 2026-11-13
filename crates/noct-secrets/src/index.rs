//! Non-secret index of credential references.
//!
//! The index lets the vault find secrets that no provider uses any more. It contains
//! only opaque UUIDs. It is written atomically, with permissions 0600 in a 0700
//! directory on Unix.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{CredentialReference, VaultError};

const INDEX_FILE: &str = "provider-credential-index.json";
const INDEX_VERSION: u8 = 1;
/// Upper bound for the index. Each reference is 36 characters, so this is far above any
/// realistic number of providers.
const MAX_INDEX_BYTES: u64 = 1_048_576;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IndexDocument {
    version: u8,
    references: Vec<String>,
}

pub(crate) struct CredentialIndex {
    path: PathBuf,
}

impl CredentialIndex {
    pub(crate) fn new(directory: &Path) -> Self {
        Self {
            path: directory.join(INDEX_FILE),
        }
    }

    /// A missing index is an empty index. Anything else that is not a regular file,
    /// symlinks included, or that does not parse, fails closed.
    pub(crate) fn load(&self) -> Result<Vec<CredentialReference>, VaultError> {
        let file = match open_index(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(_) => return Err(VaultError::CorruptStore),
        };
        let bytes = read_bounded(file).map_err(|_| VaultError::CorruptStore)?;
        parse(&bytes)
    }

    pub(crate) fn save(&self, references: &[CredentialReference]) -> Result<(), VaultError> {
        let document = IndexDocument {
            version: INDEX_VERSION,
            references: references.iter().map(ToString::to_string).collect(),
        };
        let mut bytes = serde_json::to_vec(&document).map_err(|_| VaultError::WriteFailed)?;
        bytes.push(b'\n');
        let directory = self.path.parent().ok_or(VaultError::WriteFailed)?;
        write_atomically(directory, &self.path, &bytes).map_err(|_| VaultError::WriteFailed)
    }
}

fn parse(bytes: &[u8]) -> Result<Vec<CredentialReference>, VaultError> {
    let document: IndexDocument =
        serde_json::from_slice(bytes).map_err(|_| VaultError::CorruptStore)?;
    if document.version != INDEX_VERSION {
        return Err(VaultError::CorruptStore);
    }
    let mut seen = HashSet::with_capacity(document.references.len());
    let mut references = Vec::with_capacity(document.references.len());
    for value in &document.references {
        let reference = CredentialReference::parse(value).map_err(|_| VaultError::CorruptStore)?;
        if !seen.insert(reference) {
            return Err(VaultError::CorruptStore);
        }
        references.push(reference);
    }
    Ok(references)
}

fn read_bounded(file: File) -> io::Result<Vec<u8>> {
    if !file.metadata()?.is_file() {
        return Err(io::Error::other(
            "the credential index is not a regular file",
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_INDEX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_INDEX_BYTES {
        return Err(io::Error::other("the credential index is too large"));
    }
    Ok(bytes)
}

fn write_atomically(directory: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    create_private_directory(directory)?;
    let temporary = directory.join(format!(".{INDEX_FILE}.{}.tmp", Uuid::new_v4()));
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

#[cfg(unix)]
fn open_index(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(not(unix))]
fn open_index(path: &Path) -> io::Result<File> {
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

/// Makes the rename durable on Unix. Other platforms cannot fsync a directory, so the
/// step is skipped there.
#[cfg(unix)]
fn sync_directory(directory: &Path) {
    if let Ok(handle) = File::open(directory) {
        let _ = handle.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Path) {}
