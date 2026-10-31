# PinaxCV Storage

## V1

SQLite is the only persistence implementation.

Requirements:

- bundled SQLite;
- WAL mode;
- migrations;
- transactions;
- revision-based optimistic conflict detection;
- snapshots (base CVs only, latest 50 per CV);
- latest AI review per CV (`review_sessions`);
- backup/restore;
- no secrets in the database.

Tables: `cvs` and `variants` (JSON documents with a revision; CVs are soft-deleted), `snapshots`, `review_sessions`, `settings` (AI provider and consent, new-CV defaults), `ai_cache` (answers that passed their type check), and `photos` (one cleaned photo per CV, migration `0002_photos.sql`, schema version 2, ADR-083), and `signatures` (one cleaned signature PNG per CV, migration `0003_signatures.sql`, schema version 3, ADR-090). The schema version is `PRAGMA user_version`; a database from a newer PinaxCV is refused.

Data operations from Settings (ADR-080, ADR-081):

- **Back up** writes a `VACUUM INTO` copy to a private temporary file, syncs it and renames it over the target.
- **Restore** checks integrity and schema version, keeps the current database as `<db>.before-restore-<time>` and puts it back if anything fails.
- **Clear AI cache** empties `ai_cache`.
- **Delete all data** empties every table in one transaction, then runs `VACUUM` and a WAL checkpoint so no deleted text stays in the files, and removes the restore copies.

Soft-deleted CVs are meant to be purged after 30 days; `purge_deleted` exists, but nothing calls it yet (#113).

Application/domain code must not depend on `rusqlite::Connection` or SQLite-specific SQL types.

## Future PostgreSQL

A future hosted service may implement equivalent persistence over PostgreSQL. The migration should map stable PinaxCV entity IDs and domain data into server-owned tables while adding user/tenant ownership at the server boundary.

Do not add PostgreSQL dependencies to v1.
