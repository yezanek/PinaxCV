# PinaxCV Development Plan

## Guiding principle

Keep the core portable, keep v1 simple, and defer infrastructure until users justify it.

## M0 — Architecture & Contracts

- Rename project/application concepts to PinaxCV.
- Establish workspace and crate boundaries.
- Define domain IDs and serialization contracts.
- Define repository/application-service boundaries.
- Define `cv-api` request/response types.
- Define exporter seam for PDF and HTML.
- Isolate platform services.
- Write architecture and evolution documents.
- Record unresolved decisions.

**Acceptance:** no domain/database/platform leakage; no PostgreSQL or HTTP server code.

## M1 — Core & SQLite Storage

Implement `cv-core` and `cv-store` using SQLite/WAL/migrations.

**Acceptance:** core/store tests, property tests, migration fixture, conflict handling, snapshots, backup/restore.

## M2 — Rendering & Export

Implement Typst PDF/PNG rendering, ATS extraction/checking, and the standalone HTML exporter.

HTML requirements:

- one `.html` file;
- embedded CSS;
- no JavaScript dependency;
- no external resources;
- semantic and accessible HTML;
- responsive presentation;
- clickable URLs;
- escaped user content.

**Acceptance:** PDF golden tests and ATS snapshots pass; HTML opens correctly offline and contains no external resource references.

## M3 — Desktop App

Tauri + Leptos editor, autosave, preview, PDF export, HTML export, style controls.

## M4 — Importers

JSON Resume, verified Europass, LinkedIn ZIP, text/PDF import.

## M5 — Variants & Manual Tailoring

Overlays, visibility, ordering, overrides, fit-to-length, pruning. Verify both PDF and HTML variants.

## M6 — Checks

Lint and ATS UI.

## M7 — AI

Providers, consent, redaction, verification, suggestion review, caching, evaluations.

## M8 — Linux/Windows Release

CI, AppImage, deb, NSIS, portable executable, updater, licenses, privacy, backup/restore.

## M9 — Product Validation

No major infrastructure investment. Evaluate actual usage, retention, feedback, feature requests, willingness-to-pay signals, and requests for macOS/cloud features.

## Future M10 — macOS

Only activate if demand justifies the support cost. Reuse all core/application/rendering layers.

## Future M11 — Hosted SaaS

Only activate after product validation. Introduce authentication, authorization, PostgreSQL, HTTP API, synchronization, hosted backups, deployment, and billing as appropriate.
