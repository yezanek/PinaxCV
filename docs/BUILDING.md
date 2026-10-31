# Building PinaxCV from source

PinaxCV builds from source on Linux, macOS and Windows. Linux and Windows are the v1 release platforms. macOS builds are for development and personal use only: there are no signed or notarized macOS releases until Future M10 (ADR-007), but CI builds and tests on macOS on every PR, and these instructions are kept working there.

This file is updated in every milestone that changes how PinaxCV is built. A build produces the desktop app (`pinaxcv`), the library crates and their tests. There are no installers yet: packaging is the open M8 work (AppImage #68, NSIS and portable exe #69, Flatpak #33, Snap #34).

## 1. System packages

### Debian and Ubuntu

```sh
sudo apt update
sudo apt install build-essential curl wget file git libwebkit2gtk-4.1-dev \
  libsoup-3.0-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

Debian 12 or Ubuntu 22.04 or newer (WebKitGTK 4.1 is required).

### Fedora

```sh
sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file git \
  libappindicator-gtk3-devel librsvg2-devel libxdo-devel
sudo dnf group install "c-development"
```

### Arch Linux

```sh
sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file git openssl \
  appmenu-gtk-module libappindicator-gtk3 librsvg xdotool
```

### macOS

```sh
xcode-select --install
```

The Command Line Tools are enough; the full Xcode app is not needed. Apple Silicon and Intel Macs both work.

### Windows 10 or 11

- Install the [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) and select **Desktop development with C++**.
- WebView2 is already installed on Windows 10 (1803 or later) and Windows 11. On older systems, install the [WebView2 Evergreen Bootstrapper](https://developer.microsoft.com/microsoft-edge/webview2/).
- Install [Git for Windows](https://git-scm.com/download/win). Run the commands below in PowerShell or Git Bash.

## 2. Rust and build tools

Install Rust with [rustup](https://rustup.rs) (on Windows, use the default MSVC toolchain). You do not need to pick a Rust version: `rust-toolchain.toml` pins Rust 1.97.0 with `rustfmt`, `clippy` and the `wasm32-unknown-unknown` target, and rustup installs them on the first `cargo` command inside the repository.

Then install Trunk, which builds the Leptos UI to WebAssembly, at the version CI uses:

```sh
cargo install trunk --version 0.21.14 --locked
```

## 3. Build and test

```sh
git clone https://github.com/yezanek/PinaxCV.git
cd PinaxCV
(cd ui && trunk build --locked)   # the desktop shell embeds ui/dist
cargo build --workspace --locked
cargo test --workspace --locked
```

The same checks CI runs:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p cv-core -p cv-api -p pinaxcv-ui --target wasm32-unknown-unknown --locked -- -D warnings
bash scripts/check-boundaries.sh
python3 scripts/third_party_licenses.py --check
```

On Windows, run `scripts/check-boundaries.sh` from Git Bash. The licence check needs Python 3; after changing dependencies, run `python3 scripts/third_party_licenses.py` without `--check` to regenerate `THIRD_PARTY_LICENSES.md` (ADR-082).

The AI evals replay recorded answers as part of `cargo test`. To run them against a real provider before a release, see [`testdata/evals/README.md`](../testdata/evals/README.md).

To measure preview and export speed on your own machine (spec section 16; numbers in ADR-035):

```sh
cargo run --release -p cv-render --example preview_latency
```

To run the open-source resume parser check from CI (ADR-061; needs Node.js 22):

```sh
cargo run -p cv-render --example export_fixtures -- /tmp/pinaxcv-fixtures
cd scripts/ats-parser
git clone https://github.com/xitanggg/open-resume.git && git -C open-resume checkout 4f8255a2c763479837f69f1dccf2a3338730cd79
npm ci && npx tsx check.ts /tmp/pinaxcv-fixtures
```

## 4. Run the desktop app

Build the UI, then start the shell:

```sh
(cd ui && trunk build --locked)
cargo run -p pinaxcv
```

For everyday use, build an optimised binary once and run it directly:

```sh
(cd ui && trunk build --release --locked)
cargo build --release -p pinaxcv --locked
```

The app is then `target/release/pinaxcv` (`target\release\pinaxcv.exe` on Windows). It is a single binary with the UI, templates and fonts built in, so you can copy it anywhere. On macOS it runs as a plain binary rather than an `.app`; a binary you built yourself is not quarantined, so Gatekeeper does not prompt.

Rebuild the UI with Trunk after changing anything under `ui/`; the shell embeds `ui/dist` at compile time. `cargo tauri dev` and `cargo tauri build` are not set up yet (no `beforeDevCommand`, bundling is off); they arrive with the packaging issues.

Where the app keeps things:

| | Linux | macOS | Windows |
|---|---|---|---|
| Database (`pinaxcv.sqlite3`) | `~/.local/share/dev.pinaxcv.app/` | `~/Library/Application Support/dev.pinaxcv.app/` | `%APPDATA%\dev.pinaxcv.app\` |
| Log (`pinaxcv.log`) | `~/.local/share/dev.pinaxcv.app/logs/` | `~/Library/Logs/dev.pinaxcv.app/` | `%LOCALAPPDATA%\dev.pinaxcv.app\logs\` |

API keys for AI providers go to the system keychain: the Secret Service on Linux (GNOME Keyring, KWallet and others; no extra build packages are needed), Keychain on macOS, Credential Manager on Windows. Without a keychain the app keeps the key in memory for the session.

## 5. End-to-end smoke test

CI drives the real app through [`tauri-driver`](https://v2.tauri.app/develop/tests/webdriver/) on Linux and Windows: it creates a CV, edits it, waits for the preview, exports a PDF, and switches the Style tab to two columns, moves the name card to the sidebar with its button and drags the summary card after it (`crates/e2e/tests/smoke.rs`). `cargo test` skips it unless `PINAXCV_E2E_APP` is set.

The app is built with the `e2e` feature, which saves exports into `PINAXCV_E2E_EXPORT_DIR` instead of opening the save dialog, because WebDriver cannot drive native dialogs. Release builds never enable it. `tauri-driver` starts the app, so it is the process that needs `PINAXCV_E2E_EXPORT_DIR`.

To run it on Debian or Ubuntu:

```sh
sudo apt install webkit2gtk-driver xvfb
cargo install tauri-driver --version 2.1.0 --locked
(cd ui && trunk build --locked)
cargo build -p pinaxcv --features e2e
export PINAXCV_E2E_EXPORT_DIR=$(mktemp -d) PINAXCV_E2E_APP=$PWD/target/debug/pinaxcv
xvfb-run -a sh -c 'tauri-driver & driver=$!; trap "kill $driver" EXIT
  until curl -s -o /dev/null http://127.0.0.1:4444/status; do sleep 1; done
  cargo test -p pinaxcv-e2e'
```

The shell stops `tauri-driver` when the test ends. The test uses the app's normal data folder, so it adds an "E2E smoke" CV there. On Windows, `tauri-driver` also needs `msedgedriver.exe` matching the installed WebView2 version (`--native-driver`); the CI job shows how to fetch it.

## Troubleshooting

- **`webkit2gtk-4.1` not found (Linux):** your distribution is too old or the `-dev`/`-devel` package is missing. Ubuntu 20.04 and Debian 11 only ship WebKitGTK 4.0, which Tauri 2 does not support.
- **`link.exe` not found (Windows):** the C++ Build Tools are missing, or the shell was opened before they were installed.
- **Slow first build:** the first build compiles Typst, Tauri and all their dependencies from source, which takes several minutes. Later builds are incremental.

## Appendix: the M0.5 spikes

The M0.5 spikes in `spikes/` are throwaway experiments kept for reference (ADR-022 to ADR-025). The Tauri spike is a small Tauri + Leptos window that shows a pre-rendered CV page through the `preview` URI scheme:

```sh
cd spikes/tauri-leptos/ui
trunk build --release --locked
cd ..
cargo build --release --locked -p spike-tauri
```

Then run `target/release/spike-tauri` from `spikes/tauri-leptos/` (`target\release\spike-tauri.exe` on Windows).

The PDF spike prints its measurements and writes `out.pdf` to the current directory:

```sh
cd spikes/typst-pdf
cargo run --release --locked
```
