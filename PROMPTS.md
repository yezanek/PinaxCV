# PinaxCV Agent Prompts

## M0 — Architecture & Contracts

Read `pinaxcv-spec.md`, `AGENTS.md`, and `docs/*` before editing code.

Establish the PinaxCV workspace and architecture boundaries. Define domain/application contracts without implementing PostgreSQL, HTTP, accounts, synchronization, or SaaS infrastructure. Ensure SQLite is hidden behind the storage boundary and Tauri is an adapter. Define PDF and single-file HTML exporters over `ResolvedCv`.

Run formatting, clippy, and tests. Record decisions in `docs/DECISIONS.md`.

## M0.5 — Risk spikes

Build throwaway spikes in `spikes/` for PDF text extraction from Typst output, byte-identical PDF output, live PNG preview latency over a custom URI scheme, and the Trunk + Tauri 2 + Leptos build on Linux and the Windows CI runner. Record each result with measurements in `docs/DECISIONS.md`.

## M1 — Core & Storage

Implement only the domain and SQLite persistence requirements. Keep `cv-core` platform/database/network independent. Do not expose SQLite types outside `cv-store`. Add migration, revision, snapshot, conflict, backup, and property tests.

## M2 — Rendering & Export

Implement PDF/PNG rendering and ATS checks from the specification. Add `HtmlExporter` directly from `ResolvedCv`. HTML must be standalone, UTF-8, semantic, responsive, escaped, and contain no external resource references. Add golden/snapshot tests.

## M3 — Desktop UI

Implement Tauri + Leptos around `cv-api`. Keep commands thin. Implement editor, autosave, live preview, PDF export, HTML export, style settings, and persistence restart behavior.

## M4 — Imports

Implement importers as pure transformations into `CvPartial`. They must never write directly to the DB. Preserve warnings and preview/merge semantics.

## M5 — Variants

Implement overlays without mutating the base CV. Verify that PDF and HTML exports differ appropriately between variants while the base remains unchanged.

## M6 — Checks

Implement deterministic lint and ATS UI with jump-to-field behavior.

## M7a — AI foundation and tailoring

Implement the provider abstraction (Anthropic, OpenAI, Gemini, OpenAI-compatible by base URL; no default model), keychain, consent, redaction, caching, job registry, stages A-C and the hide/collapse/order review UI. AI proposes; the user approves. Never fabricate facts.

## M7b — AI rewriting, review and import

Implement stages D-G with mandatory verification, `IMPORT_EXTRACT`, the rewrite review UI and evals. Contact details are never sent, except by `IMPORT_EXTRACT` after consent that says so.

## M8 — Release

Implement Linux/Windows packaging (AppImage, deb, Flatpak, NSIS, portable exe), updater and signing for 1.0. CI already exists from M0. Keep macOS and SaaS work out of this milestone.

## M10 — macOS, future

Only execute after explicit product-owner approval. Reuse the existing domain/application/rendering architecture; add platform-specific packaging/signing only where required.

## M11 — SaaS, future

Only execute after explicit product-owner approval and a documented product case. Design authentication, authorization, PostgreSQL, HTTP transport, synchronization, and deployment separately. Preserve the desktop product as an independent local-first application.
