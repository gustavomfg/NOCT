//! NOCT interface entry point. Trunk compiles this crate to WebAssembly.

mod app;
mod browser_guard;
mod changes_panel;
mod chat;
mod chat_panel;
mod context_picker;
mod diff;
mod dom;
mod ipc;
mod markdown;
mod model_picker;
mod provider;
mod provider_panel;
mod select;
mod settings;
mod settings_panel;
mod sidebar;
mod suggestions_panel;
mod theme;
mod workspaces;

fn main() {
    leptos::mount::mount_to_body(app::App);
}
