# M0.5 risk spikes

Throwaway experiments from milestone M0.5. Each folder is its own Cargo workspace, outside the main one, and is deleted once the real crates replace it. The decisions and measurements are recorded in `docs/DECISIONS.md` (ADR-022 to ADR-025).

- `typst-pdf/`: Typst 0.15 PDF export, PDF text read-back (`pdf-extract` and `lopdf`), byte-for-byte determinism, and incremental compile plus PNG render times for the live preview. Run `cargo run --release` inside the folder.
- `tauri-leptos/`: a Tauri 2 shell with a Leptos 0.8 CSR front end built by Trunk, serving a pre-rendered page through a custom `preview` URI scheme. Build with `trunk build --release` in `ui/`, then `cargo build --release -p spike-tauri`.
- `fonts/`: Inter 4.1 (SIL Open Font License 1.1, see `fonts/LICENSE.txt`).

`.github/workflows/spikes.yml` runs both spikes on Linux, Windows and macOS when files under `spikes/` change.
