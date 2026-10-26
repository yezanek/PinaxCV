//! Text import extraction (spec sections 9.4 and 11.12): the fallback
//! importer for pasted text or a PDF the other importers cannot read. Email
//! addresses, phone numbers and links are taken out before sending and found
//! again in the original text on this computer; every extracted bullet is
//! checked word for word against the text that was sent.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::LlmError;
use crate::payload::{EMAIL, PHONE, REDACTED, Redactor, URL, is_phone};
use crate::prompts;
use crate::stages::{StageContext, prompt_error};

/// Longest text sent, in characters: about ten pages.
pub const MAX_IMPORT_CHARS: usize = 40_000;

/// Stage output (spec section 11.12). Contact details are not asked for:
/// they were taken out of the text.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, Default, PartialEq)]
pub struct ExtractedCv {
    pub full_name: Option<String>,
    pub headline: Option<String>,
    /// City and country, as written.
    pub location: Option<String>,
    pub summary: Option<String>,
    pub sections: Vec<ExtractedSection>,
    pub skills: Vec<ExtractedSkillGroup>,
    pub languages: Vec<ExtractedLanguage>,
    /// Anything uncertain or skipped, in short sentences.
    pub import_notes: Vec<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ExtractedSection {
    pub kind: ExtractedKind,
    /// The heading as written, for sections of kind `other`.
    pub heading: Option<String>,
    pub entries: Vec<ExtractedEntry>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtractedKind {
    Experience,
    Education,
    Projects,
    Certifications,
    Other,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ExtractedEntry {
    pub title: String,
    pub organization: Option<String>,
    pub location: Option<String>,
    pub start: Option<ExtractedDate>,
    pub end: Option<ExtractedDate>,
    pub current: bool,
    pub bullets: Vec<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtractedDate {
    pub year: i32,
    pub month: Option<u8>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ExtractedSkillGroup {
    pub label: Option<String>,
    pub skills: Vec<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ExtractedLanguage {
    pub name: String,
    pub level: Option<String>,
}

/// The text to send, as the user will see it before sending.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedText {
    pub sent: String,
    /// How many contact details were taken out.
    pub redacted: usize,
}

/// Why a text cannot be sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrepareError {
    Empty,
    TooLong,
}

/// Normalizes line endings and blank runs and takes out email addresses,
/// URLs and phone numbers.
pub fn prepare_text(text: &str) -> Result<PreparedText, PrepareError> {
    let mut lines: Vec<&str> = Vec::new();
    for line in text.lines().map(str::trim_end) {
        if line.trim().is_empty() && lines.last().is_none_or(|l| l.trim().is_empty()) {
            continue;
        }
        lines.push(line);
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    let clean = lines.join("\n");
    if clean.trim().is_empty() {
        return Err(PrepareError::Empty);
    }
    if clean.chars().count() > MAX_IMPORT_CHARS {
        return Err(PrepareError::TooLong);
    }
    let sent = Redactor::patterns().text(&clean);
    // One URL match can swallow markers that were already in the text.
    let redacted = sent
        .matches(REDACTED)
        .count()
        .saturating_sub(clean.matches(REDACTED).count());
    Ok(PreparedText { sent, redacted })
}

/// Sends `sent` (from [`prepare_text`]) and returns what the model read.
pub async fn extract_cv(ctx: &StageContext<'_>, sent: &str) -> Result<ExtractedCv, LlmError> {
    let prompt = prompts::import_extract(sent).map_err(prompt_error)?;
    ctx.generate(prompt, "ExtractedCv").await
}

/// Lowercase words, without bullet marks and punctuation at the ends.
fn words(text: &str) -> String {
    text.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Whether `text` appears in `source` word for word, ignoring case,
/// spacing, line breaks and punctuation at word ends.
pub fn is_verbatim(source: &str, text: &str) -> bool {
    let needle = words(text);
    !needle.is_empty() && format!(" {} ", words(source)).contains(&format!(" {needle} "))
}

/// Contact details found in the original text, in order, without repeats.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Contacts {
    pub emails: Vec<String>,
    pub phones: Vec<String>,
    pub urls: Vec<String>,
}

/// The email addresses, phone numbers (9 digits or more) and URLs in `text`.
pub fn find_contacts(text: &str) -> Contacts {
    let mut c = Contacts::default();
    let push = |list: &mut Vec<String>, s: &str| {
        let s = s.trim_end_matches(['.', ',', ';', ')']).to_owned();
        if !list.contains(&s) {
            list.push(s);
        }
    };
    for m in EMAIL.find_iter(text) {
        push(&mut c.emails, m.as_str());
    }
    for m in URL.find_iter(text) {
        push(&mut c.urls, m.as_str());
    }
    let without_urls = URL.replace_all(text, " ");
    for m in PHONE.find_iter(&without_urls) {
        if is_phone(m.as_str()) {
            push(&mut c.phones, m.as_str().trim());
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FakeLlm;
    use crate::cache::MemoryCache;
    use serde_json::json;
    use tokio_util::sync::CancellationToken;

    const TEXT: &str = "Alex Sample\r\nalex@example.org · +49 170 1234567 · https://alex.example.org/cv\r\n\r\n\r\n\
Experience\r\nSenior Engineer, Example AG   \r\n2020 - present\r\n• Built the billing service in Rust.\r\n";

    #[test]
    fn contact_details_are_taken_out_before_sending() {
        let p = prepare_text(TEXT).expect("prepared");
        assert_eq!(p.redacted, 3);
        assert!(
            !p.sent.contains("alex@") && !p.sent.contains("1234567") && !p.sent.contains("https")
        );
        assert!(p.sent.contains("Alex Sample") && p.sent.contains("2020 - present"));
        assert!(
            !p.sent.contains('\r') && !p.sent.contains("\n\n\n"),
            "{:?}",
            p.sent
        );
        assert_eq!(prepare_text(" \n\n "), Err(PrepareError::Empty));
        let long = "word ".repeat(MAX_IMPORT_CHARS / 4);
        assert_eq!(prepare_text(&long), Err(PrepareError::TooLong));
    }

    #[test]
    fn contacts_are_found_locally() {
        let c = find_contacts(TEXT);
        assert_eq!(c.emails, ["alex@example.org"]);
        assert_eq!(c.phones, ["+49 170 1234567"]);
        assert_eq!(c.urls, ["https://alex.example.org/cv"]);
        // Year ranges are not phone numbers.
        assert!(
            find_contacts("01/2019 - 03/2021, 2019 - 2021 / 2022")
                .phones
                .is_empty()
        );
        let p = prepare_text("Engineer 01/2019 - 03/2021").expect("prepared");
        assert_eq!(p.redacted, 0);
    }

    #[test]
    fn verbatim_ignores_spacing_case_and_bullet_marks() {
        let source = "• Built the billing\n  service in Rust.";
        assert!(is_verbatim(source, "built the billing service in Rust"));
        assert!(is_verbatim(source, "- Built the billing service in Rust."));
        assert!(!is_verbatim(source, "Built the billing service in Go."));
        assert!(!is_verbatim(source, "  "));
        assert!(!is_verbatim("Rebuilt the service", "built the service"));
        assert!(!is_verbatim("Trust and safety", "Rust"));
    }

    #[tokio::test]
    async fn the_prepared_text_is_what_is_sent() {
        let llm = FakeLlm::new().reply(
            "ExtractedCv",
            json!({
                "full_name": "Alex Sample", "headline": null, "location": null, "summary": null,
                "sections": [{ "kind": "experience", "heading": null, "entries": [{
                    "title": "Senior Engineer", "organization": "Example AG", "location": null,
                    "start": { "year": 2020, "month": null }, "end": null, "current": true,
                    "bullets": ["Built the billing service in Rust."] }] }],
                "skills": [], "languages": [], "import_notes": []
            }),
        );
        let ctx = StageContext {
            llm: &llm,
            cache: &MemoryCache::default(),
            model: "m",
            cancel: CancellationToken::new(),
        };
        let p = prepare_text(TEXT).expect("prepared");
        let cv = extract_cv(&ctx, &p.sent).await.expect("extracted");
        assert_eq!(cv.sections[0].entries[0].start.map(|d| d.year), Some(2020));
        let sent = &llm.seen()[0].user;
        assert!(sent.contains(&p.sent) && !sent.contains("alex@example.org"));
    }
}
