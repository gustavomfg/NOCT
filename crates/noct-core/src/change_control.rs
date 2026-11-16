//! Agent executions and their checkpoints. Port of the execution and checkpoint records in
//! `shared/changeControl.ts`. Field order follows the TypeScript objects.

use serde::{Deserialize, Serialize};

use crate::agent::AgentMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionStatus {
    Created,
    Planning,
    Running,
    AwaitingReview,
    Validating,
    Completed,
    Failed,
    Cancelled,
}

impl ExecutionStatus {
    /// A terminal result cannot change once it is recorded.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionDecision {
    Pending,
    Accepted,
    PartiallyAccepted,
    Rejected,
    Reverted,
    Conflicted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRecord {
    pub id: String,
    pub workspace: String,
    pub conversation_id: String,
    pub prompt: String,
    pub mode: AgentMode,
    pub status: ExecutionStatus,
    pub decision: ExecutionDecision,
    pub retry_of: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub error: Option<String>,
}

/// An execution with the counts the interface lists next to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionOverview {
    #[serde(flatten)]
    pub execution: ExecutionRecord,
    pub change_count: u64,
    pub validation_count: u64,
    pub checkpoint_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckpointPhase {
    Before,
    After,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckpointStatus {
    Capturing,
    Ready,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointRecord {
    pub id: String,
    pub execution_id: String,
    pub workspace: String,
    pub phase: CheckpointPhase,
    pub status: CheckpointStatus,
    pub captured_at: String,
    pub root_path: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckpointFileKind {
    File,
    Directory,
    Symlink,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointFileRecord {
    pub id: String,
    pub checkpoint_id: String,
    pub relative_path: String,
    pub exists: bool,
    pub kind: CheckpointFileKind,
    pub size: Option<i64>,
    pub mode: Option<i64>,
    pub hash: Option<String>,
    pub content_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewDecision {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeOperation {
    Create,
    Modify,
    Delete,
    Rename,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeOrigin {
    AgentFileChange,
    AgentCommand,
    Validation,
    Documents,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeStatus {
    Pending,
    Accepted,
    Rejected,
    Edited,
    Conflicted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeSetStatus {
    Pending,
    Accepted,
    PartiallyAccepted,
    Rejected,
    Conflicted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HunkStatus {
    Pending,
    Accepted,
    Rejected,
    Edited,
    Conflicted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangePolicy {
    Allowed,
    RequiresApproval,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidationStatus {
    Unknown,
    Pending,
    Passed,
    Failed,
    Blocked,
}

/// One file touched by an execution, as recorded for review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeRecord {
    pub id: String,
    pub execution_id: String,
    pub change_set_id: String,
    pub checkpoint_id: Option<String>,
    pub relative_path: String,
    pub original_path: Option<String>,
    pub operation: ChangeOperation,
    pub origin: ChangeOrigin,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub before_size: Option<i64>,
    pub after_size: Option<i64>,
    pub status: ChangeStatus,
    pub validation_status: ValidationStatus,
    pub policy: ChangePolicy,
    pub policy_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// The file-level result of one agent mutation attempt, between two checkpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSetRecord {
    pub id: String,
    pub execution_id: String,
    pub before_checkpoint_id: String,
    pub after_checkpoint_id: String,
    pub status: ChangeSetStatus,
    pub created_at: String,
    pub updated_at: String,
}

/// A review annotation on one hunk. `final_patch` and its decision never change the bytes
/// in the workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeHunkRecord {
    pub id: String,
    pub change_id: String,
    pub sequence: i64,
    pub base_hash: String,
    pub original_patch: String,
    pub final_patch: String,
    pub status: HunkStatus,
    pub start_line: i64,
    pub end_line: i64,
    pub decision_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionCommandSource {
    Agent,
    Validation,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionCommandStatus {
    Running,
    Passed,
    Failed,
    Cancelled,
}

/// A command an execution ran. Its output is a bounded summary, never the raw log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionCommandRecord {
    pub id: String,
    pub execution_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub source: ExecutionCommandSource,
    pub status: ExecutionCommandStatus,
    pub exit_code: Option<i64>,
    pub duration_ms: Option<i64>,
    pub output_summary: String,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionErrorStage {
    Planning,
    Mutation,
    Validation,
    Decision,
    Rollback,
    Persistence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionErrorRecord {
    pub id: String,
    pub execution_id: String,
    pub stage: ExecutionErrorStage,
    pub message: String,
    pub path: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionValidationPhase {
    Before,
    Proposed,
    AfterDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionValidationLink {
    pub execution_id: String,
    pub change_id: Option<String>,
    pub validation_id: String,
    pub phase: ExecutionValidationPhase,
}
