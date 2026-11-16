//! Redaction of the application log. Port of `redactLogText` and `redactLogValue` in
//! `electron/logging/Logger.ts`: the rules, their order and their limits are the TypeScript ones.
//! `tests/fixtures/log-redaction.json` holds the TypeScript outputs the port must reproduce, written by
//! `scripts/generate-log-redaction-fixture.mjs`.
//!
//! Three details follow JavaScript, because the same log is read by both sides:
//! - `/i` folds ASCII letters only. The patterns are lowercase and the input is matched in an
//!   ASCII-lowercased copy, which keeps every byte offset. The kept text comes from the original.
//! - `\b` is the ASCII word boundary, `(?-u:\b)`, and `\s` is the JavaScript whitespace set.
//! - The text limit counts UTF-16 units, as `String#slice` does.
//!
//! Known difference: when the limit falls inside a surrogate pair, TypeScript keeps the lone high
//! surrogate and this port drops the whole pair, because a Rust string cannot hold a lone surrogate.

use std::sync::LazyLock;

use regex::{Captures, Regex};
use serde_json::{Map, Value};

use crate::text::JS_WHITESPACE_CLASS;

/// The longest text the log keeps, in UTF-16 units.
pub const MAX_TEXT_UNITS: usize = 8_000;
/// Containers at this depth or deeper become `[TRUNCATED]`.
const MAX_DEPTH: usize = 5;
/// Arrays and objects keep this many items.
const MAX_ITEMS: usize = 50;

const SENSITIVE_FIELD_NAMES: &str = "token|authorization|api[_-]?key|apikey|password|secret|credential|auth_token|refresh_token|access_token|client_secret|private_key|privatekey|prompt|content|diff|stdout|stderr|message|stack|error|line";

/// The field-name test of `redactLogValue`: unanchored, so any key that contains one of the names is
/// dropped. It is matched against the ASCII-lowercased key.
static SENSITIVE_FIELD: LazyLock<Regex> =
    LazyLock::new(|| compile(&format!("(?:{fields})", fields = SENSITIVE_FIELD_NAMES)));

/// `"name": "value"` in JSON text. The whole match becomes `"[REDACTED]"`, as in TypeScript.
static JSON_SENSITIVE: LazyLock<Regex> = LazyLock::new(|| {
    compile(&format!(
        r#"(["'])(?:{fields})["'][{ws}]*:[{ws}]*["'][^\n\r\x{{2028}}\x{{2029}}]*?["']"#,
        fields = SENSITIVE_FIELD_NAMES,
        ws = JS_WHITESPACE_CLASS,
    ))
});

/// `bearer <token>` and similar headers. The keyword is kept.
static SENSITIVE_HEADER: LazyLock<Regex> = LazyLock::new(|| {
    compile(&format!(
        r"(?-u:\b)(bearer|basic|digest|token)[{ws}]+[a-zA-Z0-9+/=_-]{{8,}}",
        ws = JS_WHITESPACE_CLASS,
    ))
});

/// `name=value` or `name: "value"` with a quoted value. The name and the separator are kept.
static KEY_VALUE_SENSITIVE: LazyLock<Regex> = LazyLock::new(|| {
    compile(&format!(
        r#"(?-u:\b)({fields})([{ws}]*[=:][{ws}]*)(?:"[^"']+"|'[^"']+')"#,
        fields = SENSITIVE_FIELD_NAMES,
        ws = JS_WHITESPACE_CLASS,
    ))
});

/// Provider keys and private key headers, matched case-sensitively in the text as it is. The headers
/// cover every algorithm (`BEGIN RSA PRIVATE KEY` included); `scripts/generate-log-redaction-fixture.mjs`
/// records where that differs from TypeScript.
static SENSITIVE_TOKENS: LazyLock<Regex> = LazyLock::new(|| {
    compile(&format!(
        r"sk-[a-zA-Z0-9]{{20,}}|pk-[a-zA-Z0-9]{{20,}}|gh[opsu]_[a-zA-Z0-9]{{36,}}|glpat-[a-zA-Z0-9_-]{{20,}}|npm_[a-zA-Z0-9]{{36,}}|AKIA[0-9A-Z]{{16}}|eyJ[a-zA-Z0-9_-]{{10,}}\.[a-zA-Z0-9_-]{{10,}}\.[a-zA-Z0-9_-]{{10,}}|-----BEGIN[{ws}]+(?:(?:RSA|DSA|EC|OPENSSH)[{ws}]+)?PRIVATE[{ws}]+KEY-----|-----BEGIN[{ws}]+(?:RSA|DSA|EC|OPENSSH)[{ws}]+KEY-----",
        ws = JS_WHITESPACE_CLASS,
    ))
});

fn compile(pattern: &str) -> Regex {
    Regex::new(pattern).expect("the log redaction pattern is valid")
}

/// Redacts a text line the way `redactLogText` does, then keeps at most [`MAX_TEXT_UNITS`] UTF-16 units.
pub fn redact_text(value: &str) -> String {
    let json = replace_ascii_insensitive(&JSON_SENSITIVE, value, |caps, text| {
        let quote = original(text, caps, 1);
        format!("{quote}[REDACTED]{quote}")
    });
    let header = replace_ascii_insensitive(&SENSITIVE_HEADER, &json, |caps, text| {
        format!("{} [REDACTED]", original(text, caps, 1))
    });
    let key_value = replace_ascii_insensitive(&KEY_VALUE_SENSITIVE, &header, |caps, text| {
        format!(
            "{}{}[REDACTED]",
            original(text, caps, 1),
            original(text, caps, 2)
        )
    });
    let tokens = SENSITIVE_TOKENS
        .replace_all(&key_value, "[REDACTED-TOKEN]")
        .into_owned();
    truncate_utf16(&tokens, MAX_TEXT_UNITS)
}

/// Redacts a structured value the way `redactLogValue` does. Strings are redacted at any depth; keys
/// that name a secret are dropped. JSON has no cycles, so the circular marker of the TypeScript
/// version never applies.
pub fn redact_value(value: &Value) -> Value {
    redact_at(value, 0)
}

fn redact_at(value: &Value, depth: usize) -> Value {
    match value {
        Value::String(text) => Value::String(redact_text(text)),
        Value::Array(_) | Value::Object(_) if depth >= MAX_DEPTH => {
            Value::String("[TRUNCATED]".to_string())
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .take(MAX_ITEMS)
                .map(|item| redact_at(item, depth + 1))
                .collect(),
        ),
        Value::Object(entries) => Value::Object(
            entries
                .iter()
                .take(MAX_ITEMS)
                .filter(|(key, _)| !is_sensitive_field(key))
                .map(|(key, item)| (key.clone(), redact_at(item, depth + 1)))
                .collect::<Map<String, Value>>(),
        ),
        other => other.clone(),
    }
}

fn is_sensitive_field(key: &str) -> bool {
    SENSITIVE_FIELD.is_match(&key.to_ascii_lowercase())
}

/// Replaces every match of `pattern`. The match runs on the ASCII-lowercased copy, which has the same
/// offsets as `text`; the replacement reads the groups from `text`, so kept text keeps its case.
fn replace_ascii_insensitive(
    pattern: &Regex,
    text: &str,
    replacement: impl Fn(&Captures<'_>, &str) -> String,
) -> String {
    let lowered = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for caps in pattern.captures_iter(&lowered) {
        let whole = caps.get(0).expect("group 0 always participates");
        out.push_str(&text[last..whole.start()]);
        out.push_str(&replacement(&caps, text));
        last = whole.end();
    }
    out.push_str(&text[last..]);
    out
}

/// The text of group `index` in the original `text`, or the empty string when the group did not match.
fn original<'t>(text: &'t str, caps: &Captures<'_>, index: usize) -> &'t str {
    caps.get(index).map_or("", |found| &text[found.range()])
}

/// The longest prefix of at most `max_units` UTF-16 units that ends on a character boundary.
fn truncate_utf16(text: &str, max_units: usize) -> String {
    // A UTF-8 character is never fewer bytes than UTF-16 units, so a shorter text needs no scan.
    if text.len() <= max_units {
        return text.to_string();
    }
    let mut units = 0;
    for (index, character) in text.char_indices() {
        units += character.len_utf16();
        if units > max_units {
            return text[..index].to_string();
        }
    }
    text.to_string()
}
