//! NOCT interface entry point. Trunk compiles this crate to WebAssembly.

mod app;
mod dom;
mod examples;
mod ipc;
mod settings;
mod theme;

fn main() {
    leptos::mount::mount_to_body(app::App);
}
