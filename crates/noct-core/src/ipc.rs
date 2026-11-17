//! IPC contracts of the first slice: `settings.get`, `settings.set`, `workspaces.list` and
//! `conversations.list`. Written by hand in Rust, shared by the Tauri shell (`noct-app`) and the
//! interface (`noct-ui`).
//!
//! The inputs are checked the way the zod schema `settingsSchema` checks them: an unknown key is an
//! error, an explicit `null` is an error, and an absent optional key is `None`. The outputs have no
//! zod schema; they follow the TypeScript types in `shared/types.ts`. The samples in `tests/fixtures/ipc`
//! are written by `scripts/generate-ipc-samples.mjs`, and `tests/ipc_contracts.rs` checks this module
//! against them: zod decides the inputs and the TypeScript compiler decides the outputs.

use std::collections::BTreeMap;

use serde::de::{self, DeserializeOwned};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

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

    pub use super::pages::{PAGE_LIMIT_MAX, PAGE_LIMIT_MIN};
    /// The longest value of a cursor field, in characters. Timestamps and identifiers are far shorter.
    pub const CURSOR_CHARACTERS_MAX: usize = 128;

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
/// an unbounded reply, and `offset` is bounded, so it cannot ask for an unbounded skip.
pub mod pages {
    use serde::de;
    use serde_json::Value;

    /// The bounds of a page. These are contract bounds of the Rust shell, with no TypeScript counterpart:
    /// the TypeScript conversation page is `COLLECTION_PAGE_LIMITS.conversations`, 100 in `shared/constants.ts`.
    pub const PAGE_LIMIT_MIN: u64 = 1;
    pub const PAGE_LIMIT_MAX: u64 = 500;
    /// The largest offset a page may skip.
    pub const OFFSET_MAX: u64 = 1_000_000;

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

    /// A JSON integer from 0 to `OFFSET_MAX`. An explicit `null` is refused, as the other optional fields are.
    pub fn offset_of<E: de::Error>(value: Value) -> Result<usize, E> {
        value
            .as_u64()
            .filter(|offset| *offset <= OFFSET_MAX)
            .and_then(|offset| usize::try_from(offset).ok())
            .ok_or_else(|| E::custom(format!("offset must be an integer from 0 to {OFFSET_MAX}")))
    }
}

/// The page of projects. `workspaces_list` takes a [`WorkspacePageRequest`] and returns a [`WorkspacePage`].
/// The order is the store's: favorites first, then the most recently opened. An offset is the number of projects
/// to skip. A project opened between two requests can move across a page boundary, so a reader that needs every
/// project asks for the maximum limit.
pub mod workspaces {
    use serde::de;
    use serde::{Deserialize, Deserializer, Serialize};
    use serde_json::{Map, Value};

    use super::pages::{limit_of, offset_of};
    use crate::contracts::Workspace;

    const REQUEST_FIELDS: &[&str] = &["limit", "offset"];

    /// A request for one page. `limit` is required; `offset` is 0 when it is absent.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct WorkspacePageRequest {
        pub limit: usize,
        pub offset: usize,
    }

    impl<'de> Deserialize<'de> for WorkspacePageRequest {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let fields = Map::<String, Value>::deserialize(deserializer)?;
            let mut limit = None;
            let mut offset = None;
            for (key, value) in fields {
                match key.as_str() {
                    "limit" => limit = Some(limit_of::<D::Error>(value)?),
                    "offset" => offset = Some(offset_of::<D::Error>(value)?),
                    _ => return Err(de::Error::unknown_field(&key, REQUEST_FIELDS)),
                }
            }
            let limit = limit.ok_or_else(|| de::Error::missing_field("limit"))?;
            Ok(Self {
                limit,
                offset: offset.unwrap_or(0),
            })
        }
    }

    /// One page of projects, in order. `has_more` says whether a project exists after this page. An unknown field
    /// is refused, as the TypeScript type refuses it.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct WorkspacePage {
        pub items: Vec<Workspace>,
        pub has_more: bool,
    }
}
