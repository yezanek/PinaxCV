# PinaxCV Application API

The application API describes PinaxCV operations independently of UI transport.

Operations, as the Tauri commands that `src-tauri` exposes over `cv-app` (registered in `src-tauri/build.rs` and the main capability):

- **CVs:** `list_cvs`, `get_cv`, `create_cv`, `save_cv` (revision-checked), `duplicate_cv`, `delete_cv` (soft delete) and `restore_cv` (undo).
- **Variants:** `list_variants`, `get_variant`, `create_variant`, `save_variant`, `delete_variant`, `duplicate_variant`, `prune_variant` (when a variant is opened, ADR-064) and `fit_variant` (fit-to-length cuts as pending suggestions, ADR-065).
- **Preview and export:** `preview` (PNG pages over the `preview` URI scheme), `export_pdf`, `export_html` and `list_templates`.
- **Checks:** `lint` and `ats_check`.
- **Import:** `import_parse`, `import_match` and `import_commit` (the import wizard, ADR-058); `import_text_prepare`, `import_text_basic` and the `ai_import` job (text import, ADR-076).
- **AI setup:** `ai_status`, `ai_configure`, `ai_forget_key`, `ai_consent` and `ai_test_connection` (ADR-069).
- **AI jobs:** `ai_tailor(AiTailor { variant_id, options }) -> JobId`, whose `ai://done` result is a `TailorResult` (ADR-072); `ai_review`, `get_review` and `review_decide` (review without a job, ADR-075); `ai_cancel(job_id)`. Long AI work runs as a job that reports `ai://progress`, `ai://done` and `ai://error` events (ADR-071).
- **Suggestions** from tailoring and fitting live on the variant. The UI accepts or rejects them with `cv_core` (`Variant::accept_suggestion`, `accept_edited`, `reject_suggestion`, `accept_all`, `reject_all`) and saves the variant with `save_variant`; nothing is applied without the user (ADR-073).
- **Settings and data:** `get_settings`, `set_cv_defaults`, `open_data_dir`, `backup_db`, `restore_db`, `clear_ai_cache`, `delete_all_data` and `third_party_licenses` (ADR-079 to ADR-082).
- **Photo:** `photo_choose`, `photo_get`, `photo_apply` and `photo_remove` (ADR-083).

The backend opens every file dialog itself (export, import, backup, restore, photo), so no command takes a file path from the UI.

Requests and responses should be serializable DTOs. Application services may use internal domain types, but transport types should not contain Tauri or database handles.

## Future HTTP

A future SaaS API may expose these operations over HTTPS. That does not mean v1 should implement HTTP. Authentication, authorization, rate limits, tenant ownership, and versioned public API concerns belong to the SaaS phase.
