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

    /// `window.requestAnimationFrame`: the callback runs before the next paint, after the page's updates.
    #[wasm_bindgen(js_namespace = window, js_name = requestAnimationFrame)]
    fn request_animation_frame(callback: &JsValue) -> f64;

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

    #[wasm_bindgen(method, getter)]
    fn id(this: &Element) -> String;

    #[wasm_bindgen(method, getter = offsetTop)]
    fn offset_top(this: &Element) -> f64;

    #[wasm_bindgen(method, getter = offsetHeight)]
    fn offset_height(this: &Element) -> f64;

    /// The box of an element on the screen, as `getBoundingClientRect` returns it.
    pub type Rect;

    #[wasm_bindgen(method, js_name = getBoundingClientRect)]
    fn bounding_rect(this: &Element) -> Rect;

    #[wasm_bindgen(method, getter)]
    fn left(this: &Rect) -> f64;

    #[wasm_bindgen(method, getter)]
    fn top(this: &Rect) -> f64;

    #[wasm_bindgen(method, getter)]
    fn bottom(this: &Rect) -> f64;

    #[wasm_bindgen(method, getter)]
    fn width(this: &Rect) -> f64;

    /// The inline style object of an element. Only custom properties are set on it, through the CSS object model, which
    /// the page policy allows; no `style` attribute is ever written.
    type Style;

    #[wasm_bindgen(method, getter)]
    fn style(this: &Element) -> Style;

    #[wasm_bindgen(method, js_name = setProperty)]
    fn set_property(this: &Style, name: &str, value: &str);

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

/// The box of the element with an id on the screen: left, top, bottom and width, in CSS pixels.
pub fn box_of(id: &str) -> Option<(f64, f64, f64, f64)> {
    element_by_id(id).map(|element| {
        let rect = element.bounding_rect();
        (rect.left(), rect.top(), rect.bottom(), rect.width())
    })
}

/// The height of the window's viewport, in CSS pixels.
pub fn viewport_height() -> f64 {
    DOCUMENT_ELEMENT.with(|root| root.client_height())
}

/// Sets a CSS custom property (`--name`) on the `<html>` element through the CSS object model. A popup that must be
/// placed by measure reads it from its classes.
pub fn set_root_property(name: &str, value: &str) {
    // Only custom properties: a declared look stays in the stylesheet.
    if !name.starts_with("--") {
        return;
    }
    DOCUMENT_ELEMENT.with(|root| root.style().set_property(name, value));
}

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

/// The key under which the "bordas sem camadas" test is remembered for this viewer.
const LAYERS_KEY: &str = "noct.layers";

/// Whether this viewer turned on the "bordas sem camadas" test. Off unless stored as on.
pub fn stored_flat_layers() -> bool {
    storage_get(LAYERS_KEY).ok().flatten().as_deref() == Some("flat")
}

/// Remembers the "bordas sem camadas" choice for this viewer.
pub fn store_flat_layers(flat: bool) {
    let _ = storage_set(LAYERS_KEY, if flat { "flat" } else { "glass" });
}

/// Draws the panes inside the window without their own blur, or with it, through `data-layers` on `<html>`.
pub fn set_flat_layers(flat: bool) {
    set_root_attribute("data-layers", if flat { "flat" } else { "glass" });
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

/// Runs `then` before the next paint, when the views that a change of state redraws are on the page.
pub fn next_frame(then: impl FnOnce() + 'static) {
    let _ = request_animation_frame(&Closure::once_into_js(then));
}

/// Keeps a scroll area at its end while the person is there. `previous` is the area's height after the last call: a
/// person whose view reached that end (within a few pixels) is still following, and the area is scrolled to its new end;
/// a person who scrolled up is left where they are. The position is read now, so a scroll the page has not yet reported
/// as an event still counts. Returns the height to pass next time.
pub fn follow_end(selector: &str, previous: f64, force: bool) -> f64 {
    let Some(area) = query_selector(selector) else {
        return previous;
    };
    let following = area.scroll_top() + area.client_height() >= previous - 32.0;
    if force || following {
        area.set_scroll_top(area.scroll_height());
    }
    area.scroll_height()
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

/// The id of the element that has the keyboard focus, or an empty text.
pub fn focused_id() -> String {
    query_selector(":focus")
        .map(|element| element.id())
        .unwrap_or_default()
}
