//! IPC contracts of the first slice: `settings.get`, `settings.set`, `workspaces.list` and
//! `conversations.list`. Written by hand in Rust, shared by the Tauri shell (`noct-app`) and the
//! interface (`noct-ui`).
//!
//! The inputs are checked the way the zod schema `settingsSchema` checks them: an unknown key is an
//! error, an explicit `null` is an error, and an absent optional key is `None`. The outputs have no
//! zod schema; they follow the TypeScript types in `shared/types.ts`. The samples in `tests/fixtures/ipc`
//! are written by `scripts/generate-ipc-samples.mjs`, and `tests/ipc_contracts.rs` checks this module
//! against them: zod decides the inputs and the TypeScript compiler decides the outputs.

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
}
