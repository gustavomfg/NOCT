//! Keeps the webview from acting as a browser in a release build: no page menu on a right click (Voltar, Recarregar,
//! Inspecionar), no shortcut that reloads, navigates or prints, and no navigation to a file dropped on the window. A
//! development build keeps them all, for the developer's tools.
//!
//! The right click keeps the system menu where it is useful: in a text box, in an editable element, and over a
//! selection, so copy and paste stay. The listeners are added from WebAssembly; the page has no inline script.

use leptos::ev;
use leptos::prelude::*;
use leptos::web_sys;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    type Selection;

    #[wasm_bindgen(js_namespace = window, js_name = getSelection)]
    fn get_selection() -> Option<Selection>;

    #[wasm_bindgen(method, js_name = toString)]
    fn text(this: &Selection) -> String;
}

/// Where a right click keeps the system menu: text boxes and editable elements.
const EDITABLE: &str = "input, textarea, [contenteditable]:not([contenteditable=\"false\"])";

/// Whether the browser behaviour is blocked in this build: in a release build only.
pub const BLOCKED: bool = !cfg!(debug_assertions);

/// What a key press does to the page, when a browser would reload, navigate or print.
pub fn is_browser_shortcut(key: &str, ctrl: bool, alt: bool, meta: bool) -> bool {
    let command = ctrl || meta;
    match key {
        "F5" | "BrowserRefresh" | "BrowserBack" | "BrowserForward" => true,
        "r" | "R" | "p" | "P" => command && !alt,
        "ArrowLeft" | "ArrowRight" => alt && !command,
        "[" | "]" => meta,
        _ => false,
    }
}

fn in_editable(target: Option<web_sys::EventTarget>) -> bool {
    let element = target.and_then(|target| match target.dyn_into::<web_sys::Element>() {
        Ok(element) => Some(element),
        Err(target) => target
            .dyn_into::<web_sys::Node>()
            .ok()
            .and_then(|node| node.parent_element()),
    });
    element.is_some_and(|element| element.closest(EDITABLE).ok().flatten().is_some())
}

fn has_selection() -> bool {
    get_selection().is_some_and(|selection| !selection.text().is_empty())
}

/// Adds the listeners, in a release build. They live as long as the page.
pub fn install() {
    if !BLOCKED {
        return;
    }
    let menu = window_event_listener(ev::contextmenu, |event| {
        if !in_editable(event.target()) && !has_selection() {
            event.prevent_default();
        }
    });
    let keys = window_event_listener(ev::keydown, |event| {
        if is_browser_shortcut(
            &event.key(),
            event.ctrl_key(),
            event.alt_key(),
            event.meta_key(),
        ) {
            event.prevent_default();
        }
    });
    // A file dropped on the window would open it in the webview. The shell's own drop handler takes it first; this keeps
    // the page from following it anyway.
    let over = window_event_listener(ev::dragover, |event| event.prevent_default());
    let drop = window_event_listener(ev::drop, |event| event.prevent_default());
    // The page never removes them.
    std::mem::forget((menu, keys, over, drop));
}

#[cfg(test)]
mod tests {
    use super::is_browser_shortcut;

    #[test]
    fn the_shortcuts_that_reload_navigate_or_print_are_told() {
        for (key, ctrl, alt, meta) in [
            ("F5", false, false, false),
            ("r", true, false, false),
            ("R", true, false, false),
            ("p", true, false, false),
            ("ArrowLeft", false, true, false),
            ("ArrowRight", false, true, false),
            ("r", false, false, true),
            ("[", false, false, true),
            ("BrowserBack", false, false, false),
        ] {
            assert!(
                is_browser_shortcut(key, ctrl, alt, meta),
                "{key} ctrl={ctrl} alt={alt} meta={meta}"
            );
        }
        for (key, ctrl, alt, meta) in [
            ("r", false, false, false),
            ("ArrowLeft", false, false, false),
            ("ArrowLeft", true, false, false),
            ("c", true, false, false),
            ("v", true, false, false),
            ("a", true, false, false),
            ("Enter", false, false, false),
            ("F2", false, false, false),
        ] {
            assert!(
                !is_browser_shortcut(key, ctrl, alt, meta),
                "{key} ctrl={ctrl} alt={alt} meta={meta}"
            );
        }
    }
}
