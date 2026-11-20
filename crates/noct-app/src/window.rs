//! The main window, created in code so that its navigation and new-window policy are attached when
//! it is built. A window declared in `tauri.conf.json` would exist before any policy could be set.

use tauri::webview::NewWindowResponse;
use tauri::window::Color;
use tauri::{App, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::navigation;

/// The label the capability in `capabilities/default.json` grants to.
pub const MAIN_WINDOW_LABEL: &str = "main";

/// The colour the window and its webview paint before the interface does. It is the `backgroundColor` of the
/// Electron window (`electron/main.ts`). The window is opaque: it asks for no transparency, so the desktop
/// does not show through it, and the interface paints over this colour.
pub const WINDOW_BACKGROUND: Color = Color(0x0b, 0x0b, 0x0e, 0xff);

/// The smallest size the window can be. The layout is checked at this size by `scripts/check-shell-ui.mjs`,
/// which reads these two constants, so the minimum and the check cannot drift apart. It is the minimum of the
/// Electron window as well (`electron/main.ts`).
pub const MIN_WINDOW_WIDTH: f64 = 720.0;
pub const MIN_WINDOW_HEIGHT: f64 = 600.0;

/// Whether the window has the web inspector: in a debug build only.
pub const DEVTOOLS: bool = cfg!(debug_assertions);

/// Opens the main window on the packaged frontend. Its navigation is limited to the application
/// (see `navigation::is_app_url`), and every request for a new window is denied. The drag and drop handler of Tauri stays
/// on (it is the default): a file dropped on the window becomes an event of the shell, not a navigation of the webview,
/// and the navigation policy refuses a `file:` page anyway.
pub fn create_main_window<R: Runtime>(app: &App<R>) -> tauri::Result<WebviewWindow<R>> {
    // The development server is an origin only in debug builds.
    let dev_server = if cfg!(debug_assertions) {
        app.config().build.dev_url.clone()
    } else {
        None
    };

    WebviewWindowBuilder::new(app, MAIN_WINDOW_LABEL, WebviewUrl::App("index.html".into()))
        .title("NOCT")
        .inner_size(1280.0, 800.0)
        .min_inner_size(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT)
        .transparent(false)
        .background_color(WINDOW_BACKGROUND)
        .on_navigation(move |url| navigation::is_app_url(url, dev_server.as_ref()))
        .on_new_window(|_, _| NewWindowResponse::Deny)
        // The inspector is for development builds; a release build has none (and Tauri's `devtools` feature is off).
        .devtools(DEVTOOLS)
        .build()
}
