//! Application services (ADR-021): each operation from spec section 10.2 loads
//! what it needs from the store, resolves, renders or validates, and saves.
//!
//! `cv-app` is native-only and knows nothing about Tauri. The desktop shell
//! wraps each call in `spawn_blocking` and adapts it to a command; a future
//! HTTP server could call the same services. File dialogs are a trait the
//! shell implements, so the UI never passes a file path (ADR-036).
//! Imports (spec section 9.5) are in `import.rs`, AI jobs in `jobs.rs`.
#![forbid(unsafe_code)]

mod ai;
mod ai_import;
mod errors;
mod files;
mod ids;
mod import;
mod jobs;
mod preview;
mod review;
mod tailor;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use cv_api::{
    ApiError, DuplicateCv, DuplicateVariant, ErrorCode, ExportHtmlRequest, ExportPdfRequest,
    ExportResult, FitOutcome, FitVariant, NewCv, NewVariant, PreviewRequest, PreviewResponse,
    PruneResult, SaveCv, SaveVariant, Target, TemplateManifest,
};
use cv_core::fit::{FitResult, fit};
use cv_core::validate;
use cv_core::{
    AtsReport, Basics, CV_SCHEMA_VERSION, Cv, CvSummary, Id, IdSource, LintIssue, Locale, Paper,
    ResolvedCv, SnapshotReason, StoredCv, StoredVariant, TemplateId, TemplateOptions, Variant,
    VariantSummary,
};
use cv_render::{ExportOptions, Exporter, HtmlExporter, PdfExporter, Renderer, ats};
use cv_store::Store;

pub use ai::{AI_CONSENT_VERSION, AiState, NoKeychain, SecretError, SecretStore, llm_error};
pub use ai_import::{TEXT_EXTENSIONS, TextCache, to_partial as extracted_to_partial};
pub use errors::{render_error, store_error};
pub use files::{ExportKind, FileDialog, write_atomically};
pub use ids::UlidIds;
pub use import::{IMPORT_EXTENSIONS, ImportCache};
pub use jobs::{JobContext, JobEvent, JobEvents, JobRegistry};
pub use preview::PreviewCache;

/// Pixels per point for preview pages (ADR-025).
pub const PREVIEW_PIXELS_PER_PT: f32 = 1.5;

pub type Result<T> = std::result::Result<T, ApiError>;

/// The services behind every command. Cheap to share behind an `Arc`.
pub struct App {
    store: Box<dyn Store>,
    renderer: Renderer,
    ids: Mutex<Box<dyn IdSource + Send>>,
    review_lock: Mutex<()>,
    dialog: Box<dyn FileDialog>,
    previews: PreviewCache,
    imports: ImportCache,
    texts: TextCache,
    ai: AiState,
    jobs: JobRegistry,
    next_preview: AtomicU64,
    preview_base: String,
}

impl App {
    /// `preview_base` is the URL prefix the shell serves preview pages under,
    /// for example `preview://localhost` (ADR-025).
    pub fn new(
        store: Box<dyn Store>,
        dialog: Box<dyn FileDialog>,
        ids: Box<dyn IdSource + Send>,
        preview_base: impl Into<String>,
    ) -> Self {
        Self {
            store,
            renderer: Renderer::new(),
            ids: Mutex::new(ids),
            review_lock: Mutex::new(()),
            dialog,
            previews: PreviewCache::default(),
            imports: ImportCache::default(),
            texts: TextCache::default(),
            ai: AiState::default(),
            jobs: JobRegistry::default(),
            next_preview: AtomicU64::new(0),
            preview_base: preview_base.into(),
        }
    }

    /// Keeps API keys in `secrets`, the OS keychain. Without this call there
    /// is no keychain and keys are held in memory for the session.
    pub fn with_secrets(mut self, secrets: Box<dyn SecretStore>) -> Self {
        self.ai.set_secrets(secrets);
        self
    }

    fn next_id(&self) -> Id {
        self.ids
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .next_id()
    }

    fn with_ids<T>(&self, f: impl FnOnce(&mut dyn IdSource) -> T) -> T {
        let mut ids = self.ids.lock().unwrap_or_else(PoisonError::into_inner);
        f(ids.as_mut())
    }

    // CVs

    pub fn list_cvs(&self) -> Result<Vec<CvSummary>> {
        self.store.list_cvs().map_err(store_error)
    }

    pub fn get_cv(&self, id: &Id) -> Result<StoredCv> {
        self.store.get_cv(id).map_err(store_error)
    }

    /// Creates a blank CV: no name, contacts or sections yet.
    pub fn create_cv(&self, req: NewCv) -> Result<StoredCv> {
        let cv = blank_cv(self.next_id(), req.title.trim());
        self.store.create_cv(&cv).map_err(store_error)
    }

    /// Validates (clamping style options) and saves. Fails with `Conflict`
    /// when `expected_revision` is stale and `Validation` on structural errors.
    pub fn save_cv(&self, req: SaveCv) -> Result<StoredCv> {
        let mut cv = req.cv;
        let issues = validate::for_save(&mut cv);
        if validate::has_errors(&issues) {
            return Err(validation_error(&issues));
        }
        self.store
            .save_cv(&cv, req.expected_revision)
            .map_err(store_error)
    }

    pub fn duplicate_cv(&self, req: DuplicateCv) -> Result<StoredCv> {
        self.with_ids(|ids| self.store.duplicate_cv(&req.id, req.new_title.trim(), ids))
            .map_err(store_error)
    }

    /// Soft delete; [`App::restore_cv`] undoes it.
    pub fn delete_cv(&self, id: &Id) -> Result<()> {
        self.store.delete_cv(id).map_err(store_error)
    }

    pub fn restore_cv(&self, id: &Id) -> Result<()> {
        self.store.restore_cv(id).map_err(store_error)
    }

    // Variants

    pub fn list_variants(&self, cv_id: &Id) -> Result<Vec<VariantSummary>> {
        self.store.list_variants(cv_id).map_err(store_error)
    }

    pub fn get_variant(&self, id: &Id) -> Result<StoredVariant> {
        self.store.get_variant(id).map_err(store_error)
    }

    pub fn create_variant(&self, req: NewVariant) -> Result<StoredVariant> {
        // Fails with NotFound before creating an orphan.
        self.store.get_cv(&req.cv_id).map_err(store_error)?;
        let mut variant = Variant::empty(self.next_id(), req.cv_id, req.name.trim());
        variant.job = req.job;
        self.store.create_variant(&variant).map_err(store_error)
    }

    pub fn save_variant(&self, req: SaveVariant) -> Result<StoredVariant> {
        self.store
            .save_variant(&req.variant, req.expected_revision)
            .map_err(store_error)
    }

    pub fn delete_variant(&self, id: &Id) -> Result<()> {
        self.store.delete_variant(id).map_err(store_error)
    }

    pub fn duplicate_variant(&self, req: DuplicateVariant) -> Result<StoredVariant> {
        self.with_ids(|ids| {
            self.store
                .duplicate_variant(&req.id, req.new_name.trim(), ids)
        })
        .map_err(store_error)
    }

    /// Drops references to base content that no longer exists, and saves the
    /// variant if anything was removed.
    pub fn prune_variant(&self, id: &Id) -> Result<PruneResult> {
        let stored = self.store.get_variant(id).map_err(store_error)?;
        let cv = self
            .store
            .get_cv(&stored.variant.cv_id)
            .map_err(store_error)?;
        let mut variant = stored.variant;
        let removed = variant.prune(&cv.cv);
        if !removed.is_empty() {
            self.store
                .save_variant(&variant, stored.revision)
                .map_err(store_error)?;
        }
        Ok(PruneResult { removed })
    }

    /// Proposes the cuts that fit the variant in `page_target` pages and
    /// saves them as pending suggestions with the page target. Earlier
    /// pending cuts are replaced; nothing is hidden until the user accepts a
    /// cut (ADR-065).
    pub fn fit_variant(&self, req: FitVariant) -> Result<FitOutcome> {
        if !(1..=2).contains(&req.page_target) {
            return Err(ApiError::new(
                ErrorCode::Validation,
                "Choose 1 or 2 pages to fit to.",
            ));
        }
        let mut cv = self
            .store
            .get_cv(&req.variant.cv_id)
            .map_err(store_error)?
            .cv;
        // Count pages as a save would render them.
        let _ = validate::for_save(&mut cv);
        let mut variant = req.variant;
        variant.page_target = Some(req.page_target);
        variant.replace_fit_cuts(Vec::new());
        let result = self
            .with_ids(|ids| fit(&cv, &variant, None, req.page_target, &self.renderer, ids))
            .map_err(|e| {
                ApiError::new(
                    ErrorCode::Render,
                    "The CV could not be rendered to count its pages.",
                )
                .with_details(serde_json::json!({ "error": e.0 }))
            })?;
        let (pages, fits) = match result {
            FitResult::Fits { pages, cuts } => {
                variant.replace_fit_cuts(cuts);
                (pages, true)
            }
            FitResult::CannotFit { pages } => (pages, false),
        };
        let variant = self
            .store
            .save_variant(&variant, req.expected_revision)
            .map_err(store_error)?;
        Ok(FitOutcome {
            variant,
            pages,
            fits,
        })
    }

    // Rendering and export

    /// Renders the editor's current state, saved or not. Each call gets its
    /// own preview id, taken before rendering, so pages from two CVs that share
    /// an editor revision never collide. Pages are kept for
    /// [`App::preview_page`] until a later preview replaces them; a slow
    /// preview that finishes after a later one is not cached.
    pub fn preview(&self, req: PreviewRequest) -> Result<PreviewResponse> {
        let preview_id = self.next_preview.fetch_add(1, Ordering::Relaxed) + 1;
        let mut cv = match req.unsaved_cv {
            Some(cv) => cv,
            None => {
                self.store
                    .get_cv(&req.target.cv_id)
                    .map_err(store_error)?
                    .cv
            }
        };
        // Render what a save would store: options clamped, nothing rejected.
        let _ = validate::for_save(&mut cv);
        let variant = match (req.unsaved_variant, &req.target.variant_id) {
            (Some(v), _) => Some(v),
            (None, Some(id)) => Some(self.load_variant(&cv.id, id)?),
            (None, None) => None,
        };
        let resolved = cv.resolve(variant.as_ref());
        let compiled = self
            .renderer
            .compile(&resolved, &ExportOptions::default())
            .map_err(|d| render_error(cv_render::RenderError::Compile(d)))?;
        let pages = compiled
            .preview_png(PREVIEW_PIXELS_PER_PT)
            .map_err(render_error)?;
        let page_count = compiled.page_count();
        self.previews.put(preview_id, pages);
        Ok(PreviewResponse {
            revision: req.revision,
            page_count,
            page_urls: (1..=page_count)
                .map(|page| preview_url(&self.preview_base, preview_id, page))
                .collect(),
            diagnostics: compiled.warnings().to_vec(),
        })
    }

    /// PNG bytes of `page` (1-based) from preview `preview_id` (the number in
    /// the page URL), for the `preview://` handler. `None` once a later
    /// preview replaced it.
    pub fn preview_page(&self, preview_id: u64, page: u32) -> Option<Vec<u8>> {
        self.previews.get(preview_id, page)
    }

    /// Validates, renders and ATS-checks the PDF, then asks where to save it.
    pub fn export_pdf(&self, req: ExportPdfRequest) -> Result<ExportResult> {
        let resolved = self.resolve_for_export(&req.target)?;
        let doc = PdfExporter {
            renderer: &self.renderer,
        }
        .export(&resolved, &ExportOptions::default())
        .map_err(render_error)?;
        let report = ats::ats_report(&doc.bytes, &resolved);
        let path = self.save_file(&doc.suggested_file_name, ExportKind::Pdf, &doc.bytes)?;
        self.snapshot_after_export(&req.target.cv_id);
        Ok(ExportResult {
            path,
            page_count: doc.page_count,
            ats: Some(report),
        })
    }

    pub fn export_html(&self, req: ExportHtmlRequest) -> Result<ExportResult> {
        let resolved = self.resolve_for_export(&req.target)?;
        let options = ExportOptions {
            include_email: req.include_email,
            include_phone: req.include_phone,
        };
        let doc = HtmlExporter
            .export(&resolved, &options)
            .map_err(render_error)?;
        let path = self.save_file(&doc.suggested_file_name, ExportKind::Html, &doc.bytes)?;
        self.snapshot_after_export(&req.target.cv_id);
        Ok(ExportResult {
            path,
            page_count: None,
            ats: None,
        })
    }

    /// The ATS check on an in-memory PDF; nothing is written.
    pub fn ats_check(&self, target: &Target) -> Result<AtsReport> {
        let (cv, variant) = self.load_target(target)?;
        let resolved = cv.resolve(variant.as_ref());
        let pdf = self
            .renderer
            .compile(&resolved, &ExportOptions::default())
            .map_err(|d| render_error(cv_render::RenderError::Compile(d)))?
            .to_pdf()
            .map_err(render_error)?;
        Ok(ats::ats_report(&pdf, &resolved))
    }

    /// Lint findings, including the page-count rule from a fresh render.
    pub fn lint(&self, target: &Target) -> Result<Vec<LintIssue>> {
        let (cv, variant) = self.load_target(target)?;
        let resolved = cv.resolve(variant.as_ref());
        let pages = self
            .renderer
            .compile(&resolved, &ExportOptions::default())
            .ok()
            .map(|c| c.page_count());
        Ok(cv_core::lint::lint(&resolved, pages))
    }

    // Templates

    /// The embedded templates' manifests (v1: only `classic`).
    pub fn list_templates(&self) -> Result<Vec<TemplateManifest>> {
        templates()
    }

    // Helpers

    fn load_variant(&self, cv_id: &Id, variant_id: &Id) -> Result<Variant> {
        let variant = self
            .store
            .get_variant(variant_id)
            .map_err(store_error)?
            .variant;
        if &variant.cv_id != cv_id {
            return Err(ApiError::new(
                ErrorCode::NotFound,
                "That variant belongs to another CV.",
            ));
        }
        Ok(variant)
    }

    fn load_target(&self, target: &Target) -> Result<(Cv, Option<Variant>)> {
        let cv = self.store.get_cv(&target.cv_id).map_err(store_error)?.cv;
        let variant = match &target.variant_id {
            Some(id) => Some(self.load_variant(&cv.id, id)?),
            None => None,
        };
        Ok((cv, variant))
    }

    fn resolve_for_export(&self, target: &Target) -> Result<ResolvedCv> {
        let (cv, variant) = self.load_target(target)?;
        let issues = validate::for_export(&cv);
        if validate::has_errors(&issues) {
            return Err(validation_error(&issues));
        }
        Ok(cv.resolve(variant.as_ref()))
    }

    /// Asks the shell for a path, then writes the file. Returns the path for
    /// display.
    fn save_file(&self, suggested: &str, kind: ExportKind, bytes: &[u8]) -> Result<String> {
        let path = self
            .dialog
            .choose_save_path(suggested, kind)
            .ok_or_else(|| ApiError::new(ErrorCode::Cancelled, "Export cancelled."))?;
        // The io::Error text can carry the path, which holds the user's name.
        write_atomically(&path, bytes).map_err(|e| {
            ApiError::new(
                ErrorCode::Io,
                "Could not write the file. Check the folder's permissions and free space.",
            )
            .with_details(serde_json::json!({ "kind": format!("{:?}", e.kind()) }))
        })?;
        Ok(path.display().to_string())
    }

    /// Export snapshots are best effort: the file is already written, and the
    /// store throttles them to one per hour.
    fn snapshot_after_export(&self, cv_id: &Id) {
        let _ = self.with_ids(|ids| self.store.add_snapshot(cv_id, SnapshotReason::Export, ids));
    }
}

/// Parses the embedded template manifests.
pub fn templates() -> Result<Vec<TemplateManifest>> {
    let classic = serde_json::from_str(cv_render::CLASSIC_MANIFEST).map_err(|_| {
        ApiError::new(
            ErrorCode::Internal,
            "The built-in template list is damaged. Please report this.",
        )
    })?;
    Ok(vec![classic])
}

/// A new CV with every optional field empty.
pub fn blank_cv(id: Id, title: &str) -> Cv {
    let title = title.trim();
    Cv {
        schema_version: CV_SCHEMA_VERSION,
        id,
        title: if title.is_empty() {
            "Untitled CV".to_owned()
        } else {
            title.to_owned()
        },
        locale: Locale::default(),
        paper: Paper::default(),
        template: TemplateId::default(),
        options: TemplateOptions::default(),
        basics: Basics {
            full_name: String::new(),
            headline: None,
            email: None,
            phone: None,
            location: None,
            links: Vec::new(),
        },
        summary: None,
        sections: Vec::new(),
    }
}

/// `{base}/{preview_id}/{page}.png`, page 1-based.
pub fn preview_url(base: &str, preview_id: u64, page: u32) -> String {
    format!("{}/{preview_id}/{page}.png", base.trim_end_matches('/'))
}

fn validation_error(issues: &[cv_core::ValidationIssue]) -> ApiError {
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == cv_core::Severity::Error)
        .collect();
    let message = errors
        .iter()
        .map(|i| i.message.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    ApiError::new(ErrorCode::Validation, message)
        .with_details(serde_json::to_value(issues).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_cv_is_valid_to_save_but_not_to_export() {
        let mut cv = blank_cv(Id::parse("01HZX3M8Q4V7K2N9P5R6T0W1YB").expect("id"), " ");
        assert_eq!(cv.title, "Untitled CV");
        assert_eq!(blank_cv(cv.id.clone(), " Main ").title, "Main");
        assert!(!validate::has_errors(&validate::for_save(&mut cv)));
        assert!(validate::has_errors(&validate::for_export(&cv)));
    }

    #[test]
    fn classic_manifest_ranges_match_the_clamping_ranges() {
        let list = templates().expect("manifests parse");
        assert_eq!(list.len(), 1);
        let classic = &list[0];
        assert_eq!(classic.id, TemplateId::default());
        assert!(classic.ats_safe);
        let defaults = TemplateOptions::default();
        for (range, (min, max), default) in [
            (
                classic.options.font_size_pt,
                TemplateOptions::FONT_SIZE_PT,
                defaults.font_size_pt,
            ),
            (
                classic.options.line_spacing,
                TemplateOptions::LINE_SPACING,
                defaults.line_spacing,
            ),
            (
                classic.options.margin_mm,
                TemplateOptions::MARGIN_MM,
                defaults.margin_mm,
            ),
        ] {
            assert_eq!((range.min, range.max, range.default), (min, max, default));
        }
    }

    #[test]
    fn preview_urls_are_one_based_and_slash_safe() {
        assert_eq!(
            preview_url("preview://localhost/", 7, 1),
            "preview://localhost/7/1.png"
        );
        assert_eq!(
            preview_url("http://preview.localhost", 3, 2),
            "http://preview.localhost/3/2.png"
        );
    }

    #[test]
    fn validation_error_lists_errors_and_carries_all_issues() {
        let issue = |code: &str, severity| cv_core::ValidationIssue {
            code: code.into(),
            severity,
            path: "/basics".into(),
            message: format!("{code}."),
        };
        let issues = [
            issue("missing_name", cv_core::Severity::Error),
            issue("clamped", cv_core::Severity::Warning),
        ];
        let err = validation_error(&issues);
        assert_eq!(err.code, ErrorCode::Validation);
        assert_eq!(err.message, "missing_name.");
        assert_eq!(
            err.details
                .as_ref()
                .and_then(|d| d.as_array())
                .map(Vec::len),
            Some(2)
        );
    }
}
