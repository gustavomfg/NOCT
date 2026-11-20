//! The window background. The shell asks for an opaque window in the colour of the Electron window, so the
//! desktop does not show through it and the first paint matches the application.

use noct_app::window::{DEVTOOLS, MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH, WINDOW_BACKGROUND};

#[test]
fn the_window_background_is_the_electron_background_and_opaque() {
    // `backgroundColor: '#0b0b0e'` in electron/main.ts
    assert_eq!(WINDOW_BACKGROUND.0, 0x0b);
    assert_eq!(WINDOW_BACKGROUND.1, 0x0b);
    assert_eq!(WINDOW_BACKGROUND.2, 0x0e);
    assert_eq!(WINDOW_BACKGROUND.3, 0xff, "the window is opaque");
}

#[test]
fn the_minimum_window_is_the_electron_minimum_that_the_layout_is_checked_at() {
    // `minWidth: 720, minHeight: 600` in electron/main.ts. `scripts/check-shell-ui.mjs` reads these constants and
    // lays the interface out at this size, so a smaller minimum would be a layout nobody has checked.
    assert_eq!(MIN_WINDOW_WIDTH, 720.0);
    assert_eq!(MIN_WINDOW_HEIGHT, 600.0);
}

#[test]
fn the_inspector_is_only_in_a_debug_build() {
    assert_eq!(DEVTOOLS, cfg!(debug_assertions));
}

/// The configuration of a release build: Tauri's `devtools` feature is not asked for anywhere, so a release window has no
/// inspector whatever a call says; the window keeps Tauri's drag and drop handler, so a dropped file is not opened by the
/// webview.
#[test]
fn a_release_build_has_no_inspector_and_keeps_the_drop_handler() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for manifest in [root.join("Cargo.toml"), root.join("../../Cargo.toml")] {
        let text = std::fs::read_to_string(&manifest).expect("the manifest is readable");
        for line in text
            .lines()
            .filter(|line| line.trim_start().starts_with("tauri"))
        {
            assert!(!line.contains("devtools"), "{}: {line}", manifest.display());
        }
    }
    let window =
        std::fs::read_to_string(root.join("src/window.rs")).expect("window.rs is readable");
    assert!(
        window.contains(".devtools(DEVTOOLS)"),
        "the window states its inspector"
    );
    assert!(
        !window.contains("disable_drag_drop_handler"),
        "the drop handler stays on, so a dropped file does not navigate the webview"
    );
}
