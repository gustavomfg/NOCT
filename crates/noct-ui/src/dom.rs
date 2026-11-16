//! The few page properties the interface reads and writes: the `data-theme` attribute of the `<html>` element,
//! and the system's colour preference. They go through the page's own DOM, as `ipc.rs` does for the shell, so
//! the interface needs no extra crate.

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// An element of the page.
    pub type Element;

    #[wasm_bindgen(method, js_name = setAttribute)]
    fn set_attribute(this: &Element, name: &str, value: &str);

    /// `document.documentElement`, the `<html>` element. The page keeps the same one for its whole life.
    #[wasm_bindgen(thread_local_v2, js_namespace = document, js_name = documentElement)]
    static DOCUMENT_ELEMENT: Element;

    /// A media query result, as `window.matchMedia` returns it.
    pub type MediaQueryList;

    #[wasm_bindgen(method, getter)]
    fn matches(this: &MediaQueryList) -> bool;

    #[wasm_bindgen(js_namespace = window, js_name = matchMedia)]
    fn match_media(query: &str) -> MediaQueryList;
}

/// Sets an attribute on the `<html>` element, for example `data-theme`.
pub fn set_root_attribute(name: &str, value: &str) {
    DOCUMENT_ELEMENT.with(|root| root.set_attribute(name, value));
}

/// True when the system asks for a dark appearance.
pub fn system_prefers_dark() -> bool {
    match_media("(prefers-color-scheme: dark)").matches()
}
