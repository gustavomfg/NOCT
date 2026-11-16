//! Review suggestions and their decision history. Port of `SuggestionRepository.ts`.
//!
//! A review reconciles its structured suggestions with the open ones of a conversation. A
//! suggestion matches by identity, an open match keeps its record, and an open suggestion
//! that no longer appears is resolved. Every status change is kept in the decision history.

use std::collections::{HashMap, HashSet};

use noct_core::limits::collection_page::SUGGESTIONS as PAGE_LIMIT;
use noct_core::suggestions::{
    DEFAULT_CONFIDENCE, DEFAULT_RESPONSIBLE, DEFAULT_SOURCE, ReviewComparison,
    ReviewComparisonItem, ReviewSeverityChange, Suggestion, SuggestionCategory, SuggestionEvidence,
    SuggestionHistoryEntry, SuggestionInput, SuggestionLevel, SuggestionReconciliation,
    SuggestionSeverity, SuggestionStatus, suggestion_identity,
};
use noct_core::text::utf16_prefix;
use rusqlite::types::ToSql;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::clock::iso_now;
use crate::codec::{enum_text, enum_value};
use crate::repositories::Page;
use crate::{Error, Result, transaction};

const NOT_FOUND: &str = "Sugestão não encontrada.";
const INVALID_ROW: &str = "A sugestão persistida está inválida.";
/// The longest result a status change keeps, in UTF-16 units, as the TypeScript repository cuts it.
const RESULT_CHARACTERS: usize = 20_000;
const RESOLVED_REASON: &str = "A sugestão não reapareceu na revisão estruturada atual.";

const SELECT: &str = "SELECT id, workspace_id, conversation_id, title, description, reasoning, \
    category, severity, affected_files, proposed_changes, expected_benefits, complexity, risk, \
    evidence, confidence, source, responsible, status, created_at, updated_at FROM suggestions";

/// The statuses a conversation's list shows. They are the active statuses.
const OPEN_STATUSES: &str = "('new','in-analysis','accepted','deferred')";

pub struct Suggestions<'a> {
    conn: &'a Connection,
}

impl<'a> Suggestions<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Open suggestions of a conversation, most recently updated first.
    pub fn list(&self, conversation_id: &str) -> Result<Vec<Suggestion>> {
        let rows = self.read(
            &format!(
                "{SELECT} WHERE conversation_id=?1 AND status IN {OPEN_STATUSES} \
                 ORDER BY updated_at DESC"
            ),
            params![conversation_id],
        )?;
        self.decode(rows)
    }

    /// One page of open suggestions. `has_more` is set when another suggestion follows it.
    pub fn page(
        &self,
        conversation_id: &str,
        offset: i64,
        limit: Option<i64>,
    ) -> Result<Page<Suggestion>> {
        let limit = limit.unwrap_or(PAGE_LIMIT as i64);
        let rows = self.read(
            &format!(
                "{SELECT} WHERE conversation_id=?1 AND status IN {OPEN_STATUSES} \
                 ORDER BY updated_at DESC LIMIT ?2 OFFSET ?3"
            ),
            params![conversation_id, limit + 1, offset],
        )?;
        let has_more = rows.len() as i64 > limit;
        let visible = usize::try_from(limit.max(0)).unwrap_or(0);
        let items = self.decode(rows.into_iter().take(visible).collect())?;
        Ok(Page { items, has_more })
    }

    /// A suggestion by identifier, optionally only within its conversation.
    pub fn get(&self, id: &str, conversation_id: Option<&str>) -> Result<Option<Suggestion>> {
        let rows = match conversation_id.filter(|value| !value.is_empty()) {
            Some(conversation_id) => self.read(
                &format!("{SELECT} WHERE id=?1 AND conversation_id=?2"),
                params![id, conversation_id],
            )?,
            None => self.read(&format!("{SELECT} WHERE id=?1"), params![id])?,
        };
        Ok(self.decode(rows)?.into_iter().next())
    }

    /// Stores a suggestion as new, with its first history entry, and returns it.
    pub fn add(
        &self,
        conversation_id: &str,
        workspace_id: &str,
        value: &SuggestionInput,
    ) -> Result<Suggestion> {
        let now = iso_now()?;
        let decision_id = Uuid::new_v4().to_string();
        let suggestion = Suggestion {
            id: Uuid::new_v4().to_string(),
            workspace_id: workspace_id.to_string(),
            conversation_id: conversation_id.to_string(),
            title: value.title.clone(),
            description: value.description.clone(),
            reasoning: value.reasoning.clone(),
            category: value.category,
            severity: value.severity,
            affected_files: value.affected_files.clone(),
            proposed_changes: value.proposed_changes.clone(),
            expected_benefits: value.expected_benefits.clone(),
            complexity: value.complexity,
            risk: value.risk,
            evidence: value.evidence.clone().unwrap_or_default(),
            confidence: value.confidence.unwrap_or(DEFAULT_CONFIDENCE),
            source: value
                .source
                .clone()
                .unwrap_or_else(|| DEFAULT_SOURCE.to_string()),
            responsible: value
                .responsible
                .clone()
                .unwrap_or_else(|| DEFAULT_RESPONSIBLE.to_string()),
            created_at: now.clone(),
            updated_at: now.clone(),
            status: SuggestionStatus::New,
            history: vec![SuggestionHistoryEntry {
                id: decision_id.clone(),
                status: SuggestionStatus::New,
                result: None,
                created_at: now.clone(),
            }],
        };
        transaction::run(self.conn, |conn| {
            insert(conn, &suggestion)?;
            conn.execute(
                "INSERT INTO suggestion_decisions(id,suggestion_id,status,result,created_at) \
                 VALUES(?1,?2,?3,?4,?5)",
                params![
                    decision_id,
                    suggestion.id,
                    SuggestionStatus::New.as_str(),
                    Option::<String>::None,
                    now
                ],
            )?;
            Ok(())
        })?;
        Ok(suggestion)
    }

    /// Reconciles the suggestions of a review with the conversation's history, as one unit.
    pub fn reconcile(
        &self,
        conversation_id: &str,
        workspace_id: &str,
        values: &[SuggestionInput],
        resolve_missing: bool,
    ) -> Result<SuggestionReconciliation> {
        transaction::run(self.conn, |_| {
            // The first suggestion of each identity wins, except that an active one replaces
            // an inactive one, and an accepted one replaces a new one. The history is read
            // newest first, as the repository returns it.
            let history = self.list_history(conversation_id)?;
            let mut by_identity: HashMap<String, Suggestion> = HashMap::new();
            let mut active_before: Vec<(String, Suggestion)> = Vec::new();
            let mut active_seen: HashSet<String> = HashSet::new();
            for suggestion in history {
                let identity = suggestion_identity(suggestion.category, &suggestion.title);
                let suggestion_is_active = suggestion.status.is_active();
                let replace = match by_identity.get(&identity) {
                    None => true,
                    Some(existing) => {
                        (!existing.status.is_active() && suggestion_is_active)
                            || (existing.status == SuggestionStatus::New
                                && suggestion.status == SuggestionStatus::Accepted)
                    }
                };
                if suggestion_is_active && !active_seen.contains(&identity) {
                    active_seen.insert(identity.clone());
                    active_before.push((identity.clone(), suggestion.clone()));
                }
                if replace {
                    by_identity.insert(identity, suggestion);
                }
            }

            let mut seen: HashSet<String> = HashSet::new();
            let mut new_suggestions = Vec::new();
            let mut persistent_suggestions = Vec::new();
            let mut severity_changes = Vec::new();
            let mut suggestions = Vec::with_capacity(values.len());
            for value in values {
                let identity = suggestion_identity(value.category, &value.title);
                seen.insert(identity.clone());
                let Some(existing) = by_identity.get(&identity).cloned() else {
                    let created = self.add(conversation_id, workspace_id, value)?;
                    by_identity.insert(identity, created.clone());
                    new_suggestions.push(comparison_item(&created));
                    suggestions.push(created);
                    continue;
                };
                if !existing.status.is_active() {
                    suggestions.push(existing);
                    continue;
                }
                persistent_suggestions.push(comparison_item(&existing));
                if existing.severity != value.severity {
                    severity_changes.push(ReviewSeverityChange {
                        id: existing.id.clone(),
                        title: existing.title.clone(),
                        from: existing.severity,
                        to: value.severity,
                    });
                }
                if !existing.status.is_open_for_update() {
                    suggestions.push(existing);
                    continue;
                }
                self.refresh_from_review(&existing.id, conversation_id, value)?;
                let updated = self
                    .get(&existing.id, Some(conversation_id))?
                    .ok_or_else(not_found)?;
                by_identity.insert(identity, updated.clone());
                suggestions.push(updated);
            }

            let mut resolved_suggestions = Vec::new();
            for (identity, suggestion) in &active_before {
                if !resolve_missing {
                    break;
                }
                if seen.contains(identity) || !suggestion.status.is_resolvable() {
                    continue;
                }
                let resolved = self.set_status(
                    &suggestion.id,
                    SuggestionStatus::Resolved,
                    Some(RESOLVED_REASON),
                )?;
                resolved_suggestions.push(comparison_item(&resolved));
            }

            Ok(SuggestionReconciliation {
                suggestions,
                comparison: ReviewComparison {
                    reviewed_at: iso_now()?,
                    new_suggestions,
                    persistent_suggestions,
                    resolved_suggestions,
                    severity_changes,
                },
            })
        })
    }

    /// Moves a suggestion to a new status when the transition is allowed, and records it.
    /// Setting the status it already has changes nothing.
    pub fn set_status(
        &self,
        id: &str,
        status: SuggestionStatus,
        result: Option<&str>,
    ) -> Result<Suggestion> {
        transaction::run(self.conn, |_| {
            let updated_at = iso_now()?;
            let stored: Option<String> = self
                .conn
                .query_row("SELECT status FROM suggestions WHERE id=?1", [id], |row| {
                    row.get(0)
                })
                .optional()?;
            let current: SuggestionStatus =
                enum_value(&stored.ok_or_else(not_found)?).ok_or_else(invalid)?;
            if current == status {
                return self.get(id, None)?.ok_or_else(not_found);
            }
            if !allowed_transition(current, status) {
                return Err(Error::Message(format!(
                    "Transição de sugestão inválida: {} → {}.",
                    current.as_str(),
                    status.as_str()
                )));
            }
            let kept = result.map(|text| utf16_prefix(text, RESULT_CHARACTERS));
            let changed = self.conn.execute(
                "UPDATE suggestions SET status=?1,result=?2,updated_at=?3 WHERE id=?4",
                params![status.as_str(), kept, updated_at, id],
            )?;
            if changed == 0 {
                return Err(not_found());
            }
            self.conn.execute(
                "INSERT INTO suggestion_decisions(id,suggestion_id,status,result,created_at) \
                 VALUES(?1,?2,?3,?4,?5)",
                params![
                    Uuid::new_v4().to_string(),
                    id,
                    status.as_str(),
                    kept,
                    updated_at
                ],
            )?;
            let conversation_id: String = self.conn.query_row(
                "SELECT conversation_id FROM suggestions WHERE id=?1",
                [id],
                |row| row.get(0),
            )?;
            self.get(id, Some(&conversation_id))?.ok_or_else(not_found)
        })
    }

    /// Refreshes an open suggestion from the review that found it again.
    fn refresh_from_review(
        &self,
        id: &str,
        conversation_id: &str,
        value: &SuggestionInput,
    ) -> Result<()> {
        let updated_at = iso_now()?;
        let affected_files = json_text(&value.affected_files)?;
        let expected_benefits = json_text(&value.expected_benefits)?;
        let evidence = json_text(&value.evidence.clone().unwrap_or_default())?;
        let confidence = value.confidence.unwrap_or(DEFAULT_CONFIDENCE);
        let source = value
            .source
            .clone()
            .unwrap_or_else(|| DEFAULT_SOURCE.to_string());
        let responsible = value
            .responsible
            .clone()
            .unwrap_or_else(|| DEFAULT_RESPONSIBLE.to_string());
        self.conn.execute(
            "UPDATE suggestions SET title=?1,description=?2,reasoning=?3,category=?4,severity=?5,\
             affected_files=?6,proposed_changes=?7,expected_benefits=?8,complexity=?9,risk=?10,\
             evidence=?11,confidence=?12,source=?13,responsible=?14,updated_at=?15 \
             WHERE id=?16 AND conversation_id=?17 AND status IN ('new','in-analysis','deferred')",
            params![
                value.title,
                value.description,
                value.reasoning,
                enum_text(&value.category)?,
                enum_text(&value.severity)?,
                affected_files,
                value.proposed_changes,
                expected_benefits,
                enum_text(&value.complexity)?,
                enum_text(&value.risk)?,
                evidence,
                confidence,
                source,
                responsible,
                updated_at,
                id,
                conversation_id,
            ],
        )?;
        Ok(())
    }

    /// Every suggestion of a conversation, including closed ones, newest first.
    fn list_history(&self, conversation_id: &str) -> Result<Vec<Suggestion>> {
        let rows = self.read(
            &format!("{SELECT} WHERE conversation_id=?1 ORDER BY updated_at DESC"),
            params![conversation_id],
        )?;
        self.decode(rows)
    }

    fn read(&self, sql: &str, params: &[&dyn ToSql]) -> Result<Vec<Stored>> {
        let mut statement = self.conn.prepare(sql)?;
        let rows = statement
            .query_map(params, stored_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Attaches each suggestion's decision history. A suggestion stored before decisions were
    /// recorded gets one legacy entry, as the TypeScript repository gives it.
    fn decode(&self, rows: Vec<Stored>) -> Result<Vec<Suggestion>> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let mut suggestions = rows
            .into_iter()
            .map(decode_row)
            .collect::<Result<Vec<_>>>()?;
        let ids: Vec<String> = suggestions.iter().map(|item| item.id.clone()).collect();
        let placeholders = (1..=ids.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let mut statement = self.conn.prepare(&format!(
            "SELECT id, suggestion_id, status, result, created_at FROM suggestion_decisions \
             WHERE suggestion_id IN ({placeholders}) ORDER BY created_at, rowid"
        ))?;
        let bound: Vec<&dyn ToSql> = ids.iter().map(|id| id as &dyn ToSql).collect();
        let decisions = statement
            .query_map(bound.as_slice(), |row| {
                Ok(Decision {
                    id: row.get(0)?,
                    suggestion_id: row.get(1)?,
                    status: row.get(2)?,
                    result: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut history: HashMap<String, Vec<SuggestionHistoryEntry>> = HashMap::new();
        for decision in decisions {
            let status = enum_value(&decision.status).ok_or_else(invalid)?;
            history
                .entry(decision.suggestion_id)
                .or_default()
                .push(SuggestionHistoryEntry {
                    id: decision.id,
                    status,
                    result: decision.result,
                    created_at: decision.created_at,
                });
        }
        for suggestion in &mut suggestions {
            suggestion.history = history.remove(&suggestion.id).unwrap_or_else(|| {
                vec![SuggestionHistoryEntry {
                    id: format!("legacy-{}", suggestion.id),
                    status: SuggestionStatus::New,
                    result: None,
                    created_at: suggestion.created_at.clone(),
                }]
            });
        }
        Ok(suggestions)
    }
}

/// The transitions a suggestion may make. A rejected, resolved or invalid suggestion is final.
fn allowed_transition(from: SuggestionStatus, to: SuggestionStatus) -> bool {
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

struct Stored {
    id: String,
    workspace_id: String,
    conversation_id: String,
    title: String,
    description: String,
    reasoning: String,
    category: String,
    severity: String,
    affected_files: String,
    proposed_changes: String,
    expected_benefits: String,
    complexity: String,
    risk: String,
    evidence: String,
    confidence: i64,
    source: String,
    responsible: String,
    status: String,
    created_at: String,
    updated_at: String,
}

fn stored_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Stored> {
    Ok(Stored {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        conversation_id: row.get(2)?,
        title: row.get(3)?,
        description: row.get(4)?,
        reasoning: row.get(5)?,
        category: row.get(6)?,
        severity: row.get(7)?,
        affected_files: row.get(8)?,
        proposed_changes: row.get(9)?,
        expected_benefits: row.get(10)?,
        complexity: row.get(11)?,
        risk: row.get(12)?,
        evidence: row.get(13)?,
        confidence: row.get(14)?,
        source: row.get(15)?,
        responsible: row.get(16)?,
        status: row.get(17)?,
        created_at: row.get(18)?,
        updated_at: row.get(19)?,
    })
}

struct Decision {
    id: String,
    suggestion_id: String,
    status: String,
    result: Option<String>,
    created_at: String,
}

fn decode_row(row: Stored) -> Result<Suggestion> {
    Ok(Suggestion {
        id: row.id,
        workspace_id: row.workspace_id,
        conversation_id: row.conversation_id,
        title: row.title,
        description: row.description,
        reasoning: row.reasoning,
        category: enum_value::<SuggestionCategory>(&row.category).ok_or_else(invalid)?,
        severity: enum_value::<SuggestionSeverity>(&row.severity).ok_or_else(invalid)?,
        affected_files: json_value::<Vec<String>>(&row.affected_files)?,
        proposed_changes: row.proposed_changes,
        expected_benefits: json_value::<Vec<String>>(&row.expected_benefits)?,
        complexity: enum_value::<SuggestionLevel>(&row.complexity).ok_or_else(invalid)?,
        risk: enum_value::<SuggestionLevel>(&row.risk).ok_or_else(invalid)?,
        evidence: json_value::<Vec<SuggestionEvidence>>(&row.evidence)?,
        confidence: row.confidence,
        source: row.source,
        responsible: row.responsible,
        created_at: row.created_at,
        updated_at: row.updated_at,
        status: enum_value::<SuggestionStatus>(&row.status).ok_or_else(invalid)?,
        history: Vec::new(),
    })
}

fn insert(conn: &Connection, suggestion: &Suggestion) -> Result<()> {
    let category = enum_text(&suggestion.category)?;
    let severity = enum_text(&suggestion.severity)?;
    let affected_files = json_text(&suggestion.affected_files)?;
    let expected_benefits = json_text(&suggestion.expected_benefits)?;
    let complexity = enum_text(&suggestion.complexity)?;
    let risk = enum_text(&suggestion.risk)?;
    let evidence = json_text(&suggestion.evidence)?;
    conn.execute(
        "INSERT INTO suggestions(id,workspace_id,conversation_id,title,description,reasoning,\
         category,severity,affected_files,proposed_changes,expected_benefits,complexity,risk,\
         evidence,confidence,source,responsible,status,created_at,updated_at) \
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
        params![
            suggestion.id,
            suggestion.workspace_id,
            suggestion.conversation_id,
            suggestion.title,
            suggestion.description,
            suggestion.reasoning,
            category,
            severity,
            affected_files,
            suggestion.proposed_changes,
            expected_benefits,
            complexity,
            risk,
            evidence,
            suggestion.confidence,
            suggestion.source,
            suggestion.responsible,
            suggestion.status.as_str(),
            suggestion.created_at,
            suggestion.updated_at,
        ],
    )?;
    Ok(())
}

fn comparison_item(suggestion: &Suggestion) -> ReviewComparisonItem {
    ReviewComparisonItem {
        id: suggestion.id.clone(),
        title: suggestion.title.clone(),
        severity: suggestion.severity,
    }
}

fn json_text<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|_| invalid())
}

fn json_value<T: DeserializeOwned>(text: &str) -> Result<T> {
    serde_json::from_str(text).map_err(|_| invalid())
}

fn not_found() -> Error {
    Error::Message(NOT_FOUND.to_string())
}

fn invalid() -> Error {
    Error::Message(INVALID_ROW.to_string())
}
