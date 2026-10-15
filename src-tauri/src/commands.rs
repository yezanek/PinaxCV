//! Tauri commands: thin adapters that run a `cv-app` service on the blocking
//! pool (ADR-036). No business logic lives here.

use std::sync::Arc;

use cv_api::{
    ApiError, DuplicateCv, DuplicateVariant, ErrorCode, ExportHtmlRequest, ExportPdfRequest,
    ExportResult, NewCv, NewVariant, PreviewRequest, PreviewResponse, PruneResult, SaveCv,
    SaveVariant, Target,
};
use cv_app::App;
use cv_core::{AtsReport, CvSummary, Id, LintIssue, StoredCv, StoredVariant, VariantSummary};
use tauri::State;

type Shared<'a> = State<'a, Arc<App>>;
type Reply<T> = Result<T, ApiError>;

/// Runs `f` off the async runtime: the services do blocking SQLite and Typst work.
async fn run<T, F>(app: Shared<'_>, f: F) -> Reply<T>
where
    T: Send + 'static,
    F: FnOnce(&App) -> Reply<T> + Send + 'static,
{
    let app = Arc::clone(app.inner());
    tauri::async_runtime::spawn_blocking(move || f(&app))
        .await
        .map_err(|_| ApiError::new(ErrorCode::Internal, "The operation stopped unexpectedly."))?
}

#[tauri::command]
pub async fn list_cvs(app: Shared<'_>) -> Reply<Vec<CvSummary>> {
    run(app, App::list_cvs).await
}

#[tauri::command]
pub async fn get_cv(app: Shared<'_>, id: Id) -> Reply<StoredCv> {
    run(app, move |a| a.get_cv(&id)).await
}

#[tauri::command]
pub async fn create_cv(app: Shared<'_>, req: NewCv) -> Reply<StoredCv> {
    run(app, move |a| a.create_cv(req)).await
}

#[tauri::command]
pub async fn save_cv(app: Shared<'_>, req: SaveCv) -> Reply<StoredCv> {
    run(app, move |a| a.save_cv(req)).await
}

#[tauri::command]
pub async fn duplicate_cv(app: Shared<'_>, req: DuplicateCv) -> Reply<StoredCv> {
    run(app, move |a| a.duplicate_cv(req)).await
}

#[tauri::command]
pub async fn delete_cv(app: Shared<'_>, id: Id) -> Reply<()> {
    run(app, move |a| a.delete_cv(&id)).await
}

#[tauri::command]
pub async fn restore_cv(app: Shared<'_>, id: Id) -> Reply<()> {
    run(app, move |a| a.restore_cv(&id)).await
}

#[tauri::command]
pub async fn list_variants(app: Shared<'_>, cv_id: Id) -> Reply<Vec<VariantSummary>> {
    run(app, move |a| a.list_variants(&cv_id)).await
}

#[tauri::command]
pub async fn get_variant(app: Shared<'_>, id: Id) -> Reply<StoredVariant> {
    run(app, move |a| a.get_variant(&id)).await
}

#[tauri::command]
pub async fn create_variant(app: Shared<'_>, req: NewVariant) -> Reply<StoredVariant> {
    run(app, move |a| a.create_variant(req)).await
}

#[tauri::command]
pub async fn save_variant(app: Shared<'_>, req: SaveVariant) -> Reply<StoredVariant> {
    run(app, move |a| a.save_variant(req)).await
}

#[tauri::command]
pub async fn delete_variant(app: Shared<'_>, id: Id) -> Reply<()> {
    run(app, move |a| a.delete_variant(&id)).await
}

#[tauri::command]
pub async fn duplicate_variant(app: Shared<'_>, req: DuplicateVariant) -> Reply<StoredVariant> {
    run(app, move |a| a.duplicate_variant(req)).await
}

#[tauri::command]
pub async fn prune_variant(app: Shared<'_>, id: Id) -> Reply<PruneResult> {
    run(app, move |a| a.prune_variant(&id)).await
}

#[tauri::command]
pub async fn preview(app: Shared<'_>, req: PreviewRequest) -> Reply<PreviewResponse> {
    run(app, move |a| a.preview(req)).await
}

#[tauri::command]
pub async fn export_pdf(app: Shared<'_>, req: ExportPdfRequest) -> Reply<ExportResult> {
    run(app, move |a| a.export_pdf(req)).await
}

#[tauri::command]
pub async fn export_html(app: Shared<'_>, req: ExportHtmlRequest) -> Reply<ExportResult> {
    run(app, move |a| a.export_html(req)).await
}

#[tauri::command]
pub async fn ats_check(app: Shared<'_>, target: Target) -> Reply<AtsReport> {
    run(app, move |a| a.ats_check(&target)).await
}

#[tauri::command]
pub async fn lint(app: Shared<'_>, target: Target) -> Reply<Vec<LintIssue>> {
    run(app, move |a| a.lint(&target)).await
}
