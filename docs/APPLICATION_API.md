# PinaxCV Application API

The application API describes PinaxCV operations independently of UI transport.

Representative operations:

- `list_cvs`
- `get_cv`
- `create_cv`
- `save_cv`
- `duplicate_cv`
- `delete_cv`
- `list_variants`
- `create_variant`
- `save_variant`
- `delete_variant`
- `resolve_variant`
- `prune_variant` and `fit_variant` (fit-to-length cuts as pending suggestions, ADR-065)
- `preview`
- `export_pdf`
- `export_html`
- `run_lint`
- `run_ats_check`
- `list_templates`
- `import_parse`, `import_match` and `import_commit` (the import wizard, ADR-058)
- `generate_tailoring_suggestions`
- `accept_suggestion`
- `reject_suggestion`

Requests and responses should be serializable DTOs. Application services may use internal domain types, but transport types should not contain Tauri or database handles.

## Future HTTP

A future SaaS API may expose these operations over HTTPS. That does not mean v1 should implement HTTP. Authentication, authorization, rate limits, tenant ownership, and versioned public API concerns belong to the SaaS phase.
