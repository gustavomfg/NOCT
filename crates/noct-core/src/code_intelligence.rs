//! The project index. Port of the project index part of `shared/codeIntelligence.ts`: the stored
//! shape of files, runs, symbols, relations, stack evidence and the summary. The validation
//! types of that file are in `validation_run`.
//!
//! The index keeps derived knowledge only. Source contents are never part of these shapes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Bumped when parser or resolution semantics change, so that persisted relations are recomputed.
pub const INDEX_VERSION: i64 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiscoveryFileClassification {
    Source,
    Configuration,
    Documentation,
    Lockfile,
    Asset,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectIndexFileState {
    Discovered,
    Pending,
    Processing,
    Indexed,
    Unsupported,
    Failed,
    Deleted,
    Excluded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectIndexRunKind {
    Initial,
    Incremental,
    Reconcile,
    Retry,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectIndexRunStatus {
    Queued,
    Running,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectIndexPhase {
    Discovering,
    Hashing,
    Parsing,
    Persisting,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectSymbolKind {
    Function,
    Class,
    Interface,
    Type,
    Enum,
    Component,
    Method,
    Variable,
    Namespace,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectImportKind {
    Default,
    Named,
    Namespace,
    SideEffect,
    Require,
    Dynamic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectExportKind {
    Named,
    Default,
    All,
    ReExport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImportResolution {
    Local,
    External,
    Unresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StackEvidenceCategory {
    Language,
    Runtime,
    Framework,
    Bundler,
    PackageManager,
    Typecheck,
    Lint,
    Test,
    Build,
    Convention,
    Script,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryExclusion {
    pub relative_path: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIndexFile {
    pub workspace: String,
    pub relative_path: String,
    pub classification: DiscoveryFileClassification,
    pub language: String,
    pub extension: String,
    pub size: i64,
    /// Filesystem times keep their fraction, so they are stored and read as REAL.
    pub mtime_ms: f64,
    pub ctime_ms: f64,
    pub mode: i64,
    pub observed_hash: Option<String>,
    pub analyzed_hash: Option<String>,
    pub state: ProjectIndexFileState,
    pub excluded: bool,
    pub exclusion_reason: Option<String>,
    pub parser_id: Option<String>,
    pub parser_version: Option<String>,
    pub error: Option<String>,
    pub discovered_at: String,
    pub analyzed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIndexRun {
    pub id: String,
    pub workspace: String,
    pub index_version: i64,
    pub kind: ProjectIndexRunKind,
    pub status: ProjectIndexRunStatus,
    pub phase: ProjectIndexPhase,
    pub total_files: i64,
    pub processed_files: i64,
    pub failed_files: i64,
    pub unsupported_files: i64,
    pub pending_files: i64,
    pub started_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub error: Option<String>,
}

/// A position in a file. Lines and columns are one-based, as the parser reports them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolLocation {
    pub start_line: i64,
    pub start_column: i64,
    pub end_line: i64,
    pub end_column: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSymbol {
    pub id: String,
    pub workspace: String,
    pub relative_path: String,
    pub analyzed_hash: String,
    pub kind: ProjectSymbolKind,
    pub name: String,
    pub qualified_name: Option<String>,
    pub scope: Option<String>,
    pub signature: Option<String>,
    pub location: SymbolLocation,
    pub exported: bool,
    pub parser_id: String,
    pub parser_version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectImport {
    pub id: String,
    pub workspace: String,
    pub source_path: String,
    pub source_hash: String,
    pub specifier: String,
    pub target_path: Option<String>,
    pub target_hash: Option<String>,
    pub kind: ProjectImportKind,
    pub imported_names: Vec<String>,
    pub location: SymbolLocation,
    pub resolution: ImportResolution,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectExport {
    pub id: String,
    pub workspace: String,
    pub source_path: String,
    pub source_hash: String,
    pub name: String,
    pub kind: ProjectExportKind,
    pub target_path: Option<String>,
    pub target_hash: Option<String>,
    pub location: SymbolLocation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StackEvidence {
    pub id: String,
    pub workspace: String,
    pub category: StackEvidenceCategory,
    pub value: String,
    pub confidence: i64,
    pub source_path: String,
    pub source_hash: String,
    pub source_line: Option<i64>,
    pub reason: String,
    pub detected_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedStack {
    pub name: String,
    pub stack: Vec<String>,
    pub primary_language: String,
    pub commands: BTreeMap<String, String>,
    pub evidence: Vec<StackEvidence>,
    pub detected_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIndexSummary {
    pub workspace: String,
    pub index_version: i64,
    pub latest_run: Option<ProjectIndexRun>,
    pub files: i64,
    pub indexed_files: i64,
    pub failed_files: i64,
    pub unsupported_files: i64,
    pub symbols: i64,
    pub imports: i64,
    pub exports: i64,
    pub stack: Option<DetectedStack>,
}
