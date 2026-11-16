//! The commands of the first slice. This file is shared by the application (`lib.rs`) and by the build
//! script (`build.rs`), which turns the list into the application manifest. Each name here must have a
//! Rust command with the same name in `commands.rs`, and a permission `allow-<name>` in the capability.

pub const SETTINGS_GET: &str = "settings_get";
pub const SETTINGS_SET: &str = "settings_set";
pub const WORKSPACES_LIST: &str = "workspaces_list";
pub const CONVERSATIONS_LIST: &str = "conversations_list";

pub const ALL: &[&str] = &[
    SETTINGS_GET,
    SETTINGS_SET,
    WORKSPACES_LIST,
    CONVERSATIONS_LIST,
];
