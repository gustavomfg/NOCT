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

use crate::text::is_js_whitespace;

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
