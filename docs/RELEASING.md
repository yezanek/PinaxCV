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
- live AI evals against each supported provider, with the model named in `PINAXCV_EVAL_MODEL` (`testdata/evals/README.md`); record again when prompts changed (the first live run replaces the hand-written recordings, #115)
- HTML standalone/export tests
- clean-install smoke tests
- backup/restore test
- `python3 scripts/third_party_licenses.py --check` (every shipped licence is compatible with Apache-2.0; run without `--check` after a dependency change)
- `PRIVACY.md` still matches what the app stores and sends
- updater test
- manual pass in a Windows 11 evaluation VM (before 1.0 and each major release)

macOS releases are a future addition. SaaS releases have a separate deployment process and are not covered by this desktop checklist.
