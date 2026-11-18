//! Provider keys in the operating system credential store, through the vault of `noct-secrets`.
//!
//! The interface sends a key once and reads back only whether one is configured. The reference that names a key is not
//! secret. It is kept in the settings table under `OPENROUTER_REFERENCE`, a key with the `maintenance.` prefix. Both the
//! shell and the Electron app hide that prefix from their settings reads, so the interface never sees the reference, and
//! the Electron app does not see it either while the two share the database. The store lock is held while the settings
//! are read or written, never while the operating system store is called, and the vault is opened only when a call needs
//! it.

use std::collections::BTreeMap;
use std::sync::Mutex;

use noct_core::ai::provider_key::{ProviderKeyInput, ProviderKeyProvider, ProviderKeyStatus};
use noct_db::store::Store;
use noct_secrets::{CredentialReference, ProviderCredentialVault, SecretString, VaultError};

use crate::error::CommandError;

/// The folder of the vault, inside the data directory.
pub const VAULT_DIRECTORY: &str = "provider-credentials";

/// The settings key that names the reference of the OpenRouter key. Empty means no key.
pub const OPENROUTER_REFERENCE: &str = "maintenance.secret.openrouter.reference";

/// The vault operations the key commands use. `ProviderCredentialVault` implements it. Tests implement it in memory,
/// so that no test writes to the operating system store.
pub trait KeyVault {
    fn create(&self, secret: &SecretString) -> Result<CredentialReference, VaultError>;
    fn replace(&self, reference: &str, secret: &SecretString) -> Result<(), VaultError>;
    fn has(&self, reference: &str) -> Result<bool, VaultError>;
    fn delete(&self, reference: &str) -> Result<bool, VaultError>;
}

impl KeyVault for ProviderCredentialVault {
    fn create(&self, secret: &SecretString) -> Result<CredentialReference, VaultError> {
        ProviderCredentialVault::create(self, secret)
    }

    fn replace(&self, reference: &str, secret: &SecretString) -> Result<(), VaultError> {
        ProviderCredentialVault::replace(self, reference, secret)
    }

    fn has(&self, reference: &str) -> Result<bool, VaultError> {
        ProviderCredentialVault::has(self, reference)
    }

    fn delete(&self, reference: &str) -> Result<bool, VaultError> {
        ProviderCredentialVault::delete(self, reference)
    }
}

impl<T: KeyVault + ?Sized> KeyVault for &T {
    fn create(&self, secret: &SecretString) -> Result<CredentialReference, VaultError> {
        (**self).create(secret)
    }

    fn replace(&self, reference: &str, secret: &SecretString) -> Result<(), VaultError> {
        (**self).replace(reference, secret)
    }

    fn has(&self, reference: &str) -> Result<bool, VaultError> {
        (**self).has(reference)
    }

    fn delete(&self, reference: &str) -> Result<bool, VaultError> {
        (**self).delete(reference)
    }
}

/// Maps a vault failure to the code the interface shows. The vault's own message is never used.
pub fn map_vault(error: VaultError) -> CommandError {
    match error {
        VaultError::SecureStorageUnavailable => CommandError::SecureStorageUnavailable,
        VaultError::InvalidSecret => CommandError::InvalidInput,
        _ => CommandError::CredentialStore,
    }
}

/// Whether a key is configured: the reference is saved and the vault holds its secret. With no saved reference the
/// vault is not opened, so a system without a credential store still answers "not configured".
pub fn status<V, F>(
    store: &Mutex<Store>,
    open: F,
    provider: ProviderKeyProvider,
) -> Result<ProviderKeyStatus, CommandError>
where
    V: KeyVault,
    F: FnOnce() -> Result<V, CommandError>,
{
    let configured = match reference(store)? {
        Some(current) => open()?.has(&current).map_err(map_vault)?,
        None => false,
    };
    Ok(ProviderKeyStatus {
        provider,
        configured,
    })
}

/// Saves the key. The input is validated before the vault is opened, so an invalid key never reaches the operating
/// system store. A key that is already configured is replaced under the same reference. The status carries no key.
pub fn save<V, F>(
    store: &Mutex<Store>,
    open: F,
    input: ProviderKeyInput,
) -> Result<ProviderKeyStatus, CommandError>
where
    V: KeyVault,
    F: FnOnce() -> Result<V, CommandError>,
{
    let ProviderKeyInput { provider, key } =
        input.validated().map_err(|_| CommandError::InvalidInput)?;
    let secret = SecretString::from(key);
    let vault = open()?;
    match reference(store)? {
        Some(current) if vault.has(&current).map_err(map_vault)? => {
            vault.replace(&current, &secret).map_err(map_vault)?;
        }
        _ => {
            // The secret is written first, then its reference. A crash between the two leaves a secret that no
            // reference names: it is never configured, and the vault's prune removes it.
            let created = vault.create(&secret).map_err(map_vault)?;
            write_reference(store, &created.to_string())?;
        }
    }
    Ok(ProviderKeyStatus {
        provider,
        configured: true,
    })
}

/// Removes the key. The secret is deleted first and the reference is cleared afterwards. If clearing fails, the status
/// still reads as not configured, because the vault no longer holds the secret. With no saved reference the vault is
/// not opened.
pub fn remove<V, F>(
    store: &Mutex<Store>,
    open: F,
    provider: ProviderKeyProvider,
) -> Result<ProviderKeyStatus, CommandError>
where
    V: KeyVault,
    F: FnOnce() -> Result<V, CommandError>,
{
    if let Some(current) = reference(store)? {
        open()?.delete(&current).map_err(map_vault)?;
        write_reference(store, "")?;
    }
    Ok(ProviderKeyStatus {
        provider,
        configured: false,
    })
}

/// The saved reference of the OpenRouter key, if one is saved.
pub fn reference(store: &Mutex<Store>) -> Result<Option<String>, CommandError> {
    let guard = store.lock().map_err(|_| CommandError::Internal)?;
    let saved = guard
        .settings()
        .value(OPENROUTER_REFERENCE)
        .map_err(|_| CommandError::Persistence)?;
    Ok(saved.filter(|value| !value.is_empty()))
}

fn write_reference(store: &Mutex<Store>, value: &str) -> Result<(), CommandError> {
    let guard = store.lock().map_err(|_| CommandError::Internal)?;
    let values = BTreeMap::from([(OPENROUTER_REFERENCE.to_string(), value.to_string())]);
    guard
        .settings()
        .set(&values)
        .map_err(|_| CommandError::Persistence)
}
