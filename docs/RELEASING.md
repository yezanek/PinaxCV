# PinaxCV Release Process

## V1 targets

- Ubuntu 22.04+ / Linux distribution supported by the chosen Tauri/WebKit packaging strategy.
- Windows 11 and supported Windows versions defined by the final Tauri baseline.

Artifacts:

- Linux AppImage
- Linux Flatpak
- Linux Snap
- Windows NSIS installer (signed from 1.0)
- Windows portable executable (no auto-update; signed from 1.0)

## Release checks

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- PDF golden/ATS tests
- live AI evals against each provider's default model (`testdata/evals/README.md`); record again when prompts changed
- HTML standalone/export tests
- clean-install smoke tests
- backup/restore test
- updater test
- manual pass in a Windows 11 evaluation VM (before 1.0 and each major release)

macOS releases are a future addition. SaaS releases have a separate deployment process and are not covered by this desktop checklist.
