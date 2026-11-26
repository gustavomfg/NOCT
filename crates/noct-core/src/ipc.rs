//! IPC contracts of the first slice: `settings.get`, `settings.set`, `workspaces.list` and
//! `conversations.list`. Written by hand in Rust, shared by the Tauri shell (`noct-app`) and the
//! interface (`noct-ui`).
//!
//! The inputs are checked the way the zod schema `settingsSchema` checks them: an unknown key is an
//! error, an explicit `null` is an error, and an absent optional key is `None`. The outputs have no
//! zod schema; they follow the TypeScript types in `shared/types.ts`. The samples in `tests/fixtures/ipc`
//! were written from the TypeScript side at the tag `electron-final`, and `tests/ipc_contracts.rs` checks this module
//! against them: zod decides the inputs and the TypeScript compiler decides the outputs.

use std::collections::BTreeMap;

use serde::de::{self, DeserializeOwned};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::limits::persistence::ASSISTANT_CHARACTERS;
use crate::text::utf16_len;
use crate::wire::present;

/// The Tauri command names of the slice. Each replaces the channel named in `shared/ipc/channels.ts`:
/// `settings_get` for `settings:get`, `settings_set` for `settings:set`, `workspaces_list` for
/// `workspaces:list` and `conversations_list` for `conversations:list`.
pub mod commands {
    pub const SETTINGS_GET: &str = "settings_get";
    pub const SETTINGS_SET: &str = "settings_set";
    pub const WORKSPACES_LIST: &str = "workspaces_list";
    pub const CONVERSATIONS_LIST: &str = "conversations_list";
}

/// The settings of the application: `AppSettings` in `shared/types.ts`, and the `settings_get` and
/// `settings_set` output.
pub mod settings {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum ApprovalPolicy {
        Untrusted,
        OnRequest,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub enum AppTheme {
        Dark,
        Light,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum AppLanguage {
        #[serde(rename = "pt-BR")]
        PtBr,
        #[serde(rename = "en")]
        En,
    }

    /// `AppSettings`. The approval policy is always present: the settings repository fills it in.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AppSettings {
        pub approval_policy: ApprovalPolicy,
        #[serde(
            default,
            deserialize_with = "present",
            skip_serializing_if = "Option::is_none"
        )]
        pub diagnostic_mode: Option<bool>,
        #[serde(
            default,
            deserialize_with = "present",
            skip_serializing_if = "Option::is_none"
        )]
        pub theme: Option<AppTheme>,
        #[serde(
            default,
            deserialize_with = "present",
            skip_serializing_if = "Option::is_none"
        )]
        pub language: Option<AppLanguage>,
        #[serde(
            default,
            deserialize_with = "present",
            skip_serializing_if = "Option::is_none"
        )]
        pub pandoc_version: Option<String>,
    }

    /// The argument of `settings_set`: the fields the interface may change. It is `settingsSchema` in
    /// `electron/ipc/registerSettingsIpc.ts`: strict, and every field optional.
    #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SettingsUpdate {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub approval_policy: Option<ApprovalPolicy>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub diagnostic_mode: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub theme: Option<AppTheme>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub language: Option<AppLanguage>,
    }

    const UPDATE_FIELDS: &[&str] = &["approvalPolicy", "diagnosticMode", "theme", "language"];

    /// Reads only a JSON object. A derived `Deserialize` would also read an array as this struct, because
    /// every field is optional, and zod refuses it. Each value is read the way its zod type reads it, so
    /// an explicit `null` is refused too.
    impl<'de> Deserialize<'de> for SettingsUpdate {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let fields = Map::<String, Value>::deserialize(deserializer)?;
            let mut update = Self::default();
            for (key, value) in fields {
                match key.as_str() {
                    "approvalPolicy" => update.approval_policy = Some(field::<_, D::Error>(value)?),
                    "diagnosticMode" => update.diagnostic_mode = Some(field::<_, D::Error>(value)?),
                    "theme" => update.theme = Some(field::<_, D::Error>(value)?),
                    "language" => update.language = Some(field::<_, D::Error>(value)?),
                    _ => return Err(de::Error::unknown_field(&key, UPDATE_FIELDS)),
                }
            }
            Ok(update)
        }
    }

    fn field<T, E>(value: Value) -> Result<T, E>
    where
        T: DeserializeOwned,
        E: de::Error,
    {
        serde_json::from_value(value).map_err(E::custom)
    }

    /// The output of `settings.get` and `settings.set`: the saved values read the way `readSettings` reads
    /// them in `electron/ipc/registerSettingsIpc.ts`. The defaults are the ones the settings repository
    /// applies. A saved key the contract does not hold is not part of the output.
    pub fn from_saved(saved: &BTreeMap<String, String>) -> AppSettings {
        let value = |key: &str| saved.get(key).map(String::as_str);
        AppSettings {
            approval_policy: match value("approvalPolicy") {
                Some("untrusted") => ApprovalPolicy::Untrusted,
                _ => ApprovalPolicy::OnRequest,
            },
            diagnostic_mode: Some(value("diagnosticMode") == Some("true")),
            theme: Some(match value("theme") {
                Some("light") => AppTheme::Light,
                _ => AppTheme::Dark,
            }),
            language: Some(match value("language") {
                Some("en") => AppLanguage::En,
                _ => AppLanguage::PtBr,
            }),
            pandoc_version: value("pandocVersion").map(str::to_string),
        }
    }

    /// The saved values for an update, as `settings.set` writes them: the enum values as stored, and
    /// `diagnosticMode` as the strings `"true"` or `"false"`. Only the fields present are written.
    pub fn to_saved(update: &SettingsUpdate) -> BTreeMap<String, String> {
        let mut saved = BTreeMap::new();
        if let Some(policy) = update.approval_policy {
            let stored = match policy {
                ApprovalPolicy::Untrusted => "untrusted",
                ApprovalPolicy::OnRequest => "on-request",
            };
            saved.insert("approvalPolicy".to_string(), stored.to_string());
        }
        if let Some(diagnostic) = update.diagnostic_mode {
            saved.insert("diagnosticMode".to_string(), diagnostic.to_string());
        }
        if let Some(theme) = update.theme {
            let stored = match theme {
                AppTheme::Dark => "dark",
                AppTheme::Light => "light",
            };
            saved.insert("theme".to_string(), stored.to_string());
        }
        if let Some(language) = update.language {
            let stored = match language {
                AppLanguage::PtBr => "pt-BR",
                AppLanguage::En => "en",
            };
            saved.insert("language".to_string(), stored.to_string());
        }
        saved
    }
}

/// The pages of conversations. `conversations_list` takes a [`ConversationPageRequest`] and returns a
/// [`ConversationPage`]. The order is `updatedAt` descending, then `id` descending, and a cursor is the key
/// of the last conversation of the page before it. The unpaginated list is not exposed as a command.
pub mod conversations {
    use serde::de;
    use serde::{Deserialize, Deserializer, Serialize};
    use serde_json::{Map, Value};

    use super::pages::limit_of;
    use crate::contracts::Conversation;

    pub use super::pages::{CURSOR_CHARACTERS_MAX, PAGE_LIMIT_MAX, PAGE_LIMIT_MIN};

    const REQUEST_FIELDS: &[&str] = &["limit", "cursor"];

    /// The key of the last conversation of a page. The next page holds the conversations after it.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct ConversationCursor {
        pub updated_at: String,
        pub id: String,
    }

    /// A request for one page. `limit` is required; `cursor` is absent for the first page, and an explicit
    /// `null` is refused, as the other optional fields of the contracts are.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ConversationPageRequest {
        pub limit: usize,
        pub cursor: Option<ConversationCursor>,
    }

    impl<'de> Deserialize<'de> for ConversationPageRequest {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let fields = Map::<String, Value>::deserialize(deserializer)?;
            let mut limit = None;
            let mut cursor = None;
            for (key, value) in fields {
                match key.as_str() {
                    "limit" => limit = Some(limit_of::<D::Error>(value)?),
                    "cursor" => cursor = Some(cursor_of::<D::Error>(value)?),
                    _ => return Err(de::Error::unknown_field(&key, REQUEST_FIELDS)),
                }
            }
            let limit = limit.ok_or_else(|| de::Error::missing_field("limit"))?;
            Ok(Self { limit, cursor })
        }
    }

    fn cursor_of<E: de::Error>(value: Value) -> Result<ConversationCursor, E> {
        if value.is_null() {
            return Err(E::custom("cursor must be an object, not null"));
        }
        let cursor: ConversationCursor = serde_json::from_value(value).map_err(E::custom)?;
        for text in [&cursor.updated_at, &cursor.id] {
            let length = text.chars().count();
            if length == 0 || length > CURSOR_CHARACTERS_MAX {
                return Err(E::custom(format!(
                    "cursor fields have from 1 to {CURSOR_CHARACTERS_MAX} characters"
                )));
            }
        }
        Ok(cursor)
    }

    /// One page of conversations, in order. `next_cursor` is present exactly when `has_more` is true.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ConversationPage {
        pub items: Vec<Conversation>,
        pub has_more: bool,
        pub next_cursor: Option<ConversationCursor>,
    }
}

/// The bounds shared by the pages of the shell. `limit` is required and bounded, so a request cannot ask for
/// an unbounded reply. Every page is read from a key cursor, so no request skips rows by count.
pub mod pages {
    use serde::de;
    use serde_json::Value;

    /// The bounds of a page. These are contract bounds of the Rust shell, with no TypeScript counterpart:
    /// the TypeScript conversation page is `COLLECTION_PAGE_LIMITS.conversations`, 100 in `shared/constants.ts`.
    pub const PAGE_LIMIT_MIN: u64 = 1;
    pub const PAGE_LIMIT_MAX: u64 = 500;
    /// The longest value of a cursor field that is a timestamp or an identifier, in characters. Both are far shorter.
    pub const CURSOR_CHARACTERS_MAX: usize = 128;

    /// A JSON integer from `PAGE_LIMIT_MIN` to `PAGE_LIMIT_MAX`. A fraction, a string or a negative number
    /// is refused, because `as_u64` returns nothing for them.
    pub fn limit_of<E: de::Error>(value: Value) -> Result<usize, E> {
        value
            .as_u64()
            .filter(|limit| (PAGE_LIMIT_MIN..=PAGE_LIMIT_MAX).contains(limit))
            .and_then(|limit| usize::try_from(limit).ok())
            .ok_or_else(|| {
                E::custom(format!(
                    "limit must be an integer from {PAGE_LIMIT_MIN} to {PAGE_LIMIT_MAX}"
                ))
            })
    }
}

/// The page of projects. `workspaces_list` takes a [`WorkspacePageRequest`] and returns a [`WorkspacePage`].
/// The order is favorites first, then the most recently opened, then the path. The path only breaks ties, which the
/// TypeScript list leaves to SQLite, so the order is total. A cursor is the key of the last project of a page; the
/// next page holds the projects after that key, so a project opened between two requests does not shift the rows
/// the walk has already read. A project whose key changes between two pages can still be skipped or read twice.
pub mod workspaces {
    use serde::de;
    use serde::{Deserialize, Deserializer, Serialize};
    use serde_json::{Map, Value};

    use super::pages::{CURSOR_CHARACTERS_MAX, limit_of};
    use crate::contracts::Workspace;

    const REQUEST_FIELDS: &[&str] = &["limit", "cursor"];

    /// The longest project path a request may carry, in characters. The TypeScript handler of `workspace.watch`
    /// takes the same bound, 4,000 characters, so both sides refuse the same paths.
    pub const PATH_CHARACTERS_MAX: usize = 4_000;

    /// The key of the last project of a page. The next page holds the projects after it.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct WorkspaceCursor {
        pub favorite: bool,
        pub last_opened_at: String,
        pub path: String,
    }

    /// A request for one page. `limit` is required; `cursor` is absent for the first page, and an explicit `null`
    /// is refused, as the other optional fields of the contracts are.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct WorkspacePageRequest {
        pub limit: usize,
        pub cursor: Option<WorkspaceCursor>,
    }

    impl<'de> Deserialize<'de> for WorkspacePageRequest {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let fields = Map::<String, Value>::deserialize(deserializer)?;
            let mut limit = None;
            let mut cursor = None;
            for (key, value) in fields {
                match key.as_str() {
                    "limit" => limit = Some(limit_of::<D::Error>(value)?),
                    "cursor" => cursor = Some(cursor_of::<D::Error>(value)?),
                    _ => return Err(de::Error::unknown_field(&key, REQUEST_FIELDS)),
                }
            }
            let limit = limit.ok_or_else(|| de::Error::missing_field("limit"))?;
            Ok(Self { limit, cursor })
        }
    }

    fn cursor_of<E: de::Error>(value: Value) -> Result<WorkspaceCursor, E> {
        if value.is_null() {
            return Err(E::custom("cursor must be an object, not null"));
        }
        let cursor: WorkspaceCursor = serde_json::from_value(value).map_err(E::custom)?;
        let bounded = [
            (&cursor.last_opened_at, CURSOR_CHARACTERS_MAX),
            (&cursor.path, PATH_CHARACTERS_MAX),
        ];
        for (text, max) in bounded {
            let length = text.chars().count();
            if length == 0 || length > max {
                return Err(E::custom(format!(
                    "cursor fields have from 1 to {max} characters"
                )));
            }
        }
        Ok(cursor)
    }

    /// One page of projects, in order. `has_more` says whether a project exists after this page, and `next_cursor`
    /// is present exactly when it does. An unknown field is refused, as the TypeScript type refuses it.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct WorkspacePage {
        pub items: Vec<Workspace>,
        pub has_more: bool,
        /// Required in the reply, even when it is `null`: serde reads a missing `Option` as `None`, which would
        /// accept a reply the TypeScript type refuses.
        #[serde(deserialize_with = "present_or_null")]
        pub next_cursor: Option<WorkspaceCursor>,
    }

    fn present_or_null<'de, D>(deserializer: D) -> Result<Option<WorkspaceCursor>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<WorkspaceCursor>::deserialize(deserializer)
    }

    const FAVORITE_FIELDS: &[&str] = &["workspace", "favorite"];

    /// A request to mark a project as a favorite, or to unmark it. Both fields are required: `workspace` is the
    /// path of a saved project, from 1 to `PATH_CHARACTERS_MAX` characters, and `favorite` is a boolean. The shell
    /// refuses a project that is not saved; this request does not create one.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct WorkspaceFavoriteRequest {
        pub workspace: String,
        pub favorite: bool,
    }

    impl<'de> Deserialize<'de> for WorkspaceFavoriteRequest {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let fields = Map::<String, Value>::deserialize(deserializer)?;
            let mut workspace = None;
            let mut favorite = None;
            for (key, value) in fields {
                match key.as_str() {
                    "workspace" => workspace = Some(path_of::<D::Error>(value)?),
                    "favorite" => {
                        favorite =
                            Some(value.as_bool().ok_or_else(|| {
                                de::Error::custom("favorite must be true or false")
                            })?)
                    }
                    _ => return Err(de::Error::unknown_field(&key, FAVORITE_FIELDS)),
                }
            }
            let workspace = workspace.ok_or_else(|| de::Error::missing_field("workspace"))?;
            let favorite = favorite.ok_or_else(|| de::Error::missing_field("favorite"))?;
            Ok(Self {
                workspace,
                favorite,
            })
        }
    }

    /// A path of 1 to `PATH_CHARACTERS_MAX` characters. A null, a number or a longer path is refused.
    fn path_of<E: de::Error>(value: Value) -> Result<String, E> {
        let Value::String(path) = value else {
            return Err(E::custom("workspace must be a path"));
        };
        let length = path.chars().count();
        if length == 0 || length > PATH_CHARACTERS_MAX {
            return Err(E::custom(format!(
                "workspace has from 1 to {PATH_CHARACTERS_MAX} characters"
            )));
        }
        Ok(path)
    }
}

/// The request of the commands that read an assistant reply: `brain_extract` and `suggestions_create`. The reply is kept
/// as it is, and it has at most `ASSISTANT_CHARACTERS` UTF-16 units, as both TypeScript schemas allow. An unknown field
/// is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyRequest {
    pub conversation_id: String,
    pub content: String,
}

const REPLY_FIELDS: &[&str] = &["conversationId", "content"];

impl<'de> Deserialize<'de> for ReplyRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let fields = Map::<String, Value>::deserialize(deserializer)?;
        let mut conversation_id = None;
        let mut content = None;
        for (key, value) in fields {
            match key.as_str() {
                "conversationId" => {
                    conversation_id = Some(brain::uuid_field::<D::Error>("conversationId", value)?)
                }
                "content" => {
                    let Value::String(text) = value else {
                        return Err(de::Error::custom("content must be a string"));
                    };
                    if utf16_len(&text) as u64 > ASSISTANT_CHARACTERS {
                        return Err(de::Error::custom("content is too long"));
                    }
                    content = Some(text);
                }
                _ => return Err(de::Error::unknown_field(&key, REPLY_FIELDS)),
            }
        }
        Ok(Self {
            conversation_id: conversation_id
                .ok_or_else(|| de::Error::missing_field("conversationId"))?,
            content: content.ok_or_else(|| de::Error::missing_field("content"))?,
        })
    }
}

/// The requests of the brain memory commands.
pub mod brain {
    use serde::de;
    use serde::{Deserialize, Deserializer, Serialize};
    use serde_json::{Map, Value};

    use crate::brain_memory::{
        BrainMemory, BrainMemoryKind, BrainMemoryScope, BrainMemoryStatus, normalize_brain_content,
    };
    use crate::validation::uuid_text;

    const CREATE_FIELDS: &[&str] = &["conversationId", "kind", "scope", "content"];

    /// A request to create a brain memory in the project of a conversation. The fields are those of
    /// `brainMemoryCreateSchema`: `conversationId` is a UUID, `kind` and `scope` are the enumerated values, and
    /// `content` is trimmed, has from 1 to `BRAIN_MEMORY_CHARACTERS` UTF-16 units, and does not look like a credential.
    /// An unknown field, a missing field or a wrong type is refused before the store is read.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct BrainMemoryCreateRequest {
        pub conversation_id: String,
        pub kind: BrainMemoryKind,
        pub scope: BrainMemoryScope,
        pub content: String,
    }

    impl<'de> Deserialize<'de> for BrainMemoryCreateRequest {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let fields = Map::<String, Value>::deserialize(deserializer)?;
            let mut conversation_id = None;
            let mut kind = None;
            let mut scope = None;
            let mut content = None;
            for (key, value) in fields {
                match key.as_str() {
                    "conversationId" => {
                        conversation_id = Some(uuid_field::<D::Error>("conversationId", value)?)
                    }
                    "kind" => kind = Some(enum_field::<BrainMemoryKind, D::Error>(value)?),
                    "scope" => scope = Some(enum_field::<BrainMemoryScope, D::Error>(value)?),
                    "content" => content = Some(content_field::<D::Error>(value)?),
                    _ => return Err(de::Error::unknown_field(&key, CREATE_FIELDS)),
                }
            }
            Ok(Self {
                conversation_id: conversation_id
                    .ok_or_else(|| de::Error::missing_field("conversationId"))?,
                kind: kind.ok_or_else(|| de::Error::missing_field("kind"))?,
                scope: scope.ok_or_else(|| de::Error::missing_field("scope"))?,
                content: content.ok_or_else(|| de::Error::missing_field("content"))?,
            })
        }
    }

    pub(super) fn uuid_field<E: de::Error>(field: &str, value: Value) -> Result<String, E> {
        let Value::String(text) = value else {
            return Err(E::custom(format!("{field} must be a UUID")));
        };
        uuid_text(field, &text).map_err(E::custom)
    }

    pub(super) fn enum_field<T: de::DeserializeOwned, E: de::Error>(value: Value) -> Result<T, E> {
        serde_json::from_value(value).map_err(E::custom)
    }

    /// The content as `safeBrainMemoryContent` reads it: trimmed, checked for length and for credentials.
    pub(super) fn content_field<E: de::Error>(value: Value) -> Result<String, E> {
        let Value::String(text) = value else {
            return Err(E::custom("content must be a string"));
        };
        normalize_brain_content(&text).ok_or_else(|| {
            E::custom("content is from 1 to 8000 characters and is not a credential")
        })
    }

    const UPDATE_FIELDS: &[&str] = &[
        "conversationId",
        "memoryId",
        "kind",
        "scope",
        "content",
        "confidence",
        "status",
    ];

    /// The request of `brain_delete`: a memory of the conversation's project. Both ids are UUIDs, and an unknown field is
    /// refused, as `brainMemoryDeleteSchema` is strict.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct BrainMemoryDeleteRequest {
        pub conversation_id: String,
        pub memory_id: String,
    }

    const DELETE_FIELDS: &[&str] = &["conversationId", "memoryId"];

    impl<'de> Deserialize<'de> for BrainMemoryDeleteRequest {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let fields = Map::<String, Value>::deserialize(deserializer)?;
            let mut conversation_id = None;
            let mut memory_id = None;
            for (key, value) in fields {
                match key.as_str() {
                    "conversationId" => {
                        conversation_id = Some(uuid_field::<D::Error>("conversationId", value)?)
                    }
                    "memoryId" => memory_id = Some(uuid_field::<D::Error>("memoryId", value)?),
                    _ => return Err(de::Error::unknown_field(&key, DELETE_FIELDS)),
                }
            }
            Ok(Self {
                conversation_id: conversation_id
                    .ok_or_else(|| de::Error::missing_field("conversationId"))?,
                memory_id: memory_id.ok_or_else(|| de::Error::missing_field("memoryId"))?,
            })
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct BrainMemoryUpdateRequest {
        pub conversation_id: String,
        pub memory_id: String,
        pub kind: Option<BrainMemoryKind>,
        pub scope: Option<BrainMemoryScope>,
        pub content: Option<String>,
        pub confidence: Option<i64>,
        pub status: Option<BrainMemoryStatus>,
    }

    impl<'de> Deserialize<'de> for BrainMemoryUpdateRequest {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let fields = Map::<String, Value>::deserialize(deserializer)?;
            let mut conversation_id = None;
            let mut memory_id = None;
            let mut kind = None;
            let mut scope = None;
            let mut content = None;
            let mut confidence = None;
            let mut status = None;
            for (key, value) in fields {
                match key.as_str() {
                    "conversationId" => {
                        conversation_id = Some(uuid_field::<D::Error>("conversationId", value)?)
                    }
                    "memoryId" => memory_id = Some(uuid_field::<D::Error>("memoryId", value)?),
                    "kind" => kind = Some(enum_field::<BrainMemoryKind, D::Error>(value)?),
                    "scope" => scope = Some(enum_field::<BrainMemoryScope, D::Error>(value)?),
                    "content" => content = Some(content_field::<D::Error>(value)?),
                    "confidence" => confidence = Some(confidence_field::<D::Error>(value)?),
                    "status" => status = Some(enum_field::<BrainMemoryStatus, D::Error>(value)?),
                    _ => return Err(de::Error::unknown_field(&key, UPDATE_FIELDS)),
                }
            }
            if kind.is_none()
                && scope.is_none()
                && content.is_none()
                && confidence.is_none()
                && status.is_none()
            {
                return Err(de::Error::custom("Informe ao menos uma alteração."));
            }
            Ok(Self {
                conversation_id: conversation_id
                    .ok_or_else(|| de::Error::missing_field("conversationId"))?,
                memory_id: memory_id.ok_or_else(|| de::Error::missing_field("memoryId"))?,
                kind,
                scope,
                content,
                confidence,
                status,
            })
        }
    }

    fn confidence_field<E: de::Error>(value: Value) -> Result<i64, E> {
        let number = value
            .as_f64()
            .ok_or_else(|| E::custom("confidence is an integer from 0 to 100"))?;
        if number.fract() != 0.0 || !(0.0..=100.0).contains(&number) {
            return Err(E::custom("confidence is an integer from 0 to 100"));
        }
        Ok(number as i64)
    }

    /// The reply of `brain_extract`: the memories that were stored, the reply without its blocks, and a warning when
    /// the memories could not be stored. `warning` is absent when there is none, as the TypeScript reply has it.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct BrainMemoryExtraction {
        pub memories: Vec<BrainMemory>,
        pub content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub warning: Option<String>,
    }
}

/// The reply of the suggestion commands.
/// The requests of the artifact commands.
pub mod artifacts {
    use serde::de;
    use serde::{Deserialize, Deserializer, Serialize};
    use serde_json::{Map, Value};

    use super::brain::uuid_field;

    /// The request of `artifacts_delete`: an artifact of a conversation, both UUIDs. The object is strict, as
    /// `artifacts.delete` checks it.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ArtifactDeleteRequest {
        pub conversation_id: String,
        pub artifact_id: String,
    }

    const DELETE_FIELDS: &[&str] = &["conversationId", "artifactId"];

    impl<'de> Deserialize<'de> for ArtifactDeleteRequest {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let fields = Map::<String, Value>::deserialize(deserializer)?;
            let mut conversation_id = None;
            let mut artifact_id = None;
            for (key, value) in fields {
                match key.as_str() {
                    "conversationId" => {
                        conversation_id = Some(uuid_field::<D::Error>("conversationId", value)?)
                    }
                    "artifactId" => {
                        artifact_id = Some(uuid_field::<D::Error>("artifactId", value)?)
                    }
                    _ => return Err(de::Error::unknown_field(&key, DELETE_FIELDS)),
                }
            }
            Ok(Self {
                conversation_id: conversation_id
                    .ok_or_else(|| de::Error::missing_field("conversationId"))?,
                artifact_id: artifact_id.ok_or_else(|| de::Error::missing_field("artifactId"))?,
            })
        }
    }

    /// The reply of `artifacts_delete`: `{ deleted: true }`, as the TypeScript shell replies.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
    pub struct ArtifactDeletion {
        pub deleted: bool,
    }
}

/// The requests of the assistant commands.
pub mod ai {
    use serde::de;
    use serde::{Deserialize, Deserializer};
    use serde_json::{Map, Value};

    use super::brain::uuid_field;
    use crate::json::serialize_within_limit;
    use crate::limits::persistence::{ASSISTANT_CHARACTERS, METADATA_CHARACTERS};
    use crate::text::utf16_len;

    /// The request of `ai_save_assistant`: a reply to store, with its optional metadata. The content is kept as it is,
    /// up to `ASSISTANT_CHARACTERS` UTF-16 units. The metadata is JSON within the metadata limit; an explicit `null` is
    /// the same as no metadata, as in the TypeScript record. Unlike the reply commands of suggestions and memories, the
    /// schema of `saveAssistant` is not strict, so an unknown field is ignored, as zod strips it.
    #[derive(Debug, Clone, PartialEq)]
    pub struct SaveAssistantRequest {
        pub conversation_id: String,
        pub content: String,
        pub metadata: Option<Value>,
    }

    impl<'de> Deserialize<'de> for SaveAssistantRequest {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let fields = Map::<String, Value>::deserialize(deserializer)?;
            let mut conversation_id = None;
            let mut content = None;
            let mut metadata = None;
            for (key, value) in fields {
                match key.as_str() {
                    "conversationId" => {
                        conversation_id = Some(uuid_field::<D::Error>("conversationId", value)?)
                    }
                    "content" => {
                        let Value::String(text) = value else {
                            return Err(de::Error::custom("content must be a string"));
                        };
                        if utf16_len(&text) as u64 > ASSISTANT_CHARACTERS {
                            return Err(de::Error::custom("content is too long"));
                        }
                        content = Some(text);
                    }
                    "metadata" => {
                        if serialize_within_limit(&value, METADATA_CHARACTERS as usize).is_err() {
                            return Err(de::Error::custom(
                                "metadata is not valid JSON within its limit",
                            ));
                        }
                        metadata = (!value.is_null()).then_some(value);
                    }
                    _ => {}
                }
            }
            Ok(Self {
                conversation_id: conversation_id
                    .ok_or_else(|| de::Error::missing_field("conversationId"))?,
                content: content.ok_or_else(|| de::Error::missing_field("content"))?,
                metadata,
            })
        }
    }
}

pub mod suggestions {
    use serde::de;
    use serde::{Deserialize, Deserializer, Serialize};
    use serde_json::{Map, Value};

    use super::brain::{enum_field, uuid_field};
    use crate::limits::persistence::SUGGESTION_RESULT_CHARACTERS;
    use crate::suggestions::{ReviewComparison, Suggestion, SuggestionStatus};
    use crate::text::utf16_len;

    /// The request of `suggestions_status`: the new status of a suggestion of a conversation, and an optional result.
    /// The result is at most `SUGGESTION_RESULT_CHARACTERS` UTF-16 units, as `suggestionStatusSchema` allows, and it is
    /// kept as it is. An unknown field is refused.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SuggestionStatusRequest {
        pub conversation_id: String,
        pub suggestion_id: String,
        pub status: SuggestionStatus,
        pub result: Option<String>,
    }

    const STATUS_FIELDS: &[&str] = &["conversationId", "suggestionId", "status", "result"];

    impl<'de> Deserialize<'de> for SuggestionStatusRequest {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let fields = Map::<String, Value>::deserialize(deserializer)?;
            let mut conversation_id = None;
            let mut suggestion_id = None;
            let mut status = None;
            let mut result = None;
            for (key, value) in fields {
                match key.as_str() {
                    "conversationId" => {
                        conversation_id = Some(uuid_field::<D::Error>("conversationId", value)?)
                    }
                    "suggestionId" => {
                        suggestion_id = Some(uuid_field::<D::Error>("suggestionId", value)?)
                    }
                    "status" => status = Some(enum_field::<SuggestionStatus, D::Error>(value)?),
                    "result" => {
                        let Value::String(text) = value else {
                            return Err(de::Error::custom("result must be a string"));
                        };
                        if utf16_len(&text) as u64 > SUGGESTION_RESULT_CHARACTERS {
                            return Err(de::Error::custom("result is too long"));
                        }
                        result = Some(text);
                    }
                    _ => return Err(de::Error::unknown_field(&key, STATUS_FIELDS)),
                }
            }
            Ok(Self {
                conversation_id: conversation_id
                    .ok_or_else(|| de::Error::missing_field("conversationId"))?,
                suggestion_id: suggestion_id
                    .ok_or_else(|| de::Error::missing_field("suggestionId"))?,
                status: status.ok_or_else(|| de::Error::missing_field("status"))?,
                result,
            })
        }
    }

    /// The reply of `suggestions_create`. A reply with a review snapshot carries the suggestions and the comparison with
    /// the previous review. A reply without a usable snapshot carries no suggestions and a warning, and keeps the earlier
    /// ones. `comparison` and `warning` are absent when they do not apply, as the TypeScript reply has them.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SuggestionCreation {
        pub suggestions: Vec<Suggestion>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub comparison: Option<ReviewComparison>,
        pub content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub warning: Option<String>,
    }
}
