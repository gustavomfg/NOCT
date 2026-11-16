//! Brain memories and their audit history. Port of `electron/database/BrainMemoryRepository.ts`.
//!
//! A memory belongs to a workspace, or to one conversation of that workspace. Its status moves
//! along a fixed lifecycle, and every change is kept in the history. Retrieval reads only active
//! memories. An equivalent memory is one that is still a candidate or active.

use std::collections::HashSet;

use noct_core::brain_memory::{
    BrainMemory, BrainMemoryCandidate, BrainMemoryHistoryAction, BrainMemoryHistoryEntry,
    BrainMemoryScope, BrainMemorySource, BrainMemoryStatus, CreateBrainMemoryInput,
    InitialBrainMemoryStatus, UpdateBrainMemoryInput, fts_query,
};
use noct_core::limits::collection_page::BRAIN_MEMORIES as PAGE_LIMIT;
use noct_core::text::js_trim;
use rusqlite::types::Value;
use rusqlite::{Connection, Row, params, params_from_iter};
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::clock::iso_now;
use crate::codec::{enum_text, enum_value};
use crate::repositories::Page;
use crate::repositories::conversations::Conversations;
use crate::{Error, Result, transaction};

const NOT_FOUND: &str = "Memória não encontrada.";
const INVALID_ROW: &str = "A memória persistida está inválida.";
const WORKSPACE_WITH_CONVERSATION: &str = "Memória do workspace não pode pertencer a uma conversa.";
const CONVERSATION_REQUIRED: &str = "Memória da conversa exige uma conversa válida.";
const OTHER_WORKSPACE: &str = "A conversa da memória não pertence ao workspace atual.";
const EDITED: &str = "Conteúdo ou classificação da memória atualizado.";
const DEFAULT_CONFIDENCE: i64 = 70;
const HISTORY_LIMIT: i64 = 500;
const USED_LIMIT: usize = 50;
const RETRIEVE_LIMIT: usize = 8;

/// The columns of a memory, read through the `memory` alias every query declares.
const COLUMNS: &str = "memory.id, memory.workspace_id, memory.conversation_id, memory.kind, \
    memory.scope, memory.status, memory.content, memory.confidence, memory.source_type, \
    memory.source_id, memory.created_at, memory.updated_at, memory.last_confirmed_at, \
    memory.last_used_at, memory.use_count";

pub struct BrainMemories<'a> {
    conn: &'a Connection,
}

impl<'a> BrainMemories<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Stores a memory in a workspace. A workspace memory keeps no conversation, and a
    /// conversation memory must name a conversation of that workspace.
    pub fn create(&self, workspace: &str, input: &CreateBrainMemoryInput) -> Result<BrainMemory> {
        let conversation_id = match input.scope {
            BrainMemoryScope::Conversation => input.conversation_id.clone(),
            BrainMemoryScope::Workspace => None,
        };
        self.assert_scope(workspace, input.scope, conversation_id.as_deref())?;
        let now = iso_now()?;
        let status = input
            .status
            .map_or(BrainMemoryStatus::Candidate, BrainMemoryStatus::from);
        let source_type = input.source_type.unwrap_or(BrainMemorySource::Manual);
        let row = BrainMemory {
            id: Uuid::new_v4().to_string(),
            workspace_id: workspace.to_string(),
            conversation_id,
            kind: input.kind,
            scope: input.scope,
            status,
            content: js_trim(&input.content).to_string(),
            confidence: input.confidence.unwrap_or(DEFAULT_CONFIDENCE),
            source_type,
            source_id: input.source_id.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
            last_confirmed_at: (status == BrainMemoryStatus::Active).then(|| now.clone()),
            last_used_at: None,
            use_count: 0,
        };
        transaction::run(self.conn, |conn| {
            conn.execute(
                "INSERT INTO brain_memories(id,workspace_id,conversation_id,kind,scope,status,\
                 content,confidence,source_type,source_id,created_at,updated_at,\
                 last_confirmed_at,last_used_at,use_count) \
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
                params![
                    row.id,
                    row.workspace_id,
                    row.conversation_id,
                    enum_text(&row.kind)?,
                    enum_text(&row.scope)?,
                    enum_text(&row.status)?,
                    row.content,
                    row.confidence,
                    enum_text(&row.source_type)?,
                    row.source_id,
                    row.created_at,
                    row.updated_at,
                    row.last_confirmed_at,
                    row.last_used_at,
                    row.use_count,
                ],
            )?;
            add_history(
                conn,
                &row.id,
                BrainMemoryHistoryAction::Created,
                None,
                row.status,
                created_summary(row.source_type),
                &now,
            )
        })?;
        Ok(row)
    }

    /// Changes a memory. A status change must follow the lifecycle, and each change that is
    /// not a status change is recorded as an edit.
    pub fn update(
        &self,
        id: &str,
        workspace: &str,
        input: &UpdateBrainMemoryInput,
    ) -> Result<BrainMemory> {
        let current = self.get(id, workspace)?.ok_or_else(|| fail(NOT_FOUND))?;
        let scope = input.scope.unwrap_or(current.scope);
        let conversation_id = match scope {
            BrainMemoryScope::Conversation => match &input.conversation_id {
                None => current.conversation_id.clone(),
                Some(value) => value.clone(),
            },
            BrainMemoryScope::Workspace => None,
        };
        self.assert_scope(workspace, scope, conversation_id.as_deref())?;

        let status = input.status.unwrap_or(current.status);
        if status != current.status && !allowed_transition(current.status, status) {
            return Err(fail(&format!(
                "Transição de memória inválida: {} → {}.",
                enum_text(&current.status)?,
                enum_text(&status)?,
            )));
        }

        let updated_at = iso_now()?;
        let kind = input.kind.unwrap_or(current.kind);
        let content = input.content.as_deref().map_or_else(
            || current.content.clone(),
            |value| js_trim(value).to_string(),
        );
        let confidence = input.confidence.unwrap_or(current.confidence);
        let last_confirmed_at =
            if status == BrainMemoryStatus::Active && current.status != BrainMemoryStatus::Active {
                Some(updated_at.clone())
            } else {
                current.last_confirmed_at.clone()
            };

        transaction::run(self.conn, |conn| {
            conn.execute(
                "UPDATE brain_memories SET conversation_id=?1,kind=?2,scope=?3,status=?4,\
                 content=?5,confidence=?6,updated_at=?7,last_confirmed_at=?8 \
                 WHERE id=?9 AND workspace_id=?10",
                params![
                    conversation_id,
                    enum_text(&kind)?,
                    enum_text(&scope)?,
                    enum_text(&status)?,
                    content,
                    confidence,
                    updated_at,
                    last_confirmed_at,
                    id,
                    workspace,
                ],
            )?;
            let edited = content != current.content
                || kind != current.kind
                || scope != current.scope
                || confidence != current.confidence;
            if edited {
                add_history(
                    conn,
                    id,
                    BrainMemoryHistoryAction::Edited,
                    Some(current.status),
                    status,
                    EDITED,
                    &updated_at,
                )?;
            }
            if status != current.status {
                let action = status_action(current.status, status);
                add_history(
                    conn,
                    id,
                    action,
                    Some(current.status),
                    status,
                    action_summary(action),
                    &updated_at,
                )?;
            }
            Ok(())
        })?;
        self.get(id, workspace)?.ok_or_else(|| fail(NOT_FOUND))
    }

    /// The history of a memory, newest first.
    pub fn list_history(&self, id: &str, workspace: &str) -> Result<Vec<BrainMemoryHistoryEntry>> {
        if self.get(id, workspace)?.is_none() {
            return Err(fail(NOT_FOUND));
        }
        let mut statement = self.conn.prepare(
            "SELECT id, memory_id, action, from_status, to_status, summary, created_at \
             FROM brain_memory_history WHERE memory_id=?1 \
             ORDER BY created_at DESC, rowid DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![id, HISTORY_LIMIT], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?;
        rows.map(|row| {
            let (id, memory_id, action, from_status, to_status, summary, created_at) = row?;
            Ok(BrainMemoryHistoryEntry {
                id,
                memory_id,
                action: stored_enum(&action)?,
                from_status: from_status
                    .as_deref()
                    .map(stored_enum::<BrainMemoryStatus>)
                    .transpose()?,
                to_status: stored_enum(&to_status)?,
                summary,
                created_at,
            })
        })
        .collect()
    }

    /// Removes a memory and, through the foreign key, its history. Reports whether one was removed.
    pub fn delete(&self, id: &str, workspace: &str) -> Result<bool> {
        let changed = self.conn.execute(
            "DELETE FROM brain_memories WHERE id=?1 AND workspace_id=?2",
            params![id, workspace],
        )?;
        Ok(changed > 0)
    }

    pub fn get(&self, id: &str, workspace: &str) -> Result<Option<BrainMemory>> {
        let rows = read(
            self.conn,
            &format!(
                "SELECT {COLUMNS} FROM brain_memories memory \
                 WHERE memory.id=?1 AND memory.workspace_id=?2"
            ),
            params![id, workspace],
        )?;
        Ok(rows.into_iter().next())
    }

    /// The memory that already holds this content, compared without case, in the same scope.
    /// Only candidates and active memories count.
    pub fn find_equivalent(
        &self,
        workspace: &str,
        scope: BrainMemoryScope,
        conversation: Option<&str>,
        content: &str,
    ) -> Result<Option<BrainMemory>> {
        let rows = read(
            self.conn,
            &format!(
                "SELECT {COLUMNS} FROM brain_memories memory \
                 WHERE memory.workspace_id=?1 AND memory.scope=?2 \
                   AND memory.conversation_id IS ?3 AND memory.content=?4 COLLATE NOCASE \
                   AND memory.status IN ('candidate','active') LIMIT 1"
            ),
            params![
                workspace,
                enum_text(&scope)?,
                conversation,
                js_trim(content),
            ],
        )?;
        Ok(rows.into_iter().next())
    }

    /// Stores the candidates an agent proposed, as one unit. A candidate equivalent to a
    /// stored memory is skipped, and so is one equivalent to an earlier candidate of the batch.
    pub fn create_candidates(
        &self,
        workspace: &str,
        current_conversation: &str,
        candidates: &[BrainMemoryCandidate],
    ) -> Result<Vec<BrainMemory>> {
        transaction::run(self.conn, |_| {
            let mut created = Vec::new();
            for candidate in candidates {
                let conversation = (candidate.scope == BrainMemoryScope::Conversation)
                    .then_some(current_conversation);
                if self
                    .find_equivalent(workspace, candidate.scope, conversation, &candidate.content)?
                    .is_some()
                {
                    continue;
                }
                created.push(self.create(
                    workspace,
                    &CreateBrainMemoryInput {
                        conversation_id: conversation.map(str::to_string),
                        kind: candidate.kind,
                        scope: candidate.scope,
                        content: candidate.content.clone(),
                        confidence: Some(candidate.confidence),
                        source_type: Some(BrainMemorySource::Agent),
                        source_id: None,
                        status: Some(InitialBrainMemoryStatus::Candidate),
                    },
                )?);
            }
            Ok(created)
        })
    }

    /// A page of a workspace's memories. A query orders them by relevance, and otherwise by
    /// the last update. One extra row tells whether a further page exists.
    pub fn list_page(
        &self,
        workspace: &str,
        offset: usize,
        limit: Option<usize>,
        query: &str,
        status: Option<BrainMemoryStatus>,
    ) -> Result<Page<BrainMemory>> {
        let limit = limit.unwrap_or(PAGE_LIMIT as usize);
        let search = fts_query(query);
        let mut values: Vec<Value> = vec![Value::from(workspace.to_string())];
        let mut filters = vec!["memory.workspace_id=?1".to_string()];
        if let Some(status) = status {
            values.push(Value::from(enum_text(&status)?));
            filters.push(format!("memory.status=?{}", values.len()));
        }
        let (source, order) = if search.is_empty() {
            (
                "brain_memories memory".to_string(),
                "memory.updated_at DESC".to_string(),
            )
        } else {
            values.push(Value::from(search));
            filters.push(format!("brain_memories_fts MATCH ?{}", values.len()));
            (
                "brain_memories memory JOIN brain_memories_fts \
                 ON brain_memories_fts.rowid=memory.rowid"
                    .to_string(),
                "bm25(brain_memories_fts), memory.updated_at DESC".to_string(),
            )
        };
        values.push(Value::Integer((limit + 1) as i64));
        let fetch = values.len();
        values.push(Value::Integer(offset as i64));
        let skip = values.len();
        let sql = format!(
            "SELECT {COLUMNS} FROM {source} WHERE {} ORDER BY {order} LIMIT ?{fetch} OFFSET ?{skip}",
            filters.join(" AND ")
        );
        let rows = read(self.conn, &sql, params_from_iter(values))?;
        let has_more = rows.len() > limit;
        Ok(Page {
            items: rows.into_iter().take(limit).collect(),
            has_more,
        })
    }

    /// The active memories that a conversation may use for a query: the workspace memories and
    /// the ones of that conversation. Without a usable query, nothing is retrieved.
    pub fn retrieve(
        &self,
        workspace: &str,
        conversation: &str,
        query: &str,
        limit: Option<usize>,
    ) -> Result<Vec<BrainMemory>> {
        let search = fts_query(query);
        if search.is_empty() {
            return Ok(Vec::new());
        }
        read(
            self.conn,
            &format!(
                "SELECT {COLUMNS} FROM brain_memories memory \
                 JOIN brain_memories_fts ON brain_memories_fts.rowid=memory.rowid \
                 WHERE memory.workspace_id=?1 AND memory.status='active' \
                   AND (memory.scope='workspace' OR memory.conversation_id=?2) \
                   AND brain_memories_fts MATCH ?3 \
                 ORDER BY bm25(brain_memories_fts), memory.confidence DESC, \
                   memory.updated_at DESC LIMIT ?4"
            ),
            params![
                workspace,
                conversation,
                search,
                limit.unwrap_or(RETRIEVE_LIMIT) as i64,
            ],
        )
    }

    /// Records one use for each distinct memory, up to the first fifty.
    pub fn mark_used(&self, ids: &[String]) -> Result<()> {
        let mut seen = HashSet::new();
        let unique: Vec<&String> = ids
            .iter()
            .filter(|id| seen.insert(id.as_str()))
            .take(USED_LIMIT)
            .collect();
        if unique.is_empty() {
            return Ok(());
        }
        let mut values = vec![Value::from(iso_now()?)];
        values.extend(unique.iter().map(|id| Value::from(id.to_string())));
        let placeholders = vec!["?"; unique.len()].join(",");
        self.conn.execute(
            &format!(
                "UPDATE brain_memories SET last_used_at=?, use_count=use_count+1 \
                 WHERE id IN ({placeholders})"
            ),
            params_from_iter(values),
        )?;
        Ok(())
    }

    fn assert_scope(
        &self,
        workspace: &str,
        scope: BrainMemoryScope,
        conversation: Option<&str>,
    ) -> Result<()> {
        // The TypeScript check is a truthiness test, so an empty identifier counts as none.
        let conversation = conversation.filter(|id| !id.is_empty());
        match scope {
            BrainMemoryScope::Workspace if conversation.is_some() => {
                Err(fail(WORKSPACE_WITH_CONVERSATION))
            }
            BrainMemoryScope::Workspace => Ok(()),
            BrainMemoryScope::Conversation => {
                let id = conversation.ok_or_else(|| fail(CONVERSATION_REQUIRED))?;
                match Conversations::new(self.conn).get(id)? {
                    Some(row) if row.workspace == workspace => Ok(()),
                    _ => Err(fail(OTHER_WORKSPACE)),
                }
            }
        }
    }
}

/// The lifecycle. Candidates are approved or disapproved, active memories become outdated or
/// are archived, outdated memories return to use or are archived, and archived ones only return.
fn allowed_transition(from: BrainMemoryStatus, to: BrainMemoryStatus) -> bool {
    use BrainMemoryStatus::{Active, Archived, Candidate, Outdated};
    matches!(
        (from, to),
        (Candidate, Active | Archived)
            | (Active, Outdated | Archived)
            | (Outdated, Active | Archived)
            | (Archived, Active)
    )
}

fn status_action(from: BrainMemoryStatus, to: BrainMemoryStatus) -> BrainMemoryHistoryAction {
    if from == BrainMemoryStatus::Candidate && to == BrainMemoryStatus::Active {
        BrainMemoryHistoryAction::Approved
    } else if from == BrainMemoryStatus::Candidate && to == BrainMemoryStatus::Archived {
        BrainMemoryHistoryAction::Disapproved
    } else if to == BrainMemoryStatus::Archived {
        BrainMemoryHistoryAction::Archived
    } else if to == BrainMemoryStatus::Outdated {
        BrainMemoryHistoryAction::MarkedOutdated
    } else {
        BrainMemoryHistoryAction::Restored
    }
}

fn action_summary(action: BrainMemoryHistoryAction) -> &'static str {
    match action {
        BrainMemoryHistoryAction::Created => "Memória criada.",
        BrainMemoryHistoryAction::Edited => "Memória editada.",
        BrainMemoryHistoryAction::Approved => "Memória aprovada para uso pelo agente.",
        BrainMemoryHistoryAction::Disapproved => "Candidata desaprovada.",
        BrainMemoryHistoryAction::MarkedOutdated => "Memória marcada como desatualizada.",
        BrainMemoryHistoryAction::Archived => "Memória arquivada.",
        BrainMemoryHistoryAction::Restored => "Memória restaurada e aprovada novamente.",
    }
}

fn created_summary(source: BrainMemorySource) -> &'static str {
    match source {
        BrainMemorySource::Agent => "Candidata proposta pelo agente.",
        BrainMemorySource::Message => "Memória criada a partir de uma mensagem.",
        BrainMemorySource::Manual => "Memória criada manualmente.",
    }
}

fn add_history(
    conn: &Connection,
    memory: &str,
    action: BrainMemoryHistoryAction,
    from: Option<BrainMemoryStatus>,
    to: BrainMemoryStatus,
    summary: &str,
    created_at: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO brain_memory_history(id,memory_id,action,from_status,to_status,summary,\
         created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            Uuid::new_v4().to_string(),
            memory,
            enum_text(&action)?,
            from.map(|status| enum_text(&status)).transpose()?,
            enum_text(&to)?,
            summary,
            created_at,
        ],
    )?;
    Ok(())
}

/// A memory as the database holds it. Enum columns are checked when the row is decoded.
struct Stored {
    id: String,
    workspace_id: String,
    conversation_id: Option<String>,
    kind: String,
    scope: String,
    status: String,
    content: String,
    confidence: i64,
    source_type: String,
    source_id: Option<String>,
    created_at: String,
    updated_at: String,
    last_confirmed_at: Option<String>,
    last_used_at: Option<String>,
    use_count: i64,
}

fn stored(row: &Row<'_>) -> rusqlite::Result<Stored> {
    Ok(Stored {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        conversation_id: row.get(2)?,
        kind: row.get(3)?,
        scope: row.get(4)?,
        status: row.get(5)?,
        content: row.get(6)?,
        confidence: row.get(7)?,
        source_type: row.get(8)?,
        source_id: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        last_confirmed_at: row.get(12)?,
        last_used_at: row.get(13)?,
        use_count: row.get(14)?,
    })
}

fn decode(row: Stored) -> Result<BrainMemory> {
    Ok(BrainMemory {
        id: row.id,
        workspace_id: row.workspace_id,
        conversation_id: row.conversation_id,
        kind: stored_enum(&row.kind)?,
        scope: stored_enum(&row.scope)?,
        status: stored_enum(&row.status)?,
        content: row.content,
        confidence: row.confidence,
        source_type: stored_enum(&row.source_type)?,
        source_id: row.source_id,
        created_at: row.created_at,
        updated_at: row.updated_at,
        last_confirmed_at: row.last_confirmed_at,
        last_used_at: row.last_used_at,
        use_count: row.use_count,
    })
}

fn read(conn: &Connection, sql: &str, params: impl rusqlite::Params) -> Result<Vec<BrainMemory>> {
    let mut statement = conn.prepare(sql)?;
    let rows = statement.query_map(params, stored)?;
    rows.map(|row| decode(row?)).collect()
}

fn stored_enum<T: DeserializeOwned>(text: &str) -> Result<T> {
    enum_value(text).ok_or_else(|| fail(INVALID_ROW))
}

fn fail(message: &str) -> Error {
    Error::Message(message.to_string())
}
