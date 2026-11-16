//! Workspace state evidence. Port of `shared/workspaceEvidence.ts`.
//!
//! Field order follows the TypeScript object, so a manifest serialized here reads back
//! byte-for-byte like one the TypeScript repository wrote.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceKind {
    Context,
    ProjectIndex,
    SemanticIndex,
    Before,
    After,
    Validation,
    Decision,
    Engineering,
}

impl EvidenceKind {
    /// The text the database stores, identical to the serialized form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Context => "context",
            Self::ProjectIndex => "project-index",
            Self::SemanticIndex => "semantic-index",
            Self::Before => "before",
            Self::After => "after",
            Self::Validation => "validation",
            Self::Decision => "decision",
            Self::Engineering => "engineering",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceReference {
    pub kind: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownWorkspacePath {
    pub path: String,
    pub hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exists: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceVerification {
    pub checked_at: String,
    pub matched_paths: u64,
    pub mismatched_paths: u64,
    pub unchecked_paths: u64,
}

/// Only the claim of currency can change: `unknown` may become `stale`, never back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Validity {
    Unknown,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Consistency {
    NonAtomic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Coverage {
    KnownPathsOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStateManifest {
    pub id: String,
    pub workspace: String,
    pub execution_id: Option<String>,
    pub kind: EvidenceKind,
    pub source_id: String,
    pub started_at: String,
    pub observed_at: String,
    pub consistency: Consistency,
    pub coverage: Coverage,
    pub validity: Validity,
    pub stale_detected_at: Option<String>,
    pub reason: Option<String>,
    pub truncated: bool,
    pub paths: Vec<KnownWorkspacePath>,
    pub references: Vec<EvidenceReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification: Option<EvidenceVerification>,
}
