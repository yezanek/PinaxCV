//! The optional signature image of the closing block (ADR-090): choosing,
//! cleaning, storing and drawing it only where it is switched on.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cv_api::{
    ErrorCode, ExportHtmlRequest, ExportPdfRequest, NewCv, NewVariant, SaveCv, SaveVariant, Target,
};
use cv_app::{App, ExportKind, FileDialog, PHOTO_EXTENSIONS, UlidIds};
use cv_core::{ClosingPart, Cv, JobTarget, StoredCv};
use cv_store::{SqliteStore, SystemClock};
use image::codecs::jpeg::JpegEncoder;
use image::{Rgb, RgbImage};

/// Opens `open` and saves into `dir`.
struct Dialog {
    open: Mutex<Option<PathBuf>>,
    dir: PathBuf,
}

impl FileDialog for Dialog {
    fn choose_save_path(&self, suggested: &str, _: ExportKind) -> Option<PathBuf> {
        Some(self.dir.join(suggested))
    }

    fn choose_open_path(&self, extensions: &[&str]) -> Option<PathBuf> {
        assert_eq!(extensions, PHOTO_EXTENSIONS);
        self.open.lock().expect("lock").clone()
    }
}

struct Fixture {
    app: App,
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let store =
            SqliteStore::open_with_clock(&dir.path().join("pinaxcv.db"), Box::new(SystemClock))
                .expect("store");
        let app = App::new(
            Box::new(store),
            Box::new(Dialog {
                open: Mutex::new(Some(dir.path().join("signature.jpg"))),
                dir: dir.path().to_owned(),
            }),
            Box::new(UlidIds),
            "preview://localhost",
        );
        Self { app, dir }
    }

    fn file(&self, bytes: &[u8]) {
        std::fs::write(self.dir.path().join("signature.jpg"), bytes).expect("write");
    }

    fn pdf_images(&self, target: Target) -> usize {
        let result = self
            .app
            .export_pdf(ExportPdfRequest {
                target,
                ats_copy: true,
            })
            .expect("export");
        images(&std::fs::read(&result.path).expect("pdf"))
    }

    fn html(&self, target: Target) -> String {
        let html = self
            .app
            .export_html(ExportHtmlRequest {
                target,
                include_email: true,
                include_phone: false,
            })
            .expect("html");
        std::fs::read_to_string(html.path).expect("read")
    }
}

/// A synthetic scan: off-white paper with a dark stroke.
fn scan() -> Vec<u8> {
    let mut page = RgbImage::from_pixel(300, 120, Rgb([248, 248, 240]));
    for x in 40..260 {
        for y in 50..58 {
            page.put_pixel(x, y, Rgb([20, 20, 70]));
        }
    }
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 90)
        .encode_image(&page)
        .expect("jpeg");
    out
}

/// Image objects in a PDF. A PNG with transparency counts twice: the image
/// and its soft mask.
fn images(pdf: &[u8]) -> usize {
    let doc = lopdf::Document::load_mem(pdf).expect("parses");
    doc.objects
        .values()
        .filter(|o| {
            o.as_stream()
                .ok()
                .and_then(|s| s.dict.get(b"Subtype").ok())
                .and_then(|t| t.as_name().ok())
                == Some(b"Image".as_slice())
        })
        .count()
}

fn filled(app: &App) -> StoredCv {
    let created = app
        .create_cv(NewCv {
            title: "Main".into(),
        })
        .expect("create");
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cvs/typical.json");
    let f: Cv =
        serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("parses");
    let mut cv = created.cv;
    cv.basics = f.basics;
    cv.sections = f.sections;
    app.save_cv(SaveCv {
        cv,
        expected_revision: created.revision,
    })
    .expect("save")
}

#[test]
fn the_signature_is_drawn_only_where_it_is_switched_on() {
    let f = Fixture::new();
    f.file(&scan());
    let stored = filled(&f.app);
    let id = stored.cv.id.clone();
    let info = f.app.signature_choose(&id).expect("choose");
    assert!(info.data_url.starts_with("data:image/png;base64,"));
    assert_eq!(f.app.signature_get(&id).expect("get"), Some(info));
    let base = Target {
        cv_id: id.clone(),
        variant_id: None,
    };
    assert_eq!(f.pdf_images(base.clone()), 0, "off until switched on");
    assert!(!f.html(base.clone()).contains("class=\"signature\""));

    let mut cv = stored.cv.clone();
    cv.closing.show.insert(ClosingPart::Signature);
    let cv = f
        .app
        .save_cv(SaveCv {
            cv,
            expected_revision: stored.revision,
        })
        .expect("save");
    assert_eq!(f.pdf_images(base.clone()), 2);
    assert!(
        f.html(base.clone())
            .contains("<img class=\"signature\" src=\"data:image/png;base64,")
    );

    // A new variant hides it until its own switch is on.
    let variant = f
        .app
        .create_variant(NewVariant {
            cv_id: id.clone(),
            name: "Acme".into(),
            job: JobTarget::default(),
        })
        .expect("variant");
    let target = Target {
        cv_id: id.clone(),
        variant_id: Some(variant.variant.id.clone()),
    };
    assert_eq!(f.pdf_images(target.clone()), 0);
    let mut v = variant.variant.clone();
    v.show_closing.insert(ClosingPart::Signature);
    f.app
        .save_variant(SaveVariant {
            variant: v,
            expected_revision: variant.revision,
        })
        .expect("save variant");
    assert_eq!(f.pdf_images(target.clone()), 2);

    // A duplicate keeps it; removing it leaves the switch with nothing to draw.
    let copy = f
        .app
        .duplicate_cv(cv_api::DuplicateCv {
            id: id.clone(),
            new_title: "Copy".into(),
        })
        .expect("duplicate");
    assert!(f.app.signature_get(&copy.cv.id).expect("get").is_some());
    f.app.signature_remove(&id).expect("remove");
    assert_eq!(f.app.signature_get(&id).expect("get"), None);
    assert_eq!(f.pdf_images(base), 0);
    assert_eq!(f.pdf_images(target), 0);
    assert!(cv.cv.closing.show.contains(&ClosingPart::Signature));
}

#[test]
fn bad_files_are_refused_and_keep_the_old_signature() {
    let f = Fixture::new();
    let id = filled(&f.app).cv.id;
    f.file(&scan());
    let kept = f.app.signature_choose(&id).expect("choose");
    f.file(b"%PDF-1.7 not an image");
    assert_eq!(
        f.app.signature_choose(&id).expect_err("pdf").code,
        ErrorCode::UnsupportedFormat
    );
    let mut blank = Vec::new();
    JpegEncoder::new_with_quality(&mut blank, 90)
        .encode_image(&RgbImage::from_pixel(50, 50, Rgb([255, 255, 255])))
        .expect("jpeg");
    f.file(&blank);
    assert_eq!(
        f.app.signature_choose(&id).expect_err("blank").code,
        ErrorCode::Validation
    );
    assert_eq!(f.app.signature_get(&id).expect("get"), Some(kept));
}
