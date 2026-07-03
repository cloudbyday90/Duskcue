# Duskcue Security Patch

This vendored copy is the crates.io `plist` 1.9.0 package with one dependency
change: `quick-xml` is raised from `0.39.2` to `0.41.0`.

The patch removes the `RUSTSEC-2026-0194` and `RUSTSEC-2026-0195` advisory path
through Tauri's `plist` dependency. Remove this vendor patch when `plist`
publishes a release that depends on `quick-xml >= 0.41`.
