//! Service tests against a real SQLite store and renderer, with a fake save
//! dialog.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cv_api::{
    DuplicateCv, ErrorCode, ExportHtmlRequest, ExportPdfRequest, NewCv, NewVariant, PreviewRequest,
    SaveCv, SaveVariant, Target,
};
use cv_app::{App, ExportKind, SaveDialog, UlidIds};
use cv_core::{Cv, JobTarget, StoredCv};
use cv_store::{SqliteStore, SystemClock};

/// Answers every save dialog with a fixed folder, or cancels.
struct FakeDialog {
    dir: Option<PathBuf>,
    asked: Mutex<Vec<(String, ExportKind)>>,
}

impl SaveDialog for FakeDialog {
    fn choose_save_path(&self, suggested: &str, kind: ExportKind) -> Option<PathBuf> {
        self.asked
            .lock()
            .expect("lock")
            .push((suggested.to_owned(), kind));
        self.dir.as_ref().map(|d| d.join(suggested))
    }
}

fn app_with(store: SqliteStore, dir: Option<&Path>) -> App {
    App::new(
        Box::new(store),
        Box::new(FakeDialog {
            dir: dir.map(Path::to_path_buf),
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
    assert_eq!(resp.page_urls, ["preview://localhost/5/1.png"]);
    let png = app.preview_page(5, 1).expect("page");
    assert!(png.starts_with(b"\x89PNG"));
    assert_eq!(app.preview_page(4, 1), None);
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
        })
        .expect("export");
    let path = dir.path().join("Jane-Doe-CV.pdf");
    assert_eq!(result.path, path.display().to_string());
    assert_eq!(result.page_count, Some(1));
    assert_eq!(result.ats.expect("ats").score, 100);
    assert!(std::fs::read(&path).expect("pdf").starts_with(b"%PDF"));
}

#[test]
fn cancelled_dialog_writes_nothing() {
    let app = app(None);
    let stored = filled(&app, "typical");
    let err = app
        .export_pdf(ExportPdfRequest {
            target: base(&stored),
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
