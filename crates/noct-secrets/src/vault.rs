use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use keyring_core::{CredentialStore, Entry, Error as KeyringError};
use secrecy::{ExposeSecret, SecretString};
use zeroize::Zeroizing;

use crate::document::{DocumentFile, MAX_DOCUMENT_BYTES, SecretDocument};
use crate::envelope::MasterKey;
use crate::index::CredentialIndex;
use crate::{CredentialReference, VaultError, default_store};

/// Service name under which the master key is stored in the operating system.
pub const SERVICE_NAME: &str = "com.nocturne.codex.provider-credentials";

/// Account name of the master key. It is the only entry the vault writes to the operating system;
/// every secret lives in the encrypted document.
pub const MASTER_KEY_ACCOUNT: &str = "master-key-v1";

/// Keeps provider secrets in an encrypted file and their master key in the operating system
/// credential store. The non-secret index lists the references that exist.
///
/// A reference is configured only when the index names it and the document holds its secret. Any
/// other state is treated as not configured: the index without the secret, the secret without an
/// index entry, or a missing document. Such a reference resolves to nothing, and `prune` removes
/// what it finds in the document that the index does not name.
///
/// Every operation holds one lock, so the index, the document and the key never change under a
/// concurrent caller of this process.
pub struct ProviderCredentialVault {
    store: Arc<CredentialStore>,
    service: String,
    index: CredentialIndex,
    document: DocumentFile,
    guard: Mutex<()>,
}

impl ProviderCredentialVault {
    /// Opens the vault on the operating system store. `directory` holds the index and the
    /// encrypted document, and is created with mode 0700 on Unix if it does not exist.
    pub fn open(directory: impl Into<PathBuf>) -> Result<Self, VaultError> {
        Ok(Self::with_store(default_store()?, directory))
    }

    /// Opens the vault on the operating system store under another service name. The operating
    /// system test uses it, so that it never reads or removes the master key of the installed
    /// application.
    pub fn open_in_service(
        directory: impl Into<PathBuf>,
        service: &str,
    ) -> Result<Self, VaultError> {
        Ok(Self::build(default_store()?, service, directory))
    }

    /// Opens the vault on an explicit store. Production code uses [`Self::open`]; tests use an
    /// in-memory store.
    pub fn with_store(store: Arc<CredentialStore>, directory: impl Into<PathBuf>) -> Self {
        Self::build(store, SERVICE_NAME, directory)
    }

    fn build(store: Arc<CredentialStore>, service: &str, directory: impl Into<PathBuf>) -> Self {
        let directory = directory.into();
        Self {
            store,
            service: service.to_string(),
            index: CredentialIndex::new(&directory),
            document: DocumentFile::new(&directory),
            guard: Mutex::new(()),
        }
    }

    /// Stores a new secret and returns its reference.
    ///
    /// Order of writes: the secret goes into the document first, then the reference into the
    /// index. If the index write fails, the secret is withdrawn from the document again. A crash
    /// between the two writes leaves a secret that no reference names. It is never configured, and
    /// `prune` removes it. A reference is never indexed without its secret.
    ///
    /// A new master key is created only for an empty vault, and only when the store is available.
    pub fn create(&self, secret: &SecretString) -> Result<CredentialReference, VaultError> {
        validate_secret(secret)?;
        let _guard = self.lock();
        let previous = self.index.load()?;
        let allow_new_key = previous.is_empty() && !self.document.exists()?;
        let reference = CredentialReference::generate();
        let key = self.master_key(allow_new_key)?;
        let mut document = self.document.load(&key)?.unwrap_or_default();
        document.insert(reference, copy_secret(secret));
        self.document.save(&key, &document)?;
        let mut references = previous;
        references.push(reference);
        if let Err(error) = self.index.save(&references) {
            document.remove(&reference);
            // Best effort. If this write also fails, the secret stays unnamed and is pruned later.
            let _ = self.document.save(&key, &document);
            return Err(error);
        }
        Ok(reference)
    }

    /// Replaces the secret behind an existing reference. Only the document changes, so the index
    /// is not written.
    pub fn replace(&self, reference: &str, secret: &SecretString) -> Result<(), VaultError> {
        let reference = CredentialReference::parse(reference)?;
        validate_secret(secret)?;
        let _guard = self.lock();
        if !self.index.load()?.contains(&reference) {
            return Err(VaultError::InvalidReference);
        }
        let key = self.master_key(false)?;
        let mut document = self.document.load(&key)?.unwrap_or_default();
        document.insert(reference, copy_secret(secret));
        self.document.save(&key, &document)
    }

    /// Returns the secret of a configured reference, or `None` when it is not configured. The
    /// secret is wrapped before it leaves this function.
    pub fn resolve(&self, reference: &str) -> Result<Option<SecretString>, VaultError> {
        let reference = CredentialReference::parse(reference)?;
        let _guard = self.lock();
        Ok(self
            .indexed_document(&reference)?
            .and_then(|document| document.get(&reference)))
    }

    /// Reports whether a reference is configured. A reference the index does not name, or a
    /// missing document, answers `false` without touching the operating system store. Otherwise
    /// the document is read, which needs the store to be unlocked.
    pub fn has(&self, reference: &str) -> Result<bool, VaultError> {
        let reference = CredentialReference::parse(reference)?;
        let _guard = self.lock();
        Ok(self
            .indexed_document(&reference)?
            .is_some_and(|document| document.contains(&reference)))
    }

    /// Reports whether the index names a reference and the document exists. This is the status the
    /// interface shows. It reads only the index file and the presence of the document, never the
    /// operating system store, so it answers while the store is locked and never prompts the user
    /// to unlock it. `true` does not prove that the document holds the secret; [`Self::has`] does.
    /// The two differ only after a crash between the writes of `create`, or after an edit made
    /// outside NOCT.
    pub fn listed(&self, reference: &str) -> Result<bool, VaultError> {
        let reference = CredentialReference::parse(reference)?;
        let _guard = self.lock();
        Ok(self.index.load()?.contains(&reference) && self.document.exists()?)
    }

    /// Removes a secret and its index entry. The secret leaves the document first, so a failure
    /// keeps the reference indexed and the operation can be retried.
    pub fn delete(&self, reference: &str) -> Result<bool, VaultError> {
        let reference = CredentialReference::parse(reference)?;
        let _guard = self.lock();
        let mut references = self.index.load()?;
        let Some(position) = references
            .iter()
            .position(|candidate| *candidate == reference)
        else {
            return Ok(false);
        };
        self.edit_document(|document| document.remove(&reference))?;
        references.remove(position);
        self.index.save(&references)?;
        Ok(true)
    }

    /// Removes every secret whose reference is not in `allowed`, and every secret the index does
    /// not name, and returns how many references left the index. The document is rewritten first,
    /// then the index. The document is read even when nothing is stale, so that orphans are found;
    /// on a locked store this fails without changing anything.
    pub fn prune(&self, allowed: &[&str]) -> Result<usize, VaultError> {
        let allowed = allowed
            .iter()
            .map(|value| CredentialReference::parse(value))
            .collect::<Result<HashSet<_>, _>>()?;
        let _guard = self.lock();
        let (kept, stale): (Vec<_>, Vec<_>) = self
            .index
            .load()?
            .into_iter()
            .partition(|reference| allowed.contains(reference));
        let keep: HashSet<CredentialReference> = kept.iter().copied().collect();
        self.edit_document(|document| document.retain(&keep))?;
        if !stale.is_empty() {
            self.index.save(&kept)?;
        }
        Ok(stale.len())
    }

    /// Clears the vault: the document, the index and the master key. It is the only way back after
    /// a lost key, and the caller must ask the user first.
    ///
    /// The store is checked first. An entry that is missing is the case a reset exists for. A
    /// locked or unavailable store fails with `SecureStorageUnavailable` and nothing changes; the
    /// caller must not offer a reset then, and no key is created.
    pub fn reset(&self) -> Result<(), VaultError> {
        let _guard = self.lock();
        let entry = self.entry(MASTER_KEY_ACCOUNT).map_err(read_error)?;
        let key_present = match entry.get_secret() {
            Ok(bytes) => {
                let _erased = Zeroizing::new(bytes);
                true
            }
            Err(KeyringError::NoEntry) => false,
            Err(error) => return Err(read_error(error)),
        };
        self.document.remove()?;
        self.index.save(&[])?;
        if key_present {
            match entry.delete_credential() {
                Ok(()) | Err(KeyringError::NoEntry) => {}
                Err(error) => return Err(write_error(error)),
            }
        }
        Ok(())
    }

    fn lock(&self) -> MutexGuard<'_, ()> {
        self.guard.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn entry(&self, account: &str) -> keyring_core::Result<Entry> {
        self.store.build(&self.service, account, None)
    }

    /// The document, when the index names the reference and the document exists. Otherwise the
    /// reference is not configured, and the operating system store is not touched.
    fn indexed_document(
        &self,
        reference: &CredentialReference,
    ) -> Result<Option<SecretDocument>, VaultError> {
        if !self.index.load()?.contains(reference) || !self.document.exists()? {
            return Ok(None);
        }
        let key = self.master_key(false)?;
        self.document.load(&key)
    }

    /// Reads the master key from the operating system store. A key is created only when
    /// `allow_new` is set, which is when the vault holds nothing yet. When secrets exist and the
    /// key is gone, the vault reports it. A new key could not open the old secrets, and creating
    /// one would hide the loss.
    fn master_key(&self, allow_new: bool) -> Result<MasterKey, VaultError> {
        let entry = self.entry(MASTER_KEY_ACCOUNT).map_err(read_error)?;
        match entry.get_secret() {
            Ok(bytes) => MasterKey::from_stored(Zeroizing::new(bytes)),
            Err(KeyringError::NoEntry) if allow_new => {
                let key = MasterKey::generate()?;
                entry.set_secret(&key.expose()[..]).map_err(write_error)?;
                Ok(key)
            }
            Err(KeyringError::NoEntry) => Err(VaultError::MasterKeyMissing),
            Err(error) => Err(read_error(error)),
        }
    }

    /// Rewrites the document when `edit` changes it. Nothing is read or written when the document
    /// does not exist, and nothing is written when `edit` changes nothing.
    fn edit_document(
        &self,
        edit: impl FnOnce(&mut SecretDocument) -> bool,
    ) -> Result<(), VaultError> {
        if !self.document.exists()? {
            return Ok(());
        }
        let key = self.master_key(false)?;
        let Some(mut document) = self.document.load(&key)? else {
            return Ok(());
        };
        if edit(&mut document) {
            self.document.save(&key, &document)?;
        }
        Ok(())
    }
}

/// A secret must not be empty, and one larger than the whole document can never be stored. The
/// bound is enforced again while the document is encoded, because escaping can grow the bytes.
fn validate_secret(secret: &SecretString) -> Result<(), VaultError> {
    let length = secret.expose_secret().len();
    if length == 0 || length > MAX_DOCUMENT_BYTES {
        return Err(VaultError::InvalidSecret);
    }
    Ok(())
}

fn copy_secret(secret: &SecretString) -> SecretString {
    SecretString::from(secret.expose_secret().to_owned())
}

/// A store that is locked or refuses access is reported as unavailable, so the user is asked to
/// unlock it instead of seeing a generic failure.
fn write_error(error: KeyringError) -> VaultError {
    match error {
        KeyringError::NoStorageAccess(_) => VaultError::SecureStorageUnavailable,
        _ => VaultError::WriteFailed,
    }
}

fn read_error(error: KeyringError) -> VaultError {
    match error {
        KeyringError::NoStorageAccess(_) => VaultError::SecureStorageUnavailable,
        _ => VaultError::ReadFailed,
    }
}
