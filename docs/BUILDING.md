# Building PinaxCV from source

PinaxCV builds from source on Linux, macOS and Windows. Linux and Windows are the v1 release platforms. macOS builds are for development and personal use only: there are no signed or notarized macOS releases until Future M10 (ADR-007), but CI builds and tests on macOS on every PR, and these instructions are kept working there.

This file is updated in every milestone that changes how PinaxCV is built. Until the desktop app lands in M3, a build produces the library crates and their tests, plus the M0.5 spike app in `spikes/tauri-leptos/`.

## 1. System packages

### Debian and Ubuntu

```sh
sudo apt update
sudo apt install build-essential curl wget file git libwebkit2gtk-4.1-dev \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
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
cargo build -p cv-core -p cv-api --target wasm32-unknown-unknown --locked
bash scripts/check-boundaries.sh
```

On Windows, run `scripts/check-boundaries.sh` from Git Bash.

To measure preview and export speed on your own machine (spec section 16; numbers in ADR-035):

```sh
cargo run --release -p cv-render --example preview_latency
```

## 4. Try the spike app (until M3)

The M0.5 spike is a small Tauri + Leptos window that shows a pre-rendered CV page through the `preview` URI scheme:

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

## 5. The desktop app (from M3)

M3 adds the app itself. It needs `tauri-cli`, which provides `cargo tauri`, at the version the project pins:

```sh
cargo install tauri-cli --version 2.12.2 --locked
```

To run the app, build the UI, then start the shell:

```sh
(cd ui && trunk build --locked)
cargo run -p pinaxcv
```

Rebuild the UI with Trunk after changing anything under `ui/`. The first screen lists your CVs; the editor has the contact fields, summary, and work experience, education and project entries with bullets, plus a live preview, autosave, and PDF and HTML export.

The database is `pinaxcv.sqlite3` in `~/.local/share/dev.pinaxcv.app/` on Linux, `~/Library/Application Support/dev.pinaxcv.app/` on macOS and `%APPDATA%\dev.pinaxcv.app\` on Windows.

This section will then cover `cargo tauri dev` for a live-reloading development build and `cargo tauri build` for a release build:

- Linux: AppImage and `.deb`.
- Windows: NSIS installer.
- macOS: an unsigned `.app` for your own machine. A locally built app is not quarantined, so it opens without Gatekeeper prompts.

## Troubleshooting

- **`webkit2gtk-4.1` not found (Linux):** your distribution is too old or the `-dev`/`-devel` package is missing. Ubuntu 20.04 and Debian 11 only ship WebKitGTK 4.0, which Tauri 2 does not support.
- **`link.exe` not found (Windows):** the C++ Build Tools are missing, or the shell was opened before they were installed.
- **Slow first build:** the first build compiles Typst, Tauri and all their dependencies from source, which takes several minutes. Later builds are incremental.
