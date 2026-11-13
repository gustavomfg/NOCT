//! Immutable observations of workspace state. Port of `WorkspaceEvidenceRepository.ts`.
//!
//! An observation never claims that independent tools read one atomic snapshot. Its
//! validity only moves from `unknown` to `stale`, and a manifest that cites a stale
//! reference becomes stale too.

use std::collections::HashSet;

use noct_core::workspace_evidence::{
    Consistency, Coverage, EvidenceKind, EvidenceReference, EvidenceVerification,
    KnownWorkspacePath, Validity, WorkspaceStateManifest,
};
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::clock::iso_now;
use crate::{Error, Result};

const MAX_PATHS: usize = 2_000;
const MAX_REFERENCES: usize = 500;
const MAX_LIST: usize = 20;
const MAX_CHAIN: usize = 100;
const IDENTITY_CONFLICT: &str =
    "Identidade de evidência já pertence a outro workspace ou execução.";
const CORRUPT: &str = "A evidência persistida está inválida.";

pub struct EvidenceInput {
    pub kind: EvidenceKind,
    pub workspace: String,
    pub source_id: String,
    pub execution_id: Option<String>,
    pub started_at: Option<String>,
    pub paths: Vec<KnownWorkspacePath>,
    pub references: Vec<EvidenceReference>,
}

pub struct WorkspaceEvidence<'a> {
    conn: &'a Connection,
}

impl<'a> WorkspaceEvidence<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Records an observation once per source. A repeated source returns the stored
    /// observation, and it is refused when it names another workspace or execution.
    pub fn record(&self, input: EvidenceInput) -> Result<WorkspaceStateManifest> {
        if let Some(existing) = self.by_source(input.kind, &input.source_id)? {
            if existing.workspace != input.workspace || existing.execution_id != input.execution_id
            {
                return Err(Error::Message(IDENTITY_CONFLICT.to_string()));
            }
            return Ok(existing);
        }
        let now = iso_now()?;
        let truncated = input.paths.len() > MAX_PATHS || input.references.len() > MAX_REFERENCES;
        let manifest = WorkspaceStateManifest {
            id: Uuid::new_v4().to_string(),
            workspace: input.workspace,
            execution_id: input.execution_id,
            kind: input.kind,
            source_id: input.source_id,
            started_at: input.started_at.unwrap_or_else(|| now.clone()),
            observed_at: now,
            consistency: Consistency::NonAtomic,
            coverage: Coverage::KnownPathsOnly,
            validity: Validity::Unknown,
            stale_detected_at: None,
            reason: None,
            truncated,
            paths: input.paths.into_iter().take(MAX_PATHS).collect(),
            references: input.references.into_iter().take(MAX_REFERENCES).collect(),
            verification: None,
        };
        let json = to_json(&manifest)?;
        self.conn.execute(
            "INSERT INTO workspace_evidence(id,workspace,execution_id,kind,source_id,manifest_json) \
             VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                manifest.id,
                manifest.workspace,
                manifest.execution_id,
                manifest.kind.as_str(),
                manifest.source_id,
                json,
            ],
        )?;
        Ok(manifest)
    }

    pub fn by_source(
        &self,
        kind: EvidenceKind,
        source_id: &str,
    ) -> Result<Option<WorkspaceStateManifest>> {
        let stored: Option<String> = self
            .conn
            .query_row(
                "SELECT manifest_json FROM workspace_evidence WHERE kind=?1 AND source_id=?2",
                params![kind.as_str(), source_id],
                |row| row.get(0),
            )
            .optional()?;
        stored.map(|json| self.current(parse(&json)?)).transpose()
    }

    /// The most recent observations of a workspace, optionally limited to one execution,
    /// with the observations they cite. At most 20 are returned.
    pub fn list(
        &self,
        workspace: &str,
        execution_id: Option<&str>,
    ) -> Result<Vec<WorkspaceStateManifest>> {
        let mut records = self.collect(
            "SELECT manifest_json FROM workspace_evidence \
             WHERE workspace=?1 AND (?2 IS NULL OR execution_id=?2) \
             ORDER BY rowid DESC LIMIT 20",
            params![workspace, execution_id],
        )?;
        if let Some(execution_id) = execution_id.filter(|id| !id.is_empty()) {
            let engineering: Option<String> = self
                .conn
                .query_row(
                    "SELECT manifest_json FROM workspace_evidence e WHERE workspace=?1 \
                     AND kind='engineering' \
                     AND EXISTS (SELECT 1 FROM json_each(e.manifest_json,'$.references') r \
                         WHERE json_extract(r.value,'$.kind')='execution' \
                         AND json_extract(r.value,'$.id')=?2) \
                     ORDER BY rowid DESC LIMIT 1",
                    params![workspace, execution_id],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(json) = engineering
                && records.len() < MAX_LIST
            {
                records.push(parse(&json)?);
            }
            // The list grows while it is walked, as the TypeScript loop does, so parents
            // of parents are followed too.
            let mut index = 0;
            while index < records.len() {
                let references = records[index].references.clone();
                for reference in references {
                    if records.len() >= MAX_LIST {
                        break;
                    }
                    if let Some(parent) = self.reference(workspace, &reference)?
                        && !records.iter().any(|item| item.id == parent.id)
                    {
                        records.push(parent);
                    }
                }
                index += 1;
            }
        }
        records
            .into_iter()
            .map(|record| self.current(record))
            .collect()
    }

    /// Marks every observation of the workspace that is not stale yet. The first reason
    /// is kept, as the TypeScript repository does.
    pub fn invalidate(&self, workspace: &str, reason: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE workspace_evidence \
             SET stale_detected_at=COALESCE(stale_detected_at,?1),stale_reason=COALESCE(stale_reason,?2) \
             WHERE workspace=?3 AND stale_detected_at IS NULL",
            params![iso_now()?, reason, workspace],
        )?;
        Ok(())
    }

    /// The manifest with its stored verification and stale state. A manifest whose
    /// reference is stale is marked stale itself and stored that way.
    pub fn current(&self, manifest: WorkspaceStateManifest) -> Result<WorkspaceStateManifest> {
        self.resolve(manifest, &mut HashSet::new())
    }

    fn resolve(
        &self,
        manifest: WorkspaceStateManifest,
        visited: &mut HashSet<String>,
    ) -> Result<WorkspaceStateManifest> {
        if visited.contains(&manifest.id) || visited.len() >= MAX_CHAIN {
            return Ok(manifest);
        }
        visited.insert(manifest.id.clone());
        let row = self
            .conn
            .query_row(
                "SELECT stale_detected_at, stale_reason, verification_json \
                 FROM workspace_evidence WHERE id=?1",
                [&manifest.id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?;
        let (stale_detected_at, stale_reason, verification) = row.unwrap_or((None, None, None));

        let mut current = manifest.clone();
        if let Some(text) = verification.filter(|text| !text.is_empty()) {
            current.verification =
                Some(serde_json::from_str::<EvidenceVerification>(&text).map_err(|_| corrupt())?);
        }
        if let Some(stale_detected_at) = stale_detected_at {
            current.validity = Validity::Stale;
            current.stale_detected_at = Some(stale_detected_at);
            current.reason = stale_reason;
            return Ok(current);
        }
        for reference in manifest.references.clone() {
            let Some(parent) = self.reference(&manifest.workspace, &reference)? else {
                continue;
            };
            let parent_id = parent.id.clone();
            if self.resolve(parent, visited)?.validity == Validity::Stale {
                self.mark_stale(
                    &manifest.id,
                    &format!("Evidência referenciada está stale: {parent_id}"),
                )?;
                return self.current(manifest);
            }
        }
        Ok(current)
    }

    /// Marks one observation stale. The first reason and the first time are kept.
    pub fn mark_stale(&self, id: &str, reason: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE workspace_evidence \
             SET stale_detected_at=COALESCE(stale_detected_at,?1),stale_reason=COALESCE(stale_reason,?2) \
             WHERE id=?3",
            params![iso_now()?, reason, id],
        )?;
        Ok(())
    }

    pub fn save_verification(&self, id: &str, verification: &EvidenceVerification) -> Result<()> {
        let json =
            serde_json::to_string(verification).map_err(|_| Error::Message(CORRUPT.to_string()))?;
        self.conn.execute(
            "UPDATE workspace_evidence SET verification_json=?1 WHERE id=?2",
            params![json, id],
        )?;
        Ok(())
    }

    /// Records the non-excluded project index files of a workspace as one observation.
    pub fn record_project_index(
        &self,
        workspace: &str,
        source_id: &str,
        started_at: &str,
    ) -> Result<WorkspaceStateManifest> {
        let mut statement = self.conn.prepare(
            "SELECT relative_path, analyzed_hash FROM project_index_files \
             WHERE workspace=?1 AND excluded=0 ORDER BY relative_path LIMIT 2001",
        )?;
        let paths = statement
            .query_map([workspace], |row| {
                Ok(KnownWorkspacePath {
                    path: row.get(0)?,
                    hash: row.get(1)?,
                    exists: None,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        self.record(EvidenceInput {
            kind: EvidenceKind::ProjectIndex,
            workspace: workspace.to_string(),
            source_id: source_id.to_string(),
            execution_id: None,
            started_at: Some(started_at.to_string()),
            paths,
            references: Vec::new(),
        })
    }

    /// The most recent manifest of each index kind, as references for other evidence.
    pub fn index_references(&self, workspace: &str) -> Result<Vec<EvidenceReference>> {
        let mut references = Vec::new();
        for kind in [EvidenceKind::ProjectIndex, EvidenceKind::SemanticIndex] {
            let id: Option<String> = self
                .conn
                .query_row(
                    "SELECT id FROM workspace_evidence WHERE workspace=?1 AND kind=?2 \
                     ORDER BY rowid DESC LIMIT 1",
                    params![workspace, kind.as_str()],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(id) = id {
                references.push(EvidenceReference {
                    kind: "manifest".to_string(),
                    id,
                    payload_hash: None,
                });
            }
        }
        Ok(references)
    }

    fn reference(
        &self,
        workspace: &str,
        reference: &EvidenceReference,
    ) -> Result<Option<WorkspaceStateManifest>> {
        let stored: Option<String> = self
            .conn
            .query_row(
                "SELECT manifest_json FROM workspace_evidence WHERE workspace=?1 AND \
                 ((?2='manifest' AND id=?3) OR (kind=?2 AND source_id=?3))",
                params![workspace, reference.kind, reference.id],
                |row| row.get(0),
            )
            .optional()?;
        stored.map(|json| parse(&json)).transpose()
    }

    fn collect(
        &self,
        sql: &str,
        params: impl rusqlite::Params,
    ) -> Result<Vec<WorkspaceStateManifest>> {
        let mut statement = self.conn.prepare(sql)?;
        let rows = statement
            .query_map(params, |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.iter().map(|json| parse(json)).collect()
    }
}

fn parse(json: &str) -> Result<WorkspaceStateManifest> {
    serde_json::from_str(json).map_err(|_| corrupt())
}

fn to_json(manifest: &WorkspaceStateManifest) -> Result<String> {
    serde_json::to_string(manifest).map_err(|_| corrupt())
}

fn corrupt() -> Error {
    Error::Message(CORRUPT.to_string())
}
