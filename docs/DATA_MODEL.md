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
- AI suggestion/review state (on the variant, or per CV in `review_sessions` for reviews without a variant)
- The parsed job requirements and gap questions from tailoring (on the variant)
- Profile photo (one per CV, stored beside the CV document rather than in it; shown only when `show_photo` is on for the CV or variant)
- Settings (AI provider, consent, defaults for new CVs) and the AI response cache

Use stable ULIDs rather than database-local integer identities.

The future SaaS schema may add server ownership relationships, but those should not be embedded into every v1 domain structure merely for anticipation.
