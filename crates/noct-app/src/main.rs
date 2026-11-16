//! NOCT desktop shell (Tauri 2). The UI runs in the system webview; privileged
//! operations are Rust commands registered here and granted through `capabilities/`.
//!
//! Phase 0 of the migration: the window opens with a placeholder page and no
//! commands yet. See docs/adr/0001-rust-migration.md for the phase plan.

// Keep release builds from opening a console window on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod navigation;
mod window;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            window::create_main_window(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the NOCT shell");
}
