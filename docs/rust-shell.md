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
| The debug window opens and shows the interface | **Not verified.** The window has not been opened in this environment |
| The release window loads the embedded interface | **Not verified** in a running window |
| Navigation policy in a running webview | **Not verified** in a running webview |
| Content-Security-Policy enforced by a webview | Verified in Chromium only (`scripts/check-shell-csp.mjs`). WebKitGTK, WebView2 and WKWebView are not verified |
| Windows and macOS | Not verified |
| The interface calls the commands | Not yet. The commands are verified through Tauri's mock runtime |

The window check is the one that matters for a user. Run `node scripts/run-rust-shell.mjs dev`, confirm the
skeleton appears, and report what you see.

## Known approximations

These are recorded in code and in the ADR as well. The workspace folder rules follow `WorkspaceTrust.ts`.

- **Access check.** `WorkspaceTrust.ts` asks the operating system with `access(R_OK | X_OK)`. The standard
  library has no such call and the shell adds no native dependency for it, so the Rust check lists the
  folder (`read_dir`) and looks up its `.` entry (`metadata`). The two agree for the owner of a folder,
  which is the case that matters for a workspace. A permission that allows listing but not searching is
  refused in both.
- **Home folder.** The blocked roots include the home folder. The shell reads `HOME` on Linux and macOS and
  `USERPROFILE` on Windows. Node's `os.homedir()` on Unix also falls back to the password database, which the
  shell does not read. If `HOME` is unset on Unix, the two can disagree about the home folder.
