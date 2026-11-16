//! The shell's window policy, locked. The values come from the context Tauri generates out of
//! `tauri.conf.json`, so a change to the policy fails here before it reaches a window. The Content-Security-
//! Policy has to keep the WebAssembly application running (`script-src 'wasm-unsafe-eval'`) and must not
//! grant inline script or inline style, which the UI does not need. `scripts/check-shell-csp.mjs` checks
//! that the policy renders the built UI.

use std::collections::HashMap;

use tauri::utils::config::Csp;

/// The context of the shell, with the runtime `main.rs` runs it on.
fn context() -> tauri::Context<tauri::Wry> {
    tauri::generate_context!()
}

/// The directives of the shipped policy, each with its source list.
fn policy() -> HashMap<String, Vec<String>> {
    let context = context();
    let Some(Csp::Policy(text)) = &context.config().app.security.csp else {
        panic!("the shell declares its policy as one string");
    };
    text.split(';')
        .map(str::trim)
        .filter(|directive| !directive.is_empty())
        .map(|directive| {
            let mut tokens = directive.split_whitespace().map(str::to_string);
            let name = tokens.next().expect("a directive has a name");
            (name, tokens.collect())
        })
        .collect()
}

#[test]
fn scripts_come_from_the_app_and_wasm_is_the_only_eval() {
    let policy = policy();

    assert_eq!(policy["script-src"], ["'self'", "'wasm-unsafe-eval'"]);
}

#[test]
fn styles_come_from_the_app_and_no_inline_style_is_granted() {
    let policy = policy();

    assert_eq!(policy["style-src"], ["'self'"]);
    assert!(!policy.contains_key("style-src-attr"));
    assert!(!policy.contains_key("style-src-elem"));
}

#[test]
fn every_other_resource_stays_on_the_app_origin_and_the_ipc_channel() {
    let policy = policy();

    assert_eq!(policy["default-src"], ["'self'"]);
    assert_eq!(policy["img-src"], ["'self'", "data:", "blob:"]);
    assert_eq!(
        policy["connect-src"],
        ["'self'", "ipc:", "http://ipc.localhost"]
    );
}

#[test]
fn no_directive_is_a_wildcard_a_remote_origin_or_an_unsafe_keyword() {
    for (name, sources) in policy() {
        for source in sources {
            assert!(
                !matches!(
                    source.as_str(),
                    "*" | "http:" | "https:" | "'unsafe-inline'" | "'unsafe-eval'"
                ),
                "{name} grants {source}"
            );
        }
    }
}

#[test]
fn the_global_tauri_object_stays_hidden() {
    let context = context();

    assert!(!context.config().app.with_global_tauri);
}

/// The main window is created in `src/window.rs`, with the navigation policy attached. A window declared
/// in the configuration would be created by Tauri before that policy exists, so none may be declared.
#[test]
fn no_window_is_created_from_the_configuration() {
    let context = context();

    assert!(context.config().app.windows.is_empty());
}

/// The capability of the main window grants the core defaults and nothing else. Whitespace is removed
/// before the comparison, so the layout of the file does not matter.
#[test]
fn the_main_window_is_granted_the_core_defaults_only() {
    let capability: String = include_str!("../capabilities/default.json")
        .split_whitespace()
        .collect();

    assert!(capability.contains("\"identifier\":\"main-window\""));
    assert!(capability.contains("\"windows\":[\"main\"]"));
    assert!(capability.contains("\"permissions\":[\"core:default\"]"));
}
