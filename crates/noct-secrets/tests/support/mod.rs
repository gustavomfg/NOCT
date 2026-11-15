//! Helpers shared by the vault tests: an in-memory credential store and scratch dirs.

use std::any::Any;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use keyring_core::api::{CredentialApi, CredentialStoreApi};
use keyring_core::{Credential, Entry, Error, Result};

type Secrets = Arc<Mutex<HashMap<(String, String), Vec<u8>>>>;

/// Credential store that keeps secrets in memory. It can refuse all access as a locked keyring
/// does. The mock store in keyring-core does not persist between entries, so it cannot back a
/// vault.
#[derive(Default)]
pub struct MemoryStore {
    secrets: Secrets,
    locked: Arc<AtomicBool>,
}

impl MemoryStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn lock(&self, locked: bool) {
        self.locked.store(locked, Ordering::SeqCst);
    }

    pub fn secret_count(&self) -> usize {
        self.secrets.lock().expect("memory store").len()
    }

    /// The bytes held under `service` and `user`, if any.
    pub fn stored(&self, service: &str, user: &str) -> Option<Vec<u8>> {
        self.secrets
            .lock()
            .expect("memory store")
            .get(&(service.to_string(), user.to_string()))
            .cloned()
    }

    /// Writes bytes under `service` and `user`, as a change made outside NOCT would.
    pub fn put(&self, service: &str, user: &str, bytes: &[u8]) {
        self.secrets
            .lock()
            .expect("memory store")
            .insert((service.to_string(), user.to_string()), bytes.to_vec());
    }

    /// Removes the entry under `service` and `user`, as a change made outside NOCT would.
    pub fn forget(&self, service: &str, user: &str) {
        self.secrets
            .lock()
            .expect("memory store")
            .remove(&(service.to_string(), user.to_string()));
    }
}

impl CredentialStoreApi for MemoryStore {
    fn vendor(&self) -> String {
        "noct-tests-memory".to_string()
    }

    fn id(&self) -> String {
        "memory".to_string()
    }

    fn build(
        &self,
        service: &str,
        user: &str,
        modifiers: Option<&HashMap<&str, &str>>,
    ) -> Result<Entry> {
        if modifiers.is_some() {
            return Err(Error::NotSupportedByStore("modifiers".to_string()));
        }
        let credential: Arc<Credential> = Arc::new(MemoryCredential {
            key: (service.to_string(), user.to_string()),
            secrets: Arc::clone(&self.secrets),
            locked: Arc::clone(&self.locked),
        });
        Ok(Entry::new_with_credential(credential))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct MemoryCredential {
    key: (String, String),
    secrets: Secrets,
    locked: Arc<AtomicBool>,
}

impl MemoryCredential {
    fn check_access(&self) -> Result<()> {
        if self.locked.load(Ordering::SeqCst) {
            return Err(Error::NoStorageAccess(Box::new(std::io::Error::other(
                "the collection is locked",
            ))));
        }
        Ok(())
    }
}

impl CredentialApi for MemoryCredential {
    fn set_secret(&self, secret: &[u8]) -> Result<()> {
        self.check_access()?;
        self.secrets
            .lock()
            .expect("memory store")
            .insert(self.key.clone(), secret.to_vec());
        Ok(())
    }

    fn get_secret(&self) -> Result<Vec<u8>> {
        self.check_access()?;
        self.secrets
            .lock()
            .expect("memory store")
            .get(&self.key)
            .cloned()
            .ok_or(Error::NoEntry)
    }

    fn delete_credential(&self) -> Result<()> {
        self.check_access()?;
        self.secrets
            .lock()
            .expect("memory store")
            .remove(&self.key)
            .map(|_| ())
            .ok_or(Error::NoEntry)
    }

    fn get_credential(&self) -> Result<Option<Arc<Credential>>> {
        Ok(None)
    }

    fn get_specifiers(&self) -> Option<(String, String)> {
        Some(self.key.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// A clean directory under the Cargo target dir for one test.
pub fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("noct-secrets")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    dir
}
