//! Tauri commands: thin adapters that run a `cv-app` service on the blocking
//! pool (ADR-036). No business logic lives here.

use std::sync::Arc;
use std::time::Instant;

use cv_api::{
    ApiError, DuplicateCv, DuplicateVariant, ErrorCode, ExportHtmlRequest, ExportPdfRequest,
    ExportResult, FitOutcome, FitVariant, ImportCommit, ImportMatch, ImportMatchRequest,
    ImportPreview, NewCv, NewVariant, PreviewRequest, PreviewResponse, PruneResult, SaveCv,
    SaveVariant, Target, TemplateManifest,
};
use cv_app::App;
use cv_core::{AtsReport, CvSummary, Id, LintIssue, StoredCv, StoredVariant, VariantSummary};
use tauri::State;

type Shared<'a> = State<'a, Arc<App>>;
type Reply<T> = Result<T, ApiError>;

/// Runs `f` off the async runtime: the services do blocking SQLite and Typst
/// work. Logs the command's name, duration and error code, never its
/// arguments or the error message, which can quote CV content.
async fn run<T, F>(app: Shared<'_>, command: &'static str, f: F) -> Reply<T>
where
    T: Send + 'static,
    F: FnOnce(&App) -> Reply<T> + Send + 'static,
{
    let app = Arc::clone(app.inner());
    let started = Instant::now();
    let result = tauri::async_runtime::spawn_blocking(move || f(&app))
        .await
        .map_err(|_| ApiError::new(ErrorCode::Internal, "The operation stopped unexpectedly."))
        .and_then(|r| r);
    let ms = started.elapsed().as_millis();
    match &result {
        Ok(_) => tracing::info!(command, ms, "ok"),
        Err(e) if e.code == ErrorCode::Cancelled => tracing::info!(command, ms, "cancelled"),
        Err(e) => tracing::warn!(command, ms, code = e.code.as_str(), "failed"),
    }
    result
}

#[tauri::command]
pub async fn list_cvs(app: Shared<'_>) -> Reply<Vec<CvSummary>> {
    run(app, "list_cvs", App::list_cvs).await
}

#[tauri::command]
pub async fn get_cv(app: Shared<'_>, id: Id) -> Reply<StoredCv> {
    run(app, "get_cv", move |a| a.get_cv(&id)).await
}

#[tauri::command]
pub async fn create_cv(app: Shared<'_>, req: NewCv) -> Reply<StoredCv> {
    run(app, "create_cv", move |a| a.create_cv(req)).await
}

#[tauri::command]
pub async fn save_cv(app: Shared<'_>, req: SaveCv) -> Reply<StoredCv> {
    run(app, "save_cv", move |a| a.save_cv(req)).await
}

#[tauri::command]
pub async fn duplicate_cv(app: Shared<'_>, req: DuplicateCv) -> Reply<StoredCv> {
    run(app, "duplicate_cv", move |a| a.duplicate_cv(req)).await
}

#[tauri::command]
pub async fn delete_cv(app: Shared<'_>, id: Id) -> Reply<()> {
    run(app, "delete_cv", move |a| a.delete_cv(&id)).await
}

#[tauri::command]
pub async fn restore_cv(app: Shared<'_>, id: Id) -> Reply<()> {
    run(app, "restore_cv", move |a| a.restore_cv(&id)).await
}

#[tauri::command]
pub async fn list_variants(app: Shared<'_>, cv_id: Id) -> Reply<Vec<VariantSummary>> {
    run(app, "list_variants", move |a| a.list_variants(&cv_id)).await
}

#[tauri::command]
pub async fn get_variant(app: Shared<'_>, id: Id) -> Reply<StoredVariant> {
    run(app, "get_variant", move |a| a.get_variant(&id)).await
}

#[tauri::command]
pub async fn create_variant(app: Shared<'_>, req: NewVariant) -> Reply<StoredVariant> {
    run(app, "create_variant", move |a| a.create_variant(req)).await
}

#[tauri::command]
pub async fn save_variant(app: Shared<'_>, req: SaveVariant) -> Reply<StoredVariant> {
    run(app, "save_variant", move |a| a.save_variant(req)).await
}

#[tauri::command]
pub async fn delete_variant(app: Shared<'_>, id: Id) -> Reply<()> {
    run(app, "delete_variant", move |a| a.delete_variant(&id)).await
}

#[tauri::command]
pub async fn duplicate_variant(app: Shared<'_>, req: DuplicateVariant) -> Reply<StoredVariant> {
    run(app, "duplicate_variant", move |a| a.duplicate_variant(req)).await
}

#[tauri::command]
pub async fn prune_variant(app: Shared<'_>, id: Id) -> Reply<PruneResult> {
    run(app, "prune_variant", move |a| a.prune_variant(&id)).await
}

#[tauri::command]
pub async fn fit_variant(app: Shared<'_>, req: FitVariant) -> Reply<FitOutcome> {
    run(app, "fit_variant", move |a| a.fit_variant(req)).await
}

#[tauri::command]
pub async fn preview(app: Shared<'_>, req: PreviewRequest) -> Reply<PreviewResponse> {
    run(app, "preview", move |a| a.preview(req)).await
}

#[tauri::command]
pub async fn export_pdf(app: Shared<'_>, req: ExportPdfRequest) -> Reply<ExportResult> {
    run(app, "export_pdf", move |a| a.export_pdf(req)).await
}

#[tauri::command]
pub async fn export_html(app: Shared<'_>, req: ExportHtmlRequest) -> Reply<ExportResult> {
    run(app, "export_html", move |a| a.export_html(req)).await
}

#[tauri::command]
pub async fn ats_check(app: Shared<'_>, target: Target) -> Reply<AtsReport> {
    run(app, "ats_check", move |a| a.ats_check(&target)).await
}

#[tauri::command]
pub async fn lint(app: Shared<'_>, target: Target) -> Reply<Vec<LintIssue>> {
    run(app, "lint", move |a| a.lint(&target)).await
}

#[tauri::command]
pub async fn list_templates(app: Shared<'_>) -> Reply<Vec<TemplateManifest>> {
    run(app, "list_templates", |a| a.list_templates()).await
}

#[tauri::command]
pub async fn import_parse(app: Shared<'_>) -> Reply<ImportPreview> {
    run(app, "import_parse", App::import_parse).await
}

#[tauri::command]
pub async fn import_match(app: Shared<'_>, req: ImportMatchRequest) -> Reply<ImportMatch> {
    run(app, "import_match", move |a| a.import_match(req)).await
}

#[tauri::command]
pub async fn import_commit(app: Shared<'_>, req: ImportCommit) -> Reply<StoredCv> {
    run(app, "import_commit", move |a| a.import_commit(req)).await
}

#[cfg(test)]
mod tests {
    //! Drives the commands through Tauri's IPC with the mock runtime, using
    //! the argument names the UI sends (`req`, `id`, `target`).

    use std::sync::Arc;

    use cv_app::{App, ExportKind, FileDialog, UlidIds};
    use cv_store::{SqliteStore, SystemClock};
    use serde_json::{Value, json};
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{
        INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
    };
    use tauri::webview::{InvokeRequest, WebviewWindow, WebviewWindowBuilder};

    /// Always cancels, so no test writes a file.
    struct Cancel;

    impl FileDialog for Cancel {
        fn choose_save_path(&self, _: &str, _: ExportKind) -> Option<std::path::PathBuf> {
            None
        }

        fn choose_open_path(&self, _: &[&str]) -> Option<std::path::PathBuf> {
            None
        }
    }

    fn window() -> WebviewWindow<MockRuntime> {
        let store = SqliteStore::open_in_memory(Box::new(SystemClock)).expect("store");
        let app = App::new(
            Box::new(store),
            Box::new(Cancel),
            Box::new(UlidIds),
            "preview://localhost",
        );
        let tauri_app = mock_builder()
            .manage(Arc::new(app))
            .invoke_handler(tauri::generate_handler![
                super::list_cvs,
                super::get_cv,
                super::create_cv,
                super::save_cv,
                super::delete_cv,
                super::restore_cv,
                super::preview,
                super::export_pdf,
                super::lint,
                super::list_templates,
                super::import_parse,
            ])
            .build(mock_context(noop_assets()))
            .expect("app");
        WebviewWindowBuilder::new(&tauri_app, "main", Default::default())
            .build()
            .expect("window")
    }

    fn invoke(w: &WebviewWindow<MockRuntime>, cmd: &str, args: Value) -> Result<Value, Value> {
        get_ipc_response(
            w,
            InvokeRequest {
                cmd: cmd.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                // Tauri serves the app from a different origin on Windows.
                url: if cfg!(windows) {
                    "http://tauri.localhost"
                } else {
                    "tauri://localhost"
                }
                .parse()
                .expect("url"),
                body: InvokeBody::Json(args),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.into(),
            },
        )
        .map(|b| b.deserialize::<Value>().expect("json"))
    }

    #[test]
    fn create_get_save_and_conflict_round_trip() {
        let w = window();
        let created =
            invoke(&w, "create_cv", json!({ "req": { "title": "Main" } })).expect("create");
        let id = created["cv"]["id"].clone();
        let revision = created["revision"].clone();

        let got = invoke(&w, "get_cv", json!({ "id": id })).expect("get");
        assert_eq!(got["cv"]["title"], "Main");

        let mut cv = got["cv"].clone();
        cv["basics"]["full_name"] = json!("Jane Doe");
        let saved = invoke(
            &w,
            "save_cv",
            json!({ "req": { "cv": cv, "expected_revision": revision } }),
        )
        .expect("save");
        assert_eq!(saved["cv"]["basics"]["full_name"], "Jane Doe");

        // A stale revision comes back as a structured conflict error.
        let err = invoke(
            &w,
            "save_cv",
            json!({ "req": { "cv": cv, "expected_revision": revision } }),
        )
        .expect_err("conflict");
        assert_eq!(err["code"], json!(cv_api::ErrorCode::Conflict));

        let list = invoke(&w, "list_cvs", json!({})).expect("list");
        assert_eq!(list.as_array().map(Vec::len), Some(1));
        invoke(&w, "delete_cv", json!({ "id": id })).expect("delete");
        assert_eq!(invoke(&w, "list_cvs", json!({})).expect("list"), json!([]));
        invoke(&w, "restore_cv", json!({ "id": id })).expect("restore");
    }

    #[test]
    fn errors_are_api_errors_not_panics() {
        let w = window();
        let missing = invoke(&w, "get_cv", json!({ "id": "01HZX3M8Q4V7K2N9P5R6T0W1YB" }))
            .expect_err("missing");
        assert_eq!(missing["code"], json!(cv_api::ErrorCode::NotFound));
        // A malformed id is rejected by deserialization before the service runs.
        assert!(invoke(&w, "get_cv", json!({ "id": "not-an-id" })).is_err());
    }

    #[test]
    fn preview_lint_and_cancelled_export() {
        let w = window();
        let created =
            invoke(&w, "create_cv", json!({ "req": { "title": "Main" } })).expect("create");
        let target = json!({ "cv_id": created["cv"]["id"], "variant_id": null });
        let preview = invoke(
            &w,
            "preview",
            json!({ "req": { "target": target, "revision": 3, "unsaved_cv": null, "unsaved_variant": null } }),
        )
        .expect("preview");
        assert_eq!(preview["revision"], 3);
        assert!(invoke(&w, "lint", json!({ "target": target })).is_ok());
        // Give the CV a name and email so it passes export validation and
        // reaches the dialog, which the test double always cancels.
        let mut cv = created["cv"].clone();
        cv["basics"]["full_name"] = json!("Jane Doe");
        cv["basics"]["email"] = json!("jane@example.com");
        invoke(
            &w,
            "save_cv",
            json!({ "req": { "cv": cv, "expected_revision": created["revision"] } }),
        )
        .expect("save");
        let cancelled = invoke(&w, "export_pdf", json!({ "req": { "target": target } }))
            .expect_err("cancelled");
        assert_eq!(cancelled["code"], json!(cv_api::ErrorCode::Cancelled));
    }

    #[test]
    fn list_templates_returns_the_classic_manifest() {
        let w = window();
        let list = invoke(&w, "list_templates", json!({})).expect("templates");
        assert_eq!(list[0]["id"], "classic");
        assert_eq!(list[0]["options"]["font_size_pt"]["min"], 9.0);
    }

    #[test]
    fn cancelled_import_is_an_api_error() {
        let w = window();
        let err = invoke(&w, "import_parse", json!({})).expect_err("cancelled");
        assert_eq!(err["code"], json!(cv_api::ErrorCode::Cancelled));
    }
}
