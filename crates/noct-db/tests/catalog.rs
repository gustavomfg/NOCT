//! Model catalog, workspace model bindings and workspace relocation, checked against
//! `ModelCatalogRepository.ts`, `WorkspaceModelBindingRepository.ts` and
//! `WorkspaceRepository.ts`.

use noct_core::ai::bindings::WorkspaceModelBindings;
use noct_core::ai::model::ModelReference;
use noct_db::repositories::artifacts::{Artifacts, NewArtifact};
use noct_db::repositories::conversations::Conversations;
use noct_db::repositories::model_catalog::ModelCatalog;
use noct_db::repositories::workspace_model_bindings::ModelBindings;
use noct_db::repositories::workspaces::Workspaces;
use noct_db::{Database, Error};
use serde_json::{Value, json};

mod common;
use common::scratch;

fn descriptor(provider: &str, model: &str) -> Value {
    json!({
        "providerId": provider,
        "modelId": model,
        "displayName": format!("Modelo {model}"),
        "source": "local",
        "capabilities": ["chat", "streaming"],
        "contextWindow": 8192,
        "availability": "available",
    })
}

fn message(error: Error) -> String {
    match error {
        Error::Message(text) => text,
        other => panic!("expected a user-facing message, got {other:?}"),
    }
}

#[test]
fn replacing_a_provider_lists_its_models_in_order_and_drops_the_old_ones() {
    let db = Database::open_runtime(&scratch("catalog-replace")).unwrap();
    let catalog = ModelCatalog::new(db.connection());
    catalog
        .replace_provider_models(
            "provider-1",
            &[descriptor("provider-1", "b"), descriptor("provider-1", "a")],
        )
        .unwrap();
    catalog
        .replace_provider_models("provider-2", &[descriptor("provider-2", "z")])
        .unwrap();

    let all: Vec<(String, String)> = catalog
        .list(None)
        .unwrap()
        .into_iter()
        .map(|model| (model.provider_id, model.model_id))
        .collect();
    assert_eq!(
        all,
        [
            ("provider-1".into(), "a".into()),
            ("provider-1".into(), "b".into()),
            ("provider-2".into(), "z".into())
        ],
    );

    catalog
        .replace_provider_models("provider-1", &[descriptor("provider-1", "c")])
        .unwrap();
    let first: Vec<String> = catalog
        .list(Some("provider-1"))
        .unwrap()
        .into_iter()
        .map(|model| model.model_id)
        .collect();
    assert_eq!(first, ["c"]);
}

#[test]
fn a_rejected_catalog_replacement_leaves_the_previous_models_in_place() {
    let db = Database::open_runtime(&scratch("catalog-reject")).unwrap();
    let catalog = ModelCatalog::new(db.connection());
    catalog
        .replace_provider_models("provider-1", &[descriptor("provider-1", "kept")])
        .unwrap();

    let other_provider = catalog.replace_provider_models(
        "provider-1",
        &[descriptor("provider-1", "a"), descriptor("provider-9", "b")],
    );
    assert!(message(other_provider.unwrap_err()).contains("outro Provider"));

    let duplicated = catalog.replace_provider_models(
        "provider-1",
        &[descriptor("provider-1", "a"), descriptor("provider-1", "a")],
    );
    assert!(message(duplicated.unwrap_err()).contains("duplicados"));

    let mut invalid = descriptor("provider-1", "bad");
    invalid["contextWindow"] = json!(0);
    assert!(
        catalog
            .replace_provider_models("provider-1", &[invalid])
            .is_err()
    );

    let remaining: Vec<String> = catalog
        .list(Some("provider-1"))
        .unwrap()
        .into_iter()
        .map(|model| model.model_id)
        .collect();
    assert_eq!(remaining, ["kept"]);
}

#[test]
fn stored_models_are_stored_trimmed_and_checked_when_read() {
    let db = Database::open_runtime(&scratch("catalog-corrupt")).unwrap();
    let catalog = ModelCatalog::new(db.connection());
    let mut padded = descriptor(" provider-1 ", " model-1 ");
    padded["displayName"] = json!("  Nome  ");
    let stored = catalog
        .replace_provider_models("provider-1", &[padded])
        .unwrap();
    assert_eq!(stored[0].model_id, "model-1");

    let text: String = db
        .connection()
        .query_row("SELECT descriptor FROM model_catalog", [], |row| row.get(0))
        .unwrap();
    assert!(
        text.starts_with(r#"{"providerId":"provider-1","modelId":"model-1","displayName":"Nome""#)
    );

    db.connection()
        .execute(
            "UPDATE model_catalog SET descriptor='{\"providerId\":\"x\"}'",
            [],
        )
        .unwrap();
    let read = catalog.list(None);
    assert_eq!(
        message(read.unwrap_err()),
        "O catálogo de modelos persistido está inválido.",
    );
}

#[test]
fn deleting_a_provider_reports_how_many_models_went() {
    let db = Database::open_runtime(&scratch("catalog-delete")).unwrap();
    let catalog = ModelCatalog::new(db.connection());
    catalog
        .replace_provider_models(
            "provider-1",
            &[descriptor("provider-1", "a"), descriptor("provider-1", "b")],
        )
        .unwrap();
    assert_eq!(catalog.delete_provider_models("provider-1").unwrap(), 2);
    assert_eq!(catalog.delete_provider_models("provider-1").unwrap(), 0);
}

fn bindings(workspace: &str, default_model: &str) -> WorkspaceModelBindings {
    WorkspaceModelBindings {
        workspace_id: workspace.to_string(),
        default_binding: Some(ModelReference {
            provider_id: "provider-1".to_string(),
            model_id: default_model.to_string(),
        }),
        embedding_binding: None,
        remote_embedding_allowed: Some(false),
    }
}

#[test]
fn bindings_store_one_record_per_workspace_and_validate_on_write() {
    let db = Database::open_runtime(&scratch("bindings")).unwrap();
    let workspaces = Workspaces::new(db.connection());
    workspaces.touch("/work/alpha").unwrap();
    workspaces.touch("/work/beta").unwrap();
    let store = ModelBindings::new(db.connection());
    store.set(bindings("/work/alpha", "first")).unwrap();
    store.set(bindings("/work/alpha", "second")).unwrap();
    store.set(bindings("/work/beta", "other")).unwrap();

    let alpha = store.get("/work/alpha").unwrap().expect("alpha bindings");
    assert_eq!(alpha.default_binding.unwrap().model_id, "second");
    assert_eq!(store.list().unwrap().len(), 2);

    let mut empty = bindings("   ", "x");
    assert!(store.set(empty.clone()).is_err());
    empty.workspace_id = "/work/gamma".to_string();
    empty.default_binding.as_mut().unwrap().model_id = " ".to_string();
    assert!(store.set(empty).is_err());

    assert!(store.delete("/work/alpha").unwrap());
    assert!(!store.delete("/work/alpha").unwrap());
    assert!(store.get("/work/alpha").unwrap().is_none());
}

#[test]
fn relocating_a_workspace_moves_its_rows_and_bindings_and_keeps_its_favorite() {
    let db = Database::open_runtime(&scratch("relocate")).unwrap();
    let source = "/work/original";
    let destination = "/work/moved";
    Workspaces::new(db.connection()).touch(source).unwrap();
    Workspaces::new(db.connection())
        .set_favorite(source, true)
        .unwrap();
    let chat = Conversations::new(db.connection()).create(source).unwrap();
    let artifact = Artifacts::new(db.connection())
        .add(&NewArtifact {
            conversation_id: &chat.id,
            workspace: source,
            kind: "document",
            title: "Plano",
            file_path: None,
            content: Some("texto"),
            metadata: None,
        })
        .unwrap();
    ModelBindings::new(db.connection())
        .set(bindings(source, "model"))
        .unwrap();

    Workspaces::new(db.connection())
        .relocate(source, destination)
        .unwrap();

    let rows = Workspaces::new(db.connection()).list().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].path, destination);
    assert_eq!(rows[0].name, "moved");
    assert!(rows[0].favorite);
    assert!(rows[0].authorized);

    let moved_chat = Conversations::new(db.connection())
        .get(&chat.id)
        .unwrap()
        .unwrap();
    assert_eq!(moved_chat.workspace, destination);
    let moved_artifact = Artifacts::new(db.connection()).list(&chat.id).unwrap();
    assert_eq!(moved_artifact[0].id, artifact.id);
    assert_eq!(moved_artifact[0].workspace, destination);

    let model_bindings = ModelBindings::new(db.connection());
    assert!(model_bindings.get(source).unwrap().is_none());
    let moved_bindings = model_bindings
        .get(destination)
        .unwrap()
        .expect("bindings followed");
    assert_eq!(moved_bindings.workspace_id, destination);
}

#[test]
fn relocation_refuses_a_missing_source_or_an_occupied_destination_without_writing() {
    let db = Database::open_runtime(&scratch("relocate-refuse")).unwrap();
    let workspaces = Workspaces::new(db.connection());
    workspaces.touch("/work/one").unwrap();
    workspaces.touch("/work/two").unwrap();
    let chat = Conversations::new(db.connection())
        .create("/work/one")
        .unwrap();

    let missing = workspaces.relocate("/work/absent", "/work/free");
    assert!(message(missing.unwrap_err()).contains("não encontrado"));

    let occupied = workspaces.relocate("/work/one", "/work/two");
    assert!(message(occupied.unwrap_err()).contains("já pertence"));

    let still_there = Conversations::new(db.connection())
        .get(&chat.id)
        .unwrap()
        .unwrap();
    assert_eq!(still_there.workspace, "/work/one");
    assert_eq!(workspaces.list().unwrap().len(), 2);
}

#[test]
fn relocating_onto_the_same_folder_only_records_it_as_opened() {
    let db = Database::open_runtime(&scratch("relocate-same")).unwrap();
    let workspaces = Workspaces::new(db.connection());
    workspaces.touch("/work/same").unwrap();
    workspaces.relocate("/work/same", "/work/same").unwrap();
    let rows = workspaces.list().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].path, "/work/same");
}
