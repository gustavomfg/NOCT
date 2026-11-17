//! Operation-scoped rollback worker: Rust spike.
//!
//! Speaks the same line protocol as `native/rollback-boundary.cpp`, consumed by
//! `electron/change-control/NativeRollbackOperation.ts`. Only the Linux backend is
//! ported. On macOS and Windows this worker answers every command with `UNSUPPORTED`
//! because the port is missing: the C++ worker has backends there (its `__APPLE__`
//! and `_WIN32` branches). The `#else` branch of the C++ worker is the fail-closed
//! answer for every other platform. See `docs/rust-native-rollback-backends.md`.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod protocol;

#[cfg(target_os = "linux")]
fn main() {
    linux::run();
}

#[cfg(not(target_os = "linux"))]
fn main() {
    use std::io::{BufRead, Write};

    // hex("Protected rollback backend not verified")
    const MESSAGE: &str =
        "50726f74656374656420726f6c6c6261636b206261636b656e64206e6f74207665726966696564";
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line == "CLOSE" {
            break;
        }
        if writeln!(stdout, "ERR\tUNSUPPORTED\t{MESSAGE}")
            .and_then(|_| stdout.flush())
            .is_err()
        {
            break;
        }
    }
}
