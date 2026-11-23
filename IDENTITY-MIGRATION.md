# NOCT identity migration

Keep package name nocturne-studio, app ID com.nocturne.codex, current user-data directory Nocturne Studio, legacy directory Nocturne Codex, database, IPC, protocol, artifact names and updater signing identifiers. These preserve installed data and update compatibility. GitHub publish target is NOCT. Public productName and generated Windows/macOS executable bundle names follow NOCT; artifact download filenames remain stable. Installed Windows/macOS upgrade behavior still needs platform validation before a new release. Existing releases and image assets retain historical names.

Historical release notes, screenshots and archived plans may retain earlier branding.

The Rust product keeps the app ID `com.nocturne.codex` as the Tauri `identifier`: it names the data folder and the
credential store's service (`crates/noct-app/tests/installers.rs` locks both). Tauri derives the data folder from the
identifier, so it is not the Electron folder `Nocturne Studio`; data of the Electron product is not opened
automatically. When changing visible branding, update the interface check (`scripts/check-shell-ui.mjs`) and the README
captures in the same change.
