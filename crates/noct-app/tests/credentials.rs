//! The provider key service: what is saved, where, and what the interface can read back. The vault is an in-memory
//! fake, so no test touches the operating system credential store, and the store is a scratch database, never the data
//! folder of an installation.

mod common;

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use common::Scratch;
use noct_app::bootstrap::{self, Persistence};
use noct_app::credentials::{self, KeyVault, OPENROUTER_REFERENCE, map_vault};
use noct_app::error::CommandError;
use noct_app::services;
use noct_core::ai::provider_key::{ProviderKeyInput, ProviderKeyProvider};
use noct_secrets::{CredentialReference, ExposeSecret, SecretString, VaultError};

const KEY: &str = "sk-or-v1-0123456789abcdef";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    Create,
    Has,
    Delete,
    Prune,
}

/// A vault held in memory. It counts the writes and can refuse one kind of operation, so the tests can check what
/// happens after a failure.
#[derive(Default)]
struct FakeVault {
    secrets: RefCell<BTreeMap<String, String>>,
    created: Cell<u64>,
    replaced: Cell<u64>,
    fault: Cell<Option<Fault>>,
}

impl FakeVault {
    fn fail(&self, fault: Fault) {
        self.fault.set(Some(fault));
    }

    fn failing(&self, fault: Fault) -> bool {
        self.fault.get() == Some(fault)
    }

    fn secret_under(&self, reference: &str) -> Option<String> {
        self.secrets.borrow().get(reference).cloned()
    }

    fn holds_nothing(&self) -> bool {
        self.secrets.borrow().is_empty()
    }

    /// Writes a secret that no reference names, as a save that stopped after its secret write leaves it.
    fn plant(&self, reference: &str, secret: &str) {
        self.secrets
            .borrow_mut()
            .insert(reference.to_string(), secret.to_string());
    }

    /// Removes the secret without going through the service, as if the operating system store had lost it.
    fn forget(&self, reference: &str) {
        self.secrets.borrow_mut().remove(reference);
    }
}

impl KeyVault for FakeVault {
    fn create(&self, secret: &SecretString) -> Result<CredentialReference, VaultError> {
        if self.failing(Fault::Create) {
            return Err(VaultError::WriteFailed);
        }
        let number = self.created.get() + 1;
        self.created.set(number);
        let reference = format!("00000000-0000-4000-8000-{number:012x}");
        self.secrets
            .borrow_mut()
            .insert(reference.clone(), secret.expose_secret().to_string());
        CredentialReference::parse(&reference)
    }

    fn replace(&self, reference: &str, secret: &SecretString) -> Result<(), VaultError> {
        self.replaced.set(self.replaced.get() + 1);
        match self.secrets.borrow_mut().get_mut(reference) {
            Some(slot) => {
                *slot = secret.expose_secret().to_string();
                Ok(())
            }
            None => Err(VaultError::InvalidReference),
        }
    }

    fn has(&self, reference: &str) -> Result<bool, VaultError> {
        if self.failing(Fault::Has) {
            return Err(VaultError::ReadFailed);
        }
        Ok(self.secrets.borrow().contains_key(reference))
    }

    fn delete(&self, reference: &str) -> Result<bool, VaultError> {
        if self.failing(Fault::Delete) {
            return Err(VaultError::WriteFailed);
        }
        Ok(self.secrets.borrow_mut().remove(reference).is_some())
    }

    fn prune(&self, allowed: &[&str]) -> Result<usize, VaultError> {
        if self.failing(Fault::Prune) {
            return Err(VaultError::WriteFailed);
        }
        let mut secrets = self.secrets.borrow_mut();
        let before = secrets.len();
        secrets.retain(|reference, _| allowed.contains(&reference.as_str()));
        Ok(before - secrets.len())
    }
}

/// A vault whose secret write announces that it started and then holds for a while. It records when the write ends, so a
/// test can tell whether another save opened its vault in the meantime.
struct SlowVault<'a> {
    inner: FakeVault,
    entered: mpsc::Sender<()>,
    left: &'a AtomicBool,
}

impl KeyVault for SlowVault<'_> {
    fn create(&self, secret: &SecretString) -> Result<CredentialReference, VaultError> {
        let _ = self.entered.send(());
        std::thread::sleep(Duration::from_millis(300));
        let result = self.inner.create(secret);
        self.left.store(true, Ordering::SeqCst);
        result
    }

    fn replace(&self, reference: &str, secret: &SecretString) -> Result<(), VaultError> {
        self.inner.replace(reference, secret)
    }

    fn has(&self, reference: &str) -> Result<bool, VaultError> {
        self.inner.has(reference)
    }

    fn delete(&self, reference: &str) -> Result<bool, VaultError> {
        self.inner.delete(reference)
    }

    fn prune(&self, allowed: &[&str]) -> Result<usize, VaultError> {
        self.inner.prune(allowed)
    }
}

fn fixture(name: &str) -> (Scratch, Persistence) {
    let scratch = Scratch::new(name);
    let persistence =
        bootstrap::open(scratch.path()).expect("the shell opens in its scratch directory");
    (scratch, persistence)
}

fn input(key: &str) -> ProviderKeyInput {
    ProviderKeyInput {
        provider: ProviderKeyProvider::OpenRouter,
        key: key.to_string(),
    }
}

/// A vault opener that fails the test if the service opens the vault where it must not.
fn never_opened() -> Result<FakeVault, CommandError> {
    panic!("the vault must not be opened for this call")
}

/// The reference the settings hold, read through the store the way the service reads it.
fn saved_reference(persistence: &Persistence) -> String {
    persistence
        .store
        .lock()
        .expect("the store is not poisoned")
        .settings()
        .value(OPENROUTER_REFERENCE)
        .expect("the reference is read")
        .unwrap_or_default()
}

#[test]
fn status_without_a_saved_key_answers_not_configured_and_never_opens_the_vault() {
    let (_scratch, persistence) = fixture("credentials-empty");

    let status = credentials::status(
        &persistence.store,
        never_opened,
        ProviderKeyProvider::OpenRouter,
    )
    .expect("the status is read");

    assert!(!status.configured);
    assert_eq!(status.provider, ProviderKeyProvider::OpenRouter);
}

#[test]
fn saving_keeps_the_secret_in_the_vault_and_only_its_reference_in_settings() {
    let (_scratch, persistence) = fixture("credentials-save");
    let vault = FakeVault::default();

    let status =
        credentials::save(&persistence.store, || Ok(&vault), input(KEY)).expect("the key is saved");

    assert!(status.configured);
    let reference = saved_reference(&persistence);
    assert!(
        CredentialReference::parse(&reference).is_ok(),
        "the settings hold a reference, not the key"
    );
    assert_eq!(vault.secret_under(&reference).as_deref(), Some(KEY));
    assert!(
        !format!(
            "{:?}",
            persistence
                .store
                .lock()
                .expect("the store is not poisoned")
                .settings()
                .get()
        )
        .contains(KEY),
        "the key is in no setting"
    );
}

#[test]
fn saving_again_replaces_the_secret_under_the_same_reference() {
    let (_scratch, persistence) = fixture("credentials-replace");
    let vault = FakeVault::default();
    credentials::save(&persistence.store, || Ok(&vault), input(KEY)).expect("the first save");
    let first = saved_reference(&persistence);

    credentials::save(
        &persistence.store,
        || Ok(&vault),
        input("sk-or-v1-replacement-value"),
    )
    .expect("the second save");

    assert_eq!(
        saved_reference(&persistence),
        first,
        "the reference is kept"
    );
    assert_eq!(vault.created.get(), 1, "the second save creates no secret");
    assert_eq!(
        vault.replaced.get(),
        1,
        "the second save replaces the secret"
    );
    assert_eq!(
        vault.secret_under(&first).as_deref(),
        Some("sk-or-v1-replacement-value")
    );
}

#[test]
fn an_invalid_key_is_refused_before_the_vault_opens_and_nothing_is_saved() {
    let (_scratch, persistence) = fixture("credentials-invalid");

    for key in ["short", "sk-or-v1-with space", "sk-or-v1-line\r\nbreak"] {
        let error = credentials::save(&persistence.store, never_opened, input(key))
            .expect_err("an invalid key is refused");
        assert_eq!(error, CommandError::InvalidInput, "{key:?} is refused");
    }
    assert_eq!(saved_reference(&persistence), "");
}

#[test]
fn a_missing_secure_store_is_reported_and_nothing_is_written() {
    let (_scratch, persistence) = fixture("credentials-unavailable");

    let error = credentials::save(
        &persistence.store,
        || Err::<&FakeVault, _>(map_vault(VaultError::SecureStorageUnavailable)),
        input(KEY),
    )
    .expect_err("the save fails without a store");

    assert_eq!(error, CommandError::SecureStorageUnavailable);
    assert_eq!(saved_reference(&persistence), "", "no reference is written");
}

#[test]
fn a_failed_secret_write_leaves_no_reference_behind() {
    let (_scratch, persistence) = fixture("credentials-create-fails");
    let vault = FakeVault::default();
    vault.fail(Fault::Create);

    let error = credentials::save(&persistence.store, || Ok(&vault), input(KEY))
        .expect_err("the write fails");

    assert_eq!(error, CommandError::CredentialStore);
    assert_eq!(saved_reference(&persistence), "");
    assert!(vault.holds_nothing());
}

#[test]
fn the_status_reads_the_vault_and_reports_a_secret_that_was_lost() {
    let (_scratch, persistence) = fixture("credentials-status");
    let vault = FakeVault::default();
    credentials::save(&persistence.store, || Ok(&vault), input(KEY)).expect("the key is saved");
    let reference = saved_reference(&persistence);

    assert!(
        credentials::status(
            &persistence.store,
            || Ok(&vault),
            ProviderKeyProvider::OpenRouter
        )
        .expect("the status is read")
        .configured
    );

    vault.forget(&reference);
    assert!(
        !credentials::status(
            &persistence.store,
            || Ok(&vault),
            ProviderKeyProvider::OpenRouter
        )
        .expect("the status is read")
        .configured,
        "a reference whose secret is gone is not configured"
    );
}

#[test]
fn a_failed_read_of_the_vault_is_reported_as_a_credential_store_failure() {
    let (_scratch, persistence) = fixture("credentials-read-fails");
    let vault = FakeVault::default();
    credentials::save(&persistence.store, || Ok(&vault), input(KEY)).expect("the key is saved");
    vault.fail(Fault::Has);

    let error = credentials::status(
        &persistence.store,
        || Ok(&vault),
        ProviderKeyProvider::OpenRouter,
    )
    .expect_err("the read fails");

    assert_eq!(error, CommandError::CredentialStore);
}

#[test]
fn removing_deletes_the_secret_and_clears_the_reference() {
    let (_scratch, persistence) = fixture("credentials-remove");
    let vault = FakeVault::default();
    credentials::save(&persistence.store, || Ok(&vault), input(KEY)).expect("the key is saved");

    let status = credentials::remove(
        &persistence.store,
        || Ok(&vault),
        ProviderKeyProvider::OpenRouter,
    )
    .expect("the key is removed");

    assert!(!status.configured);
    assert!(vault.holds_nothing());
    assert_eq!(saved_reference(&persistence), "");
    assert!(
        !credentials::status(
            &persistence.store,
            never_opened,
            ProviderKeyProvider::OpenRouter
        )
        .expect("the status is read")
        .configured,
        "after the removal the vault is not needed to answer"
    );
}

/// Two saves at the same time run one after the other: the second does not open its vault while the first is still in
/// its secret write. Without the lock of the service this test fails, because the second save would open at once.
#[test]
fn two_key_saves_at_the_same_time_run_one_after_the_other() {
    let (_scratch, persistence) = fixture("credentials-serial");
    let first_left = AtomicBool::new(false);
    let second_opened_early = AtomicBool::new(false);
    let (entered, started) = mpsc::channel();

    std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            let vault = SlowVault {
                inner: FakeVault::default(),
                entered,
                left: &first_left,
            };
            credentials::save(&persistence.store, || Ok(&vault), input(KEY))
        });
        started
            .recv()
            .expect("the first save reached its secret write");
        let second = scope.spawn(|| {
            credentials::save(
                &persistence.store,
                || {
                    if !first_left.load(Ordering::SeqCst) {
                        second_opened_early.store(true, Ordering::SeqCst);
                    }
                    Ok(FakeVault::default())
                },
                input(KEY),
            )
        });
        first
            .join()
            .expect("the first save finishes")
            .expect("the first key is saved");
        second
            .join()
            .expect("the second save finishes")
            .expect("the second key is saved");
    });

    assert!(
        !second_opened_early.load(Ordering::SeqCst),
        "the second save opened its vault while the first one still held it"
    );
}

/// A reference no setting names, as a save leaves when it stops after its secret write.
const ORPHAN: &str = "00000000-0000-4000-8000-0000000000ff";

/// The leftover of a stopped save is removed by the next save, and the key the settings name stays.
#[test]
fn an_orphan_left_by_a_stopped_save_is_removed_by_the_next_save() {
    let (_scratch, persistence) = fixture("credentials-orphan");
    let vault = FakeVault::default();
    vault.plant(ORPHAN, "orphan-secret-value");

    credentials::save(&persistence.store, || Ok(&vault), input(KEY)).expect("the key is saved");

    let reference = saved_reference(&persistence);
    assert_eq!(vault.secret_under(ORPHAN), None, "the leftover is removed");
    assert_eq!(
        vault.secret_under(&reference).as_deref(),
        Some(KEY),
        "the saved key stays"
    );
}

/// Removing a key also clears the leftovers, so nothing is left in the vault once the key is gone.
#[test]
fn removing_a_key_also_removes_its_leftovers() {
    let (_scratch, persistence) = fixture("credentials-remove-orphan");
    let vault = FakeVault::default();
    credentials::save(&persistence.store, || Ok(&vault), input(KEY)).expect("the key is saved");
    vault.plant(ORPHAN, "orphan-secret-value");

    credentials::remove(
        &persistence.store,
        || Ok(&vault),
        ProviderKeyProvider::OpenRouter,
    )
    .expect("the key is removed");

    assert!(
        vault.holds_nothing(),
        "the key and the leftover are both gone"
    );
}

/// A cleanup that fails does not fail the save: the key is saved and configured, and the leftover waits for a later call.
#[test]
fn a_failed_cleanup_does_not_fail_the_save() {
    let (_scratch, persistence) = fixture("credentials-prune-fails");
    let vault = FakeVault::default();
    vault.plant(ORPHAN, "orphan-secret-value");
    vault.fail(Fault::Prune);

    let status =
        credentials::save(&persistence.store, || Ok(&vault), input(KEY)).expect("the key is saved");

    assert!(status.configured);
    let reference = saved_reference(&persistence);
    assert_eq!(vault.secret_under(&reference).as_deref(), Some(KEY));
    assert_eq!(
        vault.secret_under(ORPHAN).as_deref(),
        Some("orphan-secret-value"),
        "the leftover waits for the next call"
    );
}

#[test]
fn removing_without_a_saved_key_does_not_open_the_vault() {
    let (_scratch, persistence) = fixture("credentials-remove-empty");

    let status = credentials::remove(
        &persistence.store,
        never_opened,
        ProviderKeyProvider::OpenRouter,
    )
    .expect("nothing to remove");

    assert!(!status.configured);
}

#[test]
fn a_failed_delete_keeps_the_reference_so_the_removal_can_be_retried() {
    let (_scratch, persistence) = fixture("credentials-delete-fails");
    let vault = FakeVault::default();
    credentials::save(&persistence.store, || Ok(&vault), input(KEY)).expect("the key is saved");
    let reference = saved_reference(&persistence);
    vault.fail(Fault::Delete);

    let error = credentials::remove(
        &persistence.store,
        || Ok(&vault),
        ProviderKeyProvider::OpenRouter,
    )
    .expect_err("the delete fails");

    assert_eq!(error, CommandError::CredentialStore);
    assert_eq!(
        saved_reference(&persistence),
        reference,
        "the reference stays"
    );
}

#[test]
fn the_settings_read_never_returns_the_reference() {
    let (_scratch, persistence) = fixture("credentials-settings-view");
    let vault = FakeVault::default();
    credentials::save(&persistence.store, || Ok(&vault), input(KEY)).expect("the key is saved");
    let reference = saved_reference(&persistence);
    assert!(!reference.is_empty());
    assert!(
        OPENROUTER_REFERENCE.starts_with("maintenance."),
        "the reference sits under the prefix that the settings reads hide"
    );
    let shared = persistence
        .store
        .lock()
        .expect("the store is not poisoned")
        .settings()
        .get()
        .expect("the settings are read");
    assert!(
        !shared.contains_key(OPENROUTER_REFERENCE),
        "the shared settings read hides the reference"
    );

    let settings =
        services::settings_get(&persistence.store.lock().expect("the store is not poisoned"))
            .expect("the settings are read");
    let json = serde_json::to_string(&settings).expect("the settings serialize");

    assert!(!json.contains("providers"), "{json}");
    assert!(!json.contains(&reference), "{json}");
    assert!(!json.contains(KEY), "{json}");
}

#[test]
fn no_reply_or_error_carries_the_key() {
    let (_scratch, persistence) = fixture("credentials-redaction");
    let vault = FakeVault::default();

    let saved = credentials::save(&persistence.store, || Ok(&vault), input(KEY)).expect("saved");
    let status = serde_json::to_string(&saved).expect("the status serializes");
    assert!(!status.contains(KEY), "{status}");

    let refused = credentials::save(
        &persistence.store,
        || Err::<&FakeVault, _>(map_vault(VaultError::WriteFailed)),
        input(KEY),
    )
    .expect_err("the open fails");
    let error = serde_json::to_string(&refused).expect("the error serializes");
    assert!(!error.contains(KEY), "{error}");
    assert!(!format!("{:?}", input(KEY)).contains(KEY));
}

#[test]
fn vault_failures_map_to_the_codes_the_interface_shows() {
    assert_eq!(
        map_vault(VaultError::SecureStorageUnavailable),
        CommandError::SecureStorageUnavailable
    );
    assert_eq!(
        map_vault(VaultError::InvalidSecret),
        CommandError::InvalidInput
    );
    for error in [
        VaultError::InvalidReference,
        VaultError::CorruptStore,
        VaultError::MasterKeyMissing,
        VaultError::WriteFailed,
        VaultError::ReadFailed,
    ] {
        assert_eq!(map_vault(error), CommandError::CredentialStore, "{error:?}");
    }
    assert_eq!(
        CommandError::SecureStorageUnavailable.code(),
        "secure-storage-unavailable"
    );
    assert_eq!(CommandError::CredentialStore.code(), "credential-store");
}
