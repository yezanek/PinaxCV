# PinaxCV Planning Update

## 2026-10-13

Folded the product owner's decisions into the specification and docs:

- Renamed the specification from `pinaxcd-spec.md` to `pinaxcv-spec.md`.
- Resolved contradictions (IDs in `ResolvedCv`, fit-to-length signature and review, Typst `null` semantics, lint page count, text overrides, synthetic v0 migration fixture, authoritative columns).
- Filled gaps (`review_sessions`, section ordering, what variants can hide, `Entry.url`, `Variant.schema_version`, import preview registry, restore procedure, glyph coverage warning).
- Security: only `https:`/`mailto:` links clickable; backend-owned file dialogs; HTML contact toggles.
- Decisions: Apache-2.0; English only; Anthropic/OpenAI/Gemini/OpenAI-compatible with no default model; Flatpak added; Windows tested via CI and an evaluation VM; signing from 1.0; no release before AI; AI split into M7a/M7b; M0.5 risk spikes.
- Fixed broken cross-references and the duplicate glossary entry; added `docs/AI_ARCHITECTURE.md`; recorded ADR-009 to ADR-020.
- Added the branch, commit and PR workflow to `AGENTS.md`, and CodeRabbit configuration.


## 2026-09-29

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
