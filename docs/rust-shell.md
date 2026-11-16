# Running the Rust shell locally

The Rust shell (`crates/noct-app`) runs the interface (`crates/noct-ui`, Leptos compiled to WebAssembly
with Trunk) inside a Tauri window. This page is the reference for running it, and for what has and has
not been checked. The decision behind the layout is in [ADR 0001](adr/0001-rust-migration.md), and the
design the interface follows is in `docs/New_Design/design.md`.

## Requirements

- Rust 1.97.1. `rust-toolchain.toml` pins it, so `rustup show` in the repository installs it.
- Trunk 0.22.1: `cargo install --locked trunk@0.22.1`. The version must match the workflow.
- Linux: the packages of `.github/workflows/rust-shell-validation.yml` (WebKitGTK 4.1, GTK 3 and the other
  Tauri dependencies). The window has been run by hand on Linux only.
- Node.js 24 runs the scripts. They use only the Node standard library, and the browser checks use the
  Playwright Chromium of the repository.

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
| Build of the interface and of the release shell with `custom-protocol` on Linux, Windows and macOS | Configured in CI (`shell` matrix); result in [Gaps in CI](#gaps-in-ci) |
| Dev port configuration (`crates/noct-app/tests/dev_server.rs`) | Verified by tests |
| `trunk serve --port 1420` answers with the page and `bootstrap.js` | Verified locally with HTTP requests, without opening the shell |
| The release binary embeds the interface | Verified locally: the wasm file name trunk wrote is in the binary of the `custom-protocol` build and not in the debug binary (CI runs the same check on three systems) |
| Test feature of Tauri stays out of the normal build | Verified: `cargo tree -p noct-app -e normal` lists Tauri without `test`; the release binary has no test mock |
| **Dev mode** of the settings area, on Linux with WebKitGTK | **Verified by hand.** Command in [Manual check](#manual-check-linux-webkitgtk) |
| **Release mode** of the settings area (embedded interface), on Linux with WebKitGTK | **Verified by hand.** Command in [Manual check](#manual-check-linux-webkitgtk) |
| `settings_get` end to end on Linux, in both modes: interface, `invoke`, access control list, command, store, contract, screen | **Verified by hand**, in a real webview, with the values of a scratch data folder |
| The new layout (sidebar, modes, Ajustes, question box, themes, glass) in a real window | **Not verified.** Checked in Chromium only (see [The interface](#the-interface)) |
| The layout in Chromium at 720x600 and 1440x920, both themes, keyboard focus, reduced motion, reduced transparency, glass on and off | Verified in Chromium with the shipped policy (`scripts/check-shell-ui.mjs`, run in CI on Linux) |
| Content-Security-Policy enforced by a webview; the WebAssembly handler runs under it | Verified in Chromium (`scripts/check-shell-csp.mjs`, run in CI on Linux). **Not verified in a webview** |
| The window is opaque in the colour of the Electron window | Verified by a test of the constant (`crates/noct-app/tests/window.rs`). A thin border with the desktop in the margins remains after the change; it is treated as the desktop's window decoration (see [Window appearance](#window-appearance)) |
| The mock follows the Tauri version named in Cargo.lock | Verified by a test (`crates/noct-app/tests/shell_ui_mock.rs`) that fails on a mismatch |
| The glass layer is on in the committed page | Verified by a test (`crates/noct-app/tests/shell_policy.rs`) |
| The glass layer costs nothing measurable in the real window | **Not measured.** See [The glass layer](#the-glass-layer) |
| The inline-style guard | Verified by a test of the sources and of the page (`crates/noct-app/tests/shell_policy.rs`), and by the DOM check of every browser scenario |
| Navigation policy in a running webview | **Not verified** |
| Commands other than `settings_get` called from a window | **Not verified** |
| WebView2 (Windows) | **Not verified** |
| WKWebView (macOS) | **Not verified** |

## Manual check, Linux (WebKitGTK)

The check was done by hand on Linux with WebKitGTK, with the scratch data folder `/tmp/noct-teste`. Both
modes were run. The window of that check showed the settings area of the earlier interface; the new layout of
[The interface](#the-interface) has not been run in a window yet.

| Mode | Exact command | Result |
| --- | --- | --- |
| dev | `node scripts/run-rust-shell.mjs dev --data-dir /tmp/noct-teste` | The window opened and showed the settings area with the default values |
| release | `node scripts/run-rust-shell.mjs release --data-dir /tmp/noct-teste` | The window opened with the embedded assets and showed the same settings area as dev |

The values shown in both modes were: Tema "NOCT escuro", Idioma "Português (Brasil)", Segurança do agente
"Comandos sempre exigem aprovação", Diagnóstico "Inativo", and Pandoc "Não informado".

This covers one chain, end to end, in a real webview: the interface calls `settings_get` through `invoke`, the
access control list allows it, the command reads the store, the reply matches the contract, and the screen shows
it. It covers Linux only, with WebKitGTK only.

## The interface

The window follows `docs/New_Design/design.md` and `docs/New_Design/mockup-vidro-painel.html`, which is the
reference and is not production code. The source of the window is `crates/noct-ui/src/app.rs`, its styles are
`crates/noct-ui/shell.css`, and the theme rules are in `crates/noct-ui/src/theme.rs`.

- **Sidebar:** the brand, the theme button (a moon when the theme is light, a sun when it is dark), the list of
  projects with the current one marked `aria-current`, a card for the model and a card for the key vault, and
  Ajustes at the bottom.
- **Modes:** Revisar, Construir and Documentar in a segmented control (`aria-pressed`). A mode leaves Ajustes.
- **Question box:** fixed at the bottom of the window, with a label. It says that questions are not answered yet.
- **Ajustes:** the settings the shell returns (`settings_get`), with the loading state, the shell's error and
  "Tentar novamente". The theme button changes the theme for the session only; the Tema row shows the saved
  setting.
- **Theme:** the saved setting (`settings_get`). A setting never chosen follows the system. The attribute
  `data-theme` on `<html>` selects the colours.
- **Example data:** the projects, the models, the vault state, the findings, the approval, the diffs and the
  document changes are EXAMPLE values in `crates/noct-ui/src/examples.rs`. The window says so, and no control in
  it runs a command or writes a file. The approval shows its result as an example.

**Verified in Chromium** (`node scripts/check-shell-ui.mjs`, run in CI on Linux, against the Trunk build under the
shipped policy): the ten scenarios pass. They cover the settings as ready, loading then ready, error then retry,
a reply outside the contract, a page outside the shell, the theme from the saved setting and from the system, the
sidebar theme button, the modes and Ajustes with their pressed and current states, the question box, the project
marks, the keyboard focus, the layout at 720x600 and 1440x920 without horizontal scroll, the glass switch, reduced
motion, and reduced transparency (which this Chromium reports, so it is checked and not only noted). No inline style
is set and the policy reports no violation.

**Not verified:** any of this in a real webview; the colours and the glass against the captures of the design,
because the captures that `design.md` names (`docs/design/painel-*.png`) are not in the repository.

**Style:** classes only. The policy grants no inline style. The interface loads no stylesheet of the React
application: `shell.css` has its own tokens, taken from the design. The Geist fonts vendored under
`crates/noct-ui/assets` are no longer copied by the build; the interface names Inter and falls back to the
system face.

## The glass layer

The glass is a layer of the page, on while `<html data-glass="on">`, which is the committed default in
`crates/noct-ui/index.html`. The window itself stays opaque (`WINDOW_BACKGROUND`). With `off`, or with
`prefers-reduced-transparency`, the panes take the solid colours of the tokens.

To compare the two in the real window, switch the attribute, start the shell again, and measure on the same
machine:

```sh
node scripts/set-glass.mjs off
node scripts/run-rust-shell.mjs dev --data-dir /tmp/noct-teste   # window with the glass off
# leave it open for 30 s idle, then 30 s moving the pointer over the cards and switching modes
pidstat -u -p "$(pgrep -f WebKitWebProcess | head -n 1)" 1 30   # CPU of the web process, sysstat
node scripts/set-glass.mjs on
node scripts/run-rust-shell.mjs dev --data-dir /tmp/noct-teste   # the same steps with the glass on
```

Record, for each setting: the CPU of the web process while idle and while moving, the GPU load if the machine
shows it (for example `intel_gpu_top` or `nvtop`), and whether resizing and switching modes is smooth. Restart the
dev run after each switch. For the release build, switch the attribute and run `node scripts/run-rust-shell.mjs
release`, which builds the interface again. Leave the page on `on` before committing: the Rust test
`the_window_starts_with_the_glass_layer_on` fails while it is `off`.

The heaviest rule is the blur of the content panel (34px, `shell.css`, `.content`), which covers the full window.
If it costs too much, the first candidate to reduce is that blur, and the panel's blur could be limited to the
sidebar and the top, as the design suggests.

**Not measured, and not verified in a real window:** the cost of the blur on WebKitGTK, WebView2 and WKWebView.
No number in this document comes from a measurement of the window.

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

## Gaps in CI

The workflow `.github/workflows/rust-shell-validation.yml` runs on pushes to `migration/rust`. Its `shell` job is a
matrix over `ubuntu-latest`, `windows-latest` and `macos-latest`, with `fail-fast: false`.

**Covered on all three systems.** The matrix run of `226d8fd` (run 43951649685) passed on Ubuntu, Windows and macOS.
On Windows and macOS nine steps ran and eleven Linux-only steps were skipped, as designed.

- The interface builds with Trunk in release mode.
- `cargo test --workspace --no-fail-fast` runs every test binary, without the ignored tests. The integration test
  executables on Windows carry the Common Controls v6 manifest that the application has: they build the application
  with `tauri::test`, which reaches `comctl32` exports, and tauri-build embeds the manifest only in binaries. The
  manifest is `crates/noct-app/windows/common-controls.manifest`, a copy of tauri-build's, linked by
  `crates/noct-app/build.rs`. Without it the Windows test executables stopped with STATUS_ENTRYPOINT_NOT_FOUND.
- The command access tests send their requests from the window's own page, so each platform's origin is the one the
  access control list checks: `tauri://localhost`, or `http://tauri.localhost` on Windows.
- The release shell builds with `--features custom-protocol`, and the step checks that the wasm file name trunk wrote
  is in the binary. The debug binary, built without the feature, is the control of that check.

**Covered on Linux only, on purpose:**

- The permission drift check (`crates/noct-app/permissions`), so that one system's output is compared with the
  committed files.
- The output differential against TypeScript, which needs the release-line bases.
- The Chromium checks: the Content-Security-Policy, the settings and the interface, the redaction fixture and the
  IPC samples. Node and Chromium run on Linux only in this workflow.

**Covered by other workflows:**

- The operating system keyring round trip, on the three systems (`.github/workflows/keyring-round-trip.yml`).

**Not covered:**

- No window is opened on any system. The window is checked by hand on Linux, and not on Windows or macOS.
- WebView2 (Windows) and WKWebView (macOS) are not exercised by any job.
- The release shell is compiled on Windows and macOS, but it is not run there.
- The row gate (`node scripts/verify-row-gate.mjs`) is not run by any workflow.
- The Windows restrictive file permissions: the POSIX modes of the database, the logs and the atomic writes apply on
  Linux and macOS only, as the tests in `crates/noct-db` gate them. On Windows the Rust code applies no ACL, so
  Windows gives new files the access rules of their folder. The TypeScript application has the same limit: its
  `chmod` calls (`electron/persistence/AtomicFile.ts`, `electron/security/ProviderCredentialVault.ts`) only change the
  read-only flag on Windows. Closing this needs an ACL decision, which is not part of this slice.
- Caching: each run builds the workspace from scratch, so the Windows and macOS jobs take most of their time to
  compile. A cache action would be a third-party action, which needs approval first.

## Known approximations

These are recorded in code and in the ADR as well. The workspace folder rules follow `WorkspaceTrust.ts`.

- **Test output on Windows.** Each mock window prints "failed to load the default window icon": the test executables
  carry no icon resource, which only the application binary gets from tauri-build. The message does not fail a test,
  and the tests do not check the icon.

- **Access check.** `WorkspaceTrust.ts` asks the operating system with `access(R_OK | X_OK)`. The standard
  library has no such call and the shell adds no native dependency for it, so the Rust check lists the
  folder (`read_dir`) and looks up its `.` entry (`metadata`). Both calls need the permissions that
  `access` checks, read and search. The tests in `crates/noct-app/tests/workspace_scope.rs` cover folders
  with no permission bits and read-only folders, as the owner. They do not cover ACLs, other users, or
  access granted through groups, where `access` and the two calls can still differ.
- **Home folder.** The blocked roots include the home folder. The shell reads `HOME` on Linux and macOS and
  `USERPROFILE` on Windows. Node's `os.homedir()` on Unix also falls back to the password database, which the
  shell does not read. If `HOME` is unset on Unix, the two can disagree about the home folder.
- **Mock fidelity.** The browser checks answer `invoke` with a mock of the shape that Tauri 2.12.5 defines. A change
  in Tauri is caught by the version test, not by the mock itself, so it reaches a real window only when someone runs
  one.
