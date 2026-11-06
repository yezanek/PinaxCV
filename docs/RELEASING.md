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

Automated, on every PR or release run:

- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (PDF golden and ATS tests, HTML export tests, backup/restore tests), on Linux and Windows (CI)
- open-source resume parser and veraPDF PDF/UA-1 check on every fixture export (CI `resume parser` job)
- `python3 scripts/third_party_licenses.py --check` (every shipped licence is compatible with Apache-2.0; run without `--check` after a dependency change)
- clean-install smoke test: the release workflow starts the AppImage on fresh Ubuntu 22.04, Ubuntu 24.04, Debian 12 and Fedora 41 containers with only an X server added, and checks that it runs and creates its database and log (issue #73)

## Release pass (manual, issue #73)

Before publishing the draft release, on a real Linux desktop (a fresh Ubuntu 22.04 or 24.04 VM is best; `libfuse2` / `libfuse2t64` installed):

1. Download the AppImage from the draft release, `chmod +x` it and start it. The first-run question about updates appears with "No" selected.
2. Create a CV, edit it, and check the preview updates.
3. Export a PDF and an HTML file; open both. Run Checks and the ATS check.
4. Import a JSON Resume or LinkedIn export (synthetic data).
5. Back up, restore the backup, and check the CV is still there. Delete all data from Settings and check the list is empty.
6. If an AI key is at hand: configure a provider, accept the consent, tailor a variant, and check the token line.
7. Updater (from the second signed release on): start the previous release's AppImage, switch on Settings > Updates, press "Check now", then "Install and restart"; the app restarts as the new version (Settings > About shows it). With the check off, nothing is requested.
8. Check `PRIVACY.md` still matches what the app stores and sends.

Before 1.0, also run the live AI evals against each supported provider (`testdata/evals/README.md`; on hold, #115).

On hold with the other packages: the manual pass in a Windows 11 evaluation VM, Windows installers and signing.

macOS releases are a future addition. SaaS releases have a separate deployment process and are not covered by this desktop checklist.
