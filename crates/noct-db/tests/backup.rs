//! The backup export and import, checked against `BackupRepository.ts`. The row gate compares the
//! export, the metrics and the imports of a whole database with the TypeScript repository; these
//! tests cover the rules of the import and the rollback of a refused one.

use noct_core::limits::DATABASE_SCHEMA_VERSION;
use noct_db::Database;
use noct_db::repositories::backup::{Backup, ImportData, ImportScope};
use rusqlite::params;
use serde_json::{Value, json};

mod common;
use common::scratch;

const AT: &str = "2026-01-01T00:00:00.000Z";
const WORKSPACE: &str = "/work/backup";

fn open(name: &str) -> Database {
    let db = Database::open(&scratch(name)).expect("open database");
    db.connection()
        .execute_batch(&format!(
            "INSERT INTO workspaces(path,name,favorite,authorized,created_at,last_opened_at) \
                 VALUES('{WORKSPACE}','backup',0,0,'{AT}','{AT}');\
             INSERT INTO conversations(id,title,workspace,created_at,updated_at) \
                 VALUES('conv-1','Conversa original','{WORKSPACE}','{AT}','{AT}');\
             INSERT INTO messages(id,conversation_id,role,content,metadata,created_at) \
                 VALUES('msg-1','conv-1','user','olá',NULL,'{AT}');"
        ))
        .expect("seed");
    db
}

fn one_text(db: &Database, sql: &str) -> String {
    db.connection()
        .query_row(sql, [], |row| row.get(0))
        .expect("one text")
}

fn one_int(db: &Database, sql: &str) -> i64 {
    db.connection()
        .query_row(sql, [], |row| row.get(0))
        .expect("one integer")
}

fn data(document: Value) -> ImportData {
    serde_json::from_value(document).expect("import document")
}

/// A document that restores one workspace, one conversation with one message and one setting.
fn restored() -> Value {
    json!({
        "workspaces": [{
            "path": "/work/restored",
            "name": "restaurado",
            "favorite": 1,
            "authorized": 1,
            "created_at": AT,
            "last_opened_at": AT
        }],
        "conversations": [{
            "id": "conv-2",
            "title": "Restaurada",
            "workspace": "/work/restored",
            "created_at": AT,
            "updated_at": AT,
            "unknownColumn": "ignorada"
        }],
        "messages": [{
            "id": "msg-2",
            "conversation_id": "conv-2",
            "role": "assistant",
            "content": "ok",
            "metadata": null,
            "created_at": AT
        }],
        "settings": { "theme": "light" }
    })
}

/// An export without its clock, which the gate and these tests compare.
fn without_clock(mut document: Value) -> Value {
    document
        .as_object_mut()
        .expect("object")
        .remove("exportedAt");
    document
}

#[test]
fn export_lists_every_collection_and_hides_the_runtime_settings() {
    let db = open("backup-export");
    db.connection()
        .execute_batch(
            "INSERT INTO settings(key,value) VALUES('theme','dark') \
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value;\
             INSERT INTO settings(key,value) VALUES('maintenance.lastQuickCheck','2026-01-01') \
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value;",
        )
        .expect("settings");

    let document = Backup::new(db.connection()).export().expect("export");

    assert_eq!(document["schemaVersion"], json!(DATABASE_SCHEMA_VERSION));
    assert!(document["exportedAt"].is_string());
    assert_eq!(
        document["conversations"].as_array().expect("array").len(),
        1
    );
    assert_eq!(document["messages"][0]["content"], json!("olá"));
    assert_eq!(document["settings"]["theme"], json!("dark"));
    assert!(
        document["settings"]
            .get("maintenance.lastQuickCheck")
            .is_none()
    );
    for collection in [
        "workspaces",
        "artifacts",
        "memories",
        "brainMemories",
        "brainMemoryHistory",
        "suggestions",
        "suggestionDecisions",
        "providerConfigs",
        "modelCatalog",
        "workspaceModelBindings",
    ] {
        assert!(document[collection].is_array(), "{collection}");
    }
}

#[test]
fn a_full_import_replaces_the_data_and_marks_restored_workspaces_unauthorized() {
    let db = open("backup-full");

    Backup::new(db.connection())
        .import(&data(restored()), ImportScope::Full)
        .expect("import");

    assert_eq!(
        one_text(&db, "SELECT path FROM workspaces"),
        "/work/restored"
    );
    assert_eq!(
        one_int(
            &db,
            "SELECT authorized FROM workspaces WHERE path='/work/restored'"
        ),
        0
    );
    assert_eq!(
        one_int(
            &db,
            "SELECT favorite FROM workspaces WHERE path='/work/restored'"
        ),
        1
    );
    assert_eq!(
        one_text(&db, "SELECT title FROM conversations"),
        "Restaurada"
    );
    assert_eq!(one_text(&db, "SELECT id FROM messages"), "msg-2");
    assert_eq!(
        one_text(&db, "SELECT value FROM settings WHERE key='theme'"),
        "light"
    );
}

#[test]
fn a_project_data_import_keeps_the_settings() {
    let db = open("backup-project");
    db.connection()
        .execute_batch("INSERT INTO settings(key,value) VALUES('theme','dark');")
        .expect("settings");

    Backup::new(db.connection())
        .import(&data(restored()), ImportScope::ProjectData)
        .expect("import");

    assert_eq!(
        one_text(&db, "SELECT title FROM conversations"),
        "Restaurada"
    );
    assert_eq!(
        one_text(&db, "SELECT value FROM settings WHERE key='theme'"),
        "dark"
    );
}

#[test]
fn an_openai_compatible_provider_is_restored_as_chat_completions() {
    let db = open("backup-provider");
    let document = json!({
        "providerConfigs": [{
            "id": "prov-1",
            "provider_type": "openai-compatible",
            "display_name": "Local",
            "source": "local",
            "base_url": "http://127.0.0.1:1234/v1",
            "enabled": 1,
            "requires_authentication": 0,
            "timeout_ms": 30000,
            "created_at": AT,
            "updated_at": AT
        }]
    });
    let mut full = restored();
    full["providerConfigs"] = document["providerConfigs"].clone();

    Backup::new(db.connection())
        .import(&data(full), ImportScope::Full)
        .expect("import");

    assert_eq!(
        one_text(
            &db,
            "SELECT provider_type FROM provider_configs WHERE id='prov-1'"
        ),
        "chat-completions"
    );
}

#[test]
fn unlisted_columns_and_rows_that_are_not_objects_are_ignored() {
    let db = open("backup-allow-list");
    let document = json!({
        "workspaces": [{
            "path": WORKSPACE,
            "name": "backup",
            "favorite": 0,
            "created_at": AT,
            "last_opened_at": AT
        }],
        "conversations": [{
            "id": "conv-1",
            "title": "Conversa original",
            "workspace": WORKSPACE,
            "created_at": AT,
            "updated_at": AT,
            "unlisted": true
        }],
        "messages": [
            "texto solto",
            null,
            {
                "id": "msg-2",
                "conversation_id": "conv-1",
                "role": "user",
                "content": "novo",
                "metadata": null,
                "created_at": AT,
                "secret": "ignored"
            }
        ]
    });

    Backup::new(db.connection())
        .import(&data(document), ImportScope::Full)
        .expect("import");

    assert_eq!(one_int(&db, "SELECT COUNT(*) FROM messages"), 1);
    assert_eq!(one_text(&db, "SELECT id FROM messages"), "msg-2");
}

#[test]
fn a_refused_value_rolls_the_whole_import_back() {
    let db = open("backup-refused");
    let before = without_clock(Backup::new(db.connection()).export().expect("before"));
    let mut document = restored();
    document["conversations"] = json!([{
        "id": "conv-2",
        "title": "ruim\u{0}",
        "workspace": "/work/restored",
        "created_at": AT,
        "updated_at": AT
    }]);

    let result = Backup::new(db.connection()).import(&data(document), ImportScope::Full);

    assert!(result.is_err());
    let after = without_clock(Backup::new(db.connection()).export().expect("after"));
    assert_eq!(before, after);
}

#[test]
fn text_longer_than_the_limit_is_refused() {
    let db = open("backup-long-text");
    let mut document = restored();
    document["conversations"][0]["title"] = json!("a".repeat(10_000_001));

    let result = Backup::new(db.connection()).import(&data(document), ImportScope::Full);

    assert!(result.is_err());
    assert_eq!(
        one_text(&db, "SELECT title FROM conversations"),
        "Conversa original"
    );
}

#[test]
fn an_export_imported_again_reproduces_the_same_document() {
    let db = open("backup-round-trip");
    db.connection()
        .execute_batch("INSERT INTO settings(key,value) VALUES('theme','dark');")
        .expect("settings");
    let exported = Backup::new(db.connection()).export().expect("export");

    Backup::new(db.connection())
        .import(&data(exported.clone()), ImportScope::Full)
        .expect("import");

    let again = Backup::new(db.connection()).export().expect("export again");
    assert_eq!(without_clock(exported), without_clock(again));
}

#[test]
fn metrics_count_the_records_and_the_content() {
    let db = open("backup-metrics");
    let before = Backup::new(db.connection())
        .export_metrics()
        .expect("metrics before");

    db.connection()
        .execute(
            "INSERT INTO conversations(id,title,workspace,created_at,updated_at) \
             VALUES('conv-9', ?1, ?2, ?3, ?3)",
            params!["x".repeat(100), WORKSPACE, AT],
        )
        .expect("conversation");
    let after = Backup::new(db.connection())
        .export_metrics()
        .expect("metrics after");

    assert_eq!(after.records, before.records + 1);
    assert!(after.estimated_bytes >= before.estimated_bytes + 100 + 64);
}
