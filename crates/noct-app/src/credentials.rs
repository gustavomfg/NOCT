//! Provider keys in the operating system credential store, through the vault of `noct-secrets`.
//!
//! The interface sends a key once and reads back only whether one is configured. The reference that names a key is not
//! secret. It is kept in the settings table under `OPENROUTER_REFERENCE`, a key with the `maintenance.` prefix. Both the
//! shell and the Electron app hide that prefix from their settings reads, so the interface never sees the reference, and
//! the Electron app does not see it either while the two share the database. The store lock is held while the settings
//! are read or written, never while the operating system store is called, and the vault is opened only when a call needs
//! it.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use noct_core::ai::provider_key::{ProviderKeyInput, ProviderKeyProvider, ProviderKeyStatus};
use noct_db::store::Store;
use noct_secrets::{CredentialReference, ProviderCredentialVault, SecretString, VaultError};

use crate::error::CommandError;

/// The folder of the vault, inside the data directory.
pub const VAULT_DIRECTORY: &str = "provider-credentials";

/// The settings key that names the reference of the OpenRouter key. Empty means no key.
pub const OPENROUTER_REFERENCE: &str = "maintenance.secret.openrouter.reference";

/// Serializes the key saves and removals of this process. A save reads the reference, asks the vault for a secret and
/// writes the reference. Two of them in parallel could let one save's cleanup delete the secret the other has just
/// created, or write a reference to a secret that is already gone. The lock is always taken before the store lock, and
/// no code takes it while holding the store lock, so the order cannot invert. It guards no data, so a poisoned lock is
/// taken over: each operation reads the reference and the vault again, and a panic cannot leave the key unusable until
/// the next start.
static KEY_OPERATIONS: Mutex<()> = Mutex::new(());

fn key_operations() -> MutexGuard<'static, ()> {
    KEY_OPERATIONS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// The vault operations the key commands use. `ProviderCredentialVault` implements it. Tests implement it in memory,
/// so that no test writes to the operating system store.
pub trait KeyVault {
    fn create(&self, secret: &SecretString) -> Result<CredentialReference, VaultError>;
    fn replace(&self, reference: &str, secret: &SecretString) -> Result<(), VaultError>;
    fn has(&self, reference: &str) -> Result<bool, VaultError>;
    fn delete(&self, reference: &str) -> Result<bool, VaultError>;
    /// Removes every secret that is not in `allowed`, and returns how many went.
    fn prune(&self, allowed: &[&str]) -> Result<usize, VaultError>;
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

    fn prune(&self, allowed: &[&str]) -> Result<usize, VaultError> {
        ProviderCredentialVault::prune(self, allowed)
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

    fn prune(&self, allowed: &[&str]) -> Result<usize, VaultError> {
        (**self).prune(allowed)
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
    let _operation = key_operations();
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
    prune_leftovers(store, &vault);
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
    let _operation = key_operations();
    if let Some(current) = reference(store)? {
        let vault = open()?;
        vault.delete(&current).map_err(map_vault)?;
        write_reference(store, "")?;
        prune_leftovers(store, &vault);
    }
    Ok(ProviderKeyStatus {
        provider,
        configured: false,
    })
}

/// Removes the secrets that no saved reference names. Such a secret is the leftover of a save that stopped before its
/// reference was written. The vault removes every secret not passed, so each provider's reference must be passed here;
/// the OpenRouter key is the only one in beta 1. If the reference cannot be read, nothing is removed: an unreadable
/// setting must never be taken for "no key". A failed cleanup is not reported, because the save or removal already
/// succeeded; the leftover waits for the next call.
fn prune_leftovers<V: KeyVault>(store: &Mutex<Store>, vault: &V) {
    let Ok(current) = reference(store) else {
        return;
    };
    let live: Vec<&str> = current.iter().map(String::as_str).collect();
    let _ = vault.prune(&live);
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
