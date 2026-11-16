//! NOCT interface entry point. Trunk compiles this crate to WebAssembly.

mod app;
mod ipc;
mod settings;

fn main() {
    leptos::mount::mount_to_body(app::App);
}
