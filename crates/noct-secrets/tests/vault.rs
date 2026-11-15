//! Vault behavior on an in-memory store. The real operating system store is covered by
//! `os_store.rs`, which is ignored by default.

mod support;

use std::fs;
use std::path::Path;

use noct_secrets::{
    CredentialReference, MASTER_KEY_ACCOUNT, ProviderCredentialVault, SERVICE_NAME, VaultError,
};
use secrecy::{ExposeSecret, SecretString};
use support::{MemoryStore, scratch};

const KNOWN_SECRET: &str = "sk-live-test-7f3a9c";
const INDEX_FILE: &str = "provider-credential-index.json";
const DOCUMENT_FILE: &str = "provider-credentials.enc";
const UNKNOWN: &str = "6f9619ff-8b86-4d11-b42d-00c04fc964ff";
/// Largest secret the document can hold: the whole encoded document is bounded at 1 MiB.
const DOCUMENT_LIMIT: usize = 1_048_576;

fn secret(value: &str) -> SecretString {
    SecretString::from(value.to_string())
}

fn vault_in(directory: &Path, store: &std::sync::Arc<MemoryStore>) -> ProviderCredentialVault {
    ProviderCredentialVault::with_store(store.clone(), directory)
}

/// A secret of about three kilobytes, with a marker that cannot occur by chance in ciphertext.
fn three_kilobyte_secret() -> String {
    "segredo-de-3kb-0123456789abcdef".repeat(100)
}

#[test]
fn create_resolves_the_secret_and_keeps_it_out_of_the_index_and_the_store() {
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

    let index = fs::read_to_string(directory.join(INDEX_FILE)).expect("index file");
    assert!(index.contains(&reference.to_string()));
    assert!(
        !index.contains(KNOWN_SECRET),
        "the index must never hold secret material"
    );
    let document = fs::read(directory.join(DOCUMENT_FILE)).expect("document file");
    assert!(
        !contains(&document, KNOWN_SECRET.as_bytes()),
        "the document must hold only ciphertext"
    );

    assert_eq!(
        store.secret_count(),
        1,
        "the store holds only the master key"
    );
    let master_key = store
        .stored(SERVICE_NAME, MASTER_KEY_ACCOUNT)
        .expect("master key");
    assert_eq!(master_key.len(), 32, "the master key is 32 bytes");
    assert!(!contains(&master_key, KNOWN_SECRET.as_bytes()));
}

#[test]
fn a_three_kilobyte_secret_round_trips_through_the_encrypted_file() {
    let directory = scratch("three-kilobytes");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    let value = three_kilobyte_secret();

    let reference = vault.create(&secret(&value)).expect("create").to_string();

    let resolved = vault
        .resolve(&reference)
        .expect("resolve")
        .expect("present");
    assert_eq!(resolved.expose_secret(), value);
    let document = fs::read(directory.join(DOCUMENT_FILE)).expect("document file");
    assert!(
        !contains(&document, b"segredo-de-3kb"),
        "the marker of the plaintext must not appear in the file"
    );
    assert_eq!(store.secret_count(), 1);
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

    assert_eq!(
        vault.replace(UNKNOWN, &secret("value")),
        Err(VaultError::InvalidReference),
    );
    assert_eq!(store.secret_count(), 0, "a rejected replace must not write");
    assert!(!directory.join(DOCUMENT_FILE).exists());
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
    let document = fs::read(directory.join(DOCUMENT_FILE)).expect("document file");
    assert!(!contains(&document, KNOWN_SECRET.as_bytes()));
    assert_eq!(vault.delete(&reference), Ok(false));
}

#[cfg(unix)]
#[test]
fn a_rejected_removal_keeps_the_reference_so_the_delete_can_be_retried() {
    use std::os::unix::fs::PermissionsExt;

    // SAFETY: geteuid has no preconditions and does not touch memory.
    if unsafe { libc::geteuid() } == 0 {
        // Root ignores directory permissions, so the failure cannot be produced here.
        return;
    }
    let directory = scratch("delete-retry");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    let reference = vault
        .create(&secret(KNOWN_SECRET))
        .expect("create")
        .to_string();

    fs::set_permissions(&directory, fs::Permissions::from_mode(0o500)).expect("read-only");
    let outcome = vault.delete(&reference);
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).expect("restore");

    assert_eq!(outcome, Err(VaultError::WriteFailed));
    assert_eq!(vault.has(&reference), Ok(true));
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
    assert_eq!(vault.has(&stale_a), Ok(false));
    assert_eq!(vault.prune(&[kept.as_str()]), Ok(0));
}

#[test]
fn malformed_references_are_rejected_before_the_store_is_touched() {
    let directory = scratch("malformed");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    vault.create(&secret(KNOWN_SECRET)).expect("create");
    let document_before = fs::read(directory.join(DOCUMENT_FILE)).expect("document file");

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
    assert_eq!(
        fs::read(directory.join(DOCUMENT_FILE)).expect("document file"),
        document_before,
        "a rejected call must not rewrite the document"
    );
}

#[test]
fn an_unindexed_but_well_formed_reference_resolves_to_nothing() {
    let directory = scratch("unindexed");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    vault.create(&secret(KNOWN_SECRET)).expect("create");

    assert!(matches!(vault.resolve(UNKNOWN), Ok(None)));
    assert_eq!(vault.has(UNKNOWN), Ok(false));
    assert_eq!(vault.delete(UNKNOWN), Ok(false));
}

#[test]
fn an_empty_vault_never_touches_the_operating_system_store() {
    let directory = scratch("empty-vault");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);

    assert!(matches!(vault.resolve(UNKNOWN), Ok(None)));
    assert_eq!(vault.has(UNKNOWN), Ok(false));
    assert_eq!(vault.delete(UNKNOWN), Ok(false));
    assert_eq!(vault.prune(&[]), Ok(0));
    assert_eq!(store.secret_count(), 0, "no master key is created by reads");
}

#[test]
fn secret_size_is_bounded_by_the_document_and_empty_secrets_are_refused() {
    let directory = scratch("size");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);

    assert_eq!(
        vault.create(&secret("")).err(),
        Some(VaultError::InvalidSecret)
    );
    assert!(vault.create(&secret(&"a".repeat(3_072))).is_ok());
    // Each emoji is four UTF-8 bytes and fits well inside the bound.
    assert!(vault.create(&secret(&"😀".repeat(1_500))).is_ok());
    assert_eq!(
        vault.create(&secret(&"a".repeat(DOCUMENT_LIMIT + 1))).err(),
        Some(VaultError::InvalidSecret),
    );
}

#[test]
fn a_secret_that_only_fits_before_escaping_is_refused_and_the_index_is_restored() {
    let directory = scratch("escaping");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    vault.create(&secret(KNOWN_SECRET)).expect("create");
    let index_before = fs::read(directory.join(INDEX_FILE)).expect("index");

    // Control characters are written as six-byte escapes, so 300,000 of them outgrow the document
    // although the raw secret is far below the bound.
    let escaping = "\u{1}".repeat(300_000);
    assert_eq!(
        vault.create(&secret(&escaping)).err(),
        Some(VaultError::InvalidSecret),
    );
    assert_eq!(
        fs::read(directory.join(INDEX_FILE)).expect("index"),
        index_before,
        "a refused secret must leave the index as it was"
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
        fs::write(directory.join(INDEX_FILE), payload).expect("index");
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
            "payload {payload} must not write a key or a secret"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_failed_write_indexes_no_reference_and_a_retry_reuses_the_key() {
    use std::os::unix::fs::PermissionsExt;

    // SAFETY: geteuid has no preconditions and does not touch memory.
    if unsafe { libc::geteuid() } == 0 {
        // Root ignores directory permissions, so the failure cannot be produced here.
        return;
    }
    let directory = scratch("write-fails");
    fs::create_dir_all(&directory).expect("directory");
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o500)).expect("read-only");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);

    let outcome = vault.create(&secret(KNOWN_SECRET));
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).expect("restore");

    assert_eq!(outcome, Err(VaultError::WriteFailed));
    assert!(
        !directory.join(INDEX_FILE).exists(),
        "no reference is indexed"
    );
    assert!(
        !directory.join(DOCUMENT_FILE).exists(),
        "no secret is written"
    );
    assert_eq!(store.secret_count(), 1, "only the key of the empty vault");
    let key = store.stored(SERVICE_NAME, MASTER_KEY_ACCOUNT).expect("key");

    let reference = vault
        .create(&secret(KNOWN_SECRET))
        .expect("the retry succeeds")
        .to_string();
    assert_eq!(
        store.stored(SERVICE_NAME, MASTER_KEY_ACCOUNT),
        Some(key),
        "the retry reuses the key"
    );
    assert_eq!(
        vault
            .resolve(&reference)
            .expect("resolve")
            .expect("present")
            .expose_secret(),
        KNOWN_SECRET
    );
}

#[cfg(unix)]
#[test]
fn the_directory_and_vault_files_are_private() {
    use std::os::unix::fs::PermissionsExt;

    let directory = scratch("permissions").join("nested");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    vault.create(&secret(KNOWN_SECRET)).expect("create");

    let mode = |path: &Path| fs::metadata(path).expect("metadata").permissions().mode() & 0o777;
    assert_eq!(mode(&directory), 0o700);
    assert_eq!(mode(&directory.join(INDEX_FILE)), 0o600);
    assert_eq!(mode(&directory.join(DOCUMENT_FILE)), 0o600);
}

#[cfg(unix)]
#[test]
fn a_symlinked_index_is_refused() {
    let directory = scratch("symlink-index");
    fs::create_dir_all(&directory).expect("directory");
    let target = directory.join("elsewhere.json");
    fs::write(&target, r#"{"version":1,"references":[]}"#).expect("target");
    std::os::unix::fs::symlink(&target, directory.join(INDEX_FILE)).expect("symlink");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);

    assert_eq!(vault.has(UNKNOWN), Err(VaultError::CorruptStore));
}

#[cfg(unix)]
#[test]
fn a_symlinked_document_is_refused() {
    let directory = scratch("symlink-document");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    let reference = vault
        .create(&secret(KNOWN_SECRET))
        .expect("create")
        .to_string();

    let target = directory.join("elsewhere.enc");
    fs::rename(directory.join(DOCUMENT_FILE), &target).expect("move");
    std::os::unix::fs::symlink(&target, directory.join(DOCUMENT_FILE)).expect("symlink");

    assert_eq!(
        vault.resolve(&reference).err(),
        Some(VaultError::CorruptStore)
    );
}

#[test]
fn a_tampered_document_fails_closed() {
    let directory = scratch("tampered");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    let reference = vault
        .create(&secret(KNOWN_SECRET))
        .expect("create")
        .to_string();

    let path = directory.join(DOCUMENT_FILE);
    let mut bytes = fs::read(&path).expect("document file");
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0x01;
    fs::write(&path, bytes).expect("tampered");

    assert_eq!(
        vault.resolve(&reference).err(),
        Some(VaultError::CorruptStore)
    );
}

#[test]
fn a_replaced_master_key_cannot_open_the_document() {
    let directory = scratch("replaced-key");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    let reference = vault
        .create(&secret(KNOWN_SECRET))
        .expect("create")
        .to_string();

    store.put(SERVICE_NAME, MASTER_KEY_ACCOUNT, &[9_u8; 32]);

    assert_eq!(
        vault.resolve(&reference).err(),
        Some(VaultError::CorruptStore)
    );
}

#[test]
fn a_lost_master_key_is_reported_and_never_replaced() {
    let directory = scratch("lost-key");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    let reference = vault
        .create(&secret(KNOWN_SECRET))
        .expect("create")
        .to_string();

    store.forget(SERVICE_NAME, MASTER_KEY_ACCOUNT);

    assert_eq!(
        vault.resolve(&reference).err(),
        Some(VaultError::MasterKeyMissing)
    );
    assert_eq!(
        vault.create(&secret("another")).err(),
        Some(VaultError::MasterKeyMissing),
    );
    assert_eq!(
        store.stored(SERVICE_NAME, MASTER_KEY_ACCOUNT),
        None,
        "a new key must not be created over stored secrets"
    );
    assert_eq!(
        vault.has(&reference),
        Err(VaultError::MasterKeyMissing),
        "a configured reference needs its key to be answered"
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
    assert_eq!(
        vault.has(&reference),
        Err(VaultError::SecureStorageUnavailable),
    );

    store.lock(false);
    let resolved = vault
        .resolve(&reference)
        .expect("resolve after unlock")
        .expect("the secret survived the lock");
    assert_eq!(resolved.expose_secret(), KNOWN_SECRET);
}

#[test]
fn a_locked_empty_vault_never_creates_a_key() {
    let directory = scratch("locked-empty");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);

    store.lock(true);
    assert_eq!(
        vault.create(&secret(KNOWN_SECRET)).err(),
        Some(VaultError::SecureStorageUnavailable),
    );
    store.lock(false);

    assert_eq!(store.secret_count(), 0, "a locked store gets no new key");
    assert!(!directory.join(INDEX_FILE).exists());
    assert!(!directory.join(DOCUMENT_FILE).exists());
}

#[test]
fn a_reference_the_document_does_not_hold_is_not_configured() {
    let directory = scratch("divergent-missing");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    let kept = vault.create(&secret("kept")).expect("create").to_string();
    let document_before = fs::read(directory.join(DOCUMENT_FILE)).expect("document");
    let lost = vault.create(&secret("lost")).expect("create").to_string();

    // The document written before the second secret: the index names it, the document does not.
    fs::write(directory.join(DOCUMENT_FILE), document_before).expect("restore document");

    assert_eq!(vault.has(&lost), Ok(false));
    assert!(matches!(vault.resolve(&lost), Ok(None)));
    assert_eq!(vault.has(&kept), Ok(true));
}

#[test]
fn an_entry_the_index_does_not_name_is_not_configured_and_prune_removes_it() {
    let directory = scratch("divergent-orphan");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    let kept = vault.create(&secret("kept")).expect("create").to_string();
    let index_before = fs::read(directory.join(INDEX_FILE)).expect("index");
    let orphan = vault.create(&secret("orphan")).expect("create").to_string();
    let index_with_orphan = fs::read(directory.join(INDEX_FILE)).expect("index");

    // The index written before the second secret: the document holds an entry it does not name.
    fs::write(directory.join(INDEX_FILE), &index_before).expect("restore index");
    assert_eq!(vault.has(&orphan), Ok(false));
    assert!(matches!(vault.resolve(&orphan), Ok(None)));

    assert_eq!(vault.prune(&[kept.as_str()]), Ok(0), "nothing is stale");

    // The index names the orphan again. The document decides, and prune has removed it from it.
    fs::write(directory.join(INDEX_FILE), &index_with_orphan).expect("restore index");
    assert_eq!(vault.has(&orphan), Ok(false));
    assert_eq!(vault.has(&kept), Ok(true));
}

#[test]
fn a_missing_document_is_not_configured_and_needs_no_key() {
    let directory = scratch("missing-document");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    let reference = vault
        .create(&secret(KNOWN_SECRET))
        .expect("create")
        .to_string();
    fs::remove_file(directory.join(DOCUMENT_FILE)).expect("remove document");

    store.lock(true);
    assert_eq!(vault.has(&reference), Ok(false));
    assert!(matches!(vault.resolve(&reference), Ok(None)));
    store.lock(false);
}

#[test]
fn reset_after_a_lost_key_clears_the_vault_and_the_vault_works_again() {
    let directory = scratch("reset-lost-key");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    let lost = vault
        .create(&secret(KNOWN_SECRET))
        .expect("create")
        .to_string();
    store.forget(SERVICE_NAME, MASTER_KEY_ACCOUNT);

    assert_eq!(vault.has(&lost), Err(VaultError::MasterKeyMissing));
    assert_eq!(
        vault.create(&secret("another")).err(),
        Some(VaultError::MasterKeyMissing),
    );
    assert_eq!(store.stored(SERVICE_NAME, MASTER_KEY_ACCOUNT), None);

    assert_eq!(vault.reset(), Ok(()));
    assert_eq!(vault.has(&lost), Ok(false));
    assert!(!directory.join(DOCUMENT_FILE).exists());

    let fresh = vault
        .create(&secret("fresh"))
        .expect("create after reset")
        .to_string();
    assert_eq!(
        vault
            .resolve(&fresh)
            .expect("resolve")
            .expect("present")
            .expose_secret(),
        "fresh"
    );
    assert_eq!(store.secret_count(), 1, "one new master key");
}

#[test]
fn reset_on_a_locked_store_is_refused_and_changes_nothing() {
    let directory = scratch("reset-locked");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    let reference = vault
        .create(&secret(KNOWN_SECRET))
        .expect("create")
        .to_string();
    let index_before = fs::read(directory.join(INDEX_FILE)).expect("index");

    store.lock(true);
    assert_eq!(vault.reset(), Err(VaultError::SecureStorageUnavailable));
    store.lock(false);

    assert_eq!(
        fs::read(directory.join(INDEX_FILE)).expect("index"),
        index_before
    );
    assert!(directory.join(DOCUMENT_FILE).exists());
    assert_eq!(
        vault
            .resolve(&reference)
            .expect("resolve")
            .expect("present")
            .expose_secret(),
        KNOWN_SECRET
    );
    assert_eq!(store.secret_count(), 1, "the key is kept");
}

#[test]
fn reset_with_the_key_present_removes_the_secrets_and_the_key() {
    let directory = scratch("reset-present");
    let store = MemoryStore::new();
    let vault = vault_in(&directory, &store);
    let reference = vault
        .create(&secret(KNOWN_SECRET))
        .expect("create")
        .to_string();

    assert_eq!(vault.reset(), Ok(()));

    assert_eq!(store.secret_count(), 0);
    assert_eq!(vault.has(&reference), Ok(false));
    assert!(!directory.join(DOCUMENT_FILE).exists());
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
