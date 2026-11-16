//! The shell's window policy, locked. The values come from the context Tauri generates out of
//! `tauri.conf.json`, so a change to the policy fails here before it reaches a window. The Content-Security-
//! Policy has to keep the WebAssembly application running (`script-src 'wasm-unsafe-eval'`) and must not
//! grant inline script or inline style, which the UI does not need. `scripts/check-shell-csp.mjs` checks
//! that the policy renders the built UI.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use tauri::utils::config::Csp;

/// The context of the shell, with the runtime `main.rs` runs it on. It comes from the library, which generates
/// it once: see `noct_app::context`.
fn context() -> tauri::Context<tauri::Wry> {
    noct_app::context()
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

/// The interface sets no inline style, because the policy grants none: a `style` attribute, a `<style>`
/// element, or a Leptos style directive would be refused by the webview. Classes and `data-theme` carry the
/// look instead (ADR 0001). The sources of the interface and its page are scanned for those forms.
#[test]
fn the_interface_sets_no_inline_style() {
    let interface = Path::new(env!("CARGO_MANIFEST_DIR")).join("../noct-ui");
    let mut files = vec![interface.join("index.html")];
    collect_rust_sources(&interface.join("src"), &mut files);
    assert!(
        files.len() >= 2,
        "the scan must reach the interface sources, found {files:?}"
    );

    const INLINE_STYLE: [&str; 5] = ["style=", "<style", "attr:style", "style:", ".style("];
    for file in &files {
        let text = fs::read_to_string(file).expect("the interface source is readable");
        for pattern in INLINE_STYLE {
            assert!(
                !text.contains(pattern),
                "{} sets inline style with `{pattern}`",
                file.display()
            );
        }
    }
}

fn collect_rust_sources(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("the interface source directory is readable") {
        let path = entry.expect("a directory entry is readable").path();
        if path.is_dir() {
            collect_rust_sources(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
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

/// The glass layer is on in the committed page (`crates/noct-ui/shell.css`, `data-glass`). `scripts/set-glass.mjs`
/// turns it off for a comparison and back on. A page committed with it off would open with the solid colours,
/// and nobody would see the change.
#[test]
fn the_window_starts_with_the_glass_layer_on() {
    let page =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../noct-ui/index.html"))
            .expect("the interface page is readable");
    let html_tag = page
        .lines()
        .find(|line| line.trim_start().starts_with("<html"))
        .expect("the page declares its <html> element");

    assert!(
        html_tag.contains(r#"data-glass="on""#),
        "the committed page must start with the glass on: {html_tag}"
    );
}
