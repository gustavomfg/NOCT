# NOCT identity migration

Keep package name nocturne-studio, app ID com.nocturne.codex, current user-data directory Nocturne Studio, legacy directory Nocturne Codex, database, IPC, protocol, artifact names and updater signing identifiers. These preserve installed data and update compatibility. GitHub publish target is NOCT. Public productName and generated Windows/macOS executable bundle names follow NOCT; artifact download filenames remain stable. Installed Windows/macOS upgrade behavior still needs platform validation before a new release. Existing releases and image assets retain historical names.

Historical release notes, screenshots and archived plans may retain earlier branding.

Renderer test selectors derive the display name from `shared/product-identity.json`.
When changing visible branding, update the tests and visually review regenerated
baselines in the same change. Keep screenshot comparison thresholds unchanged and
run `npm run test:renderer` without snapshot updates before committing.
