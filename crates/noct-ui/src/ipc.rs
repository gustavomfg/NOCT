//! The call from the interface to the shell's commands.
//!
//! The page calls `window.__TAURI_INTERNALS__.invoke`, which the shell's webview defines for every page
//! (Tauri's `scripts/core.js`). The page therefore needs neither the global Tauri object nor an npm package.
//! The promise is followed through two JavaScript callbacks, and the result is handed back as JSON text. The
//! caller checks that text against the contracts of `noct-core` (see `settings.rs`).

use std::cell::RefCell;
use std::collections::BTreeMap;
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

/// Why a call failed: the code and the message to show. The message comes from the shell when the shell
/// answered with an error, and from this module otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub code: String,
    pub message: String,
}

/// The message when the page is not running in the shell.
pub const UNAVAILABLE_MESSAGE: &str =
    "Não foi possível falar com o NOCT. Abra o aplicativo pela janela do desktop.";
/// The message when the shell answered with a reply that the contract does not allow.
pub const UNEXPECTED_MESSAGE: &str = "O NOCT respondeu de forma inesperada.";
/// The message when the shell answered with an error that is not in the shell's shape.
pub const UNKNOWN_MESSAGE: &str = "O NOCT retornou um erro sem detalhes legíveis.";

/// The reply text of a call that resolved, or the failure of a call that did not. A reply is checked against
/// the contract of its command by the caller, which is the only place that knows the shape it expects.
pub fn reply(outcome: Outcome) -> Result<String, Failure> {
    match outcome {
        Outcome::Resolved(text) => Ok(text),
        Outcome::Rejected(text) => Err(failure_of(&text)),
        Outcome::Unavailable => Err(Failure {
            code: "unavailable".to_string(),
            message: UNAVAILABLE_MESSAGE.to_string(),
        }),
    }
}

/// The failure of a reply that does not match the contract of its command.
pub fn invalid_response() -> Failure {
    Failure {
        code: "invalid-response".to_string(),
        message: UNEXPECTED_MESSAGE.to_string(),
    }
}

/// The code and message of a command error, which the shell sends as `{ "code": ..., "message": ... }`.
fn failure_of(text: &str) -> Failure {
    let parsed: Option<serde_json::Value> = serde_json::from_str(text).ok();
    let field = |name: &str| {
        parsed
            .as_ref()
            .and_then(|value| value.get(name))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    };
    match (field("code"), field("message")) {
        (Some(code), Some(message)) => Failure { code, message },
        _ => Failure {
            code: "unknown".to_string(),
            message: UNKNOWN_MESSAGE.to_string(),
        },
    }
}

#[wasm_bindgen]
extern "C" {
    /// The shell's invoke. It throws when the page is not running in the shell, which `catch` reports.
    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI_INTERNALS__"])]
    fn invoke(command: &str, payload: &JsValue) -> Result<JsPromise, JsValue>;

    /// The shell's registry of callbacks. It returns the identifier the shell uses to call back into the page.
    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI_INTERNALS__"], js_name = transformCallback)]
    fn transform_callback(callback: &JsValue, once: bool) -> Result<JsValue, JsValue>;

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
    call_with(command, "{}", done);
}

/// Calls `command` with `payload`, a JSON object, and hands its outcome to `done`, exactly once.
pub fn call_with(command: &str, payload: &str, done: impl FnOnce(Outcome) + 'static) {
    let done: Done = Rc::new(RefCell::new(Some(Box::new(done))));
    let payload = json_parse(payload);

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

/// One frame of a channel, as the shell sends it: a message with its position. The end of a channel has no message.
#[derive(serde::Deserialize)]
struct Frame {
    index: u64,
    #[serde(default)]
    message: Option<serde_json::Value>,
}

/// The messages of one channel. They are held until their turn, so they are handed on in the order the shell sent them.
#[derive(Default)]
struct Ordering {
    next: u64,
    held: BTreeMap<u64, String>,
}

/// One channel of the page: its order, and the caller that takes each message.
struct Stream {
    ordering: RefCell<Ordering>,
    on_message: RefCell<Box<dyn FnMut(String)>>,
}

/// Registers `on_message` as a channel of the shell, and returns the value a command takes for it: `__CHANNEL__:<id>`.
/// Each message reaches `on_message` as JSON text, in the order the shell sent it, even when frames arrive out of order.
/// The end of the channel is not handed on; the caller ends when its own last event arrives.
///
/// The callback stays registered for the life of the page. The shell can still send the end of a channel after the last
/// message, so the callback is never dropped while the page runs. Each question adds one small callback.
pub fn channel(on_message: impl FnMut(String) + 'static) -> Option<String> {
    let stream = Rc::new(Stream {
        ordering: RefCell::new(Ordering::default()),
        on_message: RefCell::new(Box::new(on_message)),
    });
    let callback = Closure::<dyn FnMut(JsValue)>::new(move |frame: JsValue| {
        deliver(&stream, &frame);
    });
    let id = transform_callback(callback.as_ref(), false)
        .ok()?
        .as_f64()?;
    callback.forget();
    Some(format!("__CHANNEL__:{}", id as u64))
}

fn deliver(stream: &Stream, frame: &JsValue) {
    let Ok(frame) = serde_json::from_str::<Frame>(&text_of(frame)) else {
        return;
    };
    let ready = {
        let mut ordering = stream.ordering.borrow_mut();
        if let Some(message) = frame.message {
            ordering.held.insert(frame.index, message.to_string());
        }
        let mut ready = Vec::new();
        loop {
            let next = ordering.next;
            let Some(message) = ordering.held.remove(&next) else {
                break;
            };
            ready.push(message);
            ordering.next += 1;
        }
        ready
    };
    let mut on_message = stream.on_message.borrow_mut();
    for message in ready {
        on_message(message);
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
