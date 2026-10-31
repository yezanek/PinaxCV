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
- Page layout (on the CV: one or two columns, sidebar width (its side follows the name), the order of the name, contact and summary blocks, and which blocks sit in the sidebar; ADR-084)
- Personal details (nationality, date and place of birth, gender, marital status, postal address, driving licence, right to work; in `Basics.personal`, each printed only when switched on in `show_personal` for the CV or variant, ADR-088)
- Closing block (consent clause and place for the "place, date" line; in `Cv.closing`, each part printed only when switched on for the CV or variant, ADR-089)
- Settings (AI provider, consent, defaults for new CVs) and the AI response cache

Use stable ULIDs rather than database-local integer identities.

The future SaaS schema may add server ownership relationships, but those should not be embedded into every v1 domain structure merely for anticipation.
