# Rust dependency advisories: the seven warnings of `cargo audit`

`cargo audit` (0.23.0) exits 0 on `Cargo.lock` at `bb3f357` and reports seven allowed warnings. None is an error under the
current policy. This file lists them, with the reason each is reported and the path through which the workspace reaches it.

Reproduce with `cargo audit` from the repository root. The advisory database is the local copy that `cargo audit` fetches.

The seven were already in `Cargo.lock` before the updater commit (`004dba3`): their versions in `c6318cd` and `bb3f357` are
the same. The updater adds no advisory.

`cargo deny check` passes its advisories, bans, licences and sources checks on the same lock file.

| Advisory | Crate and version | Kind | Reason given by the advisory | Path into the workspace | Alternative the advisory names |
| --- | --- | --- | --- | --- | --- |
| RUSTSEC-2024-0429 | `glib` 0.18.5 | unsound | The `VariantStrIter` iterator passes an out-argument as a shared reference, which is undefined behaviour. Fixed in 0.20.0. | `gtk` 0.18.2 → `muda` 0.20.0 → `tauri` 2.12.5 → `noct-app`, the Linux GTK stack | none named; upgrade of Tauri's GTK binding |
| RUSTSEC-2024-0370 | `proc-macro-error` 1.0.4 | unmaintained | The maintainer is unreachable: no commits for two years, no releases for four. It also pulls `syn` 1.x. | `glib-macros` 0.18.5 (a procedural macro, build time) → `glib` | `manyhow`, `proc-macro2-diagnostics` |
| RUSTSEC-2024-0436 | `paste` 1.0.15 | no longer maintained | The author archived the repository. | `leptos` 0.8.23 (a procedural macro, build time) → `noct-ui` | `pastey` (drop-in fork), `with_builtin_macros` |
| RUSTSEC-2026-0543 | `chrono` 0.4.45 | unmaintained | The crate is unmaintained; all versions are affected. | Listed in `Cargo.lock` through `serde_with`. `cargo tree --workspace --target all -e all -i chrono` finds no active path, so it is not compiled into the workspace. | `jiff`, `time` |
| RUSTSEC-2026-0370 | `zvariant` 5.15.1 | unmaintained | Merged into `zbus` 6.0.0 as its `wire` module. | `zbus` 5.19.0 → `secret-service` 5.2.0 → `zbus-secret-service-keyring-store` → `noct-secrets` (the Linux keyring) | `zbus` 6.0.0, `wire` module |
| RUSTSEC-2026-0372 | `zvariant_derive` 5.15.1 | unmaintained | Its derive macros are provided by `zbus` 6.0.0. | The same path as `zvariant`; a procedural macro, build time | `zbus` 6.0.0 |
| RUSTSEC-2026-0371 | `zbus_names` 4.3.5 | unmaintained | Merged into `zbus` 6.0.0 as its `names` module. | The same path as `zvariant`, through `zbus` | `zbus` 6.0.0, `names` module |

## What the list means for NOCT

- **Unsound (glib).** The advisory's undefined behaviour is in `VariantStrIter`. The application and `noct-ui` do not call
  `glib` directly (a search of their sources finds no use). It is reached only through Tauri's Linux GTK stack. Fixing it means
  Tauri moving to a newer GTK binding, which NOCT does not control.
- **Unmaintained, procedural macros.** `proc-macro-error`, `paste`, `zvariant_derive` run at build time as macros. They are not
  part of the shipped code, but they are part of the build.
- **Unmaintained, D-Bus (zvariant, zvariant_derive, zbus_names).** These sit under `noct-secrets`, which this change does not
  touch. They are the Linux keyring path. The advisories name `zbus` 6.0.0 as the successor.
- **Unmaintained, chrono.** It is in the lock file but not compiled for any target with the current features. It is reported
  because `cargo audit` reads the lock file.

## Owner's decision needed

Whether `cargo audit` should fail on warnings, or stay as it is. Today it does not. The seven would need a reviewed exception
list if it did. Each would be removed when Tauri or `noct-secrets` moves to the successor crate.
