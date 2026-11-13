//! Engineering intelligence: signals, insights and health snapshots. Port of
//! `electron/database/EngineeringIntelligenceRepository.ts`.
//!
//! An evaluation is saved as one unit of work: its signals are upserted, the signals of the
//! assessed categories that the evaluation no longer finds are resolved, and the snapshot is
//! stored unless one with the same state already exists. Every value is checked before anything
//! is written, and every row is checked again when it is read.

use std::collections::HashSet;

use noct_core::engineering_intelligence::{
    EngineeringEvidence, EngineeringHealthCategory, EngineeringHealthSnapshot, EngineeringInsight,
    EngineeringMetric, EngineeringSignal, EngineeringSignalSeverity,
    EngineeringSignalSnapshotState, EngineeringSignalSource, EngineeringSignalStatus,
    EngineeringSourceRuns, HealthAssessmentStatus, HealthCategoryResult, parse_insight,
    parse_signal, parse_snapshot,
};
use noct_core::limits::engineering_intelligence::{MAX_INSIGHTS, MAX_SIGNALS, MAX_SNAPSHOTS};
use noct_core::workspace_evidence::{EvidenceKind, EvidenceReference};
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, Row, named_params, params, params_from_iter};
use serde::de::DeserializeOwned;

use crate::codec::{Stored, enum_text};
use crate::repositories::workspace_evidence::{EvidenceInput, WorkspaceEvidence};
use crate::{Error, Result, transaction};

const SIGNAL_COLUMNS: &str = "id, workspace, fingerprint, category, kind, title, description, \
    severity, confidence, evidence_json, metric_json, source, policy_version, detected_at, status, \
    first_seen_at, last_seen_at, resolved_at";
const SNAPSHOT_COLUMNS: &str = "id, workspace, policy_version, evaluated_at, previous_snapshot_id, \
    sources_json, categories_json, signal_fingerprints_json, signal_states_json, state_fingerprint";
const INSIGHT_COLUMNS: &str = "id, workspace, fingerprint, title, explanation, \
    related_signal_ids_json, evidence_json, suggested_action, confidence, status, created_at, \
    updated_at, last_seen_at";

const UPSERT_SIGNAL: &str = "INSERT INTO engineering_signals(id,workspace,fingerprint,category,kind,\
    title,description,severity,confidence,evidence_json,metric_json,source,policy_version,\
    detected_at,status,first_seen_at,last_seen_at,resolved_at) \
    VALUES(:id,:workspace,:fingerprint,:category,:kind,:title,:description,:severity,:confidence,\
    :evidence_json,:metric_json,:source,:policy_version,:detected_at,:status,:first_seen_at,\
    :last_seen_at,:resolved_at) \
    ON CONFLICT(workspace,fingerprint) DO UPDATE SET category=excluded.category,kind=excluded.kind,\
    title=excluded.title,description=excluded.description,severity=excluded.severity,\
    confidence=excluded.confidence,evidence_json=excluded.evidence_json,\
    metric_json=excluded.metric_json,source=excluded.source,\
    policy_version=excluded.policy_version,detected_at=excluded.detected_at,\
    status=excluded.status,last_seen_at=excluded.last_seen_at,resolved_at=excluded.resolved_at";

const UPSERT_INSIGHT: &str = "INSERT INTO engineering_insights(id,workspace,fingerprint,title,\
    explanation,related_signal_ids_json,evidence_json,suggested_action,confidence,status,\
    created_at,updated_at,last_seen_at) \
    VALUES(:id,:workspace,:fingerprint,:title,:explanation,:related_signal_ids_json,:evidence_json,\
    :suggested_action,:confidence,:status,:created_at,:updated_at,:last_seen_at) \
    ON CONFLICT(workspace,fingerprint) DO UPDATE SET title=excluded.title,\
    explanation=excluded.explanation,related_signal_ids_json=excluded.related_signal_ids_json,\
    evidence_json=excluded.evidence_json,suggested_action=excluded.suggested_action,\
    confidence=excluded.confidence,status=excluded.status,updated_at=excluded.updated_at,\
    last_seen_at=excluded.last_seen_at";

const INVALID_SIGNAL_EVIDENCE: &str = "evidências de engineering signal";
const INVALID_SIGNAL_METRIC: &str = "métrica de engineering signal";
const INVALID_SNAPSHOT_SOURCES: &str = "fontes do snapshot de engineering health";
const INVALID_SNAPSHOT_CATEGORIES: &str = "categorias do snapshot de engineering health";
const INVALID_SNAPSHOT_FINGERPRINTS: &str = "fingerprints do snapshot de engineering health";
const INVALID_SNAPSHOT_STATES: &str = "estados dos sinais do snapshot de engineering health";
const INVALID_INSIGHT_RELATED: &str = "sinais relacionados do insight";
const INVALID_INSIGHT_EVIDENCE: &str = "evidências do insight";

/// The categories and fingerprints an evaluation may resolve beyond the ones it assessed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvaluationOptions {
    /// Categories whose missing signals are kept, even when they were assessed.
    pub skip_category_resolution: Vec<EngineeringHealthCategory>,
    /// Fingerprints that are comparable across evaluations and may be resolved when absent.
    pub resolve_signal_fingerprints: Vec<String>,
}

pub struct EngineeringIntelligence<'a> {
    conn: &'a Connection,
}

impl<'a> EngineeringIntelligence<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Saves an evaluation and returns the snapshot it stored. When a snapshot with the same state
    /// already exists, that snapshot is returned and nothing else is written.
    pub fn save_evaluation(
        &self,
        signals: &[EngineeringSignal],
        snapshot: &EngineeringHealthSnapshot,
        options: &EvaluationOptions,
    ) -> Result<EngineeringHealthSnapshot> {
        for signal in signals {
            parse_signal(signal)?;
        }
        parse_snapshot(snapshot)?;

        transaction::run(self.conn, |conn| {
            let fingerprints: HashSet<&str> = signals
                .iter()
                .map(|signal| signal.fingerprint.as_str())
                .collect();
            for signal in signals {
                upsert_signal(conn, signal)?;
            }

            let assessed: Vec<EngineeringHealthCategory> = snapshot
                .categories
                .iter()
                .filter(|result| result.status == HealthAssessmentStatus::Assessed)
                .filter(|result| !options.skip_category_resolution.contains(&result.category))
                .map(|result| result.category)
                .collect();
            if !assessed.is_empty() {
                let mut parameters = vec![Value::Text(snapshot.workspace.clone())];
                for category in &assessed {
                    parameters.push(Value::Text(enum_text(category)?));
                }
                let placeholders = vec!["?"; assessed.len()].join(",");
                let mut statement = conn.prepare(&format!(
                    "SELECT fingerprint FROM engineering_signals \
                     WHERE workspace=? AND status='active' AND category IN ({placeholders})"
                ))?;
                let active: Vec<String> = statement
                    .query_map(params_from_iter(parameters), |row| row.get(0))?
                    .collect::<rusqlite::Result<_>>()?;
                for fingerprint in active {
                    if !fingerprints.contains(fingerprint.as_str()) {
                        resolve_signal(
                            conn,
                            &snapshot.evaluated_at,
                            &snapshot.workspace,
                            &fingerprint,
                        )?;
                    }
                }
            }

            for fingerprint in &options.resolve_signal_fingerprints {
                if !fingerprints.contains(fingerprint.as_str()) {
                    resolve_signal(
                        conn,
                        &snapshot.evaluated_at,
                        &snapshot.workspace,
                        fingerprint,
                    )?;
                }
            }

            let existing = conn
                .query_row(
                    &format!(
                        "SELECT {SNAPSHOT_COLUMNS} FROM engineering_health_snapshots \
                         WHERE workspace=?1 AND state_fingerprint=?2"
                    ),
                    params![snapshot.workspace, snapshot.state_fingerprint],
                    snapshot_row,
                )
                .optional()?;
            if let Some(existing) = existing {
                return snapshot_from_row(existing);
            }

            WorkspaceEvidence::new(conn).record(EvidenceInput {
                kind: EvidenceKind::Engineering,
                workspace: snapshot.workspace.clone(),
                source_id: snapshot.id.clone(),
                execution_id: None,
                started_at: Some(snapshot.evaluated_at.clone()),
                paths: Vec::new(),
                references: evidence_references(&snapshot.sources),
            })?;

            conn.execute(
                "INSERT INTO engineering_health_snapshots(id,workspace,policy_version,evaluated_at,\
                 previous_snapshot_id,sources_json,categories_json,signal_fingerprints_json,\
                 signal_states_json,state_fingerprint) \
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    snapshot.id,
                    snapshot.workspace,
                    snapshot.policy_version,
                    snapshot.evaluated_at,
                    snapshot.previous_snapshot_id,
                    json_text(&snapshot.sources)?,
                    json_text(&snapshot.categories)?,
                    json_text(&snapshot.signal_fingerprints)?,
                    json_text(&snapshot.signal_states)?,
                    snapshot.state_fingerprint,
                ],
            )?;
            Ok(snapshot.clone())
        })
    }

    /// The signals of a workspace, most recently seen first. The limit is kept between 1 and 500.
    pub fn list_signals(
        &self,
        workspace: &str,
        status: Option<EngineeringSignalStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<EngineeringSignal>> {
        let limit = bounded_limit(limit, MAX_SIGNALS);
        let rows = match status {
            Some(status) => self.signal_rows(
                &format!(
                    "SELECT {SIGNAL_COLUMNS} FROM engineering_signals \
                     WHERE workspace=?1 AND status=?2 ORDER BY last_seen_at DESC LIMIT ?3"
                ),
                params![workspace, enum_text(&status)?, limit],
            )?,
            None => self.signal_rows(
                &format!(
                    "SELECT {SIGNAL_COLUMNS} FROM engineering_signals \
                     WHERE workspace=?1 ORDER BY last_seen_at DESC LIMIT ?2"
                ),
                params![workspace, limit],
            )?,
        };
        rows.into_iter().map(signal_from_row).collect()
    }

    pub fn get_signal(
        &self,
        workspace: &str,
        fingerprint: &str,
    ) -> Result<Option<EngineeringSignal>> {
        self.signal_rows(
            &format!(
                "SELECT {SIGNAL_COLUMNS} FROM engineering_signals WHERE workspace=?1 AND fingerprint=?2"
            ),
            params![workspace, fingerprint],
        )?
        .into_iter()
        .next()
        .map(signal_from_row)
        .transpose()
    }

    pub fn latest_snapshot(&self, workspace: &str) -> Result<Option<EngineeringHealthSnapshot>> {
        self.snapshot_rows(
            &format!(
                "SELECT {SNAPSHOT_COLUMNS} FROM engineering_health_snapshots \
                 WHERE workspace=?1 ORDER BY evaluated_at DESC LIMIT 1"
            ),
            params![workspace],
        )?
        .into_iter()
        .next()
        .map(snapshot_from_row)
        .transpose()
    }

    /// The snapshots of a workspace, most recent first. The limit is kept between 1 and 100.
    pub fn list_snapshots(
        &self,
        workspace: &str,
        limit: Option<usize>,
    ) -> Result<Vec<EngineeringHealthSnapshot>> {
        let limit = bounded_limit(limit, MAX_SNAPSHOTS);
        self.snapshot_rows(
            &format!(
                "SELECT {SNAPSHOT_COLUMNS} FROM engineering_health_snapshots \
                 WHERE workspace=?1 ORDER BY evaluated_at DESC LIMIT ?2"
            ),
            params![workspace, limit],
        )?
        .into_iter()
        .map(snapshot_from_row)
        .collect()
    }

    /// Stores an insight. An insight with the same fingerprint keeps the identity and creation
    /// time it already has, and the stored insight is returned.
    pub fn save_insight(&self, insight: &EngineeringInsight) -> Result<EngineeringInsight> {
        parse_insight(insight)?;
        transaction::run(self.conn, |conn| {
            let existing: Option<(String, String)> = conn
                .query_row(
                    "SELECT id, created_at FROM engineering_insights WHERE workspace=?1 AND fingerprint=?2",
                    params![insight.workspace, insight.fingerprint],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let value = match existing {
                Some((id, created_at)) => EngineeringInsight {
                    id,
                    created_at,
                    ..insight.clone()
                },
                None => insight.clone(),
            };
            conn.execute(
                UPSERT_INSIGHT,
                named_params! {
                    ":id": value.id,
                    ":workspace": value.workspace,
                    ":fingerprint": value.fingerprint,
                    ":title": value.title,
                    ":explanation": value.explanation,
                    ":related_signal_ids_json": json_text(&value.related_signal_ids)?,
                    ":evidence_json": json_text(&value.evidence)?,
                    ":suggested_action": value.suggested_action,
                    ":confidence": value.confidence,
                    ":status": enum_text(&value.status)?,
                    ":created_at": value.created_at,
                    ":updated_at": value.updated_at,
                    ":last_seen_at": value.last_seen_at,
                },
            )?;
            Ok(value)
        })
    }

    /// The insights of a workspace, most recently updated first. The limit is kept between 1 and 200.
    pub fn list_insights(
        &self,
        workspace: &str,
        status: Option<EngineeringSignalStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<EngineeringInsight>> {
        let limit = bounded_limit(limit, MAX_INSIGHTS);
        let rows = match status {
            Some(status) => self.insight_rows(
                &format!(
                    "SELECT {INSIGHT_COLUMNS} FROM engineering_insights \
                     WHERE workspace=?1 AND status=?2 ORDER BY updated_at DESC LIMIT ?3"
                ),
                params![workspace, enum_text(&status)?, limit],
            )?,
            None => self.insight_rows(
                &format!(
                    "SELECT {INSIGHT_COLUMNS} FROM engineering_insights \
                     WHERE workspace=?1 ORDER BY updated_at DESC LIMIT ?2"
                ),
                params![workspace, limit],
            )?,
        };
        rows.into_iter().map(insight_from_row).collect()
    }

    /// Resolves an active insight. Returns whether one was resolved.
    pub fn resolve_insight(&self, workspace: &str, id: &str, resolved_at: &str) -> Result<bool> {
        transaction::run(self.conn, |conn| {
            let changes = conn.execute(
                "UPDATE engineering_insights SET status='resolved',updated_at=?1 \
                 WHERE workspace=?2 AND id=?3 AND status='active'",
                params![resolved_at, workspace, id],
            )?;
            Ok(changes > 0)
        })
    }

    fn signal_rows(&self, sql: &str, parameters: impl rusqlite::Params) -> Result<Vec<RawSignal>> {
        let mut statement = self.conn.prepare(sql)?;
        let rows = statement.query_map(parameters, signal_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::from)
    }

    fn snapshot_rows(
        &self,
        sql: &str,
        parameters: impl rusqlite::Params,
    ) -> Result<Vec<RawSnapshot>> {
        let mut statement = self.conn.prepare(sql)?;
        let rows = statement.query_map(parameters, snapshot_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::from)
    }

    fn insight_rows(
        &self,
        sql: &str,
        parameters: impl rusqlite::Params,
    ) -> Result<Vec<RawInsight>> {
        let mut statement = self.conn.prepare(sql)?;
        let rows = statement.query_map(parameters, insight_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::from)
    }
}

/// A signal row before its JSON columns are read. Its enum columns are checked when the row is
/// read; the JSON columns and the schema are checked by `signal_from_row`.
struct RawSignal {
    id: String,
    workspace: String,
    fingerprint: String,
    category: EngineeringHealthCategory,
    kind: String,
    title: String,
    description: String,
    severity: EngineeringSignalSeverity,
    confidence: i64,
    evidence_json: String,
    metric_json: Option<String>,
    source: EngineeringSignalSource,
    policy_version: i64,
    detected_at: String,
    status: EngineeringSignalStatus,
    first_seen_at: String,
    last_seen_at: String,
    resolved_at: Option<String>,
}

fn signal_row(row: &Row<'_>) -> rusqlite::Result<RawSignal> {
    Ok(RawSignal {
        id: row.get(0)?,
        workspace: row.get(1)?,
        fingerprint: row.get(2)?,
        category: row.get::<_, Stored<EngineeringHealthCategory>>(3)?.0,
        kind: row.get(4)?,
        title: row.get(5)?,
        description: row.get(6)?,
        severity: row.get::<_, Stored<EngineeringSignalSeverity>>(7)?.0,
        confidence: row.get(8)?,
        evidence_json: row.get(9)?,
        metric_json: row.get(10)?,
        source: row.get::<_, Stored<EngineeringSignalSource>>(11)?.0,
        policy_version: row.get(12)?,
        detected_at: row.get(13)?,
        status: row.get::<_, Stored<EngineeringSignalStatus>>(14)?.0,
        first_seen_at: row.get(15)?,
        last_seen_at: row.get(16)?,
        resolved_at: row.get(17)?,
    })
}

fn signal_from_row(row: RawSignal) -> Result<EngineeringSignal> {
    let evidence: Vec<EngineeringEvidence> =
        parse_json(&row.evidence_json, INVALID_SIGNAL_EVIDENCE)?;
    let metric = row
        .metric_json
        .filter(|json| !json.is_empty())
        .map(|json| parse_json::<Option<EngineeringMetric>>(&json, INVALID_SIGNAL_METRIC))
        .transpose()?
        .flatten();
    let signal = EngineeringSignal {
        id: row.id,
        workspace: row.workspace,
        category: row.category,
        kind: row.kind,
        title: row.title,
        description: row.description,
        severity: row.severity,
        confidence: row.confidence,
        evidence,
        metric,
        source: row.source,
        policy_version: row.policy_version,
        detected_at: row.detected_at,
        fingerprint: row.fingerprint,
        status: row.status,
        first_seen_at: row.first_seen_at,
        last_seen_at: row.last_seen_at,
        resolved_at: row.resolved_at,
    };
    Ok(parse_signal(&signal)?)
}

/// A snapshot row before its JSON columns are read.
struct RawSnapshot {
    id: String,
    workspace: String,
    policy_version: i64,
    evaluated_at: String,
    previous_snapshot_id: Option<String>,
    sources_json: String,
    categories_json: String,
    signal_fingerprints_json: String,
    signal_states_json: String,
    state_fingerprint: String,
}

fn snapshot_row(row: &Row<'_>) -> rusqlite::Result<RawSnapshot> {
    Ok(RawSnapshot {
        id: row.get(0)?,
        workspace: row.get(1)?,
        policy_version: row.get(2)?,
        evaluated_at: row.get(3)?,
        previous_snapshot_id: row.get(4)?,
        sources_json: row.get(5)?,
        categories_json: row.get(6)?,
        signal_fingerprints_json: row.get(7)?,
        signal_states_json: row.get(8)?,
        state_fingerprint: row.get(9)?,
    })
}

fn snapshot_from_row(row: RawSnapshot) -> Result<EngineeringHealthSnapshot> {
    let sources: EngineeringSourceRuns = parse_json(&row.sources_json, INVALID_SNAPSHOT_SOURCES)?;
    let categories: Vec<HealthCategoryResult> =
        parse_json(&row.categories_json, INVALID_SNAPSHOT_CATEGORIES)?;
    let signal_fingerprints: Vec<String> =
        parse_json(&row.signal_fingerprints_json, INVALID_SNAPSHOT_FINGERPRINTS)?;
    let signal_states: Vec<EngineeringSignalSnapshotState> =
        parse_json(&row.signal_states_json, INVALID_SNAPSHOT_STATES)?;
    let snapshot = EngineeringHealthSnapshot {
        id: row.id,
        workspace: row.workspace,
        policy_version: row.policy_version,
        evaluated_at: row.evaluated_at,
        previous_snapshot_id: row.previous_snapshot_id,
        sources,
        categories,
        signal_fingerprints,
        signal_states,
        state_fingerprint: row.state_fingerprint,
    };
    Ok(parse_snapshot(&snapshot)?)
}

/// An insight row before its JSON columns are read.
struct RawInsight {
    id: String,
    workspace: String,
    fingerprint: String,
    title: String,
    explanation: String,
    related_signal_ids_json: String,
    evidence_json: String,
    suggested_action: Option<String>,
    confidence: i64,
    status: EngineeringSignalStatus,
    created_at: String,
    updated_at: String,
    last_seen_at: String,
}

fn insight_row(row: &Row<'_>) -> rusqlite::Result<RawInsight> {
    Ok(RawInsight {
        id: row.get(0)?,
        workspace: row.get(1)?,
        fingerprint: row.get(2)?,
        title: row.get(3)?,
        explanation: row.get(4)?,
        related_signal_ids_json: row.get(5)?,
        evidence_json: row.get(6)?,
        suggested_action: row.get(7)?,
        confidence: row.get(8)?,
        status: row.get::<_, Stored<EngineeringSignalStatus>>(9)?.0,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        last_seen_at: row.get(12)?,
    })
}

fn insight_from_row(row: RawInsight) -> Result<EngineeringInsight> {
    let related_signal_ids: Vec<String> =
        parse_json(&row.related_signal_ids_json, INVALID_INSIGHT_RELATED)?;
    let evidence: Vec<EngineeringEvidence> =
        parse_json(&row.evidence_json, INVALID_INSIGHT_EVIDENCE)?;
    let insight = EngineeringInsight {
        id: row.id,
        workspace: row.workspace,
        fingerprint: row.fingerprint,
        title: row.title,
        explanation: row.explanation,
        related_signal_ids,
        evidence,
        suggested_action: row.suggested_action,
        confidence: row.confidence,
        status: row.status,
        created_at: row.created_at,
        updated_at: row.updated_at,
        last_seen_at: row.last_seen_at,
    };
    Ok(parse_insight(&insight)?)
}

/// Writes a signal, keeping the identity and first sighting of the row it replaces.
fn upsert_signal(conn: &Connection, signal: &EngineeringSignal) -> Result<()> {
    let existing: Option<(String, String)> = conn
        .query_row(
            "SELECT id, first_seen_at FROM engineering_signals WHERE workspace=?1 AND fingerprint=?2",
            params![signal.workspace, signal.fingerprint],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let value = match existing {
        Some((id, first_seen_at)) => EngineeringSignal {
            id,
            first_seen_at,
            ..signal.clone()
        },
        None => signal.clone(),
    };
    conn.execute(
        UPSERT_SIGNAL,
        named_params! {
            ":id": value.id,
            ":workspace": value.workspace,
            ":fingerprint": value.fingerprint,
            ":category": enum_text(&value.category)?,
            ":kind": value.kind,
            ":title": value.title,
            ":description": value.description,
            ":severity": enum_text(&value.severity)?,
            ":confidence": value.confidence,
            ":evidence_json": json_text(&value.evidence)?,
            ":metric_json": value.metric.as_ref().map(json_text).transpose()?,
            ":source": enum_text(&value.source)?,
            ":policy_version": value.policy_version,
            ":detected_at": value.detected_at,
            ":status": enum_text(&value.status)?,
            ":first_seen_at": value.first_seen_at,
            ":last_seen_at": value.last_seen_at,
            ":resolved_at": value.resolved_at,
        },
    )?;
    Ok(())
}

fn resolve_signal(
    conn: &Connection,
    resolved_at: &str,
    workspace: &str,
    fingerprint: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE engineering_signals SET status='resolved',resolved_at=?1 \
         WHERE workspace=?2 AND fingerprint=?3 AND status='active'",
        params![resolved_at, workspace, fingerprint],
    )?;
    Ok(())
}

/// The evidence an evaluation records: the runs and executions it was built from.
fn evidence_references(sources: &EngineeringSourceRuns) -> Vec<EvidenceReference> {
    let mut references = Vec::new();
    if let Some(id) = &sources.project_index_run_id {
        references.push(reference("project-index", id));
    }
    if let Some(id) = &sources.semantic_index_run_id {
        references.push(reference("semantic-index", id));
    }
    references.extend(
        sources
            .validation_run_ids
            .iter()
            .map(|id| reference("validation", id)),
    );
    references.extend(
        sources
            .execution_ids
            .iter()
            .map(|id| reference("execution", id)),
    );
    references.extend(
        sources
            .change_set_ids
            .iter()
            .map(|id| reference("change-set", id)),
    );
    references
}

fn reference(kind: &str, id: &str) -> EvidenceReference {
    EvidenceReference {
        kind: kind.to_string(),
        id: id.to_string(),
        payload_hash: None,
    }
}

fn parse_json<T: DeserializeOwned>(text: &str, label: &str) -> Result<T> {
    serde_json::from_str(text)
        .map_err(|_| Error::Message(format!("O banco contém {label} inválidas.")))
}

fn json_text<T: serde::Serialize + ?Sized>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|error| Error::Message(error.to_string()))
}

/// A requested limit kept between 1 and the maximum; the default is the maximum.
fn bounded_limit(limit: Option<usize>, maximum: u64) -> i64 {
    let maximum = maximum as usize;
    limit.unwrap_or(maximum).clamp(1, maximum) as i64
}
