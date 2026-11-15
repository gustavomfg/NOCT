//! The encrypted document that holds the provider secrets, keyed by reference.
//!
//! Its plaintext is a JSON object, `{"version":1,"secrets":{"<reference>":"<secret>"}}`. It is
//! sealed with the master key and written atomically. Only the vault reads it. The renderer never
//! receives it.
//!
//! Zeroing, stated precisely. The decrypted buffer is zeroed when it is dropped. Encoding writes
//! into a buffer of the final bound, which never grows, so no earlier copy of the plaintext is
//! left in a freed allocation. Decoding copies each secret into an exact-size string and zeroes the
//! decoded copy. The one residue is inside `serde_json`: a secret that contains characters JSON
//! escapes (quotes, backslashes, control characters) passes through the decoder's scratch buffer,
//! which that crate does not zero.

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::envelope::{self, MasterKey, OVERHEAD_BYTES};
use crate::file::{read_private, remove_private, write_private_atomically};
use crate::{CredentialReference, VaultError};

const DOCUMENT_FILE: &str = "provider-credentials.enc";
const DOCUMENT_VERSION: u8 = 1;
/// Upper bound for the encoded plaintext, with all secrets together. A provider key is a few
/// hundred characters, so this bound is far above real use. It stops a damaged or hostile file
/// from being read whole, and it stops one write from producing an unbounded file.
pub(crate) const MAX_DOCUMENT_BYTES: usize = 1_048_576;
const MAX_SEALED_BYTES: u64 = (MAX_DOCUMENT_BYTES + OVERHEAD_BYTES) as u64;

#[derive(Serialize)]
struct EncodedDocument<'a> {
    version: u8,
    secrets: BTreeMap<String, &'a str>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecodedDocument {
    version: u8,
    secrets: BTreeMap<String, String>,
}

/// The secrets of every provider, in memory. Its `Debug` output shows only how many there are.
#[derive(Default)]
pub(crate) struct SecretDocument {
    secrets: BTreeMap<CredentialReference, SecretString>,
}

impl SecretDocument {
    pub(crate) fn insert(&mut self, reference: CredentialReference, secret: SecretString) {
        self.secrets.insert(reference, secret);
    }

    pub(crate) fn contains(&self, reference: &CredentialReference) -> bool {
        self.secrets.contains_key(reference)
    }

    pub(crate) fn get(&self, reference: &CredentialReference) -> Option<SecretString> {
        self.secrets
            .get(reference)
            .map(|secret| SecretString::from(secret.expose_secret().to_owned()))
    }

    /// Removes the secret, and reports whether one was there.
    pub(crate) fn remove(&mut self, reference: &CredentialReference) -> bool {
        self.secrets.remove(reference).is_some()
    }

    /// Keeps only the secrets that `keep` names, and reports whether any was dropped.
    pub(crate) fn retain(&mut self, keep: &HashSet<CredentialReference>) -> bool {
        let before = self.secrets.len();
        self.secrets.retain(|reference, _| keep.contains(reference));
        self.secrets.len() != before
    }

    fn encode(&self) -> Result<Zeroizing<Vec<u8>>, VaultError> {
        let document = EncodedDocument {
            version: DOCUMENT_VERSION,
            secrets: self
                .secrets
                .iter()
                .map(|(reference, secret)| (reference.to_string(), secret.expose_secret()))
                .collect(),
        };
        let mut bytes = Zeroizing::new(Vec::with_capacity(MAX_DOCUMENT_BYTES));
        serde_json::to_writer(BoundedWriter(&mut bytes), &document)
            .map_err(|_| VaultError::InvalidSecret)?;
        Ok(bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self, VaultError> {
        let document: DecodedDocument =
            serde_json::from_slice(bytes).map_err(|_| VaultError::CorruptStore)?;
        if document.version != DOCUMENT_VERSION {
            return Err(VaultError::CorruptStore);
        }
        let mut secrets = BTreeMap::new();
        let mut corrupt = false;
        for (reference, mut secret) in document.secrets {
            match CredentialReference::parse(&reference) {
                Ok(reference) => {
                    secrets.insert(reference, take_secret(&mut secret));
                }
                Err(_) => {
                    corrupt = true;
                    secret.zeroize();
                }
            }
        }
        if corrupt {
            return Err(VaultError::CorruptStore);
        }
        Ok(Self { secrets })
    }
}

impl fmt::Debug for SecretDocument {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretDocument")
            .field("secrets", &self.secrets.len())
            .finish()
    }
}

/// Writes into a buffer that is never grown. A write past the bound fails. The buffer is created
/// with the bound as its capacity, so no write reallocates it.
struct BoundedWriter<'a>(&'a mut Vec<u8>);

impl Write for BoundedWriter<'_> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.0.len() + buffer.len() > MAX_DOCUMENT_BYTES {
            return Err(io::Error::other("the document exceeds its bound"));
        }
        self.0.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Copies a decoded secret into an exact-size string, and zeroes the decoded copy.
fn take_secret(source: &mut String) -> SecretString {
    let secret = SecretString::from(source.as_str().to_owned());
    source.zeroize();
    secret
}

/// The sealed document on disk.
pub(crate) struct DocumentFile {
    path: PathBuf,
}

impl DocumentFile {
    pub(crate) fn new(directory: &Path) -> Self {
        Self {
            path: directory.join(DOCUMENT_FILE),
        }
    }

    /// Reports whether a document exists, without opening it. A file that cannot even be
    /// inspected is corrupt.
    pub(crate) fn exists(&self) -> Result<bool, VaultError> {
        match std::fs::symlink_metadata(&self.path) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(_) => Err(VaultError::CorruptStore),
        }
    }

    /// Opens and decrypts the document. A missing file is `Ok(None)`.
    pub(crate) fn load(&self, key: &MasterKey) -> Result<Option<SecretDocument>, VaultError> {
        let sealed = match read_private(&self.path, MAX_SEALED_BYTES) {
            Ok(Some(sealed)) => sealed,
            Ok(None) => return Ok(None),
            Err(_) => return Err(VaultError::CorruptStore),
        };
        let plaintext = envelope::open(key, &sealed)?;
        SecretDocument::decode(&plaintext).map(Some)
    }

    /// Seals the document and replaces the file atomically.
    pub(crate) fn save(
        &self,
        key: &MasterKey,
        document: &SecretDocument,
    ) -> Result<(), VaultError> {
        let plaintext = document.encode()?;
        let sealed = envelope::seal(key, &plaintext)?;
        write_private_atomically(&self.path, &sealed).map_err(|_| VaultError::WriteFailed)
    }

    /// Deletes the document. A missing file is not an error.
    pub(crate) fn remove(&self) -> Result<(), VaultError> {
        remove_private(&self.path).map_err(|_| VaultError::WriteFailed)
    }
}
