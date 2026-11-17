# Compatibility

[Português do Brasil](compatibility.pt-BR.md)

## Official application targets

| Platform | Release artifact |
| --- | --- |
| Windows 10/11 | x64 NSIS installer |
| Linux desktop | AppImage or `tar.gz` for the published build architecture |
| macOS | DMG and updater ZIP for the published build architecture |

Only artifacts published by the automated stable-release workflow after its
validation gates are official releases. Local packages and builds for another architecture are
development artifacts until they pass the same smoke and release gates.

## Development compatibility

- Node.js `>=24.18 <25`;
- npm `>=11 <12`;
- Electron 43.x;
- `better-sqlite3` 13.x with N-API, verified under the adopted Electron runtime
  and the development Node.js host.

Use `npm run test:abi` to check the packaged runtime's native module load before
running the full suite. N-API compatibility does not replace the packaged smoke.

Protected filesystem rollback has a narrower platform contract than application
startup: [V1](native-rollback-v1.md) supports local Linux tmpfs/ext4/Btrfs, macOS
APFS and Windows fixed NTFS, with conservative object/permission/size restrictions.
Other environments refuse restoration with `UNSUPPORTED`; checkpoint capture,
diff inspection and acceptance remain available. Native worker compilation also
requires the platform's C++17 toolchain; it does not change the Electron SQLite ABI.

## Minimum platform versions

Open decision 4 of [ADR 0001](adr/0001-rust-migration.md#open-decisions). The values come from
the Tauri 2 documentation read on 2026-11-17: the prerequisites page
(`v2.tauri.app/start/prerequisites`), the configuration reference
(`v2.tauri.app/reference/config`) and the Windows installer page
(`v2.tauri.app/distribute/windows-installer`). **Unconfirmed** marks a value that those
pages do not state.

| Platform | Floor | What the Tauri documentation states | Status |
| --- | --- | --- | --- |
| Windows | Windows 10 and Windows 11, x64 (the official target above) | The prerequisites page lists "Windows 7 and later". The installer page says the MSI does not work on Windows 7 by default. | The official target stands. The two Tauri statements about Windows 7 conflict; NOCT does not target Windows 7. |
| WebView2 (Windows) | Runtime required; no version floor set | Required for development on Windows. Preinstalled on Windows 10 from version 1803 and on Windows 11. `minimumWebview2Version` defaults to `null`. The installer default is `downloadBootstrapper`. | Requirement confirmed. The 1803 figure describes preinstallation, not a support floor, so NOCT sets no WebView2 version. |
| macOS | Proposed: macOS 10.15 (Catalina) | The prerequisites page says "macOS Catalina (10.15) and later". The bundle default `minimumSystemVersion` is `10.13`. | The 10.15 floor is stated, but the bundler default is lower and the pages do not reconcile them. NOCT must set `minimumSystemVersion` explicitly in phase 8. The current Electron build declares no macOS floor in `electron-builder.json5` (**unconfirmed**). WKWebView version: **unconfirmed**. |
| Linux | WebKitGTK 4.1, the `webkit2gtk-4.1` package family; version **unconfirmed** | The prerequisites page names the packages for apt, pacman, dnf, emerge and apk, and states no version. | The package family is confirmed. The WebKitGTK version floor is **unconfirmed**. The spike ran on WebKitGTK 2.54 (ADR 0001), which is a measurement, not a floor. Set the floor from the distributions the Linux artifacts target, in phase 8. |
| Rust toolchain | `1.97.1`, pinned in `rust-toolchain.toml` | The Tauri pages state no Rust version. | A project pin (commit `751894e`), not a Tauri requirement. |

Notes: the configuration reference was read in part. The `macOS` and `iOS` field descriptions
reached this review only as summaries, so they are not quoted here. The default values in the
table were read verbatim.

## AI compatibility

- no specific AI vendor is required; the minimum is an endpoint that implements
  model listing and chat completions with streaming;
- Build additionally needs a model with function calling;
- remote endpoints require HTTPS;
- local Ollama and LM Studio endpoints use loopback.

There are no dedicated native adapters for Anthropic, Gemini or GitHub Copilot
in the current release contract; those models are reached through a
Chat Completions gateway or a provider preset.
