# Installation and requirements

[Português do Brasil](installation.pt-BR.md)

## Official v1.0.5 artifacts

Install only artifacts published for the intended release:

- Windows 10/11 x64: `Nocturne-Studio-Windows-1.0.5-Setup.exe`;
- Linux desktop: `Nocturne.Studio-Linux-1.0.5.AppImage` or `tar.gz`;
- macOS ARM64: `Nocturne-Studio-Mac-1.0.5-Installer.dmg`; the updater uses
  the matching ZIP artifact.

The canonical repository and updater source are
`gustavomfg/NOCT`. The architecture is the one named by the release
asset; do not install an artifact from a package-validation workflow as if it
were a stable release.

Compare the platform-specific manifest before installing:

- Linux: `SHA256SUMS-Linux` and its detached `SHA256SUMS-Linux.sig` GPG
  signature;
- Windows: `SHA256SUMS-Windows`;
- macOS: `SHA256SUMS-macOS`.

Linux checksums are signed in the protected stable-release environment. Windows
and macOS packages are currently unsigned and not notarized; the operating
system may show an additional trust warning. No platform signature is implied
when only a checksum is present.

The published `v1.0.0` client remains compatible with the historical
`gustavomfg/Nocturne-Codex` slug through GitHub's rename redirect. New `v1.0.1`
packages use the canonical repository directly.

## AI requirements

No specific AI vendor or CLI is required. Review, Build and Docs run with any
configured provider (see [providers](providers.md)); Build needs a model with
function calling. Nocturne does not install or depend on another AI program.

## Development setup

- Node.js `>=24.18 <25`;
- npm `>=11 <12`;
- native build tooling compatible with `better-sqlite3`.

```bash
npm ci
npm run dev
```

See [development](development.md) for validation and packaging commands.
