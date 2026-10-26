//! Exports every CV fixture in `testdata/cvs` to a PDF and a JSON copy of
//! the fixture in the folder given as the only argument, for the
//! resume-parser check in CI (ADR-061).
//!
//! Run with: `cargo run -p cv-render --example export_fixtures -- out/`

use std::path::PathBuf;

use cv_core::Cv;
use cv_render::{ExportOptions, Renderer};

fn main() {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("usage: export_fixtures OUT_DIR"),
    );
    std::fs::create_dir_all(&out).expect("create output folder");
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cvs");
    let renderer = Renderer::new();
    let mut names: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("fixtures")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    names.sort();
    for path in names {
        let json = std::fs::read_to_string(&path).expect("read fixture");
        let cv: Cv = serde_json::from_str(&json).expect("fixture parses");
        let stem = path
            .file_stem()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        let compiled = renderer
            .compile(&cv.resolve(None), &ExportOptions::default())
            .unwrap_or_else(|e| panic!("{stem}: {e:?}"));
        let pdf = compiled
            .to_pdf()
            .unwrap_or_else(|e| panic!("{stem}: {e:?}"));
        std::fs::write(out.join(format!("{stem}.pdf")), pdf).expect("write pdf");
        std::fs::write(out.join(format!("{stem}.json")), json).expect("write json");
        println!("{stem}");
    }
}
