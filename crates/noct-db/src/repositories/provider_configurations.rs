//! Provider configurations. Port of `electron/database/ProviderConfigurationRepository.ts`.
//!
//! A configuration never holds secret material. Its credential is a reference into the
//! operating system store, managed by `noct-secrets`; this repository only stores and
//! checks that reference.

use noct_core::ai::ProviderSource;
use noct_core::ai::provider_configuration::{
    ProviderConfigurationInput, ProviderConfigurationSummary, identifier,
};
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::clock::iso_now;
use crate::codec::{enum_text, enum_value};
use crate::{Error, Result};

const NOT_FOUND: &str = "Configuração de Provider não encontrada.";
const INVALID_ROW: &str = "A configuração de Provider persistida está inválida.";

const SELECT: &str = "SELECT id, provider_type, display_name, source, base_url, enabled, \
    requires_authentication, credential_ref, timeout_ms, created_at, updated_at \
    FROM provider_configs";

const INSERT: &str = "INSERT INTO provider_configs(\
    id, provider_type, display_name, source, base_url, enabled, \
    requires_authentication, credential_ref, timeout_ms, created_at, updated_at\
    ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)";

const UPDATE_KEEPING_CREDENTIAL: &str = "UPDATE provider_configs SET \
    provider_type=?1, display_name=?2, source=?3, base_url=?4, enabled=?5, \
    requires_authentication=?6, timeout_ms=?7, updated_at=?8 WHERE id=?9";

const UPDATE_SETTING_CREDENTIAL: &str = "UPDATE provider_configs SET \
    provider_type=?1, display_name=?2, source=?3, base_url=?4, enabled=?5, \
    requires_authentication=?6, timeout_ms=?7, credential_ref=?8, updated_at=?9 \
    WHERE id=?10";

/// What an update does with the credential reference. The TypeScript `update` changes it
/// only when the optional argument is passed, so both outcomes are explicit here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialChange<'a> {
    Keep,
    Set(Option<&'a str>),
}

/// The outcome of [`ProviderConfigurations::delete`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deletion {
    pub deleted: bool,
    /// The reference the removed configuration held, so the caller can remove its secret.
    pub credential_reference: Option<String>,
}

pub struct ProviderConfigurations<'a> {
    conn: &'a Connection,
}

impl<'a> ProviderConfigurations<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list(&self) -> Result<Vec<ProviderConfigurationSummary>> {
        let mut statement = self
            .conn
            .prepare(&format!("{SELECT} ORDER BY enabled DESC, updated_at DESC"))?;
        let rows = statement
            .query_map([], read_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().map(summary).collect()
    }

    pub fn get(&self, id: &str) -> Result<Option<ProviderConfigurationSummary>> {
        let id = identifier(id)?;
        let row = self
            .conn
            .query_row(&format!("{SELECT} WHERE id=?1"), [&id], read_row)
            .optional()?;
        row.map(summary).transpose()
    }

    /// Stores a new configuration with a generated identifier, and returns it.
    pub fn create(
        &self,
        input: ProviderConfigurationInput,
        credential_reference: Option<&str>,
    ) -> Result<ProviderConfigurationSummary> {
        let input = input.validated()?;
        let credential_reference = credential_reference.map(identifier).transpose()?;
        let id = Uuid::new_v4().to_string();
        let timestamp = iso_now()?;
        self.conn.execute(
            INSERT,
            params![
                id,
                enum_text(&input.provider_type)?,
                input.display_name,
                enum_text(&input.source)?,
                input.base_url,
                i64::from(input.enabled),
                i64::from(input.requires_authentication),
                credential_reference,
                timeout_value(input.timeout_ms)?,
                timestamp,
                timestamp,
            ],
        )?;
        self.get(&id)?.ok_or_else(not_found)
    }

    pub fn update(
        &self,
        id: &str,
        input: ProviderConfigurationInput,
        credential: CredentialChange<'_>,
    ) -> Result<ProviderConfigurationSummary> {
        let id = identifier(id)?;
        let input = input.validated()?;
        let provider_type = enum_text(&input.provider_type)?;
        let source = enum_text(&input.source)?;
        let timeout = timeout_value(input.timeout_ms)?;
        let timestamp = iso_now()?;
        let changed = match credential {
            CredentialChange::Keep => self.conn.execute(
                UPDATE_KEEPING_CREDENTIAL,
                params![
                    provider_type,
                    input.display_name,
                    source,
                    input.base_url,
                    i64::from(input.enabled),
                    i64::from(input.requires_authentication),
                    timeout,
                    timestamp,
                    id,
                ],
            )?,
            CredentialChange::Set(reference) => {
                let reference = reference.map(identifier).transpose()?;
                self.conn.execute(
                    UPDATE_SETTING_CREDENTIAL,
                    params![
                        provider_type,
                        input.display_name,
                        source,
                        input.base_url,
                        i64::from(input.enabled),
                        i64::from(input.requires_authentication),
                        timeout,
                        reference,
                        timestamp,
                        id,
                    ],
                )?
            }
        };
        if changed == 0 {
            return Err(not_found());
        }
        self.get(&id)?.ok_or_else(not_found)
    }

    /// Replaces or clears the reference without touching the other fields.
    pub fn set_credential_reference(
        &self,
        id: &str,
        reference: Option<&str>,
    ) -> Result<ProviderConfigurationSummary> {
        let id = identifier(id)?;
        let reference = reference.map(identifier).transpose()?;
        let changed = self.conn.execute(
            "UPDATE provider_configs SET credential_ref=?1, updated_at=?2 WHERE id=?3",
            params![reference, iso_now()?, id],
        )?;
        if changed == 0 {
            return Err(not_found());
        }
        self.get(&id)?.ok_or_else(not_found)
    }

    /// The reference held by a configuration. An unknown identifier is an error, as in the
    /// TypeScript repository; a configuration without a credential yields `None`.
    pub fn credential_reference(&self, id: &str) -> Result<Option<String>> {
        let id = identifier(id)?;
        let row = self
            .conn
            .query_row(
                "SELECT credential_ref FROM provider_configs WHERE id=?1",
                [&id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?;
        row.ok_or_else(not_found)
    }

    pub fn credential_references(&self) -> Result<Vec<String>> {
        let mut statement = self.conn.prepare(
            "SELECT credential_ref FROM provider_configs WHERE credential_ref IS NOT NULL",
        )?;
        let references = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(references)
    }

    /// Removes a configuration in one statement, so the reference it held is reported
    /// exactly when the row is gone.
    pub fn delete(&self, id: &str) -> Result<Deletion> {
        let id = identifier(id)?;
        let removed = self
            .conn
            .query_row(
                "DELETE FROM provider_configs WHERE id=?1 RETURNING credential_ref",
                [&id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?;
        Ok(match removed {
            Some(credential_reference) => Deletion {
                deleted: true,
                credential_reference,
            },
            None => Deletion {
                deleted: false,
                credential_reference: None,
            },
        })
    }
}

/// A row as stored, before the contract checks of [`summary`].
struct StoredRow {
    id: String,
    provider_type: String,
    display_name: String,
    source: String,
    base_url: String,
    enabled: i64,
    requires_authentication: i64,
    credential_ref: Option<String>,
    timeout_ms: i64,
    created_at: String,
    updated_at: String,
}

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredRow> {
    Ok(StoredRow {
        id: row.get(0)?,
        provider_type: row.get(1)?,
        display_name: row.get(2)?,
        source: row.get(3)?,
        base_url: row.get(4)?,
        enabled: row.get(5)?,
        requires_authentication: row.get(6)?,
        credential_ref: row.get(7)?,
        timeout_ms: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn summary(row: StoredRow) -> Result<ProviderConfigurationSummary> {
    let invalid = || Error::Message(INVALID_ROW.to_string());
    Ok(ProviderConfigurationSummary {
        id: identifier(&row.id).map_err(|_| invalid())?,
        provider_type: enum_value(&row.provider_type).ok_or_else(invalid)?,
        display_name: row.display_name,
        source: enum_value::<ProviderSource>(&row.source).ok_or_else(invalid)?,
        base_url: row.base_url,
        enabled: flag(row.enabled).ok_or_else(invalid)?,
        requires_authentication: flag(row.requires_authentication).ok_or_else(invalid)?,
        credential_configured: row.credential_ref.is_some(),
        timeout_ms: u64::try_from(row.timeout_ms).map_err(|_| invalid())?,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn flag(value: i64) -> Option<bool> {
    match value {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

fn timeout_value(timeout_ms: u64) -> Result<i64> {
    i64::try_from(timeout_ms).map_err(|_| Error::Message(INVALID_ROW.to_string()))
}

fn not_found() -> Error {
    Error::Message(NOT_FOUND.to_string())
}
