//! Conversations and their messages. Port of `ConversationRepository.ts`.

use noct_core::json::serialize_within_limit;
use noct_core::limits::persistence::METADATA_CHARACTERS;
use rusqlite::{Connection, OptionalExtension, Params, Row, params};
use serde_json::Value;
use uuid::Uuid;

use super::Page;
use crate::clock::iso_now;
use crate::{Error, Result, transaction};

const NEW_CONVERSATION_TITLE: &str = "Nova conversa";
/// Titles derived from a prompt keep this many UTF-16 code units, as `String#slice` does.
const TITLE_UTF16_UNITS: usize = 52;
const RECENT_MESSAGES_MIN: i64 = 1;
const RECENT_MESSAGES_MAX: i64 = 200;
const METADATA_LIMIT: usize = METADATA_CHARACTERS as usize;

const CONVERSATION_COLUMNS: &str = "id, title, workspace, created_at, updated_at";
const MESSAGE_COLUMNS: &str = "id, conversation_id, role, content, metadata, created_at";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationRow {
    pub id: String,
    pub title: String,
    pub workspace: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

impl MessageRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
        }
    }
}

/// A stored message. `metadata` is the JSON text written at insert time, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageRow {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub metadata: Option<String>,
    pub created_at: String,
}

pub struct Conversations<'a> {
    conn: &'a Connection,
}

impl<'a> Conversations<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Most recently updated first.
    pub fn list(&self) -> Result<Vec<ConversationRow>> {
        conversations(
            self.conn,
            &format!("SELECT {CONVERSATION_COLUMNS} FROM conversations ORDER BY updated_at DESC"),
            [],
        )
    }

    pub fn page(&self, offset: usize, limit: usize) -> Result<Page<ConversationRow>> {
        let mut rows = conversations(
            self.conn,
            &format!(
                "SELECT {CONVERSATION_COLUMNS} FROM conversations ORDER BY updated_at DESC LIMIT ? OFFSET ?"
            ),
            params![to_i64(limit.saturating_add(1)), to_i64(offset)],
        )?;
        let has_more = rows.len() > limit;
        rows.truncate(limit);
        Ok(Page {
            items: rows,
            has_more,
        })
    }

    pub fn get(&self, id: &str) -> Result<Option<ConversationRow>> {
        let row = self
            .conn
            .query_row(
                &format!("SELECT {CONVERSATION_COLUMNS} FROM conversations WHERE id=?"),
                [id],
                conversation_from_row,
            )
            .optional()?;
        Ok(row)
    }

    pub fn create(&self, workspace: &str) -> Result<ConversationRow> {
        let now = iso_now()?;
        let row = ConversationRow {
            id: Uuid::new_v4().to_string(),
            title: NEW_CONVERSATION_TITLE.to_string(),
            workspace: workspace.to_string(),
            created_at: now.clone(),
            updated_at: now,
        };
        self.conn.execute(
            "INSERT INTO conversations (id,title,workspace,created_at,updated_at) VALUES (?,?,?,?,?)",
            params![row.id, row.title, row.workspace, row.created_at, row.updated_at],
        )?;
        Ok(row)
    }

    /// Titles the conversation from its first prompt.
    pub fn rename_from_prompt(&self, id: &str, prompt: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE conversations SET title=?, updated_at=? WHERE id=?",
            params![title_from_prompt(prompt), iso_now()?, id],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM conversations WHERE id=?", params![id])?;
        Ok(())
    }

    /// Every message, oldest first. Ties keep insertion order.
    pub fn list_messages(&self, conversation_id: &str) -> Result<Vec<MessageRow>> {
        messages(
            self.conn,
            &format!(
                "SELECT {MESSAGE_COLUMNS} FROM messages WHERE conversation_id=? ORDER BY created_at, rowid"
            ),
            [conversation_id],
        )
    }

    /// The latest user and assistant messages, oldest first. `limit` is clamped to 1..=200.
    pub fn list_recent_messages(
        &self,
        conversation_id: &str,
        limit: i64,
    ) -> Result<Vec<MessageRow>> {
        let bounded = limit.clamp(RECENT_MESSAGES_MIN, RECENT_MESSAGES_MAX);
        let mut rows = messages(
            self.conn,
            &format!(
                "SELECT {MESSAGE_COLUMNS} FROM messages
                 WHERE conversation_id=? AND role IN ('user','assistant')
                 ORDER BY created_at DESC, rowid DESC LIMIT ?"
            ),
            params![conversation_id, bounded],
        )?;
        rows.reverse();
        Ok(rows)
    }

    /// One page of messages counted from the newest, returned oldest first.
    pub fn page_messages(
        &self,
        conversation_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<MessageRow>> {
        let mut rows = messages(
            self.conn,
            &format!(
                "SELECT {MESSAGE_COLUMNS} FROM messages WHERE conversation_id=?
                 ORDER BY created_at DESC, rowid DESC LIMIT ? OFFSET ?"
            ),
            params![
                conversation_id,
                to_i64(limit.saturating_add(1)),
                to_i64(offset)
            ],
        )?;
        let has_more = rows.len() > limit;
        rows.truncate(limit);
        rows.reverse();
        Ok(Page {
            items: rows,
            has_more,
        })
    }

    /// Adds a message and refreshes the conversation's `updated_at`, atomically.
    pub fn add_message(
        &self,
        conversation_id: &str,
        role: MessageRole,
        content: &str,
        metadata: Option<&Value>,
    ) -> Result<MessageRow> {
        transaction::run(self.conn, |conn| {
            insert_message(conn, conversation_id, role, content, metadata)
        })
    }
}

/// Inserts a message without opening a transaction, for callers that already hold one.
pub fn insert_message(
    conn: &Connection,
    conversation_id: &str,
    role: MessageRole,
    content: &str,
    metadata: Option<&Value>,
) -> Result<MessageRow> {
    // A missing or null value is stored as NULL, as the TypeScript repository does.
    let metadata = match metadata.filter(|value| !value.is_null()) {
        Some(value) => Some(
            serialize_within_limit(value, METADATA_LIMIT)
                .map_err(|error| Error::Message(error.to_string()))?,
        ),
        None => None,
    };
    let row = MessageRow {
        id: Uuid::new_v4().to_string(),
        conversation_id: conversation_id.to_string(),
        role: role.as_str().to_string(),
        content: content.to_string(),
        metadata,
        created_at: iso_now()?,
    };
    conn.execute(
        "INSERT INTO messages (id,conversation_id,role,content,metadata,created_at) VALUES (?,?,?,?,?,?)",
        params![
            row.id,
            row.conversation_id,
            row.role,
            row.content,
            row.metadata,
            row.created_at
        ],
    )?;
    conn.execute(
        "UPDATE conversations SET updated_at=? WHERE id=?",
        params![row.created_at, row.conversation_id],
    )?;
    Ok(row)
}

fn conversations<P: Params>(
    conn: &Connection,
    sql: &str,
    params: P,
) -> Result<Vec<ConversationRow>> {
    let mut statement = conn.prepare(sql)?;
    let rows = statement
        .query_map(params, conversation_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn messages<P: Params>(conn: &Connection, sql: &str, params: P) -> Result<Vec<MessageRow>> {
    let mut statement = conn.prepare(sql)?;
    let rows = statement
        .query_map(params, message_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn conversation_from_row(row: &Row<'_>) -> rusqlite::Result<ConversationRow> {
    Ok(ConversationRow {
        id: row.get("id")?,
        title: row.get("title")?,
        workspace: row.get("workspace")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn message_from_row(row: &Row<'_>) -> rusqlite::Result<MessageRow> {
    Ok(MessageRow {
        id: row.get("id")?,
        conversation_id: row.get("conversation_id")?,
        role: row.get("role")?,
        content: row.get("content")?,
        metadata: row.get("metadata")?,
        created_at: row.get("created_at")?,
    })
}

fn to_i64(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Port of `renameFromPrompt`: collapse whitespace runs, trim, keep the first 52 UTF-16
/// code units, and fall back to the default title when nothing is left.
fn title_from_prompt(prompt: &str) -> String {
    let collapsed = collapse_js_whitespace(prompt);
    let title = utf16_prefix(collapsed.trim_matches(' '), TITLE_UTF16_UNITS);
    if title.is_empty() {
        NEW_CONVERSATION_TITLE.to_string()
    } else {
        title
    }
}

/// Replaces every run of JavaScript `\s` characters with one space.
fn collapse_js_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_run = false;
    for c in text.chars() {
        if is_js_whitespace(c) {
            if !in_run {
                out.push(' ');
                in_run = true;
            }
        } else {
            out.push(c);
            in_run = false;
        }
    }
    out
}

/// JavaScript's `\s` and `String#trim` set. It differs from Rust's `char::is_whitespace`:
/// it includes U+FEFF and omits U+0085.
fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0b}' | '\u{0c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

fn utf16_prefix(text: &str, units: usize) -> String {
    let prefix: Vec<u16> = text.encode_utf16().take(units).collect();
    String::from_utf16_lossy(&prefix)
}
