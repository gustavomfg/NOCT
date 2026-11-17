//! The system calls of the rollback worker, one module per supported system. Each module exports the same items, which
//! `operation.rs` uses and nothing else. The names follow the C++ worker's helpers:
//!
//! - `Object`: an owned handle of an open file or directory, held for the life of an operation.
//! - `acquire_root`, `walk`, `child`, `create_new`: open a root, a directory beneath it, an entry beneath a directory,
//!   or a new private file. None of them follows a link, and `walk` and `child` refuse a crossing into another volume.
//! - `identity`, `mode_of`, `read_all`, `write_all`, `rewind_truncate`: the metadata and bytes that bind an entry.
//! - `flush`, `flush_directory`: the durability acknowledgments the protocol waits for.
//! - `check_filesystem`: the local-filesystem gate of `INIT`.
//! - `split_relative`, `validate_retention`: the name rules of `OPEN` and `DISPLACE`.
//! - `stage`, `produced_mode`, `publish`, `displace`: the publication steps.
//! - `refused`, `absence`, `check_stage_mode`: the refusals whose message or outcome differs between the systems.
//!
//! Each module also carries the system's own refusal messages, so that the protocol answers as the matching C++ backend
//! does. The state machine in `operation.rs` is the same for all of them.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;
