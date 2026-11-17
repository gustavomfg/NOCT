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

use serde_json::Value;

use crate::limits::persistence::BRAIN_MEMORY_CHARACTERS;
use crate::text::{JS_WHITESPACE_CLASS, js_trim, utf16_len};

/// The search tokens a retrieval keeps, as the TypeScript repository does.
const SEARCH_TOKENS: usize = 12;

/// The private-use character a UTF-16 surrogate pair becomes, so that one astral character
/// counts as two units. It is never whitespace and never part of a credential pattern.
const UNIT: &str = "\u{E000}\u{E000}";

static CREDENTIAL: LazyLock<Regex> = LazyLock::new(|| {
    let space = JS_WHITESPACE_CLASS;
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

/// Whether a memory may move from one status to another. The lifecycle: candidates are approved or disapproved, active
/// memories become outdated or are archived, outdated memories return to use or are archived, and archived ones only
/// return. A status that does not change is always allowed; the caller decides whether that is a change at all.
pub fn allowed_transition(from: BrainMemoryStatus, to: BrainMemoryStatus) -> bool {
    use BrainMemoryStatus::{Active, Archived, Candidate, Outdated};
    matches!(
        (from, to),
        (Candidate, Active | Archived)
            | (Active, Outdated | Archived)
            | (Outdated, Active | Archived)
            | (Archived, Active)
    )
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

/// The content as the memory schema stores it: trimmed, from 1 to `BRAIN_MEMORY_CHARACTERS` UTF-16 units, and not a
/// credential. `None` means the content is refused. The check follows `safeBrainMemoryContent` in `shared/ipc/schemas.ts`,
/// where the trim comes first and the credential check runs on the trimmed text.
pub fn normalize_brain_content(value: &str) -> Option<String> {
    let trimmed = js_trim(value);
    let units = utf16_len(trimmed) as u64;
    if units == 0 || units > BRAIN_MEMORY_CHARACTERS || !is_safe_brain_memory_content(trimmed) {
        return None;
    }
    Some(trimmed.to_string())
}

/// The most candidates one reply can propose, as `extractBrainMemoryCandidates` keeps.
pub const EXTRACTED_CANDIDATES_MAX: usize = 5;

/// The candidates a reply proposes, and the reply without its candidate blocks.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedCandidates {
    pub candidates: Vec<BrainMemoryCandidate>,
    pub content: String,
}

/// The block a reply uses to propose memories. The marker folds ASCII letters only, as the `/i` flag does.
static MEMORY_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    let space = JS_WHITESPACE_CLASS;
    Regex::new(&format!(
        "{}[{space}]*\n([\\s\\S]*?)```",
        ascii_insensitive("```nocturne-memories")
    ))
    .expect("the memory block pattern is valid")
});

/// A literal that matches its ASCII letters in either case and nothing else, as JavaScript's `/i` without `u` does.
fn ascii_insensitive(literal: &str) -> String {
    literal
        .chars()
        .map(|c| {
            if c.is_ascii_alphabetic() {
                format!("[{}{}]", c.to_ascii_lowercase(), c.to_ascii_uppercase())
            } else {
                regex::escape(&c.to_string())
            }
        })
        .collect()
}

/// Reads the memory blocks of a reply. A block that is not JSON is skipped. Inside a block, an item that does not
/// match the candidate schema is skipped and the others are kept, and a single object counts as a one-item list. At
/// most `EXTRACTED_CANDIDATES_MAX` candidates are kept. Every block is removed from the reply, kept or not.
pub fn extract_candidates(content: &str) -> ExtractedCandidates {
    let mut candidates = Vec::new();
    for captures in MEMORY_BLOCK.captures_iter(content) {
        if candidates.len() >= EXTRACTED_CANDIDATES_MAX {
            break;
        }
        let Ok(parsed) = serde_json::from_str::<Value>(&captures[1]) else {
            continue;
        };
        let items = match parsed {
            Value::Array(items) => items,
            other => vec![other],
        };
        for item in items {
            if candidates.len() >= EXTRACTED_CANDIDATES_MAX {
                break;
            }
            if let Some(candidate) = candidate_of(item) {
                candidates.push(candidate);
            }
        }
    }
    let stripped = MEMORY_BLOCK.replace_all(content, "");
    ExtractedCandidates {
        candidates,
        content: js_trim(&stripped).to_string(),
    }
}

/// The candidate schema of `memoryCandidateSchema`: `kind`, `scope` and `content` are required, `confidence` is an
/// integer from 0 to 100 and defaults to 60, and no other key is allowed.
fn candidate_of(value: Value) -> Option<BrainMemoryCandidate> {
    let Value::Object(fields) = value else {
        return None;
    };
    let mut kind = None;
    let mut scope = None;
    let mut content = None;
    let mut confidence = None;
    for (key, field) in fields {
        match key.as_str() {
            "kind" => kind = Some(serde_json::from_value::<BrainMemoryKind>(field).ok()?),
            "scope" => scope = Some(serde_json::from_value::<BrainMemoryScope>(field).ok()?),
            "content" => match field {
                Value::String(text) => content = Some(normalize_brain_content(&text)?),
                _ => return None,
            },
            "confidence" => {
                let number = field.as_f64()?;
                if number.fract() != 0.0 || !(0.0..=100.0).contains(&number) {
                    return None;
                }
                confidence = Some(number as i64);
            }
            _ => return None,
        }
    }
    Some(BrainMemoryCandidate {
        kind: kind?,
        scope: scope?,
        content: content?,
        confidence: confidence.unwrap_or(60),
    })
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
