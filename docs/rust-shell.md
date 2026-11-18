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

**`--from-fixture <release tag>`** starts the shell from the synthetic database of a pinned release line, for example
`v1.0.2`, which is the line of `scripts/fixtures/release-lines.json`. The database is the one that `scripts/release-lines.mjs`
writes for the row gate: the TypeScript sources of that tag are compiled and seed one workspace, `fixture-project`, marked
as a favourite, with a conversation. The launcher copies the database into a new folder under `target/shell-fixtures/`,
and the shell opens the copy. The fixture and every real data folder are left untouched. The launcher prints the folder:

```sh
node scripts/run-rust-shell.mjs release --from-fixture v1.0.2
# fixture: v1.0.2; the shell opens a copy in /…/noct-rust-spike/target/shell-fixtures/v1.0.2-XXXXXX
```

- Each run makes a new folder, which stays until it is deleted. `target/shell-fixtures/` is the place to clear.
- The fixture is built from the tags of the repository, so the clone needs them (`git fetch --tags`), and from the
  Node dependencies (`npm ci`). `--check` builds and copies it too, and then starts nothing.
- `--from-fixture` and `--data-dir` cannot be used together.
- The workspace of the fixture points into `target/shell-fixtures/<tag>/source/`. If that folder is deleted, the
  workspace is listed as missing.
- The workspace is listed as authorized, as the release database marks it. A workspace restored from a backup is not
  authorized until it is chosen again; a fixture is not a backup.

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
| The layout in Chromium at the minimum window size (720x600) and 1440x920, both themes, keyboard focus, reduced motion, reduced transparency, glass on and off | Verified in Chromium with the shipped policy (`scripts/check-shell-ui.mjs`, run in CI on Linux) |
| The sidebar lists the projects of `workspaces_list`, with its loading, empty, error and retry states | Verified in Chromium with a mocked shell. **Not verified** with real workspaces in a real window |
| The minimum window size, `MIN_WINDOW_WIDTH` x `MIN_WINDOW_HEIGHT` (720x600) | Set in `window.rs` and tested. **Not verified** that the window manager enforces it in a real window |
| Content-Security-Policy enforced by a webview; the WebAssembly handler runs under it | Verified in Chromium (`scripts/check-shell-csp.mjs`, run in CI on Linux). **Not verified in a webview** |
| The window is opaque in the colour of the Electron window | Verified by a test of the constant (`crates/noct-app/tests/window.rs`). A thin border with the desktop in the margins remains after the change; it is treated as the desktop's window decoration (see [Window appearance](#window-appearance)) |
| The mock follows the Tauri version named in Cargo.lock | Verified by a test (`crates/noct-app/tests/shell_ui_mock.rs`) that fails on a mismatch |
| The glass layer is on in the committed page | Verified by a test (`crates/noct-app/tests/shell_policy.rs`) |
| The glass layer costs nothing measurable in the real window | **Not measured.** See [The glass layer](#the-glass-layer) |
| The inline-style guard | Verified by a test of the sources and of the page (`crates/noct-app/tests/shell_policy.rs`), and by the DOM check of every browser scenario |
| Navigation policy in a running webview | **Not verified** |
| Commands other than `settings_get` and `workspaces_list` called from a window | **Not verified** |
| WebView2 (Windows) | **Not verified** |
| WKWebView (macOS) | **Not verified** |
| The release-line fixture (`--from-fixture v1.0.2`) opens in the shell's store and lists its workspace | Verified on Linux with a probe that was not committed: the shell opened a copy and `workspaces_list` returned `fixture-project`, favourite and available. The copy is made by `scripts/run-rust-shell.test.mjs`. **Not verified** in a window |
| A data folder outside the user's profile is warned in the log on Windows | Verified by tests (`crates/noct-app/tests/bootstrap.rs`) on Linux, and the Windows job runs them. **Not verified** with a real profile on a Windows machine |

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

- **Sidebar:** the brand, the theme button (a moon when the theme is light, a sun when it is dark), the projects
  the shell lists (`workspaces_list`), a card for the model and a card for the key vault, and Ajustes at the bottom.
  A favourite project shows a tag. A project that cannot be used as it is shows a note under its name: the shell's
  message when it has one, otherwise the reason. A project not authorized in this session asks to be chosen again.
  The list has its own loading, empty, error and retry states. A reply outside the contract is an error, not a
  shorter list. Conversations are not listed: see [Conversations](#conversations).
- **Modes:** Revisar, Construir and Documentar in a segmented control (`aria-pressed`). A mode leaves Ajustes.
- **Question box:** fixed at the bottom of the window, with a label. It says that questions are not answered yet.
- **Ajustes:** the settings the shell returns (`settings_get`), with the loading state, the shell's error and
  "Tentar novamente". Idioma, Segurança do agente and Diagnóstico are selects. Each change is saved alone with
  `settings_set` (`{ input: { language } }`, `{ input: { approvalPolicy } }`, `{ input: { diagnosticMode } }`), and the
  area shows the settings the shell replies with. While the save runs it says so; a refused save says so with the
  shell's message, and the select goes back to the saved value. The Tema row shows the saved theme. The theme changes
  with the sidebar button, which changes it on screen and saves it (`{ input: { theme } }`); a theme the shell refuses
  is shown beside that button.
- **Theme:** the saved setting (`settings_get`). A setting never chosen follows the system. The attribute
  `data-theme` on `<html>` selects the colours.
- **Example data:** the models, the vault state and the findings are EXAMPLE values in `crates/noct-ui/src/examples.rs`.
  The window says so. Construir and Documentar are not available yet: they say so and show no example command, diff,
  approval or confirmation, so no control in the window runs a command or writes a file. The projects are not example data: they come from
  `workspaces_list`.

**Verified in Chromium** (`node scripts/check-shell-ui.mjs`, run in CI on Linux, against the Trunk build under the
shipped policy): the seventeen scenarios pass. The mock answers `settings_get` and `workspaces_list` separately, and
the check requires that the interface calls only those two, each with an empty payload, and `settings_set` with one
setting and nothing else for each change: the theme button, or a select in Ajustes. The scenarios cover the
settings as ready, loading then ready, error then retry, a reply outside the contract, a page outside the shell, the
theme from the saved setting and from the system, the sidebar theme button, the saves of each setting in Ajustes (each one alone, the reply shown, a refused one says so and goes back), the modes and Ajustes with their pressed
and current states, Construir and Documentar as not available yet with no approval, revert or confirmation control, the question box, the projects (order, favourite tag, notes, the loading, empty, error and retry
states, a reply outside the contract, and no example name), the keyboard focus, a hovered card that does not move, the
layout at the minimum window size and at 1440x920 without horizontal scroll, the glass switch, reduced motion, and
reduced transparency (which this Chromium reports, so it is checked and not only noted). No inline style is set and
the policy reports no violation.

The check fails on a regression of the hover: with a `translateY` added to the hover of the published page, the hover
scenarios fail in both themes.

**Not verified:** any of this in a real webview; the colours and the glass against the captures of the design,
because the captures that `design.md` names (`docs/design/painel-*.png`) are not in the repository.

**Style:** classes only. The policy grants no inline style. The interface loads no stylesheet of the React
application: `shell.css` has its own tokens, taken from the design. The Geist fonts vendored under
`crates/noct-ui/assets` are no longer copied by the build; the interface names Inter and falls back to the
system face.

## Conversations

The shell has `conversations_list`, a paginated command, and the interface does not call it. The design the shell
follows does not give conversations a place in the sidebar, and a conversation list would be a new screen, which is
a product decision. It waits for the owner. When it is added it must use the paginated reply and keep the loading,
empty and error states of the projects.

## Vendored fonts and their licence

`crates/noct-ui/assets/@fontsource-variable/geist` and `crates/noct-ui/assets/@fontsource-variable/geist-mono` hold
the Geist and Geist Mono variable fonts as WOFF2 files, vendored from the `@fontsource-variable` npm packages. Each
folder has its `LICENSE` next to the files. It is the SIL Open Font License, Version 1.1, with the copyright line
"Copyright 2024 The Geist Project Authors (https://github.com/vercel/geist-font)".

Checked on this machine: both `LICENSE` files are byte-identical to those of the installed packages, and every WOFF2
file is byte-identical to the file of the same name in its package. Nothing in the interface refers to these files
(`shell.css`, `index.html` and `crates/noct-ui/src` do not name Geist), so they reach no window today. The licence
travels with them if they are used.

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

## Minimum size

The window cannot be made smaller than `MIN_WINDOW_WIDTH` x `MIN_WINDOW_HEIGHT` in `crates/noct-app/src/window.rs`,
which are 720 x 600: the minimum of the Electron window (`electron/main.ts`). `crates/noct-app/tests/window.rs` fixes
the two values, and `scripts/check-shell-ui.mjs` reads them and lays the interface out at that size.

Measured in Chromium with the mock, in a temporary sweep that is not in the repository: at 560 px wide the mode control
is cut, and at 600 px it fits. At 720 px the content pane is 430 px wide. The question box stays on screen down to
480 px high. The width is the limit that matters; the height of 600 px is the Electron minimum and leaves room. This
is a Chromium measurement. Whether the window manager enforces the minimum, and the layout in WebKitGTK, WebView2 and
WKWebView, are not verified.

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
- The launcher tests (`node --test scripts/run-rust-shell.test.mjs`), which build the v1.0.2 fixture from its tag.

**Covered by other workflows:**

- The operating system keyring round trip, on the three systems (`.github/workflows/keyring-round-trip.yml`).
- The row gate (`node scripts/verify-row-gate.mjs`), on Ubuntu, in `.github/workflows/row-gate.yml`. It is not a required
  check: making it required on `main` is a setting of the repository, and the owner makes it.

**Not covered:**

- No window is opened on any system. The window is checked by hand on Linux, and not on Windows or macOS.
- WebView2 (Windows) and WKWebView (macOS) are not exercised by any job.
- The release shell is compiled on Windows and macOS, but it is not run there.
- The row gate is run on Ubuntu only, by `row-gate.yml`, and the shell matrix does not run it on Windows or macOS. The
  design of the job is in [the row gate workflow](rust-row-gate-workflow.md).
- The Windows restrictive file permissions: the POSIX modes of the database, the logs and the atomic writes apply on
  Linux and macOS only, as the tests in `crates/noct-db` gate them. On Windows the Rust code applies no ACL, so
  Windows gives new files the access rules of their folder. The TypeScript application has the same limit: its
  `chmod` calls (`electron/persistence/AtomicFile.ts`, `electron/security/ProviderCredentialVault.ts`) only change the
  read-only flag on Windows. On Windows, a data folder outside the user's profile is reported in `nocturne.log` (category
  `bootstrap.data-directory`), and the shell still opens it. The folder's ACL is not set: that needs the `windows-sys`
  dependency, which the owner has not approved yet. The analysis and the decisions are in
  [the Windows file permissions analysis](rust-windows-file-permissions.md).
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

## Updater (prepared, not configured)

- The plugin is `tauri-plugin-updater` 2.14.1, the official Tauri plugin, pinned exactly in the workspace `Cargo.toml`.
- The configuration is `plugins.updater` in `crates/noct-app/tauri.conf.json`. It ships empty: `pubkey` is `""`, `endpoints`
  is `[]`, and `requireSignedVersion` is `true`.
- The shell registers the plugin only when that section is complete and safe (`crates/noct-app/src/updater.rs`): a public
  key; at least one https endpoint; `requireSignedVersion` on; and none of `dangerousInsecureTransportProtocol`,
  `dangerousAcceptInvalidCerts`, `dangerousAcceptInvalidHostnames` or `allowDowngrades` set. A partial, insecure or
  malformed section stops the start, because it is a defect of the build, and the tests read the shipped section.
- With the shipped section, the shell makes no update request and reads no update feed. The window is granted no updater
  permission (`crates/noct-app/capabilities/default.json`, locked by `tests/capabilities.rs`).
- Not done, and kept with the owner: the signing key pair (the owner keeps custody, and nothing in the repository creates or
  holds one), the update feed and where it is hosted, and any release signing step. No key, token or secret is committed.
- To enable it, the owner fills `pubkey` and `endpoints` in a reviewed change marked `[security-review]`. The code that
  checks for and installs an update is not written yet, and it needs its own review.

### Where `requireSignedVersion` is defined

- The field is `Config::require_signed_version` in `tauri-plugin-updater` 2.14.1, file `src/config.rs`. Its doc comment says that
  the update signature must carry the version it was signed for, and that the update is rejected when that version differs from
  the one the endpoint announces. The endpoint response is fetched over TLS but is not itself signed, and the signature covers
  only the downloaded artifact. The default is `false`.
- The signed version is read from the signature's trusted comment. Releases signed before the Tauri CLI recorded it carry no
  version, so enabling the setting rejects them. Re-sign and re-publish every release that users can still update from before
  enabling it. No release exists yet.
- The public updater guide, https://v2.tauri.app/plugin/updater/ (footer "Last updated: Nov 28, 2025"), was read on 2026-11-17.
  It does not mention the setting. For the pinned version, the plugin source is the only definition found.
- NOCT sets the value to `true`. That is a decision for the owner to confirm.

## Build provenance (dry run)

- `.github/workflows/build-provenance-dry-run.yml` attests one small subject file, written by the job from the commit and the
  run. It runs on a push to `migration/rust` that changes the workflow, and on demand. It publishes nothing: no release, no
  asset, no package, no branch change.
- The only action it uses is `actions/attest-build-provenance`, pinned to the commit of `v4.2.2`
  (`4d101475d8b20a2381f78447822ac1eab6504dd8`). No other action, no repository secret. The attestation is signed with the
  OpenID Connect token of the job, which needs `id-token: write` and `attestations: write`, and no stored credential.
- The attestation is recorded in GitHub's attestation store and in the public Sigstore transparency log, as the action does.
  The subject holds only the commit SHA and the run id, which are public for this repository.
- The Electron release workflows are not changed. A real release would attest its built files, which needs its own review.

