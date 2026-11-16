//! Application settings. Port of `SettingsRepository.ts`.

use std::collections::BTreeMap;

use rusqlite::{Connection, params};

use crate::{Result, transaction};

pub struct Settings<'a> {
    conn: &'a Connection,
}

impl<'a> Settings<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Every user setting. Keys owned by the runtime (`maintenance.*`) are hidden, and
    /// three keys always carry a value the application relies on.
    pub fn get(&self) -> Result<BTreeMap<String, String>> {
        let mut statement = self
            .conn
            .prepare("SELECT key,value FROM settings WHERE key NOT LIKE 'maintenance.%'")?;
        let mut settings = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
        constrain(&mut settings, "approvalPolicy", "untrusted", "on-request");
        constrain(&mut settings, "theme", "light", "dark");
        constrain(&mut settings, "language", "en", "pt-BR");
        Ok(settings)
    }

    pub fn set(&self, values: &BTreeMap<String, String>) -> Result<()> {
        transaction::run(self.conn, |conn| {
            let mut statement = conn.prepare(
                "INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            )?;
            for (key, value) in values {
                statement.execute(params![key, value])?;
            }
            Ok(())
        })
    }
}

/// Replaces a value that is not the allowed one, or that is absent, with its fallback.
fn constrain(settings: &mut BTreeMap<String, String>, key: &str, allowed: &str, fallback: &str) {
    if settings.get(key).map(String::as_str) != Some(allowed) {
        settings.insert(key.to_string(), fallback.to_string());
    }
}
