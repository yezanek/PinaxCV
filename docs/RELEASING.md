# PinaxCV Release Process

## V1 targets

- Ubuntu 22.04+ / Linux distribution supported by the chosen Tauri/WebKit packaging strategy.
- Windows 11 and supported Windows versions defined by the final Tauri baseline.

Artifacts:

- Linux AppImage
- Linux `.deb`
- Windows NSIS installer
- Windows portable executable

## Release checks

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- PDF golden/ATS tests
- HTML standalone/export tests
- clean-install smoke tests
- backup/restore test
- updater test

macOS releases are a future addition. SaaS releases have a separate deployment process and are not covered by this desktop checklist.
