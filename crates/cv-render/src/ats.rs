//! The ATS check on the exported PDF (spec section 7.2).
//!
//! The check reads the text back out of the real PDF and compares it with
//! what the CV says the PDF should contain. It is a best-practice check, not
//! a vendor specification (docs/ATS_RULES.md).

use cv_core::{AtsCheck, AtsReport, ResolvedCv};

use crate::data::{SectionData, to_typst_data};
use crate::{ExportOptions, RenderError, TextExtractor};

/// Check names, stable for the UI and tests.
pub mod checks {
    pub const TEXT_EXTRACTED: &str = "text_extracted";
    pub const NAME_AT_TOP: &str = "name_at_top";
    pub const CONTACT_AT_TOP: &str = "contact_at_top";
    pub const SECTION_HEADINGS: &str = "section_headings";
    pub const ENTRY_DETAILS: &str = "entry_details";
    pub const CLEAN_CHARACTERS: &str = "clean_characters";
    pub const BULLETS: &str = "bullets";
    pub const READING_ORDER: &str = "reading_order";
}

/// Name within this many non-empty lines.
const NAME_LINES: usize = 3;
/// Email and phone within this many non-empty lines.
const CONTACT_LINES: usize = 8;
/// An entry's title, org and period within this many lines.
const ENTRY_WINDOW: usize = 4;

/// Reads text with `lopdf`, which keeps one output line per PDF text line
/// (ADR-024).
#[derive(Clone, Copy, Debug, Default)]
pub struct LopdfExtractor;

impl TextExtractor for LopdfExtractor {
    fn extract(&self, pdf: &[u8]) -> Result<String, RenderError> {
        let doc =
            lopdf::Document::load_mem(pdf).map_err(|e| RenderError::Extract(e.to_string()))?;
        let pages: Vec<u32> = doc.get_pages().keys().copied().collect();
        doc.extract_text(&pages)
            .map_err(|e| RenderError::Extract(e.to_string()))
    }
}

/// Runs the ATS check on `pdf`, which must be the PDF export of `cv`.
pub fn ats_report(pdf: &[u8], cv: &ResolvedCv) -> AtsReport {
    ats_report_with(&LopdfExtractor, pdf, cv)
}

/// [`ats_report`] with a chosen extractor.
pub fn ats_report_with(extractor: &dyn TextExtractor, pdf: &[u8], cv: &ResolvedCv) -> AtsReport {
    ats_report_with_options(extractor, pdf, cv, &ExportOptions::default())
}

/// [`ats_report_with`] for a PDF rendered with `options`, so contact details
/// the export left out are not expected in the text.
pub fn ats_report_with_options(
    extractor: &dyn TextExtractor,
    pdf: &[u8],
    cv: &ResolvedCv,
    options: &ExportOptions,
) -> AtsReport {
    match extractor.extract(pdf) {
        Ok(text) => {
            let checks = run_checks(&text, &Expected::from_cv(cv, options));
            AtsReport {
                score: score(&checks),
                extracted_text: text,
                checks,
            }
        }
        Err(e) => AtsReport {
            extracted_text: String::new(),
            score: 0,
            checks: vec![fail(checks::TEXT_EXTRACTED, e.to_string())],
        },
    }
}

/// What the PDF should contain, taken from the same data the template prints.
struct Expected {
    name: String,
    contacts: Vec<String>,
    headings: Vec<String>,
    entries: Vec<ExpectedEntry>,
    bullets: Vec<String>,
    /// Headings and entry titles in CV order.
    order: Vec<String>,
}

struct ExpectedEntry {
    parts: Vec<String>,
}

impl Expected {
    fn from_cv(cv: &ResolvedCv, options: &ExportOptions) -> Self {
        let data = to_typst_data(cv, options);
        let mut e = Expected {
            name: norm(&data.basics.full_name),
            contacts: [&data.basics.email, &data.basics.phone]
                .into_iter()
                .flatten()
                .map(|s| norm(s))
                .collect(),
            headings: Vec::new(),
            entries: Vec::new(),
            bullets: Vec::new(),
            order: Vec::new(),
        };
        if data.summary.is_some() {
            e.add_heading(data.labels.summary);
        }
        for section in &data.sections {
            match section {
                SectionData::Experience(s)
                | SectionData::Education(s)
                | SectionData::Projects(s)
                | SectionData::Certifications(s)
                | SectionData::Custom(s) => {
                    e.add_heading(&s.heading);
                    for entry in &s.entries {
                        let parts: Vec<String> = [&entry.title, &entry.org, &entry.period]
                            .into_iter()
                            .flatten()
                            .map(|p| norm(p))
                            .collect();
                        if let Some(title) = &entry.title {
                            e.order.push(norm(title));
                        }
                        if !parts.is_empty() {
                            e.entries.push(ExpectedEntry { parts });
                        }
                        e.bullets.extend(entry.bullets.iter().map(|b| norm(b)));
                    }
                }
                SectionData::Skills { heading, .. } | SectionData::Languages { heading, .. } => {
                    e.add_heading(heading);
                }
            }
        }
        e
    }

    fn add_heading(&mut self, heading: &str) {
        let h = norm(heading);
        if !h.is_empty() {
            self.headings.push(h.clone());
            self.order.push(h);
        }
    }
}

fn run_checks(text: &str, expected: &Expected) -> Vec<AtsCheck> {
    let lines: Vec<String> = text.lines().map(norm).filter(|l| !l.is_empty()).collect();
    let joined = lines.join(" ");
    vec![
        check_name(&lines, expected),
        check_contacts(&lines, expected),
        check_headings(&lines, expected),
        check_entries(&lines, expected),
        check_characters(text),
        check_bullets(&joined, expected),
        check_order(&joined, expected),
    ]
}

fn check_name(lines: &[String], expected: &Expected) -> AtsCheck {
    let top = lines[..lines.len().min(NAME_LINES)].join(" ");
    if expected.name.is_empty() || top.contains(&expected.name) {
        pass(checks::NAME_AT_TOP)
    } else {
        fail(
            checks::NAME_AT_TOP,
            format!(
                "\"{}\" is not in the first {NAME_LINES} lines",
                expected.name
            ),
        )
    }
}

fn check_contacts(lines: &[String], expected: &Expected) -> AtsCheck {
    let top = &lines[..lines.len().min(CONTACT_LINES)];
    let missing: Vec<&str> = expected
        .contacts
        .iter()
        .filter(|c| !top.iter().any(|l| l.contains(c.as_str())))
        .map(String::as_str)
        .collect();
    if missing.is_empty() {
        pass(checks::CONTACT_AT_TOP)
    } else {
        fail(
            checks::CONTACT_AT_TOP,
            format!(
                "not on one line within the first {CONTACT_LINES} lines: {}",
                missing.join(", ")
            ),
        )
    }
}

/// Each heading is a line of its own, in CV order.
fn check_headings(lines: &[String], expected: &Expected) -> AtsCheck {
    let mut next = 0;
    let mut missing = Vec::new();
    for heading in &expected.headings {
        let want = heading.to_lowercase();
        match lines[next..].iter().position(|l| l.to_lowercase() == want) {
            Some(i) => next += i + 1,
            None => missing.push(heading.as_str()),
        }
    }
    if missing.is_empty() {
        pass(checks::SECTION_HEADINGS)
    } else {
        fail(
            checks::SECTION_HEADINGS,
            format!("missing or out of order: {}", missing.join(", ")),
        )
    }
}

fn check_entries(lines: &[String], expected: &Expected) -> AtsCheck {
    let mut next = 0;
    let mut failed = Vec::new();
    for entry in &expected.entries {
        let found = (next..lines.len()).find(|&i| {
            let window = lines[i..lines.len().min(i + ENTRY_WINDOW)].join(" ");
            entry.parts.iter().all(|p| window.contains(p.as_str()))
        });
        match found {
            Some(i) => next = i + 1,
            None => failed.push(entry.parts.join(" / ")),
        }
    }
    if failed.is_empty() {
        pass(checks::ENTRY_DETAILS)
    } else {
        fail(
            checks::ENTRY_DETAILS,
            format!(
                "title, organisation and dates not within {ENTRY_WINDOW} lines: {}",
                failed.join("; ")
            ),
        )
    }
}

fn check_characters(text: &str) -> AtsCheck {
    let mut bad: Vec<String> = Vec::new();
    for c in text.chars().filter(|&c| is_bad_char(c)) {
        let code = format!("U+{:04X}", u32::from(c));
        if !bad.contains(&code) {
            bad.push(code);
        }
    }
    if bad.is_empty() {
        pass(checks::CLEAN_CHARACTERS)
    } else {
        fail(
            checks::CLEAN_CHARACTERS,
            format!(
                "private-use, replacement or ligature characters: {}",
                bad.join(", ")
            ),
        )
    }
}

/// The private use areas (BMP and planes 15 and 16), U+FFFD and the Latin
/// ligatures U+FB00..=U+FB06.
fn is_bad_char(c: char) -> bool {
    matches!(
        c,
        '\u{E000}'..='\u{F8FF}'
            | '\u{F0000}'..='\u{FFFFD}'
            | '\u{100000}'..='\u{10FFFD}'
            | '\u{FFFD}'
            | '\u{FB00}'..='\u{FB06}'
    )
}

/// A bullet passes when its words appear in order, or, for wraps inside a
/// long word such as a URL, when its characters appear with whitespace removed.
fn check_bullets(joined: &str, expected: &Expected) -> AtsCheck {
    let squashed = squash(joined);
    let missing: Vec<&str> = expected
        .bullets
        .iter()
        .filter(|b| !joined.contains(b.as_str()) && !squashed.contains(&squash(b)))
        .map(String::as_str)
        .collect();
    if missing.is_empty() {
        pass(checks::BULLETS)
    } else {
        fail(
            checks::BULLETS,
            format!(
                "{} bullet(s) not found: {}",
                missing.len(),
                missing.join("; ")
            ),
        )
    }
}

fn check_order(joined: &str, expected: &Expected) -> AtsCheck {
    let mut from = 0;
    for item in &expected.order {
        match joined[from..].find(item.as_str()) {
            Some(i) => from += i + item.len(),
            None => {
                return fail(
                    checks::READING_ORDER,
                    format!("\"{item}\" is missing or out of order"),
                );
            }
        }
    }
    pass(checks::READING_ORDER)
}

/// Collapses every whitespace run (including no-break spaces) to one space.
fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn score(checks: &[AtsCheck]) -> u8 {
    let passed = checks.iter().filter(|c| c.passed).count();
    let pct = (passed * 100 + checks.len() / 2) / checks.len().max(1);
    u8::try_from(pct).unwrap_or(100)
}

fn pass(name: &str) -> AtsCheck {
    AtsCheck {
        name: name.to_owned(),
        passed: true,
        detail: None,
    }
}

fn fail(name: &str, detail: String) -> AtsCheck {
    AtsCheck {
        name: name.to_owned(),
        passed: false,
        detail: Some(detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdata;

    /// Hands back fixed text instead of reading a PDF.
    struct FakeExtractor(Result<String, &'static str>);

    impl TextExtractor for FakeExtractor {
        fn extract(&self, _pdf: &[u8]) -> Result<String, RenderError> {
            self.0
                .clone()
                .map_err(|e| RenderError::Extract(e.to_owned()))
        }
    }

    /// The real text of the `typical` fixture's PDF.
    fn typical_text() -> String {
        let cv = testdata::fixture("typical").resolve(None);
        let pdf = crate::Renderer::new()
            .compile(&cv, &ExportOptions::default())
            .expect("compiles")
            .to_pdf()
            .expect("pdf");
        LopdfExtractor.extract(&pdf).expect("extracts")
    }

    fn report(text: String) -> AtsReport {
        let cv = testdata::fixture("typical").resolve(None);
        ats_report_with(&FakeExtractor(Ok(text)), b"", &cv)
    }

    fn failed(r: &AtsReport) -> Vec<String> {
        r.failed_checks().map(|c| c.name.clone()).collect()
    }

    #[test]
    fn faithful_text_passes_every_check() {
        let r = report(typical_text());
        assert_eq!(r.failed_checks().count(), 0, "{:?}", r.checks);
        assert_eq!(r.score, 100);
        assert_eq!(r.checks.len(), 7);
    }

    #[test]
    fn name_pushed_down_fails() {
        let text = format!("x\ny\nz\n{}", typical_text());
        assert_eq!(failed(&report(text)), [checks::NAME_AT_TOP]);
    }

    #[test]
    fn split_email_fails() {
        let text = typical_text().replacen("@example", "@\nexample", 1);
        assert!(
            failed(&report(text))
                .iter()
                .any(|n| n == checks::CONTACT_AT_TOP)
        );
    }

    #[test]
    fn reordered_sections_fail_headings_and_order() {
        let text = typical_text().replacen("Education", "Educat1on", 1);
        let f = failed(&report(text));
        assert!(f.iter().any(|n| n == checks::SECTION_HEADINGS), "{f:?}");
        assert!(f.iter().any(|n| n == checks::READING_ORDER), "{f:?}");
    }

    #[test]
    fn ligature_and_replacement_characters_fail() {
        let text = format!("{}\n\u{FB01} \u{FFFD} \u{E000}", typical_text());
        let r = report(text);
        assert_eq!(failed(&r), [checks::CLEAN_CHARACTERS]);
        let detail = r.checks[4].detail.as_deref().expect("detail");
        assert!(
            detail.contains("U+FB01") && detail.contains("U+FFFD") && detail.contains("U+E000")
        );
    }

    #[test]
    fn supplementary_private_use_characters_fail() {
        let text = format!("{}\n\u{F0000} \u{10FFFD}", typical_text());
        let r = report(text);
        assert_eq!(failed(&r), [checks::CLEAN_CHARACTERS]);
        let detail = r.checks[4].detail.as_deref().expect("detail");
        assert!(detail.contains("U+F0000") && detail.contains("U+10FFFD"));
    }

    #[test]
    fn contacts_left_out_of_the_export_are_not_expected() {
        let cv = testdata::fixture("typical").resolve(None);
        let options = ExportOptions {
            include_email: false,
            include_phone: false,
            photo: None,
        };
        let pdf = crate::Renderer::new()
            .compile(&cv, &options)
            .expect("compiles")
            .to_pdf()
            .expect("pdf");
        let r = ats_report_with_options(&LopdfExtractor, &pdf, &cv, &options);
        assert_eq!(r.failed_checks().count(), 0, "{:?}", r.checks);
        let r = ats_report_with(&LopdfExtractor, &pdf, &cv);
        assert_eq!(failed(&r), [checks::CONTACT_AT_TOP]);
    }

    #[test]
    fn lost_bullet_and_scattered_entry_fail() {
        let cv = testdata::fixture("typical").resolve(None);
        let data = to_typst_data(&cv, &ExportOptions::default());
        let SectionData::Experience(exp) = &data.sections[0] else {
            panic!("experience")
        };
        let bullet = exp.entries[0].bullets[0].clone();
        let text = typical_text().replacen(&bullet, "", 1);
        assert_eq!(failed(&report(text)), [checks::BULLETS]);

        let period = exp.entries[0].period.clone().expect("period");
        let text = typical_text().replacen(&period, "", 1);
        assert!(
            failed(&report(text))
                .iter()
                .any(|n| n == checks::ENTRY_DETAILS)
        );
    }

    #[test]
    fn wrapped_long_word_still_counts_as_present() {
        let e = Expected {
            name: String::new(),
            contacts: vec![],
            headings: vec![],
            entries: vec![],
            bullets: vec!["see https://example.com/a/very/long/path".into()],
            order: vec![],
        };
        assert!(check_bullets("see https://example.com/a/ve ry/long/path", &e).passed);
        assert!(!check_bullets("see https://example.com/other", &e).passed);
    }

    #[test]
    fn extraction_error_scores_zero() {
        let cv = testdata::fixture("minimal").resolve(None);
        let r = ats_report_with(&FakeExtractor(Err("broken")), b"", &cv);
        assert_eq!(r.score, 0);
        assert_eq!(failed(&r), [checks::TEXT_EXTRACTED]);
        assert_eq!(r.failed_checks().count(), 1);
    }

    #[test]
    fn lopdf_rejects_non_pdf_input() {
        assert!(matches!(
            LopdfExtractor.extract(b"not a pdf"),
            Err(RenderError::Extract(_))
        ));
        let cv = testdata::fixture("minimal").resolve(None);
        assert_eq!(ats_report(b"not a pdf", &cv).score, 0);
    }

    #[test]
    fn score_rounds_share_of_passed_checks() {
        let c = |p| AtsCheck {
            name: "x".to_owned(),
            passed: p,
            detail: None,
        };
        assert_eq!(score(&[c(true), c(true), c(false)]), 67);
        assert_eq!(score(&[]), 0);
    }
}
