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
| **Dev mode**: the window opens and shows the settings area with the default values, on Linux with WebKitGTK | **Verified by hand.** Command in [Manual check](#manual-check-linux-webkitgtk) |
| **Release mode** (embedded interface): the window opens and shows the same settings area, on Linux with WebKitGTK | **Verified by hand.** Command in [Manual check](#manual-check-linux-webkitgtk) |
| `settings_get` end to end on Linux, in both modes: interface, `invoke`, access control list, command, store, contract, screen | **Verified by hand**, in a real webview, with the values of a scratch data folder |
| The window is opaque in the colour of the Electron window | Verified by a test of the constant (`crates/noct-app/tests/window.rs`). A thin border with the desktop in the margins remains after the change; it is treated as the desktop's window decoration (see [Window appearance](#window-appearance)) |
| The settings area with a mocked shell, under the shipped policy, in Chromium | Verified in CI (`scripts/check-shell-ui.mjs`) and by the contract cases in `cargo test -p noct-ui` |
| The mock follows the Tauri version named in Cargo.lock | Verified by a test (`crates/noct-app/tests/shell_ui_mock.rs`) that fails on a mismatch |
| Navigation policy in a running webview | **Not verified** |
| Content-Security-Policy enforced by a webview | Verified in Chromium only (`scripts/check-shell-csp.mjs`, `scripts/check-shell-ui.mjs`). No policy violation was recorded in the manual check, but no console log was taken either |
| Commands other than `settings_get` called from a window | **Not verified** |
| WebView2 (Windows) | **Not verified** |
| WKWebView (macOS) | **Not verified** |
| Release shell with `custom-protocol` built and run on Windows and macOS | **Not verified, and not in CI** (see [Gaps in CI](#gaps-in-ci)) |

## Manual check, Linux (WebKitGTK)

The check was done by hand on Linux with WebKitGTK, with the scratch data folder `/tmp/noct-teste`. Both
modes were run:

| Mode | Exact command | Result |
| --- | --- | --- |
| dev | `node scripts/run-rust-shell.mjs dev --data-dir /tmp/noct-teste` | The window opened and showed the settings area with the default values |
| release | `node scripts/run-rust-shell.mjs release --data-dir /tmp/noct-teste` | The window opened with the embedded assets and showed the same settings area as dev |

The values shown in both modes were: Tema "NOCT escuro", Idioma "Português (Brasil)", Segurança do agente
"Comandos sempre exigem aprovação", Diagnóstico "Inativo", and Pandoc "Não informado". The earlier check, whose
mode was not recorded, saw the skeleton: "NOCT", "0 toques", the "Tocar" button and "0 caracteres".

This covers one chain, end to end, in a real webview: the interface calls `settings_get` through `invoke`, the
access control list allows it, the command reads the store, the reply matches the contract, and the screen shows
it. It covers Linux only, with WebKitGTK only.

## Window appearance

On Linux the window first showed rounded corners and the desktop in its margins. That was not intended. The shell
asks for no transparency and no custom frame. tao gives the window an alpha channel only when `transparent`
is set, and wry clears the WebKit background only in that case (`wry-0.57.2/src/webkitgtk/mod.rs`). The
Electron window is opaque, with `backgroundColor: '#0b0b0e'` (`electron/main.ts`).

The main window now sets `transparent(false)` and `background_color` to `#0b0b0e`, the Electron colour
(`crates/noct-app/src/window.rs`, `WINDOW_BACKGROUND`). After the change, a thin border with the desktop in the
margins and rounded corners remains, much smaller than before. The person who checked treats it as the
desktop's window decoration, not the application's. The shell does not draw that frame and does not ask for it,
so no change is made here. Removing the decoration would change the title bar, which is a separate decision.

## The settings area

The interface reads its settings when the page opens (`crates/noct-ui/src/app.rs`). It calls
`settings_get` through `window.__TAURI_INTERNALS__.invoke`, which the shell's webview defines for every page
(`crates/noct-ui/src/ipc.rs`). The page needs neither the global Tauri object nor an npm package. The reply
is checked against the contract of `noct-core` (`crates/noct-core/tests/fixtures/ipc/settings-get.json`): a
reply that does not match is an error, not a default. The area shows a loading state, the settings, or the
shell's error with "Tentar novamente".

- **Verified by hand on Linux, in both modes:** the values above, from `settings_get` in a real webview.
- **Verified in Chromium with a mocked shell** under the shipped policy (`node scripts/check-shell-ui.mjs`,
  run in CI): ready, loading then ready, error then retry, a reply outside the contract, and a page outside
  the shell. The check also fails when an inline style is added (tested by inserting one).
- **Not verified:** the same call on Windows (WebView2) and macOS (WKWebView), and any command other than
  `settings_get`.
- **The mock follows Tauri 2.12.5.** `scripts/check-shell-ui.mjs` declares `MOCKED_TAURI_VERSION`, and
  `crates/noct-app/tests/shell_ui_mock.rs` fails when Cargo.lock names another version of Tauri. Before
  updating the constant, check the mock's `invoke` against the new `scripts/core.js` of that version.
- **Style:** classes only. The policy grants no inline style, so `crates/noct-ui/shell.css` uses the Nocturne
  tokens and the existing `error-card` and `settings-section` classes.

## Gaps in CI

These are not changed by this document. The workflow is not modified here.

- `.github/workflows/rust-shell-validation.yml` runs on `ubuntu-latest` only. No job builds or runs the shell on
  Windows or macOS.
- No job compiles the release shell with `custom-protocol` (`cargo build -p noct-app --release --features
  custom-protocol`), on any system. The workflow builds the interface in release (`trunk build --release`) and
  the shell in debug (`cargo build -p noct-app`, `cargo test --workspace`).
- The release check above, by hand, is not in CI.

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
