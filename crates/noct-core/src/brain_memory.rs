//! Brain memories. Port of `shared/brainMemory.ts`: the stored shape, the lifecycle
//! vocabulary, the search expression that retrieval uses, and the credential filter that keeps
//! secrets out of memory.
//!
//! The credential filter answers as the TypeScript filter does, on purpose. `\s` is JavaScript
//! whitespace, `\S{n}` counts UTF-16 code units, and `/i` folds ASCII letters only. The row gate
//! compares the answers of both sides on the same inputs.

use std::sync::LazyLock;

use icu_normalizer::ComposingNormalizer;
use regex::Regex;
use serde::{Deserialize, Serialize};

/// The search tokens a retrieval keeps, as the TypeScript repository does.
const SEARCH_TOKENS: usize = 12;

/// JavaScript `\s`. Rust `\s` would match U+0085 and miss U+FEFF, so the class is spelled out.
const JS_WHITESPACE: &str = r"\t\n\x{0B}\x{0C}\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}";

/// The private-use character a UTF-16 surrogate pair becomes, so that one astral character
/// counts as two units. It is never whitespace and never part of a credential pattern.
const UNIT: &str = "\u{E000}\u{E000}";

static CREDENTIAL: LazyLock<Regex> = LazyLock::new(|| {
    let space = JS_WHITESPACE;
    let non_space = format!("[^{space}]");
    Regex::new(&format!(
        "authorization[{space}]*[:=][{space}]*(?:bearer[{space}]+)?{non_space}{{8,}}\
         |(?:api[_-]?key|token|password|senha|secret)[{space}]*[:=][{space}]*{non_space}{{12,}}\
         |sk-[a-z0-9_-]{{16,}}\
         |gh[pousr]_[a-z0-9]{{20,}}"
    ))
    .expect("the credential pattern is valid")
});

static SEARCH_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\p{L}\p{N}_-]{2,}").expect("the search token pattern is valid"));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BrainMemoryKind {
    Fact,
    Decision,
    Preference,
    Constraint,
    Learning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BrainMemoryScope {
    Workspace,
    Conversation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BrainMemoryStatus {
    Candidate,
    Active,
    Outdated,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BrainMemorySource {
    Manual,
    Message,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BrainMemoryHistoryAction {
    Created,
    Edited,
    Approved,
    Disapproved,
    MarkedOutdated,
    Archived,
    Restored,
}

/// The statuses a memory can be created with. Other statuses are reached by transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InitialBrainMemoryStatus {
    Candidate,
    Active,
}

impl From<InitialBrainMemoryStatus> for BrainMemoryStatus {
    fn from(status: InitialBrainMemoryStatus) -> Self {
        match status {
            InitialBrainMemoryStatus::Candidate => Self::Candidate,
            InitialBrainMemoryStatus::Active => Self::Active,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrainMemoryHistoryEntry {
    pub id: String,
    pub memory_id: String,
    pub action: BrainMemoryHistoryAction,
    pub from_status: Option<BrainMemoryStatus>,
    pub to_status: BrainMemoryStatus,
    pub summary: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrainMemory {
    pub id: String,
    pub workspace_id: String,
    pub conversation_id: Option<String>,
    pub kind: BrainMemoryKind,
    pub scope: BrainMemoryScope,
    pub status: BrainMemoryStatus,
    pub content: String,
    pub confidence: i64,
    pub source_type: BrainMemorySource,
    pub source_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_confirmed_at: Option<String>,
    pub last_used_at: Option<String>,
    pub use_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBrainMemoryInput {
    /// Read only for the conversation scope. A workspace memory never keeps one.
    pub conversation_id: Option<String>,
    pub kind: BrainMemoryKind,
    pub scope: BrainMemoryScope,
    pub content: String,
    pub confidence: Option<i64>,
    pub source_type: Option<BrainMemorySource>,
    pub source_id: Option<String>,
    pub status: Option<InitialBrainMemoryStatus>,
}

/// An update changes only the fields it carries. `conversation_id` is doubly optional: `None`
/// keeps the conversation, and `Some(None)` clears it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateBrainMemoryInput {
    pub kind: Option<BrainMemoryKind>,
    pub scope: Option<BrainMemoryScope>,
    pub conversation_id: Option<Option<String>>,
    pub content: Option<String>,
    pub confidence: Option<i64>,
    pub status: Option<BrainMemoryStatus>,
}

/// A memory an agent proposed, before it is stored as a candidate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrainMemoryCandidate {
    pub kind: BrainMemoryKind,
    pub scope: BrainMemoryScope,
    pub content: String,
    pub confidence: i64,
}

/// Whether a memory's content is free of credentials, and so may be persisted.
pub fn is_safe_brain_memory_content(value: &str) -> bool {
    // ASCII lowercasing gives the matches of the `/i` flag: JavaScript never folds a non-ASCII
    // character onto an ASCII one, and the patterns are written in lowercase.
    let mut widened = String::with_capacity(value.len());
    for c in value.chars() {
        let c = c.to_ascii_lowercase();
        if c.len_utf16() == 2 {
            widened.push_str(UNIT);
        } else {
            widened.push(c);
        }
    }
    !CREDENTIAL.is_match(&widened)
}

/// The FTS5 expression a search uses: the Unicode tokens of the NFKC text, each quoted and
/// prefix-matched, joined with `OR`. An empty string means the query has no usable token.
pub fn fts_query(value: &str) -> String {
    let normalized = ComposingNormalizer::new_nfkc().normalize(value);
    SEARCH_TOKEN
        .find_iter(&normalized)
        .take(SEARCH_TOKENS)
        .map(|token| format!("\"{}\"*", token.as_str().replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ")
}
