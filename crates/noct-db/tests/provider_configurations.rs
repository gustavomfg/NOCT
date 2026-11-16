//! Provider configurations, checked against `ProviderConfigurationRepository.ts`.

use noct_core::ai::ProviderSource;
use noct_core::ai::provider_configuration::{
    ProviderConfigurationInput, ProviderConfigurationType, identifier,
};
use noct_db::repositories::provider_configurations::{CredentialChange, ProviderConfigurations};
use noct_db::{Database, Error};

mod common;
use common::scratch;

const REFERENCE: &str = "6f9619ff-8b86-4d11-b42d-00c04fc964ff";
const OTHER_REFERENCE: &str = "0f3e5b8a-2c4d-4e6f-8a9b-1c2d3e4f5a6b";

fn input(name: &str, enabled: bool) -> ProviderConfigurationInput {
    ProviderConfigurationInput {
        provider_type: ProviderConfigurationType::ChatCompletions,
        display_name: name.to_string(),
        source: ProviderSource::Local,
        base_url: "http://127.0.0.1:11434".to_string(),
        enabled,
        requires_authentication: true,
        timeout_ms: 30_000,
    }
}

fn message(error: Error) -> String {
    match error {
        Error::Message(text) => text,
        other => panic!("expected a user-facing message, got {other}"),
    }
}

fn open(name: &str) -> Database {
    Database::open_runtime(&scratch(name)).expect("open database")
}

#[test]
fn create_returns_a_summary_with_a_generated_identifier() {
    let db = open("provider-create");
    let providers = ProviderConfigurations::new(db.connection());

    let created = providers
        .create(
            ProviderConfigurationInput {
                display_name: "  Local  ".to_string(),
                base_url: " http://127.0.0.1:11434 ".to_string(),
                ..input("x", true)
            },
            None,
        )
        .expect("create");

    assert_eq!(
        identifier(&created.id).expect("canonical identifier"),
        created.id
    );
    assert_eq!(created.id, created.id.to_lowercase());
    assert_eq!(created.display_name, "Local");
    assert_eq!(created.base_url, "http://127.0.0.1:11434");
    assert!(created.enabled);
    assert!(!created.credential_configured);
    assert_eq!(created.created_at, created.updated_at);
    assert_eq!(providers.get(&created.id).expect("get"), Some(created));
}

#[test]
fn the_summary_never_carries_the_credential_reference() {
    let db = open("provider-reference-hidden");
    let providers = ProviderConfigurations::new(db.connection());

    let created = providers
        .create(input("Remoto", true), Some(REFERENCE))
        .expect("create");

    assert!(created.credential_configured);
    let serialized = serde_json::to_string(&created).expect("json");
    assert!(
        !serialized.contains(REFERENCE),
        "the reference must not reach the renderer"
    );
    assert_eq!(
        providers
            .credential_reference(&created.id)
            .expect("reference"),
        Some(REFERENCE.to_string()),
    );
}

#[test]
fn invalid_input_or_reference_writes_nothing() {
    let db = open("provider-invalid-create");
    let providers = ProviderConfigurations::new(db.connection());

    let too_fast = ProviderConfigurationInput {
        timeout_ms: 999,
        ..input("Lento", true)
    };
    assert!(providers.create(too_fast, None).is_err());
    assert!(
        providers
            .create(input("Ruim", true), Some("not-a-uuid"))
            .is_err()
    );
    assert!(providers.list().expect("list").is_empty());
}

#[test]
fn a_credential_reference_belongs_to_one_configuration() {
    let db = open("provider-unique-reference");
    let providers = ProviderConfigurations::new(db.connection());
    providers
        .create(input("Primeiro", true), Some(REFERENCE))
        .expect("first");

    assert!(
        providers
            .create(input("Segundo", true), Some(REFERENCE))
            .is_err()
    );
    assert_eq!(providers.list().expect("list").len(), 1);
}

#[test]
fn update_keeps_sets_or_clears_the_credential_reference() {
    let db = open("provider-update-credential");
    let providers = ProviderConfigurations::new(db.connection());
    let created = providers
        .create(input("Remoto", true), Some(REFERENCE))
        .expect("create");

    let kept = providers
        .update(
            &created.id,
            input("Remoto v2", true),
            CredentialChange::Keep,
        )
        .expect("keep");
    assert_eq!(kept.display_name, "Remoto v2");
    assert!(kept.credential_configured);
    assert_eq!(
        providers.credential_reference(&created.id).expect("kept"),
        Some(REFERENCE.to_string()),
    );

    providers
        .update(
            &created.id,
            input("Remoto v2", true),
            CredentialChange::Set(None),
        )
        .expect("clear");
    assert_eq!(
        providers
            .credential_reference(&created.id)
            .expect("cleared"),
        None
    );

    let replaced = providers
        .update(
            &created.id,
            input("Remoto v2", true),
            CredentialChange::Set(Some(OTHER_REFERENCE)),
        )
        .expect("replace");
    assert!(replaced.credential_configured);
    assert_eq!(
        providers
            .credential_reference(&created.id)
            .expect("replaced"),
        Some(OTHER_REFERENCE.to_string()),
    );
}

#[test]
fn set_credential_reference_changes_only_the_reference() {
    let db = open("provider-set-reference");
    let providers = ProviderConfigurations::new(db.connection());
    let created = providers
        .create(input("Local", false), None)
        .expect("create");

    let updated = providers
        .set_credential_reference(&created.id, Some(REFERENCE))
        .expect("set");

    assert!(updated.credential_configured);
    assert!(!updated.enabled);
    assert_eq!(updated.display_name, "Local");
    assert_eq!(
        providers.credential_references().expect("references"),
        vec![REFERENCE.to_string()],
    );
}

#[test]
fn unknown_configurations_are_reported_as_missing() {
    let db = open("provider-missing");
    let providers = ProviderConfigurations::new(db.connection());
    let unknown = "6f9619ff-8b86-4d11-b42d-00c04fc964ff";

    assert_eq!(
        message(
            providers
                .update(unknown, input("x", true), CredentialChange::Keep)
                .expect_err("update")
        ),
        "Configuração de Provider não encontrada.",
    );
    assert_eq!(
        message(
            providers
                .set_credential_reference(unknown, None)
                .expect_err("set")
        ),
        "Configuração de Provider não encontrada.",
    );
    assert_eq!(
        message(
            providers
                .credential_reference(unknown)
                .expect_err("reference")
        ),
        "Configuração de Provider não encontrada.",
    );
    assert_eq!(providers.get(unknown).expect("get"), None);
}

#[test]
fn malformed_identifiers_are_rejected_before_the_database_is_read() {
    let db = open("provider-malformed-id");
    let providers = ProviderConfigurations::new(db.connection());

    assert!(providers.get("not-a-uuid").is_err());
    assert!(providers.delete("not-a-uuid").is_err());
    assert!(providers.credential_reference("not-a-uuid").is_err());
    assert!(
        providers
            .update("not-a-uuid", input("x", true), CredentialChange::Keep)
            .is_err()
    );
}

#[test]
fn list_puts_enabled_configurations_first_then_the_most_recently_updated() {
    let db = open("provider-order");
    let providers = ProviderConfigurations::new(db.connection());
    let disabled = providers
        .create(input("Desligado", false), None)
        .expect("disabled");
    let older = providers
        .create(input("Antigo", true), None)
        .expect("older");
    let newer = providers
        .create(input("Recente", true), None)
        .expect("newer");
    // Timestamps are fixed here so the order does not depend on the clock.
    db.connection()
        .execute_batch(&format!(
            "UPDATE provider_configs SET updated_at='2026-01-01T00:00:01.000Z' WHERE id='{}';\
             UPDATE provider_configs SET updated_at='2026-01-01T00:00:02.000Z' WHERE id='{}';\
             UPDATE provider_configs SET updated_at='2026-01-01T00:00:03.000Z' WHERE id='{}';",
            older.id, newer.id, disabled.id,
        ))
        .expect("fix timestamps");

    let names: Vec<String> = providers
        .list()
        .expect("list")
        .into_iter()
        .map(|row| row.display_name)
        .collect();
    assert_eq!(names, vec!["Recente", "Antigo", "Desligado"]);
}

#[test]
fn delete_reports_the_reference_it_removed() {
    let db = open("provider-delete");
    let providers = ProviderConfigurations::new(db.connection());
    let created = providers
        .create(input("Remoto", true), Some(REFERENCE))
        .expect("create");

    let deleted = providers.delete(&created.id).expect("delete");
    assert!(deleted.deleted);
    assert_eq!(deleted.credential_reference, Some(REFERENCE.to_string()));
    assert_eq!(providers.get(&created.id).expect("gone"), None);

    let again = providers.delete(&created.id).expect("delete again");
    assert!(!again.deleted);
    assert_eq!(again.credential_reference, None);
}

#[test]
fn a_stored_row_that_breaks_the_contract_is_rejected_on_read() {
    let db = open("provider-corrupt-row");
    let providers = ProviderConfigurations::new(db.connection());
    let created = providers
        .create(input("Local", true), None)
        .expect("create");
    db.connection()
        .execute(
            "UPDATE provider_configs SET provider_type='openai-compatible' WHERE id=?1",
            [&created.id],
        )
        .expect("corrupt");

    assert_eq!(
        message(providers.get(&created.id).expect_err("get")),
        "A configuração de Provider persistida está inválida.",
    );
}
