//! Build script. It turns the list of the commands of the first slice into the application manifest, so
//! that every command must be granted by a capability before a window may call it.

#[path = "src/command_names.rs"]
mod command_names;

fn main() {
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(command_names::ALL));
    tauri_build::try_build(attributes).expect("the Tauri build script failed");
}
