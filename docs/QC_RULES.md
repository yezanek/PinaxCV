# Quality check rules and their sources

PinaxCV's checks (spec section 7.1, `cv_core::lint`) mix two kinds of rule. Each hint says which kind it is, so a user can tell a standard from advice (ADR-093).

- **Standard:** grounded in a published standard. The thresholds are the standard's own.
- **Advice:** common recruiter or career-service advice. Useful, but not a standard; the thresholds are PinaxCV's.

| Code | Severity | Kind | Source |
|---|---|---|---|
| `low_contrast` | Warning | Standard | WCAG 2.2 success criterion 1.4.3 (Contrast, Minimum): 4.5:1 for text. The accent colours headings on a white page. |

The ATS rules (`docs/ATS_RULES.md`) are vendor best practice, not a standard.
