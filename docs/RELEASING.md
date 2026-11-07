# PinaxCV Release Process

## V1 targets

- Ubuntu 22.04+ / Linux distribution supported by the chosen Tauri/WebKit packaging strategy.
- Windows 11 and supported Windows versions defined by the final Tauri baseline.

Artifacts:

- Linux AppImage, built by `.github/workflows/release.yml` on a `v*` tag (ADR-094). This is the only release for now.
- On hold, to be done later: Linux Flatpak, Linux Snap, Windows NSIS installer and portable executable (signed from 1.0).

## Making a release

0. Once, before the first release: make the updater key pair with `cargo tauri signer generate -w pinaxcv-updater.key` (keep the file and its password safe and out of the repository). Add the private key's contents as the repository secret `TAURI_SIGNING_PRIVATE_KEY` and its password as `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, and the public key as the repository variable `TAURI_UPDATER_PUBKEY` (Settings > Secrets and variables > Actions). Without them the AppImage is built but cannot update itself (ADR-095).
1. Set the version in the root `Cargo.toml` (`[workspace.package]`) and add a `CHANGELOG.md` entry; merge to `main`.
2. Push the tag: `git tag v<version> && git push origin v<version>`. The tag must match the version or the workflow stops.
3. The workflow builds the AppImage on Ubuntu 22.04 and attaches it to a draft GitHub release. Check the release checks below, then publish the draft.

## Release checks

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- PDF golden/ATS tests
- live AI evals against each supported provider, with the model named in `PINAXCV_EVAL_MODEL` (`testdata/evals/README.md`); record again when prompts changed (a passing live run with `PINAXCV_EVAL_RECORD=1` replaces the hand-written recordings, #115)
- HTML standalone/export tests
- clean-install smoke tests
- backup/restore test
- `python3 scripts/third_party_licenses.py --check` (every shipped licence is compatible with Apache-2.0; run without `--check` after a dependency change)
- `PRIVACY.md` still matches what the app stores and sends
- updater test: install the previous release's AppImage, switch on Settings > Updates, and check that "Check now" finds the new release and "Install and restart" replaces the AppImage and restarts it (#73)
- manual pass in a Windows 11 evaluation VM (required once the Windows packages leave hold; then before 1.0 and each major release)

macOS releases are a future addition. SaaS releases have a separate deployment process and are not covered by this desktop checklist.
