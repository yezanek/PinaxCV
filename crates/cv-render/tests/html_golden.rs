//! HTML golden suite: every fixture's standalone HTML export is compared
//! with a reviewed snapshot in `tests/snapshots/`.

use std::path::PathBuf;

use cv_core::Cv;
use cv_render::{ExportOptions, to_html};

const FIXTURES: [&str; 13] = [
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
    "two_column",
    "two_column_long",
    "european",
];

fn fixture(name: &str) -> Cv {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/cvs")
        .join(format!("{name}.json"));
    let json = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    serde_json::from_str(&json).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn html_exports_match_snapshots() {
    // The HTML defaults the export dialog offers: email on, phone off.
    let options = ExportOptions::html_default();
    for name in FIXTURES {
        let doc = to_html(&fixture(name).resolve(None), &options);
        // The stylesheet is the same in every file; snapshot the content.
        let body = &doc[doc.find("<body").expect("body")..];
        insta::assert_snapshot!(format!("html__{name}"), body);
    }
}
