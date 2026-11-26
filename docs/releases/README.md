# Release notes of the Rust line

`shell-release.yml` publishes a `rust-vX.Y.Z` tag only when `docs/releases/<tag>.md` exists for that tag, and it fails
without it. Write the notes of a version here before tagging it.

The notes of the Electron line (up to `1.0.5`) are not kept on this branch. They are in the tag `electron-final`, for
example `git show electron-final:docs/releases/v1.0.5.md`.
