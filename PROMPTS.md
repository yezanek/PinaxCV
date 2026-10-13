# PinaxCV Agent Prompts

## M0 — Architecture & Contracts

Read `pinaxcd-spec.md`, `AGENTS.md`, and `docs/*` before editing code.

Establish the PinaxCV workspace and architecture boundaries. Define domain/application contracts without implementing PostgreSQL, HTTP, accounts, synchronization, or SaaS infrastructure. Ensure SQLite is hidden behind the storage boundary and Tauri is an adapter. Define PDF and single-file HTML exporters over `ResolvedCv`.

Run formatting, clippy, and tests. Record decisions in `docs/DECISIONS.md`.

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

## M7 — AI

Implement provider abstraction, consent, redaction, verification, caching, review UI, and evals. AI proposes; the user approves. Never send contact details. Never fabricate facts.

## M8 — Release

Implement Linux/Windows CI and packaging. Keep macOS and SaaS work out of this milestone.

## M10 — macOS, future

Only execute after explicit product-owner approval. Reuse the existing domain/application/rendering architecture; add platform-specific packaging/signing only where required.

## M11 — SaaS, future

Only execute after explicit product-owner approval and a documented product case. Design authentication, authorization, PostgreSQL, HTTP transport, synchronization, and deployment separately. Preserve the desktop product as an independent local-first application.
