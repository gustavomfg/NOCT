//! Text rules shared by the repositories. They follow JavaScript's string semantics,
//! because the TypeScript application stored data with them.

/// Replaces every run of JavaScript `\s` characters with one space.
pub fn collapse_js_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_run = false;
    for c in text.chars() {
        if is_js_whitespace(c) {
            if !in_run {
                out.push(' ');
                in_run = true;
            }
        } else {
            out.push(c);
            in_run = false;
        }
    }
    out
}

/// JavaScript `\s` as the body of a regex character class, matching [`is_js_whitespace`]. Rust's `\s`
/// would match U+0085 and miss U+FEFF, so a pattern that follows JavaScript spells the class out.
pub const JS_WHITESPACE_CLASS: &str = r"\t\n\x{0B}\x{0C}\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}";

/// JavaScript's `\s` and `String#trim` set. It differs from Rust's `char::is_whitespace`:
/// it includes U+FEFF and omits U+0085.
pub fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0b}' | '\u{0c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// The first `units` UTF-16 code units, as `String#slice(0, units)` returns them. A
/// split surrogate pair cannot be represented in a Rust string and becomes U+FFFD.
pub fn utf16_prefix(text: &str, units: usize) -> String {
    let prefix: Vec<u16> = text.encode_utf16().take(units).collect();
    String::from_utf16_lossy(&prefix)
}

/// `String#trim`: removes the JavaScript whitespace set from both ends.
pub fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_whitespace)
}

/// Length in UTF-16 code units, which is what `string.length` measures in JavaScript and
/// what every persistence limit counts.
pub fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// A literal that matches its ASCII letters in either case and nothing else, as JavaScript's `/i` without `u` does.
pub fn ascii_insensitive(literal: &str) -> String {
    literal
        .chars()
        .map(|c| {
            if c.is_ascii_alphabetic() {
                format!("[{}{}]", c.to_ascii_lowercase(), c.to_ascii_uppercase())
            } else {
                regex::escape(&c.to_string())
            }
        })
        .collect()
}
