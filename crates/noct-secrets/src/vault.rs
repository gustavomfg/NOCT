use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use keyring_core::{CredentialStore, Entry, Error as KeyringError};
use noct_core::limits::provider_configuration::CREDENTIAL_CHARACTERS;
use secrecy::{ExposeSecret, SecretString};

use crate::index::CredentialIndex;
use crate::{CredentialReference, VaultError, default_store};

/// Service name under which every provider secret is stored in the operating system.
pub const SERVICE_NAME: &str = "com.nocturne.codex.provider-credentials";

/// Keeps provider secrets in the operating system credential store and tracks their
/// references in a non-secret index.
///
/// Every operation holds one lock, so the index and the store never change under a
/// concurrent caller of this process.
pub struct ProviderCredentialVault {
    store: Arc<CredentialStore>,
    index: CredentialIndex,
    guard: Mutex<()>,
}

impl ProviderCredentialVault {
    /// Opens the vault on the operating system store. `directory` holds the index and
    /// is created with mode 0700 on Unix if it does not exist.
    pub fn open(directory: impl Into<PathBuf>) -> Result<Self, VaultError> {
        Ok(Self::with_store(default_store()?, directory))
    }

    /// Opens the vault on an explicit store. Production code uses [`Self::open`]; tests
    /// use an in-memory store.
    pub fn with_store(store: Arc<CredentialStore>, directory: impl Into<PathBuf>) -> Self {
        let directory = directory.into();
        Self {
            store,
            index: CredentialIndex::new(&directory),
            guard: Mutex::new(()),
        }
    }

    /// Stores a new secret and returns its reference. If the index cannot be written,
    /// the secret is removed again so that no unindexed secret is left behind.
    pub fn create(&self, secret: &SecretString) -> Result<CredentialReference, VaultError> {
        validate_secret(secret)?;
        let _guard = self.lock();
        let mut references = self.index.load()?;
        let reference = CredentialReference::generate();
        self.entry(&reference)
            .and_then(|entry| entry.set_password(secret.expose_secret()))
            .map_err(write_error)?;
        references.push(reference);
        if let Err(error) = self.index.save(&references) {
            let _ = self
                .entry(&reference)
                .and_then(|entry| entry.delete_credential());
            return Err(error);
        }
        Ok(reference)
    }

    /// Replaces the secret behind an existing reference.
    pub fn replace(&self, reference: &str, secret: &SecretString) -> Result<(), VaultError> {
        let reference = CredentialReference::parse(reference)?;
        validate_secret(secret)?;
        let _guard = self.lock();
        if !self.index.load()?.contains(&reference) {
            return Err(VaultError::InvalidReference);
        }
        self.entry(&reference)
            .and_then(|entry| entry.set_password(secret.expose_secret()))
            .map_err(write_error)
    }

    /// Returns the secret, or `None` when the reference is unknown or its secret has
    /// been removed outside NOCT. The secret is wrapped before it leaves this function.
    pub fn resolve(&self, reference: &str) -> Result<Option<SecretString>, VaultError> {
        let reference = CredentialReference::parse(reference)?;
        let _guard = self.lock();
        if !self.index.load()?.contains(&reference) {
            return Ok(None);
        }
        match self
            .entry(&reference)
            .and_then(|entry| entry.get_password())
        {
            Ok(secret) => Ok(Some(SecretString::from(secret))),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(error) => Err(read_error(error)),
        }
    }

    /// Reports whether the reference is indexed. It does not read the secret, so it
    /// never triggers an operating system prompt.
    pub fn has(&self, reference: &str) -> Result<bool, VaultError> {
        let reference = CredentialReference::parse(reference)?;
        let _guard = self.lock();
        Ok(self.index.load()?.contains(&reference))
    }

    /// Removes a secret and its index entry. The secret goes first, so a failure keeps
    /// the reference indexed and the operation can be retried.
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
        self.remove_secret(&reference)?;
        references.remove(position);
        self.index.save(&references)?;
        Ok(true)
    }

    /// Removes every secret whose reference is not in `allowed`, and returns how many
    /// were removed. After a failure, the remaining stale secrets stay indexed.
    pub fn prune(&self, allowed: &[&str]) -> Result<usize, VaultError> {
        let allowed = allowed
            .iter()
            .map(|value| CredentialReference::parse(value))
            .collect::<Result<HashSet<_>, _>>()?;
        let _guard = self.lock();
        let references = self.index.load()?;
        let mut kept = Vec::with_capacity(references.len());
        let mut removed = 0;
        let mut failure = None;
        for reference in references {
            if failure.is_some() || allowed.contains(&reference) {
                kept.push(reference);
                continue;
            }
            match self.remove_secret(&reference) {
                Ok(()) => removed += 1,
                Err(error) => {
                    failure = Some(error);
                    kept.push(reference);
                }
            }
        }
        if removed > 0 {
            self.index.save(&kept)?;
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(removed),
        }
    }

    fn lock(&self) -> MutexGuard<'_, ()> {
        self.guard.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn entry(&self, reference: &CredentialReference) -> keyring_core::Result<Entry> {
        self.store.build(SERVICE_NAME, &reference.to_string(), None)
    }

    fn remove_secret(&self, reference: &CredentialReference) -> Result<(), VaultError> {
        match self
            .entry(reference)
            .and_then(|entry| entry.delete_credential())
        {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(error) => Err(write_error(error)),
        }
    }
}

/// Same rule as the TypeScript vault: the limit counts UTF-16 units, as JavaScript does.
fn validate_secret(secret: &SecretString) -> Result<(), VaultError> {
    let length = secret.expose_secret().encode_utf16().count();
    if length == 0 || length as u64 > CREDENTIAL_CHARACTERS {
        return Err(VaultError::InvalidSecret);
    }
    Ok(())
}

/// A store that is locked or refuses access is reported as unavailable, so the user is
/// asked to unlock it instead of seeing a generic failure.
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
