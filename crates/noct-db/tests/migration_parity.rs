//! Schema parity with the TypeScript migrations.
//!
//! `fixtures/schema-hashes.json` is produced by `scripts/generate-schema-fixtures.mjs`,
//! which runs the real `electron/database/migrations.ts` under better-sqlite3. Each
//! fingerprint covers the user version, every schema object and every column. A
//! mismatch means the Rust migrations no longer produce the schema the TypeScript
//! application produces, so user data would diverge between the two implementations.

use std::collections::BTreeMap;

use noct_db::{migrations, schema};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

const FIXTURE: &str = include_str!("fixtures/schema-hashes.json");

struct Reference {
    latest: u32,
    hashes: BTreeMap<String, String>,
}

fn reference() -> Reference {
    let value: serde_json::Value = serde_json::from_str(FIXTURE).expect("fixture is valid JSON");
    let latest = u32::try_from(value["latest"].as_u64().expect("latest version")).unwrap();
    let hashes = value["hashes"]
        .as_object()
        .expect("hashes object")
        .iter()
        .map(|(name, hash)| {
            (
                name.clone(),
                hash.as_str().expect("hash string").to_string(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    Reference { latest, hashes }
}

/// Matches the runtime's connection setup. better-sqlite3 enables foreign keys by
/// default, and the migrations were fingerprinted with them on.
fn connection() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory database");
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .expect("foreign keys");
    conn
}

fn fingerprint(conn: &Connection) -> String {
    let dump = schema::dump(conn).expect("schema dump");
    Sha256::digest(dump.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn fixture_covers_every_scenario() {
    let reference = reference();
    let expected = 1 + 2 * (reference.latest as usize - 1);
    assert_eq!(
        reference.latest,
        noct_core::limits::DATABASE_SCHEMA_VERSION as u32
    );
    assert_eq!(reference.hashes.len(), expected);
    assert!(reference.hashes.contains_key("fresh"));
}

#[test]
fn fresh_database_matches_typescript() {
    let reference = reference();
    let mut conn = connection();
    migrations::migrate_latest(&mut conn, 0).expect("fresh migration");
    assert_eq!(fingerprint(&conn), reference.hashes["fresh"]);
}

#[test]
fn every_legacy_state_matches_typescript() {
    let reference = reference();
    for version in 1..reference.latest {
        let mut conn = connection();
        migrations::migrate_to(&mut conn, 0, version).expect("state migration");
        let name = format!("state-{version:02}");
        assert_eq!(fingerprint(&conn), reference.hashes[&name], "{name}");
    }
}

#[test]
fn every_upgrade_path_matches_typescript() {
    let reference = reference();
    for version in 1..reference.latest {
        let mut conn = connection();
        migrations::migrate_to(&mut conn, 0, version).expect("legacy state");
        migrations::migrate_latest(&mut conn, version).expect("upgrade");
        let name = format!("upgrade-{version:02}");
        assert_eq!(fingerprint(&conn), reference.hashes[&name], "{name}");
    }
}
