//! Generates `allow-*` permissions for PinaxCV's own commands only (spec
//! section 10.3); `capabilities/main.json` grants them to the main window.

/// Must match the handlers registered in `main.rs`.
const COMMANDS: &[&str] = &[
    "list_cvs",
    "get_cv",
    "create_cv",
    "save_cv",
    "duplicate_cv",
    "delete_cv",
    "restore_cv",
    "list_variants",
    "get_variant",
    "create_variant",
    "save_variant",
    "delete_variant",
    "duplicate_variant",
    "prune_variant",
    "fit_variant",
    "ai_status",
    "ai_configure",
    "ai_forget_key",
    "ai_consent",
    "ai_test_connection",
    "ai_cancel",
    "ai_tailor",
    "ai_review",
    "ai_import",
    "import_text_prepare",
    "import_text_basic",
    "get_review",
    "review_decide",
    "preview",
    "export_pdf",
    "export_html",
    "ats_check",
    "lint",
    "list_templates",
    "import_parse",
    "import_match",
    "import_commit",
    "get_settings",
    "set_cv_defaults",
    "open_data_dir",
    "backup_db",
    "restore_db",
    "clear_ai_cache",
    "delete_all_data",
    "third_party_licenses",
    "photo_choose",
    "photo_get",
    "photo_apply",
    "photo_remove",
    "signature_choose",
    "signature_get",
    "signature_remove",
];

fn main() {
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    if let Err(e) = tauri_build::try_build(attributes) {
        panic!("tauri build failed: {e:#}");
    }
}
