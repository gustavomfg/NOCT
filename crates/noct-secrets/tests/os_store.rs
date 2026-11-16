//! Exercises the real operating system credential store. Ignored by default: it needs a running
//! Secret Service (Linux), Keychain (macOS) or Credential Manager (Windows).
//!
//!   cargo test -p noct-secrets --test os_store -- --ignored
//!
//! The test runs under its own service name. It never reads or removes the master key of an
//! installed application, and it removes the master key it created even when an assertion fails.

#[allow(dead_code)]
mod support;

use noct_secrets::{MASTER_KEY_ACCOUNT, ProviderCredentialVault, default_store};
use secrecy::{ExposeSecret, SecretString};
use support::scratch;

const SERVICE_PREFIX: &str = "com.nocturne.codex.provider-credentials.os-test";

/// Removes the test's master key from the operating system store when the test ends.
struct RemoveMasterKey {
    service: String,
}

impl Drop for RemoveMasterKey {
    fn drop(&mut self) {
        if let Ok(store) = default_store()
            && let Ok(entry) = store.build(&self.service, MASTER_KEY_ACCOUNT, None)
        {
            let _ = entry.delete_credential();
        }
    }
}

#[test]
#[ignore = "needs the operating system credential store"]
fn operating_system_store_round_trips_a_three_kilobyte_secret() {
    let service = format!("{SERVICE_PREFIX}.{}", std::process::id());
    let _cleanup = RemoveMasterKey {
        service: service.clone(),
    };
    let directory = scratch("os-store");
    let vault = ProviderCredentialVault::open_in_service(&directory, &service)
        .expect("the operating system credential store is available");
    // About three kilobytes: the size that a Windows credential blob cannot hold in UTF-16.
    let value = "segredo-de-teste-0123456789".repeat(115);

    let reference = vault
        .create(&SecretString::from(value.clone()))
        .expect("create")
        .to_string();

    let resolved = vault
        .resolve(&reference)
        .expect("resolve")
        .expect("the secret was stored");
    assert_eq!(resolved.expose_secret(), value);

    let master_key = default_store()
        .expect("store")
        .build(&service, MASTER_KEY_ACCOUNT, None)
        .expect("entry")
        .get_secret()
        .expect("master key");
    assert_eq!(master_key.len(), 32, "the operating system holds 32 bytes");

    assert_eq!(vault.delete(&reference), Ok(true));
    assert!(matches!(vault.resolve(&reference), Ok(None)));
}
