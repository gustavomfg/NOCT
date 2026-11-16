//! Build script. It turns the list of the commands of the first slice into the application manifest, so
//! that every command must be granted by a capability before a window may call it. On Windows it also gives
//! the integration tests the Common Controls v6 manifest that the application has (see `link_test_manifest`).

use std::env;
use std::path::Path;

#[path = "src/command_names.rs"]
mod command_names;

fn main() {
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(command_names::ALL));
    tauri_build::try_build(attributes).expect("the Tauri build script failed");
    link_test_manifest();
}

/// The integration tests that build the application with `tauri::test` reach the menu code, which calls
/// `TaskDialogIndirect` from `comctl32.dll`, an export of Common Controls v6. tauri-build embeds the
/// application manifest that asks for v6 only in the binaries. A test executable without it loads the old
/// library, and Windows stops it with STATUS_ENTRYPOINT_NOT_FOUND before any test runs. The tests get the
/// same manifest as the application, from `windows/common-controls.manifest`, which is a copy of tauri-build's.
fn link_test_manifest() {
    let windows_msvc = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if !windows_msvc {
        return;
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("windows")
        .join("common-controls.manifest");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}",
        manifest.display()
    );
}
