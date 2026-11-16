//! Non-secret index of credential references.
//!
//! The index is the vault's record of which references exist, so that `has` and the pruning of
//! unused secrets never need the operating system store. It contains only opaque UUIDs. The
//! values live in the encrypted document. It is written atomically, with permissions 0600 in a
//! 0700 directory on Unix.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::file::{read_private, write_private_atomically};
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

    /// A missing index is an empty index. Anything else that is not a regular file, symlinks
    /// included, or that does not parse, fails closed.
    pub(crate) fn load(&self) -> Result<Vec<CredentialReference>, VaultError> {
        match read_private(&self.path, MAX_INDEX_BYTES) {
            Ok(Some(bytes)) => parse(&bytes),
            Ok(None) => Ok(Vec::new()),
            Err(_) => Err(VaultError::CorruptStore),
        }
    }

    pub(crate) fn save(&self, references: &[CredentialReference]) -> Result<(), VaultError> {
        let document = IndexDocument {
            version: INDEX_VERSION,
            references: references.iter().map(ToString::to_string).collect(),
        };
        let mut bytes = serde_json::to_vec(&document).map_err(|_| VaultError::WriteFailed)?;
        bytes.push(b'\n');
        write_private_atomically(&self.path, &bytes).map_err(|_| VaultError::WriteFailed)
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
