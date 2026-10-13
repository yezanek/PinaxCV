# PinaxCV Agent Instructions

## Source of truth

`pinaxcd-spec.md` is the authoritative v1 requirements specification. Architecture documents refine boundaries and future evolution; they must not silently change product behavior.

If a requirement is ambiguous, choose the simplest implementation, record the decision in `docs/DECISIONS.md`, and continue.

## Product boundary

V1 is a standalone local-first desktop application for Linux and Windows.

Do not add:

- PinaxCV accounts
- PinaxCV authentication
- PostgreSQL
- hosted APIs
- cloud synchronization
- multi-tenancy
- billing
- telemetry
- a PinaxCV server

macOS and SaaS are future phases and must not increase v1 operational complexity.

## Architecture rules

- `cv-core` must remain independent of Tauri, SQLite, PostgreSQL, HTTP, filesystem, and OS APIs.
- `cv-store` contains the SQLite implementation. Do not expose `rusqlite` types above the storage boundary.
- Application services use explicit request/response structures.
- Tauri commands are adapters; business logic belongs below them.
- Use stable ULID domain IDs; do not depend on database-generated IDs.
- `cv-render` consumes `ResolvedCv` and does not access persistence.
- PDF and HTML are separate exporters from the same resolved domain model.
- HTML export is one self-contained UTF-8 file with embedded CSS and no external resources.
- `cv-import` never writes directly to persistence.
- `cv-ai` never reads the database directly and never applies suggestions without approval.
- Platform-specific code must be isolated.

## Coding rules

- Verify current APIs for pinned dependencies before implementation.
- Prefer boring, explicit Rust.
- `#![forbid(unsafe_code)]` in project crates.
- Every public function in `cv-core`, `cv-render`, `cv-import`, and `cv-ai` has at least one test.
- Run formatting, clippy, and tests before declaring a milestone complete.
- Do not weaken security/privacy requirements to make a test pass.

## Milestones

Work milestone by milestone. A milestone is complete only when its acceptance criteria pass.

Start with M0 Architecture & Contracts, then M1 Core & Storage, M2 Rendering & Export, M3 App Shell, M4 Imports, M5 Variants, M6 Checks, M7 AI, M8 Packaging, and M9 Product Validation.

Future M10 macOS and M11 SaaS are not part of the v1 implementation unless explicitly activated by the product owner.
