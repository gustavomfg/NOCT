//! Engineering intelligence: health signals, the insights they support and the snapshots that
//! evaluate them. Port of the persisted shapes in `shared/engineeringIntelligence.ts` and of the
//! checks in `shared/engineeringIntelligenceSchemas.ts`.
//!
//! A `parse_*` function checks a value against its schema and returns it as the schema reads it:
//! strings are trimmed, as Zod's `trim()` does. A write stores what it was given after the check,
//! and a read returns the checked value. Nullable keys are read as `null` when missing, where Zod
//! rejects the object; rows always hold every key, so only edited data can show the difference.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::limits::code_intelligence::MAX_QUERY_RESULTS;
use crate::limits::engineering_intelligence::{
    MAX_DESCRIPTION_CHARACTERS, MAX_EVIDENCE_DETAIL_CHARACTERS, MAX_EVIDENCE_PER_SIGNAL,
    MAX_SIGNALS, MAX_TITLE_CHARACTERS,
};
use crate::text::utf16_len;
use crate::validation::{ContractError, text};

/// The categories every snapshot assesses, in the order the policy lists them.
pub const ENGINEERING_HEALTH_CATEGORIES: [EngineeringHealthCategory; 8] = [
    EngineeringHealthCategory::Architecture,
    EngineeringHealthCategory::Testing,
    EngineeringHealthCategory::Security,
    EngineeringHealthCategory::Documentation,
    EngineeringHealthCategory::Dependencies,
    EngineeringHealthCategory::Performance,
    EngineeringHealthCategory::DeveloperExperience,
    EngineeringHealthCategory::Release,
];

const ID_CHARACTERS: u64 = 512;
const PATH_CHARACTERS: u64 = 4_000;
const METRIC_NAME_CHARACTERS: u64 = 200;
const METRIC_UNIT_CHARACTERS: u64 = 50;
const METRIC_TEXT_CHARACTERS: u64 = 500;
const KIND_CHARACTERS: u64 = 200;
const SUGGESTED_ACTION_CHARACTERS: u64 = 2_000;

static TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:(?:[0-9]{2}[2468][048]|[0-9]{2}[13579][26]|[0-9]{2}0[48]|[02468][048]00|[13579][26]00)-02-29|[0-9]{4}-(?:(?:0[13578]|1[02])-(?:0[1-9]|[12][0-9]|3[01])|(?:0[469]|11)-(?:0[1-9]|[12][0-9]|30)|(?:02)-(?:0[1-9]|1[0-9]|2[0-8])))T(?:(?:[01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9](?:\.[0-9]+)?(?:Z|[+-](?:[01][0-9]|2[0-3]):[0-5][0-9]))$",
    )
    .expect("timestamp pattern is valid")
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EngineeringHealthCategory {
    Architecture,
    Testing,
    Security,
    Documentation,
    Dependencies,
    Performance,
    DeveloperExperience,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EngineeringSignalSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EngineeringSignalStatus {
    Active,
    Resolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HealthAssessmentStatus {
    Assessed,
    Partial,
    NotAssessed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EngineeringEvidenceSource {
    ProjectIndexFile,
    ProjectIndexRun,
    ProjectSymbol,
    ProjectImport,
    ProjectExport,
    StackEvidence,
    SemanticIndexRun,
    SemanticUnit,
    ValidationRun,
    Execution,
    ChangeSet,
    Change,
    GitState,
    LocalMetric,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EngineeringSignalSource {
    Deterministic,
    Correlation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EngineeringEvidenceValidity {
    Current,
    Stale,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineeringEvidenceLocation {
    pub start_line: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_column: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_column: Option<i64>,
}

/// A bounded reference to local evidence. It carries no source text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineeringEvidence {
    pub id: String,
    pub source: EngineeringEvidenceSource,
    pub source_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<EngineeringEvidenceLocation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validity: Option<EngineeringEvidenceValidity>,
    pub detail: String,
    pub observed_at: String,
}

/// A metric value: a finite number, a text of at most 500 characters, a flag, or nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EngineeringMetricValue {
    Number(f64),
    Text(String),
    Flag(bool),
    Null(()),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineeringMetric {
    pub name: String,
    pub value: EngineeringMetricValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

/// Evidence quality and coverage of a signal, never model confidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineeringSignal {
    pub id: String,
    pub workspace: String,
    pub category: EngineeringHealthCategory,
    pub kind: String,
    pub title: String,
    pub description: String,
    pub severity: EngineeringSignalSeverity,
    pub confidence: i64,
    pub evidence: Vec<EngineeringEvidence>,
    pub metric: Option<EngineeringMetric>,
    pub source: EngineeringSignalSource,
    pub policy_version: i64,
    pub detected_at: String,
    pub fingerprint: String,
    pub status: EngineeringSignalStatus,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HealthEvidenceCoverage {
    pub available: i64,
    pub expected: Option<i64>,
    pub percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HealthCategoryResult {
    pub category: EngineeringHealthCategory,
    pub status: HealthAssessmentStatus,
    pub score: Option<f64>,
    pub coverage: HealthEvidenceCoverage,
    pub signal_ids: Vec<String>,
    pub evidence: Vec<EngineeringEvidence>,
    pub evaluated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineeringSourceRuns {
    pub project_index_run_id: Option<String>,
    pub semantic_index_run_id: Option<String>,
    pub validation_run_ids: Vec<String>,
    pub execution_ids: Vec<String>,
    pub change_set_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineeringHealthSnapshot {
    pub id: String,
    pub workspace: String,
    pub policy_version: i64,
    pub evaluated_at: String,
    pub previous_snapshot_id: Option<String>,
    pub sources: EngineeringSourceRuns,
    pub categories: Vec<HealthCategoryResult>,
    pub signal_fingerprints: Vec<String>,
    pub signal_states: Vec<EngineeringSignalSnapshotState>,
    pub state_fingerprint: String,
}

/// The state a snapshot keeps of one signal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineeringSignalSnapshotState {
    pub fingerprint: String,
    pub category: EngineeringHealthCategory,
    pub severity: EngineeringSignalSeverity,
    pub status: EngineeringSignalStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineeringInsight {
    pub id: String,
    pub workspace: String,
    pub fingerprint: String,
    pub title: String,
    pub explanation: String,
    pub related_signal_ids: Vec<String>,
    pub evidence: Vec<EngineeringEvidence>,
    pub suggested_action: Option<String>,
    pub confidence: i64,
    pub status: EngineeringSignalStatus,
    pub created_at: String,
    pub updated_at: String,
    pub last_seen_at: String,
}

/// Checks a signal and returns it with its strings trimmed.
pub fn parse_signal(value: &EngineeringSignal) -> Result<EngineeringSignal, ContractError> {
    Ok(EngineeringSignal {
        id: identifier("signal.id", &value.id)?,
        workspace: text("signal.workspace", &value.workspace, PATH_CHARACTERS)?,
        category: value.category,
        kind: text("signal.kind", &value.kind, KIND_CHARACTERS)?,
        title: text("signal.title", &value.title, MAX_TITLE_CHARACTERS)?,
        description: text(
            "signal.description",
            &value.description,
            MAX_DESCRIPTION_CHARACTERS,
        )?,
        severity: value.severity,
        confidence: integer_between("signal.confidence", value.confidence, 0, 100)?,
        evidence: evidence_list(
            "signal.evidence",
            &value.evidence,
            1,
            MAX_EVIDENCE_PER_SIGNAL,
        )?,
        metric: value.metric.as_ref().map(parse_metric).transpose()?,
        source: value.source,
        policy_version: at_least("signal.policyVersion", value.policy_version, 1)?,
        detected_at: timestamp("signal.detectedAt", &value.detected_at)?,
        fingerprint: hash("signal.fingerprint", &value.fingerprint)?,
        status: value.status,
        first_seen_at: timestamp("signal.firstSeenAt", &value.first_seen_at)?,
        last_seen_at: timestamp("signal.lastSeenAt", &value.last_seen_at)?,
        resolved_at: value
            .resolved_at
            .as_deref()
            .map(|resolved| timestamp("signal.resolvedAt", resolved))
            .transpose()?,
    })
}

/// Checks a snapshot and returns it with its strings trimmed.
pub fn parse_snapshot(
    value: &EngineeringHealthSnapshot,
) -> Result<EngineeringHealthSnapshot, ContractError> {
    if value.categories.len() != ENGINEERING_HEALTH_CATEGORIES.len() {
        return Err(ContractError::invalid("snapshot.categories"));
    }
    Ok(EngineeringHealthSnapshot {
        id: identifier("snapshot.id", &value.id)?,
        workspace: text("snapshot.workspace", &value.workspace, PATH_CHARACTERS)?,
        policy_version: at_least("snapshot.policyVersion", value.policy_version, 1)?,
        evaluated_at: timestamp("snapshot.evaluatedAt", &value.evaluated_at)?,
        previous_snapshot_id: optional_identifier(
            "snapshot.previousSnapshotId",
            value.previous_snapshot_id.as_deref(),
        )?,
        sources: parse_sources(&value.sources)?,
        categories: value
            .categories
            .iter()
            .map(parse_category)
            .collect::<Result<_, _>>()?,
        signal_fingerprints: bounded_list(
            "snapshot.signalFingerprints",
            &value.signal_fingerprints,
            MAX_SIGNALS,
            hash,
        )?,
        signal_states: bounded_items(
            "snapshot.signalStates",
            &value.signal_states,
            MAX_SIGNALS,
            parse_signal_state,
        )?,
        state_fingerprint: hash("snapshot.stateFingerprint", &value.state_fingerprint)?,
    })
}

/// Checks an insight and returns it with its strings trimmed.
pub fn parse_insight(value: &EngineeringInsight) -> Result<EngineeringInsight, ContractError> {
    Ok(EngineeringInsight {
        id: identifier("insight.id", &value.id)?,
        workspace: text("insight.workspace", &value.workspace, PATH_CHARACTERS)?,
        fingerprint: hash("insight.fingerprint", &value.fingerprint)?,
        title: text("insight.title", &value.title, MAX_TITLE_CHARACTERS)?,
        explanation: text(
            "insight.explanation",
            &value.explanation,
            MAX_DESCRIPTION_CHARACTERS,
        )?,
        related_signal_ids: bounded_list(
            "insight.relatedSignalIds",
            &value.related_signal_ids,
            MAX_SIGNALS,
            identifier,
        )?,
        evidence: evidence_list(
            "insight.evidence",
            &value.evidence,
            1,
            MAX_EVIDENCE_PER_SIGNAL,
        )?,
        suggested_action: value
            .suggested_action
            .as_deref()
            .map(|action| {
                text(
                    "insight.suggestedAction",
                    action,
                    SUGGESTED_ACTION_CHARACTERS,
                )
            })
            .transpose()?,
        confidence: integer_between("insight.confidence", value.confidence, 0, 100)?,
        status: value.status,
        created_at: timestamp("insight.createdAt", &value.created_at)?,
        updated_at: timestamp("insight.updatedAt", &value.updated_at)?,
        last_seen_at: timestamp("insight.lastSeenAt", &value.last_seen_at)?,
    })
}

fn parse_evidence(value: &EngineeringEvidence) -> Result<EngineeringEvidence, ContractError> {
    Ok(EngineeringEvidence {
        id: identifier("evidence.id", &value.id)?,
        source: value.source,
        source_id: identifier("evidence.sourceId", &value.source_id)?,
        relative_path: value
            .relative_path
            .as_deref()
            .map(|path| text("evidence.relativePath", path, PATH_CHARACTERS))
            .transpose()?,
        source_hash: value
            .source_hash
            .as_deref()
            .map(|digest| hash("evidence.sourceHash", digest))
            .transpose()?,
        run_id: optional_identifier("evidence.runId", value.run_id.as_deref())?,
        location: value.location.map(parse_location).transpose()?,
        validity: value.validity,
        detail: text(
            "evidence.detail",
            &value.detail,
            MAX_EVIDENCE_DETAIL_CHARACTERS,
        )?,
        observed_at: timestamp("evidence.observedAt", &value.observed_at)?,
    })
}

fn parse_location(
    value: EngineeringEvidenceLocation,
) -> Result<EngineeringEvidenceLocation, ContractError> {
    Ok(EngineeringEvidenceLocation {
        start_line: at_least("evidence.location.startLine", value.start_line, 1)?,
        start_column: optional_at_least("evidence.location.startColumn", value.start_column, 1)?,
        end_line: optional_at_least("evidence.location.endLine", value.end_line, 1)?,
        end_column: optional_at_least("evidence.location.endColumn", value.end_column, 1)?,
    })
}

fn parse_metric(value: &EngineeringMetric) -> Result<EngineeringMetric, ContractError> {
    let checked = match &value.value {
        EngineeringMetricValue::Number(number) if !number.is_finite() => {
            return Err(ContractError::invalid("metric.value"));
        }
        EngineeringMetricValue::Text(content) => {
            if utf16_len(content) as u64 > METRIC_TEXT_CHARACTERS {
                return Err(ContractError::invalid("metric.value"));
            }
            EngineeringMetricValue::Text(content.clone())
        }
        other => other.clone(),
    };
    Ok(EngineeringMetric {
        name: text("metric.name", &value.name, METRIC_NAME_CHARACTERS)?,
        value: checked,
        unit: value
            .unit
            .as_deref()
            .map(|unit| text("metric.unit", unit, METRIC_UNIT_CHARACTERS))
            .transpose()?,
    })
}

fn parse_category(value: &HealthCategoryResult) -> Result<HealthCategoryResult, ContractError> {
    Ok(HealthCategoryResult {
        category: value.category,
        status: value.status,
        score: value
            .score
            .map(|score| finite_between("category.score", score, 0.0, 100.0))
            .transpose()?,
        coverage: parse_coverage(&value.coverage)?,
        signal_ids: bounded_list(
            "category.signalIds",
            &value.signal_ids,
            MAX_SIGNALS,
            identifier,
        )?,
        evidence: evidence_list(
            "category.evidence",
            &value.evidence,
            0,
            MAX_EVIDENCE_PER_SIGNAL,
        )?,
        evaluated_at: timestamp("category.evaluatedAt", &value.evaluated_at)?,
    })
}

fn parse_coverage(value: &HealthEvidenceCoverage) -> Result<HealthEvidenceCoverage, ContractError> {
    Ok(HealthEvidenceCoverage {
        available: at_least("coverage.available", value.available, 0)?,
        expected: value
            .expected
            .map(|expected| at_least("coverage.expected", expected, 0))
            .transpose()?,
        percent: value
            .percent
            .map(|percent| finite_between("coverage.percent", percent, 0.0, 100.0))
            .transpose()?,
    })
}

fn parse_sources(value: &EngineeringSourceRuns) -> Result<EngineeringSourceRuns, ContractError> {
    Ok(EngineeringSourceRuns {
        project_index_run_id: optional_identifier(
            "sources.projectIndexRunId",
            value.project_index_run_id.as_deref(),
        )?,
        semantic_index_run_id: optional_identifier(
            "sources.semanticIndexRunId",
            value.semantic_index_run_id.as_deref(),
        )?,
        validation_run_ids: bounded_list(
            "sources.validationRunIds",
            &value.validation_run_ids,
            MAX_QUERY_RESULTS,
            identifier,
        )?,
        execution_ids: bounded_list(
            "sources.executionIds",
            &value.execution_ids,
            MAX_QUERY_RESULTS,
            identifier,
        )?,
        change_set_ids: bounded_list(
            "sources.changeSetIds",
            &value.change_set_ids,
            MAX_QUERY_RESULTS,
            identifier,
        )?,
    })
}

fn parse_signal_state(
    value: &EngineeringSignalSnapshotState,
) -> Result<EngineeringSignalSnapshotState, ContractError> {
    Ok(EngineeringSignalSnapshotState {
        fingerprint: hash("signalState.fingerprint", &value.fingerprint)?,
        category: value.category,
        severity: value.severity,
        status: value.status,
    })
}

fn evidence_list(
    field: &str,
    items: &[EngineeringEvidence],
    minimum: usize,
    maximum: u64,
) -> Result<Vec<EngineeringEvidence>, ContractError> {
    let length = items.len();
    if length < minimum || length as u64 > maximum {
        return Err(ContractError::invalid(field));
    }
    items.iter().map(parse_evidence).collect()
}

/// A list of at most `maximum` items, each checked by `check`.
fn bounded_list(
    field: &str,
    items: &[String],
    maximum: u64,
    check: fn(&str, &str) -> Result<String, ContractError>,
) -> Result<Vec<String>, ContractError> {
    if items.len() as u64 > maximum {
        return Err(ContractError::invalid(field));
    }
    items.iter().map(|item| check(field, item)).collect()
}

/// A list of at most `maximum` structured items, each checked by `check`.
fn bounded_items<T, U>(
    field: &str,
    items: &[T],
    maximum: u64,
    check: fn(&T) -> Result<U, ContractError>,
) -> Result<Vec<U>, ContractError> {
    if items.len() as u64 > maximum {
        return Err(ContractError::invalid(field));
    }
    items.iter().map(check).collect()
}

fn identifier(field: &str, value: &str) -> Result<String, ContractError> {
    text(field, value, ID_CHARACTERS)
}

fn optional_identifier(field: &str, value: Option<&str>) -> Result<Option<String>, ContractError> {
    value.map(|id| identifier(field, id)).transpose()
}

/// A timestamp with a time zone offset, as `z.string().datetime({ offset: true })` reads it. The
/// value is not trimmed: a timestamp with surrounding spaces is rejected.
fn timestamp(field: &str, value: &str) -> Result<String, ContractError> {
    if TIMESTAMP.is_match(value) {
        Ok(value.to_string())
    } else {
        Err(ContractError::invalid(field))
    }
}

/// A SHA-256 digest written as 64 lowercase hexadecimal characters.
fn hash(field: &str, value: &str) -> Result<String, ContractError> {
    let valid = value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if valid {
        Ok(value.to_string())
    } else {
        Err(ContractError::invalid(field))
    }
}

fn integer_between(
    field: &str,
    value: i64,
    minimum: i64,
    maximum: i64,
) -> Result<i64, ContractError> {
    if (minimum..=maximum).contains(&value) {
        Ok(value)
    } else {
        Err(ContractError::invalid(field))
    }
}

fn at_least(field: &str, value: i64, minimum: i64) -> Result<i64, ContractError> {
    integer_between(field, value, minimum, i64::MAX)
}

fn optional_at_least(
    field: &str,
    value: Option<i64>,
    minimum: i64,
) -> Result<Option<i64>, ContractError> {
    value
        .map(|number| at_least(field, number, minimum))
        .transpose()
}

fn finite_between(
    field: &str,
    value: f64,
    minimum: f64,
    maximum: f64,
) -> Result<f64, ContractError> {
    if value.is_finite() && (minimum..=maximum).contains(&value) {
        Ok(value)
    } else {
        Err(ContractError::invalid(field))
    }
}
