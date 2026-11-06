# Quality check rules and their sources

PinaxCV's checks (spec section 7.1, `cv_core::lint`) mix two kinds of rule. Each hint says which kind it is, so a user can tell a standard from advice (ADR-093).

- **Standard:** grounded in a published standard. The thresholds are the standard's own.
- **Advice:** common recruiter or career-service advice. Useful, but not a standard; the thresholds are PinaxCV's.

| Code | Severity | Kind | Source |
|---|---|---|---|
| `low_contrast` | Warning | Standard | WCAG 2.2 success criterion 1.4.3 (Contrast, Minimum): 4.5:1 for text. The accent colours headings on a white page. |
| `phone_format` | Info | Standard | ITU-T E.123 (notation for international numbers): `+` country code, then the number. |
| `language_level` | Info | Standard | CEFR (Common European Framework of Reference for Languages, Council of Europe): A1 to C2, as Europass uses. ILR and ACTFL levels are accepted too. |
| `long_sentence` | Info | Standard | ISO 24495-1:2023 (plain language) sets principles, not numbers: readers must find, understand and use the text. The 30-word limit is PinaxCV's. |
| `passive_voice` | Info | Standard | ISO 24495-1: say who does what. Heuristic: a form of "to be" followed by a past participle. |
| `undefined_acronym` | Info | Standard | ISO 24495-1: explain terms the reader may not know. An all-caps word (3 to 12 characters) used once, not in brackets, not on the well-known list (`cv_core::wording`), and not also written in lower case in the CV. |
| `first_person` | Info | Advice | Recruiter and career-service guides: bullets leave out "I" and "my". |
| `tense_mixed` | Info | Advice | Past roles in the past tense; a current role in the present (or the past for finished results). Only the bullet's first verb is read. |
| `cliche` | Info | Advice | Phrases recruiters see in most CVs ("team player", "results-driven", ...). |
| `old_experience` | Info | Advice | Many guides keep the last 15 years in detail. Counted in months from today; an end date without a month counts as December. |

The wording rules are English only, like `weak_opening`, and never more than Info: they are heuristics and can be wrong.

The ATS rules (`docs/ATS_RULES.md`) are vendor best practice, not a standard.
