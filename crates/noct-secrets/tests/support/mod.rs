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

/// Credential store that keeps secrets in memory. It can reject writes, or refuse all
/// access as a locked keyring does. The mock store in keyring-core does not persist
/// between entries, so it cannot back a vault.
#[derive(Default)]
pub struct MemoryStore {
    secrets: Secrets,
    reject_writes: Arc<AtomicBool>,
    locked: Arc<AtomicBool>,
}

impl MemoryStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn reject_writes(&self, reject: bool) {
        self.reject_writes.store(reject, Ordering::SeqCst);
    }

    pub fn lock(&self, locked: bool) {
        self.locked.store(locked, Ordering::SeqCst);
    }

    pub fn secret_count(&self) -> usize {
        self.secrets.lock().expect("memory store").len()
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
            reject_writes: Arc::clone(&self.reject_writes),
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
    reject_writes: Arc<AtomicBool>,
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

    fn check_writes(&self) -> Result<()> {
        self.check_access()?;
        if self.reject_writes.load(Ordering::SeqCst) {
            return Err(Error::Invalid(
                "store".to_string(),
                "writes rejected by the test".to_string(),
            ));
        }
        Ok(())
    }
}

impl CredentialApi for MemoryCredential {
    fn set_secret(&self, secret: &[u8]) -> Result<()> {
        self.check_writes()?;
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
        self.check_writes()?;
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
