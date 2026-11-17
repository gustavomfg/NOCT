//! The commands of the first slice. This file is shared by the application (`lib.rs`) and by the build
//! script (`build.rs`), which turns the list into the application manifest. Each name here must have a
//! Rust command with the same name in `commands.rs`, and a permission `allow-<name>` in the capability.

pub const SETTINGS_GET: &str = "settings_get";
pub const SETTINGS_SET: &str = "settings_set";
pub const WORKSPACES_LIST: &str = "workspaces_list";
pub const BRAIN_CREATE: &str = "brain_create";
pub const BRAIN_EXTRACT: &str = "brain_extract";
pub const BRAIN_UPDATE: &str = "brain_update";
pub const SUGGESTIONS_CREATE: &str = "suggestions_create";
pub const SUGGESTIONS_STATUS: &str = "suggestions_status";
pub const MODELS_SET_BINDINGS: &str = "models_set_bindings";
pub const AI_SAVE_ASSISTANT: &str = "ai_save_assistant";
pub const CONVERSATIONS_DELETE: &str = "conversations_delete";
pub const WORKSPACE_REMOVE: &str = "workspace_remove";
pub const BRAIN_DELETE: &str = "brain_delete";
pub const ARTIFACTS_DELETE: &str = "artifacts_delete";
pub const CONVERSATIONS_CREATE: &str = "conversations_create";
pub const CONVERSATIONS_LIST: &str = "conversations_list";
pub const WORKSPACE_SET_FAVORITE: &str = "workspace_set_favorite";

pub const ALL: &[&str] = &[
    SETTINGS_GET,
    SETTINGS_SET,
    WORKSPACES_LIST,
    BRAIN_CREATE,
    BRAIN_EXTRACT,
    BRAIN_UPDATE,
    SUGGESTIONS_CREATE,
    SUGGESTIONS_STATUS,
    MODELS_SET_BINDINGS,
    AI_SAVE_ASSISTANT,
    CONVERSATIONS_DELETE,
    WORKSPACE_REMOVE,
    BRAIN_DELETE,
    ARTIFACTS_DELETE,
    CONVERSATIONS_CREATE,
    CONVERSATIONS_LIST,
    WORKSPACE_SET_FAVORITE,
];
