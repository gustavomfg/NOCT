//! Wire-level helpers shared by every command. Mirrors the C++ worker's framing so
//! the TypeScript client sees identical bytes.

/// Largest single rollback file, matching the C++ `MAX_BYTES` budget.
pub const MAX_BYTES: usize = 32 * 1024 * 1024;

/// Refusal carried back to the client as `ERR <code> <hex(message)>`.
#[derive(Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: &'static str,
    pub message: &'static str,
}

impl Failure {
    pub const fn new(code: &'static str, message: &'static str) -> Self {
        Self { code, message }
    }
}

pub type Outcome<T> = Result<T, Failure>;

pub fn require(condition: bool, message: &'static str) -> Outcome<()> {
    if condition {
        Ok(())
    } else {
        Err(Failure::new("CONFLICT", message))
    }
}

fn nibble(digit: u8) -> Outcome<u8> {
    match digit {
        b'0'..=b'9' => Ok(digit - b'0'),
        b'a'..=b'f' => Ok(digit - b'a' + 10),
        _ => Err(Failure::new("CONFLICT", "Invalid hex encoding")),
    }
}

pub fn unhex(text: &[u8]) -> Outcome<Vec<u8>> {
    require(
        text.len().is_multiple_of(2) && text.len() <= MAX_BYTES * 2,
        "Invalid frame",
    )?;
    text.chunks_exact(2)
        .map(|pair| -> Outcome<u8> { Ok((nibble(pair[0])? << 4) | nibble(pair[1])?) })
        .collect()
}

pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    out
}

/// Splits a tab-separated frame with the same field semantics as the C++
/// `std::getline(..., '\t')` loop: an empty frame has no fields, and a single
/// trailing tab does not produce an extra empty field.
pub fn split_frame(line: &[u8]) -> Vec<&[u8]> {
    if line.is_empty() {
        return Vec::new();
    }
    let mut fields: Vec<&[u8]> = line.split(|&byte| byte == b'\t').collect();
    if line.last() == Some(&b'\t') {
        fields.pop();
    }
    fields
}
