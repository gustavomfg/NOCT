//! The few page properties the interface reads and writes: the `data-theme` attribute of the `<html>` element,
//! and the system's colour preference. They go through the page's own DOM, as `ipc.rs` does for the shell, so
//! the interface needs no extra crate.

use std::cell::RefCell;
use std::rc::Rc;

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

    /// The promise that the clipboard returns for a write.
    pub type ClipboardPromise;

    #[wasm_bindgen(method, js_name = then)]
    fn then(this: &ClipboardPromise, on_resolved: &JsValue, on_rejected: &JsValue) -> JsValue;

    /// `navigator.clipboard.writeText`. It throws when the page has no clipboard, which `catch` reports.
    #[wasm_bindgen(catch, js_namespace = ["navigator", "clipboard"], js_name = writeText)]
    fn write_text(text: &str) -> Result<ClipboardPromise, JsValue>;

    /// A media query result, as `window.matchMedia` returns it.
    pub type MediaQueryList;

    #[wasm_bindgen(method, getter)]
    fn matches(this: &MediaQueryList) -> bool;

    #[wasm_bindgen(js_namespace = window, js_name = matchMedia)]
    fn match_media(query: &str) -> MediaQueryList;

    /// The first element that matches a selector, as `document.querySelector` returns it.
    #[wasm_bindgen(js_namespace = document, js_name = querySelector)]
    fn query_selector(selector: &str) -> Option<Element>;

    #[wasm_bindgen(method, getter = scrollHeight)]
    fn scroll_height(this: &Element) -> f64;

    #[wasm_bindgen(method, getter = scrollTop)]
    fn scroll_top(this: &Element) -> f64;

    #[wasm_bindgen(method, setter = scrollTop)]
    fn set_scroll_top(this: &Element, value: f64);

    #[wasm_bindgen(method, getter = clientHeight)]
    fn client_height(this: &Element) -> f64;

    #[wasm_bindgen(method)]
    fn focus(this: &Element);

    #[wasm_bindgen(method, getter = offsetTop)]
    fn offset_top(this: &Element) -> f64;

    #[wasm_bindgen(method, getter = offsetHeight)]
    fn offset_height(this: &Element) -> f64;

    /// The element with an id, as `document.getElementById` returns it.
    #[wasm_bindgen(js_namespace = document, js_name = getElementById)]
    fn element_by_id(id: &str) -> Option<Element>;

    /// `window.localStorage.getItem`. A storage that is blocked throws, which `catch` reports.
    #[wasm_bindgen(catch, js_namespace = ["window", "localStorage"], js_name = getItem)]
    fn storage_get(key: &str) -> Result<Option<String>, JsValue>;

    /// `window.localStorage.setItem`. A storage that is full or blocked throws, which `catch` reports.
    #[wasm_bindgen(catch, js_namespace = ["window", "localStorage"], js_name = setItem)]
    fn storage_set(key: &str, value: &str) -> Result<(), JsValue>;
}

/// The key under which the glass choice is remembered for this viewer.
const GLASS_KEY: &str = "noct.glass";

/// The key under which the last project chosen is remembered for this viewer.
const PROJECT_KEY: &str = "noct.project";

/// Sets an attribute on the `<html>` element, for example `data-theme`.
pub fn set_root_attribute(name: &str, value: &str) {
    DOCUMENT_ELEMENT.with(|root| root.set_attribute(name, value));
}

type Done = Rc<RefCell<Option<Box<dyn FnOnce(bool)>>>>;

/// Copies `text` to the system clipboard. `done` is told whether the copy worked, once the clipboard has answered. When
/// the page has no clipboard, it is told `false` at once.
pub fn copy_text(text: &str, done: impl FnOnce(bool) + 'static) {
    let done: Done = Rc::new(RefCell::new(Some(Box::new(done))));
    let promise = match write_text(text) {
        Ok(promise) => promise,
        Err(_) => {
            finish(&done, false);
            return;
        }
    };
    let on_resolved = settle(&done, true);
    let on_rejected = settle(&done, false);
    promise.then(&on_resolved, &on_rejected);
}

fn settle(done: &Done, copied: bool) -> JsValue {
    let done = Rc::clone(done);
    Closure::once_into_js(move |_: JsValue| finish(&done, copied))
}

fn finish(done: &Done, copied: bool) {
    let next = done.borrow_mut().take();
    if let Some(next) = next {
        next(copied);
    }
}

/// True when the system asks for a dark appearance.
pub fn system_prefers_dark() -> bool {
    match_media("(prefers-color-scheme: dark)").matches()
}

/// The glass choice this viewer made before, or `None` when none was made or the storage cannot be read.
pub fn stored_glass() -> Option<bool> {
    storage_get(GLASS_KEY)
        .ok()
        .flatten()
        .map(|value| value != "off")
}

/// Remembers the glass choice for this viewer. A storage that refuses the write keeps the choice for this session only.
pub fn store_glass(on: bool) {
    let _ = storage_set(GLASS_KEY, if on { "on" } else { "off" });
}

/// Shows the glass layer, or not, through `data-glass` on the `<html>` element.
pub fn set_glass(on: bool) {
    set_root_attribute("data-glass", if on { "on" } else { "off" });
}

/// The path of the last project this viewer chose, or `None` when none was chosen or the storage cannot be read.
pub fn stored_project() -> Option<String> {
    storage_get(PROJECT_KEY)
        .ok()
        .flatten()
        .filter(|path| !path.is_empty())
}

/// Remembers the project this viewer chose, so the window opens on it next time. A refused write is ignored.
pub fn store_project(path: &str) {
    let _ = storage_set(PROJECT_KEY, path);
}

/// Keeps a scrolling area at its end while its content grows. `previous` is the content height the last call saw: when
/// the view was at the end then (within a few lines), or `force` is set, the area is scrolled to its new end. Returns the
/// height to pass next time. An area that is not on the page is left alone.
pub fn follow_end(selector: &str, previous: f64, force: bool) -> f64 {
    let Some(area) = query_selector(selector) else {
        return previous;
    };
    let height = area.scroll_height();
    let distance = previous - area.scroll_top() - area.client_height();
    if force || distance < 96.0 {
        area.set_scroll_top(height);
    }
    height
}

/// Scrolls a list so that one of its options is in view, when the arrows move past the visible part. The list must be the
/// option's offset parent (it is positioned).
pub fn keep_in_view(list: &str, option: &str) {
    let (Some(list), Some(option)) = (element_by_id(list), element_by_id(option)) else {
        return;
    };
    let top = option.offset_top();
    let bottom = top + option.offset_height();
    let view_top = list.scroll_top();
    let view_bottom = view_top + list.client_height();
    if top < view_top {
        list.set_scroll_top(top);
    } else if bottom > view_bottom {
        list.set_scroll_top(bottom - list.client_height());
    }
}

/// Moves the keyboard focus to the element with an id, when it is on the page.
pub fn focus(id: &str) {
    if let Some(element) = element_by_id(id) {
        element.focus();
    }
}
