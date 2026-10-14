//! Golden ATS suite (spec section 8.1): every template × every fixture is
//! exported to PDF, read back, checked with `ats_report`, and its extracted
//! text compared with a reviewed snapshot in `tests/snapshots/`.

use std::path::PathBuf;

use cv_core::Cv;
use cv_render::ats::{LopdfExtractor, ats_report, checks};
use cv_render::{ExportOptions, MISSING_GLYPHS, Renderer, TextExtractor};

const TEMPLATES: [&str; 1] = ["classic"];

const FIXTURES: [&str; 10] = [
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
            let failed: Vec<_> = report.checks.iter().filter(|c| !c.passed).collect();
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
