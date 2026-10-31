//! Service tests against a real SQLite store and renderer, with a fake save
//! dialog.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cv_api::{
    DuplicateCv, EntryChoice, EntryDecision, ErrorCode, ExportHtmlRequest, ExportPdfRequest,
    FitVariant, ImportCommit, ImportField, ImportMatchRequest, ImportTarget, NewCv, NewVariant,
    PreviewRequest, SaveCv, SaveVariant, Target,
};
use cv_app::{App, ExportKind, FileDialog, UlidIds};
use cv_core::{
    Cv, JobTarget, Section, SnapshotReason, StoredCv, SuggestionSource, SuggestionStatus,
};
use cv_store::{SqliteStore, Store, SystemClock};

/// Answers every save dialog with a fixed folder, or cancels.
struct FakeDialog {
    dir: Option<PathBuf>,
    open: Option<PathBuf>,
    asked: Mutex<Vec<(String, ExportKind)>>,
}

impl FileDialog for FakeDialog {
    fn choose_save_path(&self, suggested: &str, kind: ExportKind) -> Option<PathBuf> {
        self.asked
            .lock()
            .expect("lock")
            .push((suggested.to_owned(), kind));
        self.dir.as_ref().map(|d| d.join(suggested))
    }

    fn choose_open_path(&self, _: &[&str]) -> Option<PathBuf> {
        self.open.clone()
    }
}

fn app_with(store: SqliteStore, dir: Option<&Path>) -> App {
    importing(store, dir, None)
}

/// An app whose open dialog picks `open`.
fn importing(store: SqliteStore, dir: Option<&Path>, open: Option<&Path>) -> App {
    App::new(
        Box::new(store),
        Box::new(FakeDialog {
            dir: dir.map(Path::to_path_buf),
            open: open.map(Path::to_path_buf),
            asked: Mutex::new(Vec::new()),
        }),
        Box::new(UlidIds),
        "preview://localhost",
    )
}

fn app(dir: Option<&Path>) -> App {
    let store = SqliteStore::open_in_memory(Box::new(SystemClock)).expect("store");
    app_with(store, dir)
}

fn fixture(name: &str) -> Cv {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/cvs")
        .join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(path).expect("read")).expect("parse")
}

/// Creates a CV and fills it with the content of fixture `name`.
fn filled(app: &App, name: &str) -> StoredCv {
    let created = app
        .create_cv(NewCv {
            title: "Main".into(),
        })
        .expect("create");
    let f = fixture(name);
    let mut cv = created.cv;
    cv.basics = f.basics;
    cv.summary = f.summary;
    cv.sections = f.sections;
    cv.layout = f.layout;
    app.save_cv(SaveCv {
        cv,
        expected_revision: created.revision,
    })
    .expect("save")
}

fn base(cv: &StoredCv) -> Target {
    Target {
        cv_id: cv.cv.id.clone(),
        variant_id: None,
    }
}

#[test]
fn create_save_conflict_delete_restore() {
    let app = app(None);
    let created = app
        .create_cv(NewCv {
            title: "  Main  ".into(),
        })
        .expect("create");
    assert_eq!(created.cv.title, "Main");
    assert_eq!(app.list_cvs().expect("list").len(), 1);

    let mut cv = created.cv.clone();
    cv.basics.full_name = "Jane Doe".into();
    let saved = app
        .save_cv(SaveCv {
            cv: cv.clone(),
            expected_revision: created.revision,
        })
        .expect("save");
    assert_eq!(saved.revision, created.revision + 1);
    assert_eq!(
        app.get_cv(&cv.id).expect("get").cv.basics.full_name,
        "Jane Doe"
    );

    let stale = app
        .save_cv(SaveCv {
            cv: cv.clone(),
            expected_revision: created.revision,
        })
        .expect_err("stale revision");
    assert_eq!(stale, cv_api::ApiError::conflict(saved.revision));

    app.delete_cv(&cv.id).expect("delete");
    assert!(app.list_cvs().expect("list").is_empty());
    app.restore_cv(&cv.id).expect("restore");
    assert_eq!(app.list_cvs().expect("list").len(), 1);

    let copy = app
        .duplicate_cv(DuplicateCv {
            id: cv.id.clone(),
            new_title: "Copy".into(),
        })
        .expect("duplicate");
    assert_ne!(copy.cv.id, cv.id);
    assert_eq!(copy.cv.title, "Copy");
}

#[test]
fn structural_errors_block_saving() {
    let app = app(None);
    let stored = filled(&app, "typical");
    let mut cv = stored.cv.clone();
    let first = cv.sections[0].clone();
    cv.sections.push(first);
    let err = app
        .save_cv(SaveCv {
            cv,
            expected_revision: stored.revision,
        })
        .expect_err("duplicate ids");
    assert_eq!(err.code, ErrorCode::Validation);
    assert!(err.details.is_some());
    assert_eq!(
        app.get_cv(&stored.cv.id).expect("get").revision,
        stored.revision
    );
}

#[test]
fn preview_serves_png_pages_for_unsaved_edits() {
    let app = app(None);
    let stored = filled(&app, "typical");
    let mut unsaved = stored.cv.clone();
    unsaved.basics.full_name = "Janet Doe".into();
    let resp = app
        .preview(PreviewRequest {
            target: base(&stored),
            revision: 5,
            unsaved_cv: Some(unsaved),
            unsaved_variant: None,
        })
        .expect("preview");
    assert_eq!(resp.revision, 5);
    assert_eq!(resp.page_count, 1);
    assert_eq!(resp.page_urls, ["preview://localhost/1/1.png"]);
    let png = app.preview_page(1, 1).expect("page");
    assert!(png.starts_with(b"\x89PNG"));
    assert_eq!(app.preview_page(5, 1), None);
    // The unsaved edit was rendered, not saved.
    assert_eq!(
        app.get_cv(&stored.cv.id).expect("get").cv.basics.full_name,
        "Jane Doe"
    );
}

#[test]
fn pdf_export_writes_the_chosen_file_and_reports_ats() {
    let dir = tempfile::tempdir().expect("tempdir");
    let app = app(Some(dir.path()));
    let stored = filled(&app, "typical");
    let result = app
        .export_pdf(ExportPdfRequest {
            target: base(&stored),
            ats_copy: true,
        })
        .expect("export");
    let path = dir.path().join("Jane-Doe-CV.pdf");
    assert_eq!(result.path, path.display().to_string());
    assert_eq!(result.page_count, Some(1));
    assert_eq!(result.ats.expect("ats").score, 100);
    assert!(std::fs::read(&path).expect("pdf").starts_with(b"%PDF"));
    assert_eq!(result.ats_copy_path, None, "one column: no copy");
    assert!(!dir.path().join("Jane-Doe-CV-ats.pdf").exists());
}

#[test]
fn two_column_pdf_export_also_writes_a_one_column_ats_copy() {
    let dir = tempfile::tempdir().expect("tempdir");
    let app = app(Some(dir.path()));
    let stored = filled(&app, "two_column");
    let result = app
        .export_pdf(ExportPdfRequest {
            target: base(&stored),
            ats_copy: true,
        })
        .expect("export");
    let copy = dir.path().join("Jane-Doe-CV-ats.pdf");
    assert_eq!(result.ats_copy_path, Some(copy.display().to_string()));
    let bytes = std::fs::read(&copy).expect("copy");
    let text =
        cv_render::TextExtractor::extract(&cv_render::ats::LopdfExtractor, &bytes).expect("text");
    // One column: the summary and jobs come before skills, as in the CV order.
    let at = |s: &str| text.find(s).unwrap_or_else(|| panic!("{s} missing"));
    assert!(at("Jane Doe") < at("Summary"));
    assert!(at("Work Experience") < at("Skills"));

    std::fs::remove_file(&copy).expect("remove");
    let result = app
        .export_pdf(ExportPdfRequest {
            target: base(&stored),
            ats_copy: false,
        })
        .expect("export");
    assert_eq!(result.ats_copy_path, None);
    assert!(!copy.exists(), "turned off");
}

#[test]
fn the_ats_copy_never_replaces_someone_elses_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let app = app(Some(dir.path()));
    let stored = filled(&app, "two_column");
    let export = || {
        app.export_pdf(ExportPdfRequest {
            target: base(&stored),
            ats_copy: true,
        })
        .expect("export")
    };
    // A new PDF next to an unrelated file with the copy's name: kept.
    let other = dir.path().join("Jane-Doe-CV-ats.pdf");
    std::fs::write(&other, b"not ours").expect("write");
    let second = dir.path().join("Jane-Doe-CV-ats-2.pdf");
    assert_eq!(export().ats_copy_path, Some(second.display().to_string()));
    assert_eq!(std::fs::read(&other).expect("read"), b"not ours");
}

#[test]
fn exporting_again_replaces_the_earlier_ats_copy() {
    let dir = tempfile::tempdir().expect("tempdir");
    let app = app(Some(dir.path()));
    let stored = filled(&app, "two_column");
    let export = || {
        app.export_pdf(ExportPdfRequest {
            target: base(&stored),
            ats_copy: true,
        })
        .expect("export")
    };
    let copy = dir.path().join("Jane-Doe-CV-ats.pdf");
    assert_eq!(export().ats_copy_path, Some(copy.display().to_string()));
    // The dialog asked before replacing the PDF; its copy goes with it.
    assert_eq!(export().ats_copy_path, Some(copy.display().to_string()));
    assert!(!dir.path().join("Jane-Doe-CV-ats-2.pdf").exists());
}

#[test]
fn cancelled_dialog_writes_nothing() {
    let app = app(None);
    let stored = filled(&app, "typical");
    let err = app
        .export_pdf(ExportPdfRequest {
            target: base(&stored),
            ats_copy: true,
        })
        .expect_err("cancelled");
    assert_eq!(err.code, ErrorCode::Cancelled);
}

#[test]
fn incomplete_cv_cannot_be_exported() {
    let dir = tempfile::tempdir().expect("tempdir");
    let app = app(Some(dir.path()));
    let blank = app
        .create_cv(NewCv {
            title: "Blank".into(),
        })
        .expect("create");
    let err = app
        .export_html(ExportHtmlRequest {
            target: base(&blank),
            include_email: true,
            include_phone: false,
        })
        .expect_err("no name");
    assert_eq!(err.code, ErrorCode::Validation);
    assert_eq!(std::fs::read_dir(dir.path()).expect("list").count(), 0);
}

#[test]
fn html_export_respects_contact_choices() {
    let dir = tempfile::tempdir().expect("tempdir");
    let app = app(Some(dir.path()));
    let stored = filled(&app, "typical");
    let result = app
        .export_html(ExportHtmlRequest {
            target: base(&stored),
            include_email: true,
            include_phone: false,
        })
        .expect("export");
    assert_eq!(result.ats, None);
    let html = std::fs::read_to_string(dir.path().join("Jane-Doe-CV.html")).expect("html");
    assert!(html.contains("jane@example.com"));
    assert!(!html.contains("+49 30"));
}

#[test]
fn variants_render_their_overlay_and_prune_stale_references() {
    let app = app(None);
    let stored = filled(&app, "typical");
    let variant = app
        .create_variant(NewVariant {
            cv_id: stored.cv.id.clone(),
            name: "Acme".into(),
            job: JobTarget::default(),
        })
        .expect("variant");
    let hidden = stored.cv.sections[0].id().clone();
    let mut v = variant.variant.clone();
    v.hidden.insert(hidden.clone());
    let saved = app
        .save_variant(SaveVariant {
            variant: v,
            expected_revision: variant.revision,
        })
        .expect("save variant");
    let target = Target {
        cv_id: stored.cv.id.clone(),
        variant_id: Some(saved.variant.id.clone()),
    };
    let report = app.ats_check(&target).expect("ats");
    assert!(!report.extracted_text.contains("Work Experience"));
    assert!(app.lint(&target).is_ok());

    // A variant of another CV is not accepted for this one.
    let other = filled(&app, "minimal");
    let wrong = Target {
        cv_id: other.cv.id.clone(),
        variant_id: Some(saved.variant.id.clone()),
    };
    assert_eq!(
        app.ats_check(&wrong).expect_err("wrong cv").code,
        ErrorCode::NotFound
    );

    // Removing the hidden section from the base makes the reference stale.
    let mut cv = app.get_cv(&stored.cv.id).expect("get");
    cv.cv.sections.remove(0);
    app.save_cv(SaveCv {
        cv: cv.cv,
        expected_revision: cv.revision,
    })
    .expect("save");
    let pruned = app.prune_variant(&saved.variant.id).expect("prune");
    assert_eq!(pruned.removed, [hidden]);
    assert!(
        app.get_variant(&saved.variant.id)
            .expect("get")
            .variant
            .hidden
            .is_empty()
    );
    assert!(
        app.prune_variant(&saved.variant.id)
            .expect("again")
            .removed
            .is_empty()
    );
}

#[test]
fn variant_for_missing_cv_is_refused() {
    let app = app(None);
    let err = app
        .create_variant(NewVariant {
            cv_id: cv_core::Id::parse("01HZX3M8Q4V7K2N9P5R6T0W1YB").expect("id"),
            name: "x".into(),
            job: JobTarget::default(),
        })
        .expect_err("no cv");
    assert_eq!(err.code, ErrorCode::NotFound);
}

/// M3 acceptance: data survives closing and reopening the app.
#[test]
fn data_survives_a_restart() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("pinaxcv.sqlite3");
    let id = {
        let app = app_with(SqliteStore::open(&db).expect("open"), None);
        filled(&app, "typical").cv.id
    };
    let app = app_with(SqliteStore::open(&db).expect("reopen"), None);
    let cv = app.get_cv(&id).expect("get").cv;
    assert_eq!(cv.basics.full_name, "Jane Doe");
    assert_eq!(cv.sections.len(), fixture("typical").sections.len());
}

/// The synthetic Europass sample, written to `dir`, with `edits` applied.
fn europass_file(dir: &Path, edits: &[(&str, &str)]) -> PathBuf {
    let mut xml = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/import/europass-sample.xml"),
    )
    .expect("read sample");
    for (from, to) in edits {
        xml = xml.replace(from, to);
    }
    let path = dir.join("cv.xml");
    std::fs::write(&path, xml).expect("write sample");
    path
}

#[test]
fn import_into_a_new_cv() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = europass_file(dir.path(), &[]);
    let store = SqliteStore::open_in_memory(Box::new(SystemClock)).expect("store");
    let app = importing(store, None, Some(&file));
    let preview = app.import_parse().expect("parse");
    assert_eq!(preview.full_name.as_deref(), Some("Alex Sample"));
    let stored = app
        .import_commit(ImportCommit {
            preview_id: preview.preview_id,
            target: ImportTarget::New { title: " ".into() },
            use_imported: Vec::new(),
            entries: Vec::new(),
            use_imported_photo: false,
        })
        .expect("commit");
    assert_eq!(stored.cv.title, "Alex Sample");
    assert_eq!(stored.cv.basics.email.as_deref(), Some("alex@example.org"));
    assert_eq!(stored.cv.sections, preview.sections);
    assert_eq!(app.list_cvs().expect("list").len(), 1);
    // A committed preview is gone.
    let again = app.import_match(ImportMatchRequest {
        preview_id: preview.preview_id,
        cv_id: stored.cv.id.clone(),
    });
    assert_eq!(again.map_err(|e| e.code), Err(ErrorCode::Import));
}

/// Spec section 9.5: the merge review's choices are applied, and the CV is
/// snapshotted before it changes.
#[test]
fn import_merges_into_an_existing_cv_after_a_snapshot() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("pinaxcv.sqlite3");
    let file = europass_file(
        dir.path(),
        &[
            ("Example Works", "Example AG"),
            ("Platform Engineer", "Backend Engineer"),
            ("2019-05", "2017-01"),
        ],
    );
    let app = importing(SqliteStore::open(&db).expect("open"), None, Some(&file));
    let base = filled(&app, "typical");
    let preview = app.import_parse().expect("parse");
    let matched = app
        .import_match(ImportMatchRequest {
            preview_id: preview.preview_id,
            cv_id: base.cv.id.clone(),
        })
        .expect("match");
    let fields: Vec<_> = matched.conflicts.iter().map(|c| c.field).collect();
    assert_eq!(fields, [ImportField::FullName, ImportField::Email]);
    assert_eq!(matched.conflicts[0].mine, "Jane Doe");
    assert_eq!(matched.matches.len(), 1);
    let m = &matched.matches[0];

    // A stale revision is refused before anything changes.
    let stale = app.import_commit(ImportCommit {
        preview_id: preview.preview_id,
        target: ImportTarget::Merge {
            cv_id: base.cv.id.clone(),
            expected_revision: base.revision + 1,
        },
        use_imported: Vec::new(),
        entries: Vec::new(),
        use_imported_photo: false,
    });
    assert_eq!(stale.map_err(|e| e.code), Err(ErrorCode::Conflict));

    let merged = app
        .import_commit(ImportCommit {
            preview_id: preview.preview_id,
            target: ImportTarget::Merge {
                cv_id: base.cv.id.clone(),
                expected_revision: base.revision,
            },
            use_imported: vec![ImportField::Email],
            entries: vec![EntryDecision {
                imported: m.imported.clone(),
                choice: EntryChoice::Replace {
                    base: m.base.clone(),
                },
            }],
            use_imported_photo: false,
        })
        .expect("merge");
    assert_eq!(merged.cv.basics.full_name, "Jane Doe");
    assert_eq!(merged.cv.basics.email.as_deref(), Some("alex@example.org"));
    let Some(Section::Experience(x)) = merged.cv.sections.first() else {
        panic!("experience first");
    };
    assert_eq!(x.entries.len(), 2, "replaced, not added");
    let replaced = x.entries.iter().find(|e| e.id == m.base).expect("kept id");
    assert_eq!(replaced.bullets[0].text, "Ran the build farm.");

    let check = SqliteStore::open(&db).expect("second connection");
    let snapshots = check.list_snapshots(&base.cv.id).expect("snapshots");
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].reason, SnapshotReason::Import);
}

#[test]
fn cancelled_or_unsupported_imports_save_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = SqliteStore::open_in_memory(Box::new(SystemClock)).expect("store");
    let cancelled = importing(store, None, None).import_parse();
    assert_eq!(cancelled.map_err(|e| e.code), Err(ErrorCode::Cancelled));

    let file = dir.path().join("notes.xml");
    std::fs::write(&file, "<notes>not a CV</notes>").expect("write");
    let store = SqliteStore::open_in_memory(Box::new(SystemClock)).expect("store");
    let app = importing(store, None, Some(&file));
    let err = app.import_parse().expect_err("unsupported");
    assert_eq!(err.code, ErrorCode::UnsupportedFormat);
    assert!(app.list_cvs().expect("list").is_empty());
}

#[test]
fn fit_variant_saves_pending_cuts_that_fit_once_accepted() {
    let app = app(None);
    let stored = filled(&app, "long");
    let variant = app
        .create_variant(NewVariant {
            cv_id: stored.cv.id.clone(),
            name: "Short".into(),
            job: JobTarget::default(),
        })
        .expect("variant");
    let target = Target {
        cv_id: stored.cv.id.clone(),
        variant_id: Some(variant.variant.id.clone()),
    };
    let before = app
        .preview(PreviewRequest {
            target: target.clone(),
            revision: 1,
            unsaved_cv: None,
            unsaved_variant: None,
        })
        .expect("preview")
        .page_count;
    assert!(before > 1, "the long fixture should need more than a page");

    let outcome = app
        .fit_variant(FitVariant {
            variant: variant.variant.clone(),
            expected_revision: variant.revision,
            page_target: 1,
        })
        .expect("fit");
    let mut fitted = outcome.variant;
    assert_eq!(fitted.variant.page_target, Some(1));
    assert!(fitted.revision > variant.revision);
    assert!(fitted.variant.hidden.is_empty() && fitted.variant.collapsed.is_empty());
    assert!(!fitted.variant.suggestions.is_empty());
    assert!(
        fitted.variant.suggestions.iter().all(|s| {
            s.status == SuggestionStatus::Pending && s.source == SuggestionSource::Fit
        })
    );

    // Fitting again replaces the pending cuts instead of adding to them.
    let again = app
        .fit_variant(FitVariant {
            variant: fitted.variant.clone(),
            expected_revision: fitted.revision,
            page_target: 1,
        })
        .expect("fit again");
    assert_eq!(
        again.variant.variant.suggestions.len(),
        fitted.variant.suggestions.len()
    );
    fitted = again.variant;

    assert!(outcome.fits, "long fits on one page once cut");
    {
        let ids: Vec<_> = fitted
            .variant
            .suggestions
            .iter()
            .map(|s| s.id.clone())
            .collect();
        for id in &ids {
            assert!(fitted.variant.accept_suggestion(id));
        }
        let pages = app
            .preview(PreviewRequest {
                target,
                revision: 2,
                unsaved_cv: None,
                unsaved_variant: Some(fitted.variant.clone()),
            })
            .expect("preview")
            .page_count;
        assert_eq!(pages, outcome.pages);
        assert!(pages <= 1);
    }

    let err = app
        .fit_variant(FitVariant {
            variant: fitted.variant,
            expected_revision: fitted.revision,
            page_target: 3,
        })
        .expect_err("3 pages");
    assert_eq!(err.code, ErrorCode::Validation);
}
