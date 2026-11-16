//! Validation runs. Port of the validation records in `shared/codeIntelligence.ts`.
//!
//! The run status is not the change validation status: a run can be queued or cancelled,
//! while a change only records whether its validation passed or failed.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidationKind {
    Typecheck,
    Lint,
    Test,
    Build,
    Smoke,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidationRunStatus {
    Queued,
    Running,
    Passed,
    Failed,
    Cancelled,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationArtifact {
    pub path: String,
    pub kind: String,
    pub size: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationRun {
    pub id: String,
    pub workspace: String,
    /// Omitted, not null, when the run belongs to no execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<String>,
    pub kind: ValidationKind,
    pub command: String,
    pub args: Vec<String>,
    pub status: ValidationRunStatus,
    pub exit_code: Option<i64>,
    pub duration_ms: Option<i64>,
    pub output_summary: String,
    pub artifacts: Vec<ValidationArtifact>,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub error: Option<String>,
}
