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

The Rust shell on `migration/rust` has the Linux rollback backend only. On macOS and
Windows it refuses restoration with `UNSUPPORTED`. See
[Rust rollback backends](rust-native-rollback-backends.md).

## Minimum platform versions

Open decision 4 of [ADR 0001](adr/0001-rust-migration.md#open-decisions) is resolved for the platform floors.
**Project decision, 2026-11-17, by the project owner:** Windows 10 and later (x64); macOS 10.15 and later; Linux by the
reference distribution, Ubuntu 22.04 and later. Tauri documents what it requires and what its configuration defaults to.
It does not set NOCT's support floor, so each row shows the Tauri statement next to the decided floor.

Sources, read on 2026-11-17:

- [Prerequisites](https://v2.tauri.app/start/prerequisites/), "Last updated: Aug 20, 2026";
- [Configuration reference](https://v2.tauri.app/reference/config/), "Last updated: Jul 23, 2023";
- [Windows installer](https://v2.tauri.app/distribute/windows-installer/), "Last updated: Nov 5, 2026".

The pages were read through a fetch tool, not a browser. Text in quotation marks is copied from the
fetched page. Other text is a paraphrase.

| Platform | Floor | What the Tauri documentation states | Status |
| --- | --- | --- | --- |
| Windows | Windows 10 and later, x64 (decided) | The prerequisites page lists "Windows 7 and later" under System Dependencies. The installer page says "By default, the Microsoft Installer (`.msi`) does not work on Windows 7" and has a section "Supporting Windows 7". Neither page states a minimum for a shipped application. | Decided: Windows 10 and later, x64, with no build number set. Windows 7 is not supported. The two statements are about different things: the prerequisites page covers the development toolchain, and the installer note covers MSI bootstrapping on Windows 7. NOCT does not target Windows 7. No build number is set. |
| WebView2 (Windows) | Runtime required. No version floor set. | The Windows tip says "WebView 2 is already installed on Windows 10 (from version 1803 onward) and later versions of Windows." The `bundle > windows` default block sets `minimumWebview2Version` to `null`, and `webviewInstallMode` defaults to a silent `downloadBootstrapper`. The installer page says "Your application WILL NOT work if the user does not have the runtime installed." | Confirmed: the runtime is required. The 1803 figure describes preinstallation, not a support floor. NOCT sets no WebView2 version. |
| macOS | macOS 10.15 (Catalina) and later (decided) | The prerequisites page lists "macOS Catalina (10.15) and later" under System Dependencies. The configuration reference defines `minimumSystemVersion` as "A version string indicating the minimum macOS X version that the bundled application supports. Defaults to `10.13`." | Decided: 10.15. The two Tauri values differ: 10.15 is the development prerequisite, and 10.13 is the bundler default for the application. `bundle.macOS.minimumSystemVersion` is not set in the repository yet. Set it to 10.15 when the bundle configuration is written, because otherwise the Tauri default applies. `electron-builder.json5` sets no macOS minimum either. WKWebView version: not stated by Tauri, not checked. |
| Linux | Ubuntu 22.04 and later, as the reference distribution, with WebKitGTK 4.1 (`webkit2gtk-4.1` family). | The prerequisites page gives 4.1 development packages per distribution, for example `libwebkit2gtk-4.1-dev` (Debian), `webkit2gtk-4.1` (Arch), `webkit2gtk4.1-devel` (Fedora) and `webkit2gtk-4.1-dev` (Alpine). It states no WebKitGTK version. The openSUSE tab names `webkit2gtk3-devel`. | Decided by the reference distribution: Ubuntu 22.04 and later. The package family is confirmed by Tauri. The WebKitGTK version in Ubuntu 22.04 was not checked, and this document sets none. The spike ran on WebKitGTK 2.54.2 (ADR 0001), which is a measurement, not a floor. No CI job runs on Ubuntu 22.04: the workflows use `ubuntu-latest` and `ubuntu-24.04`. Whether `webkit2gtk3-devel` on openSUSE provides the 4.1 API was not checked. |
| Rust toolchain | 1.97.1 | The Tauri pages state no Rust version. | A project pin in `rust-toolchain.toml`, set by commit `751894e`. Not a Tauri requirement. |

Not checked: any Tauri statement on the WKWebView version, and the iOS fields of the configuration
reference. The configuration page was read in full except for one span, which was read only through a
summary. The fields used above were read verbatim from other parts of the page.

## AI compatibility

- no specific AI vendor is required; the minimum is an endpoint that implements
  model listing and chat completions with streaming;
- Build additionally needs a model with function calling;
- remote endpoints require HTTPS;
- local Ollama and LM Studio endpoints use loopback.

There are no dedicated native adapters for Anthropic, Gemini or GitHub Copilot
in the current release contract; those models are reached through a
Chat Completions gateway or a provider preset.
