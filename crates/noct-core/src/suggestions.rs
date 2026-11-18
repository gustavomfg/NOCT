//! Review suggestions. Port of `shared/suggestions.ts`: the stored shape, the identity that
//! matches a suggestion across reviews, and title sanitizing. Reading agent output into
//! suggestions belongs to the AI layer, which is not ported yet.
//!
//! The identity depends on Unicode normalization and general categories, as the TypeScript
//! identity does (`normalize('NFKD')`, `\p{M}`, `\p{L}`, `\p{N}`, `\p{Cc}`). The ICU
//! tables provide them, and the row gate compares identities computed by both sides.

use icu_normalizer::DecomposingNormalizer;
use icu_properties::CodePointMapData;
use icu_properties::props::{GeneralCategory, GeneralCategoryGroup};
use serde::{Deserialize, Serialize};

use crate::text::{JS_WHITESPACE_CLASS, ascii_insensitive, is_js_whitespace, js_trim, utf16_len};
use std::sync::LazyLock;

use regex::Regex;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SuggestionCategory {
    Architecture,
    Security,
    Performance,
    Bug,
    Cleanup,
    Testing,
    Documentation,
    Dependency,
    Accessibility,
}

impl SuggestionCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Architecture => "architecture",
            Self::Security => "security",
            Self::Performance => "performance",
            Self::Bug => "bug",
            Self::Cleanup => "cleanup",
            Self::Testing => "testing",
            Self::Documentation => "documentation",
            Self::Dependency => "dependency",
            Self::Accessibility => "accessibility",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SuggestionSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SuggestionStatus {
    New,
    InAnalysis,
    Accepted,
    Rejected,
    Resolved,
    Deferred,
    Invalid,
}

impl SuggestionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::InAnalysis => "in-analysis",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Resolved => "resolved",
            Self::Deferred => "deferred",
            Self::Invalid => "invalid",
        }
    }

    /// Open suggestions: the ones a review still shows and can reconcile.
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::New | Self::InAnalysis | Self::Accepted | Self::Deferred
        )
    }

    /// Statuses a review may still update. An accepted suggestion is kept as it is.
    pub fn is_open_for_update(self) -> bool {
        matches!(self, Self::New | Self::InAnalysis | Self::Deferred)
    }

    /// Statuses a review may resolve when the suggestion no longer reappears.
    pub fn is_resolvable(self) -> bool {
        matches!(self, Self::New | Self::InAnalysis)
    }
}

/// The complexity and risk scale of a suggestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SuggestionLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionEvidence {
    pub source: String,
    pub detail: String,
    /// Omitted, not null, when no location is given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionHistoryEntry {
    pub id: String,
    pub status: SuggestionStatus,
    pub result: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewComparisonItem {
    pub id: String,
    pub title: String,
    pub severity: SuggestionSeverity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSeverityChange {
    pub id: String,
    pub title: String,
    pub from: SuggestionSeverity,
    pub to: SuggestionSeverity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewComparison {
    pub reviewed_at: String,
    pub new_suggestions: Vec<ReviewComparisonItem>,
    pub persistent_suggestions: Vec<ReviewComparisonItem>,
    pub resolved_suggestions: Vec<ReviewComparisonItem>,
    pub severity_changes: Vec<ReviewSeverityChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionReconciliation {
    pub suggestions: Vec<Suggestion>,
    pub comparison: ReviewComparison,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub id: String,
    pub workspace_id: String,
    pub conversation_id: String,
    pub title: String,
    pub description: String,
    pub reasoning: String,
    pub category: SuggestionCategory,
    pub severity: SuggestionSeverity,
    pub affected_files: Vec<String>,
    pub proposed_changes: String,
    pub expected_benefits: Vec<String>,
    pub complexity: SuggestionLevel,
    pub risk: SuggestionLevel,
    pub evidence: Vec<SuggestionEvidence>,
    pub confidence: i64,
    pub source: String,
    pub responsible: String,
    pub created_at: String,
    pub updated_at: String,
    pub status: SuggestionStatus,
    pub history: Vec<SuggestionHistoryEntry>,
}

/// What a review proposes. Optional fields take the defaults the repository applies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionInput {
    pub title: String,
    pub description: String,
    pub reasoning: String,
    pub category: SuggestionCategory,
    pub severity: SuggestionSeverity,
    pub affected_files: Vec<String>,
    pub proposed_changes: String,
    pub expected_benefits: Vec<String>,
    pub complexity: SuggestionLevel,
    pub risk: SuggestionLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Vec<SuggestionEvidence>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responsible: Option<String>,
}

/// The confidence a suggestion gets when its input gives none.
pub const DEFAULT_CONFIDENCE: i64 = 60;
pub const DEFAULT_SOURCE: &str = "Análise do agente";
pub const DEFAULT_RESPONSIBLE: &str = "Agente de revisão";

/// `sanitizeSuggestionTitle`: control characters become spaces, runs of whitespace collapse
/// to one space, and the ends are trimmed.
pub fn sanitize_suggestion_title(value: &str) -> String {
    value
        .chars()
        .map(|c| if is_control(c) { ' ' } else { c })
        .collect::<String>()
        .split(is_js_whitespace)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `suggestionIdentity`: the category and the title in a form that ignores case, accents,
/// punctuation and spacing, so the same finding matches across reviews.
pub fn suggestion_identity(category: SuggestionCategory, title: &str) -> String {
    format!("{}:{}", category.as_str(), normalized_title(title))
}

fn normalized_title(title: &str) -> String {
    let sanitized = sanitize_suggestion_title(title);
    let decomposed = DecomposingNormalizer::new_nfkd().normalize(&sanitized);
    let unmarked: String = decomposed
        .chars()
        .filter(|c| !is_group(*c, GeneralCategoryGroup::Mark))
        .collect();
    let lowered = unmarked.to_lowercase();

    // Each run of characters outside letters and numbers becomes one space, and the
    // result has no leading or trailing space.
    let mut normalized = String::with_capacity(lowered.len());
    let mut pending_space = false;
    for c in lowered.chars() {
        if is_group(c, GeneralCategoryGroup::Letter) || is_group(c, GeneralCategoryGroup::Number) {
            if pending_space && !normalized.is_empty() {
                normalized.push(' ');
            }
            pending_space = false;
            normalized.push(c);
        } else {
            pending_space = true;
        }
    }
    normalized
}

fn is_control(c: char) -> bool {
    is_group(c, GeneralCategoryGroup::Control)
}

fn is_group(c: char, group: GeneralCategoryGroup) -> bool {
    group.contains(general_category(c))
}

fn general_category(c: char) -> GeneralCategory {
    CodePointMapData::<GeneralCategory>::new().get(c)
}

/// The transitions a suggestion may make, as `SuggestionRepository.setStatus` allows them. A rejected, resolved or
/// invalid suggestion is final, and a deferred one may return to analysis.
pub fn allowed_suggestion_transition(from: SuggestionStatus, to: SuggestionStatus) -> bool {
    use SuggestionStatus::{Accepted, Deferred, InAnalysis, Invalid, New, Rejected, Resolved};
    match from {
        New => matches!(
            to,
            InAnalysis | Accepted | Rejected | Resolved | Deferred | Invalid
        ),
        InAnalysis => matches!(to, Accepted | Rejected | Resolved | Deferred | Invalid),
        Accepted => matches!(to, Resolved | Rejected | Deferred | Invalid),
        Deferred => matches!(to, InAnalysis | Accepted | Rejected | Resolved | Invalid),
        Rejected | Resolved | Invalid => false,
    }
}

/// Where the structured review snapshot of a reply stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionSnapshot {
    /// The reply carries a snapshot that reads as suggestions.
    Valid,
    /// The reply carries a snapshot marker or a JSON block at its end, but the snapshot does not read.
    Invalid,
    /// The reply carries no snapshot at all.
    Absent,
}

/// The suggestions a reply proposes, and the reply without its snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct SuggestionExtraction {
    pub suggestions: Vec<SuggestionInput>,
    pub content: String,
    pub structured: bool,
    pub snapshot: SuggestionSnapshot,
}

/// The snapshot block a review writes: a `nocturne-suggestions` fence, read with `\b` after the marker.
static SUGGESTION_MARKER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        "{}(?-u:\\b)",
        ascii_insensitive("```nocturne-suggestions")
    ))
    .expect("the suggestion marker pattern is valid")
});

static SUGGESTION_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    let space = JS_WHITESPACE_CLASS;
    Regex::new(&format!(
        "{}[{space}]*\\n([\\s\\S]*?)```",
        ascii_insensitive("```nocturne-suggestions")
    ))
    .expect("the suggestion block pattern is valid")
});

/// A JSON fence that ends the reply, read when no marker is present.
static SUGGESTION_FALLBACK: LazyLock<Regex> = LazyLock::new(|| {
    let space = JS_WHITESPACE_CLASS;
    Regex::new(&format!(
        "{}[{space}]*\\n([\\s\\S]*?)```[{space}]*$",
        ascii_insensitive("```json")
    ))
    .expect("the JSON fallback pattern is valid")
});

/// `extractSuggestions`: reads the snapshot of a review reply. A reply with one marker and one block is read strictly:
/// every item must match, or the whole snapshot is refused. Without a marker, a JSON fence at the very end is read the
/// same way. The reply comes back without its snapshot when one is read, and trimmed in every case.
pub fn extract_suggestions(content: &str) -> SuggestionExtraction {
    let marker_count = SUGGESTION_MARKER.find_iter(content).count();
    if marker_count > 0 {
        let blocks: Vec<_> = SUGGESTION_BLOCK.captures_iter(content).collect();
        let parsed = if marker_count == 1 && blocks.len() == 1 {
            parse_strict_array(&blocks[0][1])
        } else {
            None
        };
        return match parsed {
            Some(suggestions) => SuggestionExtraction {
                suggestions,
                content: js_trim(&SUGGESTION_BLOCK.replace_all(content, "")).to_string(),
                structured: true,
                snapshot: SuggestionSnapshot::Valid,
            },
            None => SuggestionExtraction {
                suggestions: Vec::new(),
                content: js_trim(content).to_string(),
                structured: false,
                snapshot: SuggestionSnapshot::Invalid,
            },
        };
    }
    if let Some(fallback) = SUGGESTION_FALLBACK.captures(content) {
        let start = fallback.get(0).map_or(0, |whole| whole.start());
        return match parse_strict_array(&fallback[1]) {
            Some(suggestions) => SuggestionExtraction {
                suggestions,
                content: js_trim(&content[..start]).to_string(),
                structured: true,
                snapshot: SuggestionSnapshot::Valid,
            },
            None => SuggestionExtraction {
                suggestions: Vec::new(),
                content: js_trim(content).to_string(),
                structured: false,
                snapshot: SuggestionSnapshot::Invalid,
            },
        };
    }
    SuggestionExtraction {
        suggestions: Vec::new(),
        content: js_trim(content).to_string(),
        structured: false,
        snapshot: SuggestionSnapshot::Absent,
    }
}

/// `parseStrictSuggestionArray`: a JSON array whose every item matches the suggestion schema, or nothing at all.
fn parse_strict_array(text: &str) -> Option<Vec<SuggestionInput>> {
    let Value::Array(items) = serde_json::from_str::<Value>(text).ok()? else {
        return None;
    };
    items.iter().map(suggestion_of).collect()
}

/// `suggestionInputSchema`: the required fields, the defaults the schema gives the others, and the bounds of each.
/// Lengths are UTF-16 units, as zod counts them. An unknown key is ignored, as zod strips it.
fn suggestion_of(value: &Value) -> Option<SuggestionInput> {
    let Value::Object(fields) = value else {
        return None;
    };
    Some(SuggestionInput {
        title: title_of(fields.get("title")?)?,
        description: text_of(fields.get("description")?, 10_000)?,
        reasoning: text_of(fields.get("reasoning")?, 10_000)?,
        category: enum_of(fields.get("category")?)?,
        severity: enum_of(fields.get("severity")?)?,
        affected_files: list_of(fields, "affectedFiles", 100, |item| text_of(item, 1_000))?,
        proposed_changes: proposed_changes_of(fields.get("proposedChanges"))?,
        expected_benefits: list_of(fields, "expectedBenefits", 20, |item| text_of(item, 1_000))?,
        complexity: enum_or(fields.get("complexity"), SuggestionLevel::Medium)?,
        risk: enum_or(fields.get("risk"), SuggestionLevel::Medium)?,
        evidence: Some(list_of(fields, "evidence", 50, evidence_of)?),
        confidence: Some(confidence_of(fields.get("confidence"))?),
        source: Some(text_or(fields.get("source"), 500, DEFAULT_SOURCE)?),
        responsible: Some(text_or(
            fields.get("responsible"),
            500,
            DEFAULT_RESPONSIBLE,
        )?),
    })
}

/// A trimmed text from 1 to `max` units.
fn text_of(value: &Value, max: usize) -> Option<String> {
    let Value::String(raw) = value else {
        return None;
    };
    let trimmed = js_trim(raw);
    let units = utf16_len(trimmed);
    (1..=max).contains(&units).then(|| trimmed.to_string())
}

/// The title: at most 200 units before the sanitizer, and from 1 to 200 after it.
fn title_of(value: &Value) -> Option<String> {
    let Value::String(raw) = value else {
        return None;
    };
    if utf16_len(raw) > 200 {
        return None;
    }
    let sanitized = sanitize_suggestion_title(raw);
    let units = utf16_len(&sanitized);
    (1..=200).contains(&units).then_some(sanitized)
}

/// The proposed changes: the text as it is, at most 100,000 units, empty when absent.
fn proposed_changes_of(value: Option<&Value>) -> Option<String> {
    match value {
        None => Some(String::new()),
        Some(Value::String(text)) if utf16_len(text) <= 100_000 => Some(text.clone()),
        Some(_) => None,
    }
}

/// `z.array(item).max(max).default([])`: an absent list is empty, and a present one is a list of at most `max` items,
/// each of which must be valid.
fn list_of<T>(
    fields: &Map<String, Value>,
    key: &str,
    max: usize,
    item: impl Fn(&Value) -> Option<T>,
) -> Option<Vec<T>> {
    match fields.get(key) {
        None => Some(Vec::new()),
        Some(Value::Array(items)) if items.len() <= max => items.iter().map(item).collect(),
        Some(_) => None,
    }
}

/// An evidence item: `source` and `detail` are required, `location` is optional, and no other key is allowed.
fn evidence_of(value: &Value) -> Option<SuggestionEvidence> {
    let Value::Object(fields) = value else {
        return None;
    };
    let mut source = None;
    let mut detail = None;
    let mut location = None;
    for (key, field) in fields {
        match key.as_str() {
            "source" => source = Some(text_of(field, 500)?),
            "detail" => detail = Some(text_of(field, 4_000)?),
            "location" => location = Some(text_of(field, 1_000)?),
            _ => return None,
        }
    }
    Some(SuggestionEvidence {
        source: source?,
        detail: detail?,
        location,
    })
}

/// A required enumerated value.
fn enum_of<T: DeserializeOwned>(value: &Value) -> Option<T> {
    serde_json::from_value(value.clone()).ok()
}

/// An enumerated value with a default when the field is absent.
fn enum_or<T: DeserializeOwned>(value: Option<&Value>, default: T) -> Option<T> {
    match value {
        None => Some(default),
        Some(value) => enum_of(value),
    }
}

/// A trimmed text with a default when the field is absent.
fn text_or(value: Option<&Value>, max: usize, default: &str) -> Option<String> {
    match value {
        None => Some(default.to_string()),
        Some(value) => text_of(value, max),
    }
}

/// The confidence: a whole number from 0 to 100, `DEFAULT_CONFIDENCE` when absent.
fn confidence_of(value: Option<&Value>) -> Option<i64> {
    let Some(value) = value else {
        return Some(DEFAULT_CONFIDENCE);
    };
    let number = value.as_f64()?;
    (number.fract() == 0.0 && (0.0..=100.0).contains(&number)).then_some(number as i64)
}

/// `reviewComparisonMarkdown`: the comparison with the previous review, in the words the interface shows.
pub fn review_comparison_markdown(comparison: &ReviewComparison) -> String {
    let severity_changes = if comparison.severity_changes.is_empty() {
        "nenhuma".to_string()
    } else {
        comparison
            .severity_changes
            .iter()
            .map(|item| {
                format!(
                    "{}: {} → {}",
                    item.title,
                    severity_word(item.from),
                    severity_word(item.to)
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    };
    format!(
        "### Comparação com a revisão anterior\n\n- Novas: {}\n- Persistentes: {}\n- Resolvidas nesta revisão: {}\n- Mudanças de severidade: {}",
        comparison.new_suggestions.len(),
        comparison.persistent_suggestions.len(),
        comparison.resolved_suggestions.len(),
        severity_changes
    )
}

fn severity_word(severity: SuggestionSeverity) -> &'static str {
    match severity {
        SuggestionSeverity::Info => "info",
        SuggestionSeverity::Low => "low",
        SuggestionSeverity::Medium => "medium",
        SuggestionSeverity::High => "high",
        SuggestionSeverity::Critical => "critical",
    }
}
