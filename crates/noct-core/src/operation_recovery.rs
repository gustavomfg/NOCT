//! Operations that were interrupted when the application last stopped. Port of
//! `shared/operationRecovery.ts`.

use serde::{Deserialize, Serialize};

/// A row recorded at startup. Nothing here means the operation may resume: the
/// reconciliation only records what was left unfinished.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterruptedOperation {
    pub id: String,
    pub workspace: String,
    pub execution_id: Option<String>,
    pub kind: String,
    pub source_id: String,
    pub original_status: String,
    pub detected_at: String,
    pub message: String,
}
