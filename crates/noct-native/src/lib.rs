//! Operation-scoped rollback worker: Rust port of `native/rollback-boundary.cpp`.
//!
//! The worker speaks the line protocol that `electron/change-control/NativeRollbackOperation.ts` uses. The protocol
//! state machine is in `operation`, the same on every system. The system calls are in `platform`, one module per
//! supported system. A system with no module answers every line with `UNSUPPORTED`, as the C++ worker's fallback does.
//! See `docs/rust-native-rollback-backends.md`.

pub mod protocol;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
mod operation;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
mod platform;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub use operation::{Operation, run};

/// The answer of a system with no backend: every line gets `UNSUPPORTED` until `CLOSE`.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn run() {
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
