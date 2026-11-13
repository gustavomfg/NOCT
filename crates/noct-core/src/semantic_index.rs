//! The semantic index: rebuildable, workspace-scoped units of source text, their runs and their
//! search filters. Port of the persisted part of `shared/semanticIndex.ts`. The embedding binding
//! and the search scores belong to the AI layer and are not part of this module.

use serde::{Deserialize, Serialize};

/// Bumped when the stored shape of the index changes.
pub const SEMANTIC_INDEX_VERSION: i64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SemanticUnitKind {
    Symbol,
    MarkdownSection,
    Configuration,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SemanticUnitStatus {
    Pending,
    Processing,
    LexicalOnly,
    Indexed,
    Stale,
    Failed,
    Incompatible,
    Excluded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SemanticRunStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SemanticRunKind {
    Initial,
    Incremental,
    Rebuild,
    Retry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticEmbeddingSpace {
    pub provider_id: String,
    pub model_id: String,
    pub model_version: Option<String>,
    pub dimensions: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticUnitLocation {
    pub start_line: i64,
    pub start_column: i64,
    pub end_line: i64,
    pub end_column: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_offset: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticUnit {
    pub id: String,
    pub workspace: String,
    pub relative_path: String,
    pub kind: SemanticUnitKind,
    pub language: Option<String>,
    pub symbol_id: Option<String>,
    pub symbol_name: Option<String>,
    pub location: SemanticUnitLocation,
    pub source_hash: String,
    pub chunk_hash: String,
    pub normalized_text: String,
    pub chunk_strategy_version: String,
    pub embedding_space: Option<SemanticEmbeddingSpace>,
    pub status: SemanticUnitStatus,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub indexed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticIndexRun {
    pub id: String,
    pub workspace: String,
    pub kind: SemanticRunKind,
    pub status: SemanticRunStatus,
    pub total_files: i64,
    pub processed_files: i64,
    pub indexed_units: i64,
    pub lexical_only_units: i64,
    pub failed_files: i64,
    pub excluded_files: i64,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub updated_at: String,
    pub error: Option<String>,
}

/// The filters of a unit query. An empty list does not restrict the query.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticSearchFilters {
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub kinds: Vec<SemanticUnitKind>,
    #[serde(default)]
    pub symbols: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticIndexSummary {
    pub workspace: String,
    pub index_version: i64,
    pub latest_run: Option<SemanticIndexRun>,
    pub files: i64,
    pub units: i64,
    pub indexed_units: i64,
    pub lexical_only_units: i64,
    pub stale_units: i64,
    pub failed_units: i64,
    pub excluded_units: i64,
}
