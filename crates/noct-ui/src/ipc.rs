//! The call from the interface to the shell's commands.
//!
//! The page calls `window.__TAURI_INTERNALS__.invoke`, which the shell's webview defines for every page
//! (Tauri's `scripts/core.js`). The page therefore needs neither the global Tauri object nor an npm package.
//! The promise is followed through two JavaScript callbacks, and the result is handed back as JSON text. The
//! caller checks that text against the contracts of `noct-core` (see `settings.rs`).

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;

/// What a call came back with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The command's result, as JSON text.
    Resolved(String),
    /// The command's error, as JSON text. It is empty when the error cannot be written as JSON.
    Rejected(String),
    /// The page is not running in the shell, so there is no command to call.
    Unavailable,
}

#[wasm_bindgen]
extern "C" {
    /// The shell's invoke. It throws when the page is not running in the shell, which `catch` reports.
    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI_INTERNALS__"])]
    fn invoke(command: &str, payload: &JsValue) -> Result<JsPromise, JsValue>;

    /// A JavaScript promise.
    pub type JsPromise;

    #[wasm_bindgen(method, js_name = then)]
    fn then(this: &JsPromise, on_resolved: &JsValue, on_rejected: &JsValue) -> JsValue;

    #[wasm_bindgen(js_namespace = JSON, js_name = parse)]
    fn json_parse(text: &str) -> JsValue;

    #[wasm_bindgen(js_namespace = JSON, js_name = stringify)]
    fn json_stringify(value: &JsValue) -> JsValue;
}

type Done = Rc<RefCell<Option<Box<dyn FnOnce(Outcome)>>>>;

/// Calls `command` with an empty payload and hands its outcome to `done`, exactly once.
pub fn call(command: &str, done: impl FnOnce(Outcome) + 'static) {
    let done: Done = Rc::new(RefCell::new(Some(Box::new(done))));
    let payload = json_parse("{}");

    let on_resolved = {
        let done = Rc::clone(&done);
        Closure::once_into_js(move |value: JsValue| {
            finish(&done, Outcome::Resolved(text_of(&value)))
        })
    };
    let on_rejected = {
        let done = Rc::clone(&done);
        Closure::once_into_js(move |value: JsValue| {
            finish(&done, Outcome::Rejected(text_of(&value)))
        })
    };

    match invoke(command, &payload) {
        Ok(promise) => {
            promise.then(&on_resolved, &on_rejected);
        }
        Err(_) => finish(&done, Outcome::Unavailable),
    }
}

fn finish(done: &Done, outcome: Outcome) {
    let next = done.borrow_mut().take();
    if let Some(next) = next {
        next(outcome);
    }
}

fn text_of(value: &JsValue) -> String {
    json_stringify(value).as_string().unwrap_or_default()
}
