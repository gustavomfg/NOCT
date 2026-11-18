//! NOCT interface entry point. Trunk compiles this crate to WebAssembly.

mod app;
mod chat;
mod chat_panel;
mod dom;
mod ipc;
mod provider;
mod provider_panel;
mod settings;
mod sidebar;
mod theme;
mod workspaces;

fn main() {
    leptos::mount::mount_to_body(app::App);
}
