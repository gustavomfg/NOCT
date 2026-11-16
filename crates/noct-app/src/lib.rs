//! NOCT desktop shell (Tauri 2). The UI runs in the system webview; privileged operations are Rust
//! commands granted through `capabilities/`.
//!
//! The shell is a library so that its tests run the code the binary runs. See
//! docs/adr/0001-rust-migration.md for the phase plan.

pub mod bootstrap;
pub mod command_names;
pub mod commands;
pub mod error;
pub mod logging;
pub mod navigation;
pub mod services;
pub mod window;
pub mod workspace_scope;

use tauri::{Manager, Runtime};

/// The commands of the first slice, as a handler for the application builder. The same list is the one the
/// build script grants to the capabilities, in `command_names::ALL`.
pub fn command_handler<R: Runtime>()
-> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        commands::settings_get,
        commands::settings_set,
        commands::workspaces_list,
        commands::conversations_list,
    ]
}

/// Runs the shell: opens the persistence in the data directory, then the main window.
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(command_handler())
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
