//! The CV fixtures in `testdata/cvs/` (spec section 8.1), embedded for tests.

use cv_core::Cv;

/// Every fixture name, in the spec's order.
pub const FIXTURES: [&str; 12] = [
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
];

/// Parses the fixture `name`. Panics on an unknown name or broken JSON.
pub fn fixture(name: &str) -> Cv {
    let json = match name {
        "minimal" => include_str!("../../../testdata/cvs/minimal.json"),
        "typical" => include_str!("../../../testdata/cvs/typical.json"),
        "long" => include_str!("../../../testdata/cvs/long.json"),
        "unicode" => include_str!("../../../testdata/cvs/unicode.json"),
        "cjk" => include_str!("../../../testdata/cvs/cjk.json"),
        "empty_sections" => include_str!("../../../testdata/cvs/empty_sections.json"),
        "long_strings" => include_str!("../../../testdata/cvs/long_strings.json"),
        "current_job" => include_str!("../../../testdata/cvs/current_job.json"),
        "special_chars" => include_str!("../../../testdata/cvs/special_chars.json"),
        "unsafe_links" => include_str!("../../../testdata/cvs/unsafe_links.json"),
        "two_column" => include_str!("../../../testdata/cvs/two_column.json"),
        "two_column_long" => include_str!("../../../testdata/cvs/two_column_long.json"),
        other => panic!("unknown fixture {other}"),
    };
    serde_json::from_str(json).unwrap_or_else(|e| panic!("fixture {name}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fixture_parses_and_saves_without_errors() {
        for name in FIXTURES {
            let mut cv = fixture(name);
            let issues = cv_core::validate::for_save(&mut cv);
            assert!(issues.is_empty(), "{name}: {issues:?}");
            let export = cv_core::validate::for_export(&cv);
            assert!(export.is_empty(), "{name}: {export:?}");
        }
    }
}
