# Running the Rust shell locally

The Rust shell (`crates/noct-app`) runs the interface (`crates/noct-ui`, Leptos compiled to WebAssembly
with Trunk) inside a Tauri window. This page is the reference for running it, and for what has and has
not been checked. The decision behind the layout is in [ADR 0001](adr/0001-rust-migration.md).

## Requirements

- Rust 1.97.1. `rust-toolchain.toml` pins it, so `rustup show` in the repository installs it.
- Trunk 0.22.1: `cargo install --locked trunk@0.22.1`. The version must match the workflow.
- The Linux packages of `.github/workflows/rust-shell-validation.yml` (WebKitGTK 4.1, GTK 3 and the
  other Tauri dependencies). Other platforms are not verified yet.
- Node.js 24 runs the script. The script uses only the Node standard library.

## Two modes

```sh
node scripts/run-rust-shell.mjs dev        # debug shell, interface served by trunk serve
node scripts/run-rust-shell.mjs release    # release shell, interface embedded in the binary
node scripts/run-rust-shell.mjs dev --check   # prints the plan and verifies the tools; starts nothing
```

**dev** starts `trunk serve --port 1420` in `crates/noct-ui`, waits until the page answers, and then runs
`cargo run -p noct-app`. The debug shell loads `build.devUrl` (`http://localhost:1420`). The Trunk server
stops when the shell exits. Running `cargo run` by hand does not start Trunk: the `beforeDevCommand` in
`tauri.conf.json` is run only by the Tauri CLI. Without a server on port 1420 the window cannot load its
interface.

**release** runs `trunk build --release`, which writes `crates/noct-ui/dist`, and then
`cargo run -p noct-app --release --features custom-protocol`. The feature makes the shell embed `dist` and
load it from the packaged origin. Without it a release build would load `build.devUrl` instead, and the
navigation policy refuses the dev server outside debug builds. Build the interface again after changing it,
because `dist` is embedded when the shell is compiled.

Both modes accept `--data-dir <absolute path>`, which sets `NOCT_DATA_DIR` for the shell. Use a scratch
folder when trying the shell. Without the flag the shell uses the application data folder of its
identifier (`com.nocturne.codex`), for example `~/.local/share/com.nocturne.codex` on Linux. That folder
is not the one the Electron application uses (`Nocturne Studio`), so the shell does not open an existing
installation's database by default.

## What has been verified

| Check | Status |
| --- | --- |
| Unit and integration tests (`cargo test --workspace`) | Verified locally and in CI |
| Dev port configuration (`crates/noct-app/tests/dev_server.rs`) | Verified by tests |
| `trunk serve --port 1420` answers with the page and `bootstrap.js` | Verified locally with HTTP requests, without opening the shell |
| `trunk build --release` and the release shell with `custom-protocol` compile | Verified locally; the release binary contains the embedded interface |
| Test feature of Tauri stays out of the normal build | Verified: `cargo tree -p noct-app -e normal` lists Tauri without `test`; the release binary has no test mock |
| **The window opens and shows the skeleton** (NOCT, "0 toques", "Tocar", "0 caracteres") | **Verified by hand on Linux with WebKitGTK.** Mode and exact command: see [Manual check](#manual-check-linux-webkitgtk) |
| The window is opaque in the colour of the Electron window | Verified by a test of the constant (`crates/noct-app/tests/window.rs`). **Its appearance after the change is not verified** (see [Window appearance](#window-appearance)) |
| The settings area reads `settings_get` | Verified in Chromium with a mocked shell (`scripts/check-shell-ui.mjs`, in CI) and by the contract cases in `cargo test -p noct-ui`. **Not verified in a running webview** |
| Navigation policy in a running webview | **Not verified** in a running webview |
| Content-Security-Policy enforced by a webview | Verified in Chromium only (`scripts/check-shell-csp.mjs`, `scripts/check-shell-ui.mjs`). WebKitGTK, WebView2 and WKWebView are not verified for policy violations |
| The real IPC call from the page to the shell in a running window | **Not verified.** Tauri's mock runtime covers the commands; no window has called them yet |
| The release window (embedded interface) in a running window | **Not verified** |
| Windows (WebView2) and macOS (WKWebView) | **Not verified** |

## Manual check, Linux (WebKitGTK)

On Linux with WebKitGTK, the window opened and showed the skeleton: "NOCT", "0 toques", the "Tocar" button and
"0 caracteres". The record does not say which mode ran (`dev`, `release`, or both), and it does not give the
exact command. Until that is recorded, only this sentence is verified, for the mode that was run:

- `node scripts/run-rust-shell.mjs dev` (debug shell, `trunk serve` on port 1420)
- `node scripts/run-rust-shell.mjs release` (release shell, embedded interface)

Add `--data-dir <absolute path to a scratch folder>` when the data folder must not be the default one.

## Window appearance

On Linux the window showed rounded corners and the desktop in its margins. That was not intended. The shell
asks for no transparency and no custom frame. tao gives the window an alpha channel only when `transparent`
is set, and wry clears the WebKit background only in that case (`wry-0.57.2/src/webkitgtk/mod.rs`). The
Electron window is opaque, with `backgroundColor: '#0b0b0e'` (`electron/main.ts`).

The main window now sets `transparent(false)` and `background_color` to `#0b0b0e`, the Electron colour
(`crates/noct-app/src/window.rs`, `WINDOW_BACKGROUND`). **The appearance after this change is not verified.**
If the rounded corners or the margins remain, the cause is the window decoration of the desktop (the GTK
frame or the compositor), not the shell's background. Removing that decoration changes the title bar, so it
is a separate decision.

## The settings area

The interface reads its settings when the page opens (`crates/noct-ui/src/app.rs`). It calls
`settings_get` through `window.__TAURI_INTERNALS__.invoke`, which the shell's webview defines for every page
(`crates/noct-ui/src/ipc.rs`). The page needs neither the global Tauri object nor an npm package. The reply
is checked against the contract of `noct-core` (`crates/noct-core/tests/fixtures/ipc/settings-get.json`): a
reply that does not match is an error, not a default. The area shows a loading state, the settings, or the
shell's error with "Tentar novamente".

- **Verified:** the contract cases in Rust (`cargo test -p noct-ui`), and five browser scenarios with a
  mocked shell under the shipped policy (`node scripts/check-shell-ui.mjs`, run in CI): ready, loading
  then ready, error then retry, a reply outside the contract, and a page outside the shell. The check also
  fails when an inline style is added (tested by inserting one).
- **Not verified:** the call through the shell's real IPC in a running webview. The mock reproduces the
  shape of `__TAURI_INTERNALS__.invoke` as Tauri 2.12.5 defines it; a change in Tauri would not be caught by
  the mock, only by the window.
- **Style:** classes only. The policy grants no inline style, so `crates/noct-ui/shell.css` uses the Nocturne
  tokens and the existing `error-card` and `settings-section` classes.

## Known approximations

These are recorded in code and in the ADR as well. The workspace folder rules follow `WorkspaceTrust.ts`.

- **Access check.** `WorkspaceTrust.ts` asks the operating system with `access(R_OK | X_OK)`. The standard
  library has no such call and the shell adds no native dependency for it, so the Rust check lists the
  folder (`read_dir`) and looks up its `.` entry (`metadata`). Both calls need the permissions that
  `access` checks, read and search. The tests in `crates/noct-app/tests/workspace_scope.rs` cover folders
  with no permission bits and read-only folders, as the owner. They do not cover ACLs, other users, or
  access granted through groups, where `access` and the two calls can still differ.
- **Home folder.** The blocked roots include the home folder. The shell reads `HOME` on Linux and macOS and
  `USERPROFILE` on Windows. Node's `os.homedir()` on Unix also falls back to the password database, which the
  shell does not read. If `HOME` is unset on Unix, the two can disagree about the home folder.
