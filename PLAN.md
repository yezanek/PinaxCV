# PinaxCV Development Plan

## Guiding principle

Keep the core portable, keep v1 simple, and defer infrastructure until users justify it.

Each milestone is delivered as one or more feature branches and pull requests (see `AGENTS.md`). CI runs on `ubuntu-22.04` and `windows-latest` from the first commit, plus a non-blocking `macos-latest` job that only warns early about macOS problems (ADR-007).

## M0 — Architecture & Contracts

- Cargo workspace and crate boundaries; `rust-toolchain.toml`; committed `Cargo.lock`.
- Domain IDs and serialization contracts in `cv-core`.
- Repository/application-service boundaries; `cv-api` request/response types and `ApiError`.
- Exporter seam for PDF and HTML; `TextExtractor`, `PageCounter`, `Llm` traits.
- Platform services isolated.
- CI with a dependency-boundary check.

**Acceptance:** no domain/database/platform leakage (enforced in CI); no PostgreSQL or HTTP server code; CI green on Linux and Windows.

## M0.5 — Risk Spikes

PDF text extraction from Typst output, deterministic PDF bytes, live preview latency (PNG over a custom URI scheme), Trunk + Tauri 2 + Leptos build on Linux and the Windows CI runner.

**Acceptance:** one recorded decision per spike in `docs/DECISIONS.md`, with measurements.

## M1 — Core & SQLite Storage

Implement `cv-core` and `cv-store` using SQLite/WAL/migrations.

**Acceptance:** core/store tests, property tests, migration fixture, conflict handling, snapshots, backup/restore.

## M2 — Rendering & Export

Typst PDF/PNG rendering, ATS extraction/checking, glyph-coverage warnings, and the standalone HTML exporter.

HTML requirements:

- one `.html` file;
- embedded CSS, system fonts, automatic dark mode, print styles;
- no JavaScript dependency;
- no external resources;
- semantic and accessible HTML;
- responsive presentation;
- only `https:` and `mailto:` links are clickable;
- optional email (default on) and phone (default off);
- escaped user content.

**Acceptance:** PDF golden tests and ATS snapshots pass; HTML opens correctly offline and contains no external resource references.

## M3 — Desktop App

Tauri + Leptos editor, autosave, preview, PDF export, HTML export, style controls, backend-owned file dialogs.

## M4 — Importers

JSON Resume, Europass and LinkedIn ZIP (verified against real exports supplied by the product owner), heuristic text/PDF import.

## M5 — Variants & Manual Tailoring

Overlays, visibility of any element, ordering (including sections), overrides of bullets, descriptions and summary, fit-to-length with reviewable cuts, pruning. Verify both PDF and HTML variants.

## M6 — Checks

Lint and ATS UI.

## M7a — AI Foundation & Tailoring

Providers (Anthropic, OpenAI, Gemini, OpenAI-compatible), keychain, consent, redaction, caching, job parse, relevance, selection, review UI.

## M7b — AI Rewriting & Import

Rewrite with verification, summary, gaps, review, AI text import, evals (recorded in CI, live by hand before releases).

## M8 — Linux/Windows Release (1.0)

AppImage, deb, Flatpak, NSIS, portable executable, updater, Windows signing, licenses, privacy, backup/restore, manual pass in a Windows 11 evaluation VM.

## M9 — Product Validation

No major infrastructure investment. Evaluate actual usage, retention, feedback, feature requests, willingness-to-pay signals, and requests for macOS/cloud features.

## Future M10 — macOS

Only activate if demand justifies the support cost. Reuse all core/application/rendering layers.

## Future M11 — Hosted SaaS

Only activate after product validation. Introduce authentication, authorization, PostgreSQL, HTTP API, synchronization, hosted backups, deployment, and billing as appropriate.
