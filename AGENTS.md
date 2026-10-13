# PinaxCV Agent Instructions

## Source of truth

`pinaxcv-spec.md` is the authoritative v1 requirements specification. Architecture documents refine boundaries and future evolution; they must not silently change product behavior.

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
- Only `https:` and `mailto:` links are clickable in PDF and HTML exports.
- The backend owns every file dialog; the UI never passes file paths to commands.
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

## Branches, commits and pull requests

- One short-lived branch per feature, named `<type>/<milestone>-<topic>`: `feat/m1-resolve`, `fix/m3-autosave-conflict`, `docs/spec-cleanup`, `ci/windows-runner`, `chore/...`. Large milestones are split into several feature PRs.
- PRs may be stacked: a branch that depends on unmerged work targets the branch below it, and is retargeted to `main` once that merges.
- Commit messages and PR titles follow [Conventional Commits](https://www.conventionalcommits.org/) (`feat(cv-core): add variant resolve`).
- Every PR starts as a draft and is marked ready only when CI is green on Linux and Windows and review comments are addressed.
- CodeRabbit reviews every PR. Fix valid findings; reply on the thread with a reason when declining one. Never weaken the spec or these rules to satisfy a review comment.
- The product owner reviews and merges. Agents never merge.

## Milestones

Work milestone by milestone. A milestone is complete only when its acceptance criteria pass.

Start with M0 Architecture & Contracts, then M0.5 Risk Spikes, M1 Core & Storage, M2 Rendering & Export, M3 App Shell, M4 Imports, M5 Variants, M6 Checks, M7a AI Foundation & Tailoring, M7b AI Rewriting & Import, M8 Packaging (1.0 release), and M9 Product Validation.

Future M10 macOS and M11 SaaS are not part of the v1 implementation unless explicitly activated by the product owner.
