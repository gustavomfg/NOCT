//! Models discovered per provider. Port of `ModelCatalogRepository.ts`.

use std::collections::HashSet;

use noct_core::ai::model::ModelDescriptor;
use noct_core::limits::model::IDENTIFIER_CHARACTERS;
use noct_core::validation::{ContractError, text};
use rusqlite::{Connection, params};
use serde_json::Value;

use crate::clock::iso_now;
use crate::{Error, Result, transaction};

const PROVIDER_ID_FIELD: &str = "providerId";

pub struct ModelCatalog<'a> {
    conn: &'a Connection,
}

impl<'a> ModelCatalog<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Every stored model, or only those of one provider, ordered as the TypeScript
    /// repository orders them.
    pub fn list(&self, provider_id: Option<&str>) -> Result<Vec<ModelDescriptor>> {
        let rows = match provider_id {
            None => self.rows(
                "SELECT provider_id, model_id, descriptor FROM model_catalog \
                 ORDER BY provider_id, model_id",
                [],
            )?,
            Some(provider_id) => {
                let provider_id = provider_text(provider_id)?;
                self.rows(
                    "SELECT provider_id, model_id, descriptor FROM model_catalog \
                     WHERE provider_id=?1 ORDER BY model_id",
                    [provider_id],
                )?
            }
        };
        rows.into_iter().map(parse_row).collect()
    }

    /// Replaces every model of `provider_id` with `inputs`, all in one transaction.
    /// Inputs are validated first: a model of another provider or a duplicate rejects
    /// the whole replacement.
    pub fn replace_provider_models(
        &self,
        provider_id: &str,
        inputs: &[Value],
    ) -> Result<Vec<ModelDescriptor>> {
        let provider_id = provider_text(provider_id)?;
        let descriptors = inputs
            .iter()
            .map(|input| parse_descriptor(input.clone()))
            .collect::<Result<Vec<_>>>()?;
        let mut keys = HashSet::with_capacity(descriptors.len());
        for descriptor in &descriptors {
            if descriptor.provider_id != provider_id {
                return Err(Error::Message(
                    "O catálogo contém um modelo de outro Provider.".to_string(),
                ));
            }
            if !keys.insert(descriptor.model_id.clone()) {
                return Err(Error::Message(
                    "O catálogo contém modelos duplicados.".to_string(),
                ));
            }
        }

        transaction::run(self.conn, |conn| {
            conn.execute(
                "DELETE FROM model_catalog WHERE provider_id=?1",
                params![provider_id],
            )?;
            let updated_at = iso_now()?;
            for descriptor in &descriptors {
                let descriptor_json = serde_json::to_string(descriptor)
                    .map_err(|error| Error::Message(error.to_string()))?;
                conn.execute(
                    "INSERT INTO model_catalog(provider_id, model_id, descriptor, updated_at) \
                     VALUES(?1, ?2, ?3, ?4)",
                    params![
                        descriptor.provider_id,
                        descriptor.model_id,
                        descriptor_json,
                        updated_at
                    ],
                )?;
            }
            Ok(())
        })?;
        Ok(descriptors)
    }

    /// Returns how many models were removed.
    pub fn delete_provider_models(&self, provider_id: &str) -> Result<usize> {
        let provider_id = provider_text(provider_id)?;
        Ok(self.conn.execute(
            "DELETE FROM model_catalog WHERE provider_id=?1",
            params![provider_id],
        )?)
    }

    fn rows<P: rusqlite::Params>(&self, sql: &str, params: P) -> Result<Vec<StoredModel>> {
        let mut statement = self.conn.prepare(sql)?;
        let rows = statement
            .query_map(params, |row| {
                Ok(StoredModel {
                    provider_id: row.get(0)?,
                    model_id: row.get(1)?,
                    descriptor: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

struct StoredModel {
    provider_id: String,
    model_id: String,
    descriptor: String,
}

fn provider_text(provider_id: &str) -> Result<String> {
    text(PROVIDER_ID_FIELD, provider_id, IDENTIFIER_CHARACTERS).map_err(contract_error)
}

fn parse_descriptor(input: Value) -> Result<ModelDescriptor> {
    serde_json::from_value::<ModelDescriptor>(input)
        .map_err(|error| Error::Message(error.to_string()))?
        .validated()
        .map_err(contract_error)
}

/// A persisted model must still parse, and its identifiers must match its row.
fn parse_row(row: StoredModel) -> Result<ModelDescriptor> {
    let invalid = || Error::Message("O catálogo de modelos persistido está inválido.".to_string());
    let descriptor: ModelDescriptor =
        serde_json::from_str(&row.descriptor).map_err(|_| invalid())?;
    let descriptor = descriptor.validated().map_err(|_| invalid())?;
    if descriptor.provider_id != row.provider_id || descriptor.model_id != row.model_id {
        return Err(invalid());
    }
    Ok(descriptor)
}

fn contract_error(error: ContractError) -> Error {
    Error::Message(error.to_string())
}
