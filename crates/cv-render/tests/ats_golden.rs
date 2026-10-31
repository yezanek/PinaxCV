//! Golden ATS suite (spec section 8.1): every template × every fixture is
//! exported to PDF, read back, checked with `ats_report`, and its extracted
//! text compared with a reviewed snapshot in `tests/snapshots/`.

use std::path::PathBuf;

use cv_core::Cv;
use cv_render::ats::{LopdfExtractor, ats_report, checks};
use cv_render::{ExportOptions, MISSING_GLYPHS, Renderer, TextExtractor};

const TEMPLATES: [&str; 1] = ["classic"];

const FIXTURES: [&str; 11] = [
    "minimal",
    "typical",
    "long",
    "unicode",
    "cjk",
    "empty_sections",
    "long_strings",
    "current_job",
    "special_chars",
    "unsafe_links",
    "european",
];

fn fixture(name: &str) -> Cv {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/cvs")
        .join(format!("{name}.json"));
    let json = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    serde_json::from_str(&json).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn pdf(cv: &Cv) -> (Vec<u8>, Vec<cv_core::Diagnostic>) {
    let compiled = Renderer::new()
        .compile(&cv.resolve(None), &ExportOptions::default())
        .expect("compiles");
    let warnings = compiled.warnings().to_vec();
    (compiled.to_pdf().expect("pdf"), warnings)
}

#[test]
fn every_fixture_passes_the_ats_check() {
    for template in TEMPLATES {
        for name in FIXTURES {
            let cv = fixture(name);
            let (bytes, warnings) = pdf(&cv);
            let report = ats_report(&bytes, &cv.resolve(None));
            let failed: Vec<_> = report.failed_checks().collect();
            if name == "cjk" {
                // Inter has no CJK glyphs (ADR-033): the export warns and the
                // ATS check fails instead of passing silently.
                assert!(
                    warnings.iter().any(|w| w.code == MISSING_GLYPHS),
                    "cjk must report missing glyphs, not print tofu silently"
                );
                assert!(
                    failed.iter().any(|c| c.name == checks::CLEAN_CHARACTERS),
                    "{failed:#?}"
                );
            } else {
                assert!(failed.is_empty(), "{template}/{name}: {failed:#?}");
                assert_eq!(report.score, 100, "{template}/{name}");
                assert!(warnings.is_empty(), "{template}/{name}: {warnings:?}");
            }
            insta::assert_snapshot!(format!("{template}__{name}"), report.extracted_text);
        }
    }
}

/// Typst-significant characters print literally and never become markup.
#[test]
fn special_characters_print_literally() {
    let (bytes, _) = pdf(&fixture("special_chars"));
    let text = LopdfExtractor.extract(&bytes).expect("extracts");
    for s in ["&", "#", "$", "@", "\\", "*", "_", "`"] {
        assert!(text.contains(s), "{s:?} missing from:\n{text}");
    }
}

/// Line-joining extractors such as `pdf-extract` must still see separate
/// header words (ADR-024).
#[test]
fn header_words_do_not_merge_for_line_joining_extractors() {
    let cv = fixture("typical");
    let (bytes, _) = pdf(&cv);
    let text = pdf_extract::extract_text_from_mem(&bytes).expect("pdf-extract");
    let norm = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let b = &cv.basics;
    fn field(f: &Option<String>) -> &str {
        f.as_deref().expect("typical fills the header").trim()
    }
    let link = b.links.first().expect("typical has a link");
    let header = format!(
        "{} {} {} | {} | {} {}: {} ",
        b.full_name.trim(),
        field(&b.headline),
        field(&b.email),
        field(&b.phone),
        field(&b.location),
        link.label.trim(),
        link.url.trim(),
    );
    assert!(norm.starts_with(&header), "header words merged:\n{norm}");
}

/// The profile photo is an image only (ADR-083): the PDF holds it, but the
/// extracted text, the ATS result and the page count do not change.
#[test]
fn the_photo_adds_an_image_but_no_text() {
    let photo: std::sync::Arc<[u8]> = include_bytes!("fixtures/photo.jpg").as_slice().into();
    for name in ["typical", "long", "minimal"] {
        let cv = fixture(name);
        let mut shown = cv.resolve(None);
        shown.show_photo = true;
        let renderer = Renderer::new();
        let plain = renderer
            .compile(&cv.resolve(None), &ExportOptions::default())
            .expect("compiles");
        let options = ExportOptions {
            photo: Some(photo.clone()),
            ..ExportOptions::default()
        };
        let with_photo = renderer.compile(&shown, &options).expect("compiles");
        assert!(
            with_photo.warnings().is_empty(),
            "{name}: {:?}",
            with_photo.warnings()
        );
        assert_eq!(with_photo.page_count(), plain.page_count(), "{name}");
        let (plain_pdf, photo_pdf) = (
            plain.to_pdf().expect("pdf"),
            with_photo.to_pdf().expect("pdf"),
        );
        assert_eq!(images(&plain_pdf), 0, "{name}");
        assert_eq!(images(&photo_pdf), 1, "{name}");
        let text = LopdfExtractor.extract(&plain_pdf).expect("extracts");
        assert_eq!(
            LopdfExtractor.extract(&photo_pdf).expect("extracts"),
            text,
            "{name}"
        );
        let report = ats_report(&photo_pdf, &shown);
        assert!(report.failed_checks().next().is_none(), "{name}");

        // Given a photo but not shown (a variant with the switch off): no image.
        let hidden = renderer
            .compile(&cv.resolve(None), &options)
            .expect("compiles")
            .to_pdf()
            .expect("pdf");
        assert_eq!(images(&hidden), 0, "{name}");
    }
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

/// The closing block (ADR-089) prints as plain text after the last section,
/// not in a page footer, and the ATS check still passes.
#[test]
fn the_closing_block_is_text_after_the_last_section() {
    use cv_core::{CalendarDate, ClosingPart};
    let mut cv = fixture("typical");
    cv.closing.consent = Some(
        "Autorizzo il trattamento dei miei dati personali presenti nel CV ai sensi del \
         Regolamento UE 2016/679 (GDPR) e del D.Lgs. 196/2003."
            .into(),
    );
    cv.closing.place = Some("Berlin".into());
    cv.closing.show = [ClosingPart::Consent, ClosingPart::PlaceDate].into();
    let resolved = cv.resolve(None);
    let options = ExportOptions {
        date: CalendarDate::parse("2026-10-30"),
        ..ExportOptions::default()
    };
    let compiled = Renderer::new()
        .compile(&resolved, &options)
        .expect("compiles");
    assert!(compiled.warnings().is_empty(), "{:?}", compiled.warnings());
    let bytes = compiled.to_pdf().expect("pdf");
    let report = ats_report(&bytes, &resolved);
    let failed: Vec<_> = report.failed_checks().collect();
    assert!(failed.is_empty(), "{failed:#?}");
    let text = report
        .extracted_text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        text.ends_with("Regolamento UE 2016/679 (GDPR) e del D.Lgs. 196/2003. Berlin, 30 Oct 2026"),
        "{text}"
    );
}
