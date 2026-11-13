//! Exercises the real operating system credential store. Ignored by default: it needs a
//! running Secret Service (Linux), Keychain (macOS) or Credential Manager (Windows).
//!
//!   cargo test -p noct-secrets --test os_store -- --ignored
//!
//! The test writes one temporary secret and removes it again.

#[allow(dead_code)]
mod support;

use noct_secrets::ProviderCredentialVault;
use secrecy::{ExposeSecret, SecretString};
use support::scratch;

#[test]
#[ignore = "needs the operating system credential store"]
fn operating_system_store_round_trips_a_secret() {
    let directory = scratch("os-store");
    let vault = ProviderCredentialVault::open(&directory)
        .expect("the operating system credential store is available");

    let reference = vault
        .create(&SecretString::from("noct-test-secret".to_string()))
        .expect("create");
    let reference = reference.to_string();

    let resolved = vault
        .resolve(&reference)
        .expect("resolve")
        .expect("the secret was stored");
    assert_eq!(resolved.expose_secret(), "noct-test-secret");

    assert_eq!(vault.delete(&reference), Ok(true));
    assert!(matches!(vault.resolve(&reference), Ok(None)));
}
