//! Typst PDF and preview rendering, the standalone HTML exporter and ATS text checks.
//! Consumes `ResolvedCv` only and never touches persistence.
//!
#![forbid(unsafe_code)]

pub mod ats;
pub mod data;
mod html;
mod render;
#[cfg(test)]
mod testdata;

use std::sync::Arc;

use cv_core::{CalendarDate, Diagnostic, ResolvedCv, ValidationIssue};

pub use html::{HtmlExporter, to_html};
pub use render::{
    CLASSIC_MANIFEST, Compiled, MAX_PIXELS_PER_PT, MISSING_GLYPHS, PdfExporter, Renderer,
};

/// Options shared by every export format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportOptions {
    pub include_email: bool,
    pub include_phone: bool,
    /// The CV's profile photo as JPEG, drawn when the resolved CV shows it
    /// (ADR-083). It adds no text to any export.
    pub photo: Option<Arc<[u8]>>,
    /// The export date, printed in the closing block's "place, date" line
    /// when the CV shows it (ADR-089). `None` prints the place alone.
    pub date: Option<CalendarDate>,
}

impl Default for ExportOptions {
    /// Both included: the PDF always prints contact details. The HTML export
    /// passes the user's choices instead (email on, phone off by default).
    fn default() -> Self {
        Self {
            include_email: true,
            include_phone: true,
            photo: None,
            date: None,
        }
    }
}

impl ExportOptions {
    /// The HTML export dialog's starting choices (spec section 6.0): email
    /// on, phone off. A page meant for a public website does not publish a
    /// phone number unless the user ticks it.
    pub const fn html_default() -> Self {
        Self {
            include_email: true,
            include_phone: false,
            photo: None,
            date: None,
        }
    }
}

/// A finished export, ready to be written to the path the user chose.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportedDocument {
    pub bytes: Vec<u8>,
    pub media_type: &'static str,
    pub suggested_file_name: String,
    /// PDF only.
    pub page_count: Option<u32>,
    /// Warnings that did not block the export, such as missing glyphs.
    pub warnings: Vec<Diagnostic>,
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("the CV is not ready to export")]
    Validation(Vec<ValidationIssue>),
    #[error("the template failed to compile")]
    Compile(Vec<Diagnostic>),
    #[error("export failed: {0}")]
    Export(String),
    #[error("could not read text from the PDF: {0}")]
    Extract(String),
}

/// One output format produced from a resolved CV (PDF, HTML; DOCX/ODT later).
pub trait Exporter {
    /// Renders `cv` to this format; never writes files or reads persistence.
    fn export(
        &self,
        cv: &ResolvedCv,
        options: &ExportOptions,
    ) -> Result<ExportedDocument, RenderError>;
}

/// Reads plain text back out of a PDF, for the ATS check and the PDF importer.
pub trait TextExtractor: Send + Sync {
    /// Returns the PDF's text in reading order, one line per text line.
    fn extract(&self, pdf: &[u8]) -> Result<String, RenderError>;
}

/// Builds a portable file name such as `Jane-Doe-CV.pdf` from the CV owner's name.
///
/// Keeps letters and digits (including non-ASCII letters), turns every other run
/// of characters into a single hyphen, and falls back to `CV.<ext>` when no
/// usable characters remain.
pub fn suggested_file_name(full_name: &str, extension: &str) -> String {
    let mut stem = String::new();
    for c in full_name.chars() {
        if c.is_alphanumeric() {
            stem.push(c);
        } else if !stem.is_empty() && !stem.ends_with('-') {
            stem.push('-');
        }
    }
    let stem = stem.trim_end_matches('-');
    if stem.is_empty() {
        format!("CV.{extension}")
    } else {
        format!("{stem}-CV.{extension}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Async callers move render errors across `spawn_blocking`.
    const _: () = {
        const fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<RenderError>();
    };

    #[test]
    fn suggested_file_name_is_portable() {
        assert_eq!(suggested_file_name("Jane Doe", "pdf"), "Jane-Doe-CV.pdf");
        assert_eq!(
            suggested_file_name("  José  Müller-Łódź ", "html"),
            "José-Müller-Łódź-CV.html"
        );
        assert_eq!(suggested_file_name("a/b\\c:d", "pdf"), "a-b-c-d-CV.pdf");
        assert_eq!(suggested_file_name("", "pdf"), "CV.pdf");
        assert_eq!(suggested_file_name("../..", "pdf"), "CV.pdf");
    }

    #[test]
    fn default_export_options_include_contacts() {
        let o = ExportOptions::default();
        assert!(o.include_email && o.include_phone);
    }
}
