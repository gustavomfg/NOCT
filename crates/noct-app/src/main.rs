//! NOCT desktop shell binary. The shell itself lives in the library (`lib.rs`).

// Keep release builds from opening a console window on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    noct_app::run();
}
