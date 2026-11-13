//! Vault behavior on an in-memory store. The real operating system store is covered by
//! `os_store.rs`, which is ignored by default.

mod support;

use std::fs;
use std::path::Path;

use noct_secrets::{CredentialReference, ProviderCredentialVault, VaultError};
use secrecy::{ExposeSecret, SecretString};
use support::{MemoryStore, scratch};

const KNOWN_SECRET: &str = "sk-live-test-7f3a9c";

fn secret(value: &str) -> SecretString {
    SecretString::from(value.to_string())
}

fn vault_in(directory: &Path, store: &std::sync::Arc<MemoryStore>) -> ProviderCredentialVault {
    ProviderCredentialVault::with_store(store.clone(), directory)
}

#[test]
fn create_resolves_the_secret_and_keeps_it_out_of_the_index() {
    let directory = scratch("create-resolve");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);

    let reference = vault.create(&secret(KNOWN_SECRET)).expect("create");

    assert_eq!(
        CredentialReference::parse(&reference.to_string()).expect("canonical reference"),
        reference,
    );
    let resolved = vault
        .resolve(&reference.to_string())
        .expect("resolve")
        .expect("the secret exists");
    assert_eq!(resolved.expose_secret(), KNOWN_SECRET);
    assert_eq!(store.secret_count(), 1);

    let index =
        fs::read_to_string(directory.join("provider-credential-index.json")).expect("index file");
    assert!(index.contains(&reference.to_string()));
    assert!(
        !index.contains(KNOWN_SECRET),
        "the index must never hold secret material"
    );
}

#[test]
fn replace_changes_the_secret_behind_the_same_reference() {
    let directory = scratch("replace");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    let reference = vault
        .create(&secret("old-key"))
        .expect("create")
        .to_string();

    vault
        .replace(&reference, &secret("new-key"))
        .expect("replace");

    let resolved = vault
        .resolve(&reference)
        .expect("resolve")
        .expect("present");
    assert_eq!(resolved.expose_secret(), "new-key");
    assert_eq!(store.secret_count(), 1);
}

#[test]
fn replace_rejects_a_reference_that_is_not_indexed() {
    let directory = scratch("replace-unknown");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    let unknown = "6f9619ff-8b86-4d11-b42d-00c04fc964ff";

    assert_eq!(
        vault.replace(unknown, &secret("value")),
        Err(VaultError::InvalidReference),
    );
    assert_eq!(store.secret_count(), 0, "a rejected replace must not write");
}

#[test]
fn delete_removes_the_secret_and_the_reference() {
    let directory = scratch("delete");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    let reference = vault
        .create(&secret(KNOWN_SECRET))
        .expect("create")
        .to_string();

    assert_eq!(vault.delete(&reference), Ok(true));

    assert!(matches!(vault.resolve(&reference), Ok(None)));
    assert_eq!(vault.has(&reference), Ok(false));
    assert_eq!(store.secret_count(), 0);
    assert_eq!(vault.delete(&reference), Ok(false));
}

#[test]
fn a_rejected_removal_keeps_the_reference_so_the_delete_can_be_retried() {
    let directory = scratch("delete-retry");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    let reference = vault
        .create(&secret(KNOWN_SECRET))
        .expect("create")
        .to_string();

    store.reject_writes(true);
    assert_eq!(vault.delete(&reference), Err(VaultError::WriteFailed));
    assert_eq!(vault.has(&reference), Ok(true));

    store.reject_writes(false);
    assert_eq!(vault.delete(&reference), Ok(true));
    assert_eq!(vault.has(&reference), Ok(false));
}

#[test]
fn prune_removes_only_secrets_that_no_provider_lists() {
    let directory = scratch("prune");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    let kept = vault.create(&secret("kept")).expect("create").to_string();
    let stale_a = vault
        .create(&secret("stale-a"))
        .expect("create")
        .to_string();
    let stale_b = vault
        .create(&secret("stale-b"))
        .expect("create")
        .to_string();

    assert_eq!(vault.prune(&[kept.as_str()]), Ok(2));

    assert_eq!(
        vault
            .resolve(&kept)
            .expect("resolve")
            .expect("kept")
            .expose_secret(),
        "kept"
    );
    assert!(matches!(vault.resolve(&stale_a), Ok(None)));
    assert!(matches!(vault.resolve(&stale_b), Ok(None)));
    assert_eq!(store.secret_count(), 1);
    assert_eq!(vault.prune(&[kept.as_str()]), Ok(0));
}

#[test]
fn malformed_references_are_rejected_before_the_store_is_touched() {
    let directory = scratch("malformed");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    vault.create(&secret(KNOWN_SECRET)).expect("create");

    let malformed = [
        "",
        "not-a-uuid",
        "6F9619FF-8B86-4D11-B42D-00C04FC964FF",
        "6f9619ff8b864d11b42d00c04fc964ff",
        "{6f9619ff-8b86-4d11-b42d-00c04fc964ff}",
        " 6f9619ff-8b86-4d11-b42d-00c04fc964ff",
    ];
    for value in malformed {
        assert!(
            matches!(vault.resolve(value), Err(VaultError::InvalidReference)),
            "resolve {value:?}"
        );
        assert_eq!(
            vault.has(value),
            Err(VaultError::InvalidReference),
            "has {value:?}"
        );
        assert_eq!(
            vault.delete(value),
            Err(VaultError::InvalidReference),
            "delete {value:?}"
        );
        assert_eq!(
            vault.replace(value, &secret("x")),
            Err(VaultError::InvalidReference),
            "replace {value:?}",
        );
        assert_eq!(
            vault.prune(&[value]),
            Err(VaultError::InvalidReference),
            "prune {value:?}"
        );
    }
    assert_eq!(store.secret_count(), 1);
}

#[test]
fn an_unindexed_but_well_formed_reference_resolves_to_nothing() {
    let directory = scratch("unindexed");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    let unknown = "6f9619ff-8b86-4d11-b42d-00c04fc964ff";

    assert!(matches!(vault.resolve(unknown), Ok(None)));
    assert_eq!(vault.has(unknown), Ok(false));
    assert_eq!(vault.delete(unknown), Ok(false));
}

#[test]
fn secret_length_counts_utf16_units_like_javascript() {
    let directory = scratch("length");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);

    assert_eq!(
        vault.create(&secret("")).err(),
        Some(VaultError::InvalidSecret)
    );
    assert!(vault.create(&secret(&"a".repeat(64_000))).is_ok());
    assert_eq!(
        vault.create(&secret(&"a".repeat(64_001))).err(),
        Some(VaultError::InvalidSecret),
    );
    // Each emoji is two UTF-16 units, so 32,000 of them fill the limit exactly.
    assert!(vault.create(&secret(&"😀".repeat(32_000))).is_ok());
    assert_eq!(
        vault.create(&secret(&"😀".repeat(32_001))).err(),
        Some(VaultError::InvalidSecret),
    );
}

#[test]
fn an_invalid_index_fails_closed_before_any_secret_is_written() {
    let payloads = [
        "not json",
        r#"{"version":2,"references":[]}"#,
        r#"{"version":1,"references":["not-a-uuid"]}"#,
        r#"{"version":1,"references":["6f9619ff-8b86-4d11-b42d-00c04fc964ff","6f9619ff-8b86-4d11-b42d-00c04fc964ff"]}"#,
        r#"{"version":1,"references":[],"extra":true}"#,
    ];
    for (index, payload) in payloads.iter().enumerate() {
        let directory = scratch(&format!("corrupt-index-{index}"));
        fs::create_dir_all(&directory).expect("directory");
        fs::write(directory.join("provider-credential-index.json"), payload).expect("index");
        let store = MemoryStore::new();
        let vault = vault_in(&directory, &store);

        assert_eq!(
            vault.create(&secret(KNOWN_SECRET)),
            Err(VaultError::CorruptStore),
            "payload {payload}",
        );
        assert_eq!(
            store.secret_count(),
            0,
            "payload {payload} must not write a secret"
        );
    }
}

#[test]
fn a_failed_index_write_removes_the_new_secret_again() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        // SAFETY: geteuid has no preconditions and does not touch memory.
        if unsafe { libc::geteuid() } == 0 {
            // Root ignores directory permissions, so the failure cannot be produced here.
            return;
        }
        let directory = scratch("index-write-fails");
        fs::create_dir_all(&directory).expect("directory");
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o500)).expect("read-only");
        let store = MemoryStore::new();
        let vault = vault_in(&directory, &store);

        let outcome = vault.create(&secret(KNOWN_SECRET));
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).expect("restore");

        assert_eq!(outcome, Err(VaultError::WriteFailed));
        assert_eq!(
            store.secret_count(),
            0,
            "no secret may survive without an index entry"
        );
    }
}

#[cfg(unix)]
#[test]
fn the_directory_and_index_are_private() {
    use std::os::unix::fs::PermissionsExt;

    let directory = scratch("permissions").join("nested");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    vault.create(&secret(KNOWN_SECRET)).expect("create");

    let directory_mode = fs::metadata(&directory)
        .expect("directory")
        .permissions()
        .mode();
    let index_mode = fs::metadata(directory.join("provider-credential-index.json"))
        .expect("index")
        .permissions()
        .mode();
    assert_eq!(directory_mode & 0o777, 0o700);
    assert_eq!(index_mode & 0o777, 0o600);
}

#[cfg(unix)]
#[test]
fn a_symlinked_index_is_refused() {
    let directory = scratch("symlink-index");
    fs::create_dir_all(&directory).expect("directory");
    let target = directory.join("elsewhere.json");
    fs::write(&target, r#"{"version":1,"references":[]}"#).expect("target");
    std::os::unix::fs::symlink(&target, directory.join("provider-credential-index.json"))
        .expect("symlink");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);

    assert_eq!(
        vault.has("6f9619ff-8b86-4d11-b42d-00c04fc964ff"),
        Err(VaultError::CorruptStore)
    );
}

#[test]
fn a_locked_store_reports_secure_storage_unavailable_and_keeps_the_index() {
    let directory = scratch("locked");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    let reference = vault
        .create(&secret(KNOWN_SECRET))
        .expect("create")
        .to_string();

    store.lock(true);
    assert_eq!(
        vault.create(&secret("another")).err(),
        Some(VaultError::SecureStorageUnavailable),
    );
    assert_eq!(
        vault.replace(&reference, &secret("another")),
        Err(VaultError::SecureStorageUnavailable),
    );
    assert_eq!(
        vault.resolve(&reference).err(),
        Some(VaultError::SecureStorageUnavailable),
    );
    assert_eq!(
        vault.delete(&reference),
        Err(VaultError::SecureStorageUnavailable),
    );
    assert_eq!(vault.has(&reference), Ok(true));

    store.lock(false);
    let resolved = vault
        .resolve(&reference)
        .expect("resolve after unlock")
        .expect("the secret survived the lock");
    assert_eq!(resolved.expose_secret(), KNOWN_SECRET);
}
