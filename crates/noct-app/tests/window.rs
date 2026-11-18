//! The window background. The shell asks for an opaque window in the colour of the Electron window, so the
//! desktop does not show through it and the first paint matches the application.

use noct_app::window::{MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH, WINDOW_BACKGROUND};

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
