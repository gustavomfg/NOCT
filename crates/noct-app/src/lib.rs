//! NOCT desktop shell (Tauri 2). The UI runs in the system webview; privileged operations are Rust
//! commands granted through `capabilities/`.
//!
//! The shell is a library so that its tests run the code the binary runs. See
//! docs/adr/0001-rust-migration.md for the phase plan.

pub mod bootstrap;
pub mod logging;
pub mod navigation;
pub mod window;

use tauri::Manager;

/// Runs the shell: opens the persistence in the data directory, then the main window.
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let default = app.path().app_data_dir()?;
            let directory = bootstrap::resolve_data_directory(
                std::env::var_os(bootstrap::DATA_DIRECTORY_VARIABLE),
                default,
            )?;
            app.manage(bootstrap::open(&directory)?);
            window::create_main_window(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the NOCT shell");
}
