# PinaxCV Changelog

## 2026-10-26

Implementation reached the end of M8's non-packaging work (issues #32 to #74, PRs #76 to #111):

- **Editor (M3):** UI strings in one i18n module, Style tab, undo/redo and shortcuts, drag-and-drop reordering, CV cards with duplicate and undoable delete, add-section menu with custom sections, preview zoom and template diagnostics, local log file, tauri-driver smoke test on Linux and Windows.
- **Import (M4):** import wizard with merge review; JSON Resume, LinkedIn data export ZIP, and skills and certifications from PDF text.
- **Variants (M5):** reorder, per-variant text, duplicate and prune, fit to 1 or 2 pages with reviewed cuts.
- **Checks (M6):** Checks tab with jump-to-field and the ATS check panel; CI parses exports with an open-source resume parser, and PDFs declare PDF/UA-1 when they can.
- **AI (M7a, M7b):** Anthropic, OpenAI, Gemini and OpenAI-compatible providers; keychain keys and consent; redaction, prompts and cache; jobs with progress and cancel; tailoring stages A to G with fact checks and a review list; AI text import with contact redaction; evals with recorded answers; opt-in Experience reorder.
- **M8:** Settings (new CV defaults, data folder, about box), backup and restore, Delete all data and Clear AI cache, `PRIVACY.md` and generated third-party licences, optional profile photo.
- Decisions ADR-050 to ADR-083 in `docs/DECISIONS.md`. Linux ships as AppImage, Flatpak and Snap; the `.deb` was dropped (ADR-049).

## 2026-10-13 (planning)

Folded the product owner's decisions into the specification and docs:

- Renamed the specification from `pinaxcd-spec.md` to `pinaxcv-spec.md`.
- Resolved contradictions (IDs in `ResolvedCv`, fit-to-length signature and review, Typst `null` semantics, lint page count, text overrides, synthetic v0 migration fixture, authoritative columns).
- Filled gaps (`review_sessions`, section ordering, what variants can hide, `Entry.url`, `Variant.schema_version`, import preview registry, restore procedure, glyph coverage warning).
- Security: only `https:`/`mailto:` links clickable; backend-owned file dialogs; HTML contact toggles.
- Decisions: Apache-2.0; English only; Anthropic/OpenAI/Gemini/OpenAI-compatible with no default model; Flatpak added; Windows tested via CI and an evaluation VM; signing from 1.0; no release before AI; AI split into M7a/M7b; M0.5 risk spikes.
- Fixed broken cross-references and the duplicate glossary entry; added `docs/AI_ARCHITECTURE.md`; recorded ADR-009 to ADR-020.
- Added the branch, commit and PR workflow to `AGENTS.md`, and CodeRabbit configuration.

## 2026-09-29 (planning)

Integrated the latest product decisions:

- Product renamed from the generic CV Builder to **PinaxCV**.
- Initial release explicitly targets Linux and Windows.
- macOS is deferred until user interest justifies platform support.
- Hosted SaaS is deferred until product demand and a business case justify infrastructure costs.
- Future SaaS is expected to use PostgreSQL.
- Added explicit persistence/application-service boundaries to make SQLite replaceable later.
- Added M0 Architecture & Contracts before implementation milestones.
- Added standalone single-file HTML export for personal websites.
- Added HTML exporter architecture, UI flow, tests, and acceptance criteria.
- Added product evolution, SaaS migration, storage, platform, rendering, application API, security, and release documentation.
