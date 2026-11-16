//! The window background. The shell asks for an opaque window in the colour of the Electron window, so the
//! desktop does not show through it and the first paint matches the application.

use noct_app::window::WINDOW_BACKGROUND;

#[test]
fn the_window_background_is_the_electron_background_and_opaque() {
    // `backgroundColor: '#0b0b0e'` in electron/main.ts
    assert_eq!(WINDOW_BACKGROUND.0, 0x0b);
    assert_eq!(WINDOW_BACKGROUND.1, 0x0b);
    assert_eq!(WINDOW_BACKGROUND.2, 0x0e);
    assert_eq!(WINDOW_BACKGROUND.3, 0xff, "the window is opaque");
}
