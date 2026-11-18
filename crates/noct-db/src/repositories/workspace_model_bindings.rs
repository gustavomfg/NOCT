//! Model bindings per workspace. Port of `WorkspaceModelBindingRepository.ts`.

use noct_core::ai::bindings::WorkspaceModelBindings;
use noct_core::ai::model::ModelReference;
use noct_core::validation::text;
use rusqlite::{Connection, OptionalExtension, params};

use crate::clock::iso_now;
use crate::{Error, Result};

const WORKSPACE_ID_CHARACTERS: u64 = 512;

pub struct ModelBindings<'a> {
    conn: &'a Connection,
}

impl<'a> ModelBindings<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list(&self) -> Result<Vec<WorkspaceModelBindings>> {
        let mut statement = self.conn.prepare(
            "SELECT workspace_id, bindings FROM workspace_model_bindings ORDER BY workspace_id",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(workspace_id, bindings)| parse_row(&workspace_id, &bindings))
            .collect()
    }

    pub fn get(&self, workspace_id: &str) -> Result<Option<WorkspaceModelBindings>> {
        let workspace_id = workspace_text(workspace_id)?;
        let row = self
            .conn
            .query_row(
                "SELECT workspace_id, bindings FROM workspace_model_bindings WHERE workspace_id=?1",
                [&workspace_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        row.map(|(stored_id, bindings)| parse_row(&stored_id, &bindings))
            .transpose()
    }

    /// Stores the bindings after validating them, and returns the validated value.
    pub fn set(&self, input: WorkspaceModelBindings) -> Result<WorkspaceModelBindings> {
        let bindings = input.validated().map_err(contract_error)?;
        let json =
            serde_json::to_string(&bindings).map_err(|error| Error::Message(error.to_string()))?;
        self.conn.execute(
            "INSERT INTO workspace_model_bindings(workspace_id, bindings, updated_at) \
             VALUES(?1, ?2, ?3) ON CONFLICT(workspace_id) DO UPDATE SET \
             bindings=excluded.bindings, updated_at=excluded.updated_at",
            params![bindings.workspace_id, json, iso_now()?],
        )?;
        Ok(bindings)
    }

    /// Stores the default model and keeps every other field of the row. The row is read, its default is replaced, and
    /// the whole row is written again, all in one transaction: either all of it is stored or none is. `None` clears the
    /// default only. A missing row starts with no other field set.
    pub fn set_default_model(
        &self,
        workspace_id: &str,
        default: Option<ModelReference>,
    ) -> Result<WorkspaceModelBindings> {
        let workspace_id = workspace_text(workspace_id)?;
        let default = default
            .map(ModelReference::validated)
            .transpose()
            .map_err(contract_error)?;
        let transaction = self.conn.unchecked_transaction()?;
        let current = self
            .get(&workspace_id)?
            .unwrap_or_else(|| WorkspaceModelBindings {
                workspace_id: workspace_id.clone(),
                default_binding: None,
                embedding_binding: None,
                remote_embedding_allowed: None,
            });
        let saved = self.set(WorkspaceModelBindings {
            default_binding: default,
            ..current
        })?;
        transaction.commit()?;
        Ok(saved)
    }

    pub fn delete(&self, workspace_id: &str) -> Result<bool> {
        let workspace_id = workspace_text(workspace_id)?;
        let changed = self.conn.execute(
            "DELETE FROM workspace_model_bindings WHERE workspace_id=?1",
            [workspace_id],
        )?;
        Ok(changed > 0)
    }
}

fn workspace_text(workspace_id: &str) -> Result<String> {
    text("workspaceId", workspace_id, WORKSPACE_ID_CHARACTERS).map_err(contract_error)
}

fn parse_row(workspace_id: &str, bindings: &str) -> Result<WorkspaceModelBindings> {
    let invalid =
        || Error::Message("Os bindings de modelos persistidos estão inválidos.".to_string());
    let parsed: WorkspaceModelBindings = serde_json::from_str(bindings).map_err(|_| invalid())?;
    let parsed = parsed.validated().map_err(|_| invalid())?;
    if parsed.workspace_id != workspace_id {
        return Err(invalid());
    }
    Ok(parsed)
}

fn contract_error(error: noct_core::validation::ContractError) -> Error {
    Error::Message(error.to_string())
}
