# PinaxCV Data Model

The v1 data model is user-owned local CV data stored in SQLite.

Core entities include:

- CV
- Section
- Entry
- Bullet
- Skill group
- Skill
- Language
- Certification
- Link
- Variant
- Variant overlay fields
- Revision/snapshot
- AI suggestion/review state

Use stable ULIDs rather than database-local integer identities.

The future SaaS schema may add server ownership relationships, but those should not be embedded into every v1 domain structure merely for anticipation.
