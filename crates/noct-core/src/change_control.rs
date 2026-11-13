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
