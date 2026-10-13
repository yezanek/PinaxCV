# PinaxCV Storage

## V1

SQLite is the only persistence implementation.

Requirements:

- bundled SQLite;
- WAL mode;
- migrations;
- transactions;
- revision-based optimistic conflict detection;
- snapshots;
- backup/restore;
- no secrets in the database.

Application/domain code must not depend on `rusqlite::Connection` or SQLite-specific SQL types.

## Future PostgreSQL

A future hosted service may implement equivalent persistence over PostgreSQL. The migration should map stable PinaxCV entity IDs and domain data into server-owned tables while adding user/tenant ownership at the server boundary.

Do not add PostgreSQL dependencies to v1.
