//! Text rules shared by the repositories. They follow JavaScript's string semantics,
//! because the TypeScript application stored data with them.

/// Replaces every run of JavaScript `\s` characters with one space.
pub(crate) fn collapse_js_whitespace(text: &str) -> String {
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

/// JavaScript's `\s` and `String#trim` set. It differs from Rust's `char::is_whitespace`:
/// it includes U+FEFF and omits U+0085.
pub(crate) fn is_js_whitespace(c: char) -> bool {
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
pub(crate) fn utf16_prefix(text: &str, units: usize) -> String {
    let prefix: Vec<u16> = text.encode_utf16().take(units).collect();
    String::from_utf16_lossy(&prefix)
}
