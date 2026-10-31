//! The optional profile photo (ADR-083): choosing, cropping, storing and
//! drawing it only where it is switched on, and importing it from a
//! Europass CV (issue #116).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cv_api::{
    ErrorCode, ExportHtmlRequest, ExportPdfRequest, ImportCommit, ImportMatchRequest, ImportTarget,
    NewCv, NewVariant, PhotoApply, PhotoCrop, PhotoShape, SaveCv, SaveVariant, Target,
};
use cv_app::{App, ExportKind, FileDialog, MAX_PHOTO_FILE, PHOTO_EXTENSIONS, UlidIds};
use cv_core::{Cv, JobTarget, StoredCv};
use cv_store::{SqliteStore, SystemClock};
use image::codecs::jpeg::JpegEncoder;
use image::{ImageReader, RgbImage};

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
        // The photo dialog, or the import dialog for a Europass file.
        if extensions.contains(&"xml") {
            return self
                .open
                .lock()
                .expect("lock")
                .as_ref()
                .map(|p| p.with_file_name("cv.xml"));
        }
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
                open: Mutex::new(Some(dir.path().join("photo.jpg"))),
                dir: dir.path().to_owned(),
            }),
            Box::new(UlidIds),
            "preview://localhost",
        );
        Self { app, dir }
    }

    fn file(&self, bytes: &[u8]) {
        std::fs::write(self.dir.path().join("photo.jpg"), bytes).expect("write");
    }

    /// Writes the synthetic Europass sample with `photo` attached as its
    /// profile picture, for the import dialog to open.
    fn europass(&self, photo: &[u8]) {
        use base64::Engine as _;
        let sample = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/import/europass-sample.xml"),
        )
        .expect("sample");
        let b64 = base64::engine::general_purpose::STANDARD.encode(photo);
        let attachment = format!(
            "<eures:Attachment><oa:EmbeddedData mimeCode=\"image/jpeg\" encodingCode=\"base64Binary\">{b64}</oa:EmbeddedData><hr:Instructions>ProfilePicture</hr:Instructions></eures:Attachment>\n  </CandidateProfile>"
        );
        let xml = sample.replacen("</CandidateProfile>", &attachment, 1);
        std::fs::write(self.dir.path().join("cv.xml"), xml).expect("write");
    }

    /// Imports the Europass file into `target`.
    fn import(&self, target: ImportTarget, use_imported_photo: bool) -> StoredCv {
        let preview = self.app.import_parse().expect("parse");
        self.app
            .import_commit(ImportCommit {
                preview_id: preview.preview_id,
                target,
                use_imported: Vec::new(),
                entries: Vec::new(),
                use_imported_photo,
            })
            .expect("commit")
    }

    fn pdf_images(&self, target: Target) -> usize {
        let result = self
            .app
            .export_pdf(ExportPdfRequest { target })
            .expect("export");
        images(&std::fs::read(&result.path).expect("pdf"))
    }
}

/// A 120 × 80 JPEG whose EXIF says "rotate 90° clockwise" and carries a
/// description standing in for camera and GPS data.
fn camera_jpeg() -> Vec<u8> {
    let mut img = RgbImage::new(120, 80);
    for (x, _, p) in img.enumerate_pixels_mut() {
        *p = image::Rgb([u8::try_from(x * 2).expect("small"), 90, 160]);
    }
    let mut plain = Vec::new();
    JpegEncoder::new_with_quality(&mut plain, 90)
        .encode_image(&img)
        .expect("encode");
    let marker = b"SECRET-GPS-48.85N-2.35E\0";
    // Big-endian TIFF with two entries: Orientation = 6 and ImageDescription.
    let mut tiff = b"MM\0*\0\0\0\x08\0\x02".to_vec();
    tiff.extend([0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, 6, 0, 0]);
    let offset = 8 + 2 + 2 * 12 + 4;
    tiff.extend([
        0x01,
        0x0e,
        0,
        2,
        0,
        0,
        0,
        u8::try_from(marker.len()).expect("short"),
    ]);
    tiff.extend(u32::try_from(offset).expect("small").to_be_bytes());
    tiff.extend([0, 0, 0, 0]);
    tiff.extend(marker);
    let mut app1 = b"Exif\0\0".to_vec();
    app1.extend(tiff);
    let mut out = vec![0xff, 0xd8, 0xff, 0xe1];
    out.extend(u16::try_from(app1.len() + 2).expect("small").to_be_bytes());
    out.extend(app1);
    out.extend(&plain[2..]);
    out
}

fn decode_data_url(url: &str) -> Vec<u8> {
    use base64::Engine as _;
    let b64 = url
        .strip_prefix("data:image/jpeg;base64,")
        .expect("jpeg data URL");
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .expect("base64")
}

fn size(jpeg: &[u8]) -> (u32, u32) {
    ImageReader::with_format(std::io::Cursor::new(jpeg), image::ImageFormat::Jpeg)
        .into_dimensions()
        .expect("dimensions")
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

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

fn crop(x: u32, y: u32, width: u32, height: u32, shape: PhotoShape) -> PhotoCrop {
    PhotoCrop {
        x,
        y,
        width,
        height,
        shape,
    }
}

#[test]
fn a_chosen_photo_is_upright_and_loses_its_metadata() {
    let f = Fixture::new();
    let original = camera_jpeg();
    assert!(contains(&original, b"SECRET-GPS"));
    f.file(&original);
    let source = f.app.photo_choose().expect("choose");
    assert_eq!((source.width, source.height), (80, 120), "turned upright");
    let jpeg = decode_data_url(&source.data_url);
    assert_eq!(size(&jpeg), (80, 120));
    assert!(!contains(&jpeg, b"SECRET-GPS") && !contains(&jpeg, b"Exif"));

    let cv = filled(&f.app);
    let info = f
        .app
        .photo_apply(PhotoApply {
            cv_id: cv.cv.id.clone(),
            source_id: source.source_id,
            crop: crop(0, 10, 80, 107, PhotoShape::Portrait),
        })
        .expect("apply");
    // Trimmed to exactly 3:4, and never scaled up.
    assert_eq!(info.crop, crop(1, 11, 78, 104, PhotoShape::Portrait));
    let image = decode_data_url(&info.data_url);
    assert_eq!(size(&image), (78, 104));
    assert!(!contains(&image, b"SECRET-GPS"));
    assert_eq!(f.app.photo_get(&cv.cv.id).expect("get"), Some(info.clone()));
    // Nothing of the original file's metadata reaches the database.
    drop(f.app);
    let db = std::fs::read(f.dir.path().join("pinaxcv.db")).expect("db");
    assert!(!contains(&db, b"SECRET-GPS"));
}

#[test]
fn the_photo_is_drawn_only_where_it_is_switched_on() {
    let f = Fixture::new();
    f.file(&camera_jpeg());
    let stored = filled(&f.app);
    let id = stored.cv.id.clone();
    let source = f.app.photo_choose().expect("choose");
    f.app
        .photo_apply(PhotoApply {
            cv_id: id.clone(),
            source_id: source.source_id,
            crop: crop(0, 0, 80, 80, PhotoShape::Square),
        })
        .expect("apply");
    let base = Target {
        cv_id: id.clone(),
        variant_id: None,
    };
    assert_eq!(f.pdf_images(base.clone()), 0, "off until switched on");

    let mut cv = f.app.get_cv(&id).expect("get");
    cv.cv.basics.show_photo = true;
    let cv = f
        .app
        .save_cv(SaveCv {
            cv: cv.cv,
            expected_revision: cv.revision,
        })
        .expect("save");
    assert_eq!(f.pdf_images(base.clone()), 1);
    let html = f
        .app
        .export_html(ExportHtmlRequest {
            target: base.clone(),
            include_email: true,
            include_phone: false,
        })
        .expect("html");
    let html = std::fs::read_to_string(html.path).expect("read");
    assert!(html.contains("<img class=\"photo\" src=\"data:image/jpeg;base64,"));

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
    v.show_photo = true;
    f.app
        .save_variant(SaveVariant {
            variant: v,
            expected_revision: variant.revision,
        })
        .expect("save variant");
    assert_eq!(f.pdf_images(target.clone()), 1);

    // A duplicate keeps it; removing it leaves the switch with nothing to draw.
    let copy = f
        .app
        .duplicate_cv(cv_api::DuplicateCv {
            id: id.clone(),
            new_title: "Copy".into(),
        })
        .expect("duplicate");
    assert!(f.app.photo_get(&copy.cv.id).expect("get").is_some());
    f.app.photo_remove(&id).expect("remove");
    assert_eq!(f.app.photo_get(&id).expect("get"), None);
    assert_eq!(f.pdf_images(base), 0);
    assert_eq!(f.pdf_images(target), 0);
    assert!(cv.cv.basics.show_photo);
}

#[test]
fn the_crop_can_be_redone_from_the_kept_original() {
    let f = Fixture::new();
    f.file(&camera_jpeg());
    let cv = filled(&f.app);
    let source = f.app.photo_choose().expect("choose");
    let first = f
        .app
        .photo_apply(PhotoApply {
            cv_id: cv.cv.id.clone(),
            source_id: source.source_id,
            crop: crop(0, 0, 80, 80, PhotoShape::Square),
        })
        .expect("apply");
    let again = f
        .app
        .photo_apply(PhotoApply {
            cv_id: cv.cv.id.clone(),
            source_id: None,
            crop: crop(20, 40, 60, 80, PhotoShape::Portrait),
        })
        .expect("recrop");
    assert_eq!(again.original, first.original);
    assert_eq!(again.crop, crop(20, 40, 60, 80, PhotoShape::Portrait));
    assert_eq!(size(&decode_data_url(&again.data_url)), (60, 80));
}

#[test]
fn bad_files_and_crops_are_refused() {
    let f = Fixture::new();
    let cv = filled(&f.app);
    f.file(b"%PDF-1.7 not a photo");
    assert_eq!(
        f.app.photo_choose().expect_err("pdf").code,
        ErrorCode::UnsupportedFormat
    );
    let mut big = camera_jpeg();
    big.resize(MAX_PHOTO_FILE + 1, 0);
    f.file(&big);
    assert_eq!(
        f.app.photo_choose().expect_err("too big").code,
        ErrorCode::Validation
    );
    f.file(&camera_jpeg());
    let source = f.app.photo_choose().expect("choose");
    for bad in [
        crop(40, 0, 80, 80, PhotoShape::Square),
        crop(0, 0, 10, 10, PhotoShape::Square),
    ] {
        let err = f
            .app
            .photo_apply(PhotoApply {
                cv_id: cv.cv.id.clone(),
                source_id: source.source_id,
                crop: bad,
            })
            .expect_err("bad crop");
        assert_eq!(err.code, ErrorCode::Validation);
    }
    let err = f
        .app
        .photo_apply(PhotoApply {
            cv_id: cv.cv.id.clone(),
            source_id: None,
            crop: crop(0, 0, 80, 80, PhotoShape::Square),
        })
        .expect_err("no photo yet");
    assert_eq!(err.code, ErrorCode::NotFound);
    assert_eq!(f.app.photo_get(&cv.cv.id).expect("get"), None);
}

#[test]
fn checks_note_a_shown_photo() {
    let f = Fixture::new();
    f.file(&camera_jpeg());
    let stored = filled(&f.app);
    let id = stored.cv.id.clone();
    let target = Target {
        cv_id: id.clone(),
        variant_id: None,
    };
    let noted = |app: &App| {
        app.lint(&target)
            .expect("lint")
            .iter()
            .any(|i| i.code == cv_core::lint::codes::PHOTO)
    };
    // Switched on without a photo, or a photo switched off: nothing to note.
    let mut cv = stored.cv.clone();
    cv.basics.show_photo = true;
    f.app
        .save_cv(SaveCv {
            cv,
            expected_revision: stored.revision,
        })
        .expect("save");
    assert!(!noted(&f.app));
    let source = f.app.photo_choose().expect("choose");
    f.app
        .photo_apply(PhotoApply {
            cv_id: id,
            source_id: source.source_id,
            crop: crop(0, 0, 80, 80, PhotoShape::Square),
        })
        .expect("apply");
    assert!(noted(&f.app));
}

#[test]
fn a_europass_photo_is_imported_cleaned_cropped_and_hidden() {
    let f = Fixture::new();
    f.europass(&camera_jpeg());
    let preview = f.app.import_parse().expect("parse");
    let shown = decode_data_url(preview.photo.as_deref().expect("photo in preview"));
    // Upright 80 × 120, cropped to the centred 3:4 frame.
    assert_eq!(size(&shown), (78, 104));
    assert!(!contains(&shown, b"SECRET-GPS") && !contains(&shown, b"Exif"));

    let stored = f
        .app
        .import_commit(ImportCommit {
            preview_id: preview.preview_id,
            target: ImportTarget::New {
                title: String::new(),
            },
            use_imported: Vec::new(),
            entries: Vec::new(),
            use_imported_photo: false,
        })
        .expect("commit");
    assert!(!stored.cv.basics.show_photo, "photos stay opt-in");
    let info = f.app.photo_get(&stored.cv.id).expect("get").expect("photo");
    assert_eq!(info.crop, crop(1, 8, 78, 104, PhotoShape::Portrait));
    assert_eq!((info.original.width, info.original.height), (80, 120));
    assert_eq!(decode_data_url(&info.data_url), shown);
    drop(f.app);
    let db = std::fs::read(f.dir.path().join("pinaxcv.db")).expect("db");
    assert!(!contains(&db, b"SECRET-GPS"));
}

#[test]
fn a_merge_keeps_the_cvs_photo_unless_told_otherwise() {
    let f = Fixture::new();
    // A CV without a photo takes the imported one, with no question.
    let bare = filled(&f.app);
    f.europass(&camera_jpeg());
    let preview = f.app.import_parse().expect("parse");
    let matched = f
        .app
        .import_match(ImportMatchRequest {
            preview_id: preview.preview_id,
            cv_id: bare.cv.id.clone(),
        })
        .expect("match");
    assert!(!matched.photo_conflict);
    let merged = f.import(
        ImportTarget::Merge {
            cv_id: bare.cv.id.clone(),
            expected_revision: bare.revision,
        },
        false,
    );
    assert!(!merged.cv.basics.show_photo);
    assert!(f.app.photo_get(&bare.cv.id).expect("get").is_some());

    // A CV with its own (square) photo asks, and keeps it by default.
    let own = filled(&f.app);
    f.file(&camera_jpeg());
    let source = f.app.photo_choose().expect("choose");
    let mine = f
        .app
        .photo_apply(PhotoApply {
            cv_id: own.cv.id.clone(),
            source_id: source.source_id,
            crop: crop(0, 0, 80, 80, PhotoShape::Square),
        })
        .expect("apply");
    let preview = f.app.import_parse().expect("parse");
    let matched = f
        .app
        .import_match(ImportMatchRequest {
            preview_id: preview.preview_id,
            cv_id: own.cv.id.clone(),
        })
        .expect("match");
    assert!(matched.photo_conflict);
    let kept = f.import(
        ImportTarget::Merge {
            cv_id: own.cv.id.clone(),
            expected_revision: own.revision,
        },
        false,
    );
    assert_eq!(f.app.photo_get(&own.cv.id).expect("get"), Some(mine));
    let replaced = f.import(
        ImportTarget::Merge {
            cv_id: own.cv.id.clone(),
            expected_revision: kept.revision,
        },
        true,
    );
    assert!(!replaced.cv.basics.show_photo);
    let now = f.app.photo_get(&own.cv.id).expect("get").expect("photo");
    assert_eq!(now.crop.shape, PhotoShape::Portrait);
}

#[test]
fn an_unreadable_europass_photo_is_skipped_with_a_warning() {
    let f = Fixture::new();
    // Starts like a JPEG but does not decode.
    f.europass(&[0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3]);
    let preview = f.app.import_parse().expect("parse");
    assert_eq!(preview.photo, None);
    assert!(
        preview
            .warnings
            .iter()
            .any(|w| w.contains("photo could not be read")),
        "{:?}",
        preview.warnings
    );
    let stored = f.import(
        ImportTarget::New {
            title: String::new(),
        },
        false,
    );
    assert_eq!(f.app.photo_get(&stored.cv.id).expect("get"), None);
}
