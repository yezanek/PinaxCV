//! Entry points that take a file's bytes and return an [`ImportResult`].

use cv_core::Id;

use crate::europass::{AttachmentError, embedded_xml};
use crate::europass_map::to_partial;
use crate::europass_xml::{XmlError, parse_candidate};
use crate::json_resume::{import_json_resume, looks_like_json};
use crate::linkedin_zip::{import_linkedin_zip, looks_like_zip};
use crate::partial::ImportResult;
use crate::pdf_map::text_to_partial;
use crate::pdf_text::page_text;

/// Largest input file (spec section 9: 20 MB).
pub const MAX_INPUT: usize = 20 * 1024 * 1024;

/// Why a file could not be imported, with a message for the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    TooLarge,
    /// The file is not in a supported format; the message says what to try.
    UnsupportedFormat(String),
    Malformed(String),
}

/// Imports a Europass CV from its PDF export (with the embedded XML) or from
/// the bare XML file.
pub fn import_europass(
    bytes: &[u8],
    fresh: &mut dyn FnMut() -> Id,
) -> Result<ImportResult, ImportError> {
    if bytes.len() > MAX_INPUT {
        return Err(ImportError::TooLarge);
    }
    let xml = if bytes.starts_with(b"%PDF") {
        match embedded_xml(bytes) {
            Ok(xml) => xml,
            Err(AttachmentError::TooLarge) => return Err(ImportError::TooLarge),
            Err(AttachmentError::NotPdf) => {
                return Err(ImportError::Malformed(
                    "The PDF file could not be read.".into(),
                ));
            }
            Err(AttachmentError::NoEuropassXml) => {
                return Err(ImportError::UnsupportedFormat(
                    "This PDF has no Europass data inside. Export the CV again from the Europass editor, or use the text import."
                        .into(),
                ));
            }
        }
    } else {
        bytes.to_vec()
    };
    let cv = parse_candidate(&xml).map_err(|e| match e {
        XmlError::Malformed => ImportError::Malformed("The Europass XML could not be read.".into()),
        XmlError::NotEuropass => ImportError::UnsupportedFormat(
            "The file doesn't look like a Europass CV. Expected a Europass PDF or XML export."
                .into(),
        ),
    })?;
    Ok(to_partial(&cv, fresh))
}

/// Imports any CV file: JSON Resume for JSON, a ZIP as a LinkedIn data
/// export, otherwise Europass first, and for a PDF without Europass data, the
/// generic text importer (ADR-047).
pub fn import_any(
    bytes: &[u8],
    fresh: &mut dyn FnMut() -> Id,
) -> Result<ImportResult, ImportError> {
    if bytes.len() > MAX_INPUT {
        return Err(ImportError::TooLarge);
    }
    if looks_like_json(bytes) {
        return import_json_resume(bytes, fresh);
    }
    if looks_like_zip(bytes) {
        return import_linkedin_zip(bytes, fresh);
    }
    match import_europass(bytes, fresh) {
        Err(ImportError::UnsupportedFormat(_)) if bytes.starts_with(b"%PDF") => {
            let text = page_text(bytes)
                .map_err(|_| ImportError::Malformed("The PDF file could not be read.".into()))?;
            if text.trim().is_empty() {
                return Err(ImportError::UnsupportedFormat(
                    "This PDF has no text to import. It may be a scanned image.".into(),
                ));
            }
            Ok(text_to_partial(&text, fresh))
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> impl FnMut() -> Id {
        let mut n = 0u8;
        move || {
            n += 1;
            Id::parse(&format!("01HZX3M8Q4V7K2N9P5R6T0W1{n:02}")).expect("id")
        }
    }

    const XML: &str = r#"<Candidate xmlns="http://www.europass.eu/1.0"><CandidatePerson><PersonName><GivenName>Jane</GivenName><FamilyName>Doe</FamilyName></PersonName></CandidatePerson></Candidate>"#;

    #[test]
    fn bare_xml_imports() {
        let r = import_europass(XML.as_bytes(), &mut ids()).expect("import");
        assert_eq!(r.cv_partial.full_name.as_deref(), Some("Jane Doe"));
    }

    #[test]
    fn import_any_tries_europass_then_falls_back_to_text() {
        let r = import_any(XML.as_bytes(), &mut ids()).expect("europass");
        assert_eq!(r.cv_partial.full_name.as_deref(), Some("Jane Doe"));
        // A PDF with no pages has no Europass data and no text.
        let mut doc = lopdf::Document::with_version("1.7");
        let pages = doc.add_object(lopdf::dictionary! { "Type" => "Pages", "Kids" => Vec::<lopdf::Object>::new(), "Count" => 0 });
        let catalog = doc.add_object(lopdf::dictionary! { "Type" => "Catalog", "Pages" => pages });
        doc.trailer.set("Root", catalog);
        let mut pdf = Vec::new();
        doc.save_to(&mut pdf).expect("save");
        assert!(
            matches!(import_any(&pdf, &mut ids()), Err(ImportError::UnsupportedFormat(m)) if m.contains("no text"))
        );
        assert!(matches!(
            import_any(b"<resume/>", &mut ids()),
            Err(ImportError::UnsupportedFormat(_))
        ));
    }

    #[test]
    fn bare_xml_up_to_the_input_cap_imports() {
        // Larger than the 10 MB attachment cap, within the 20 MB input cap.
        let mut big = XML.as_bytes().to_vec();
        big.resize(15 * 1024 * 1024, b' ');
        let r = import_europass(&big, &mut ids()).expect("import");
        assert_eq!(r.cv_partial.full_name.as_deref(), Some("Jane Doe"));
    }

    #[test]
    fn unsupported_and_broken_inputs_explain_themselves() {
        assert!(matches!(
            import_europass(b"<resume/>", &mut ids()),
            Err(ImportError::UnsupportedFormat(_))
        ));
        assert!(matches!(
            import_europass(b"<Candidate", &mut ids()),
            Err(ImportError::Malformed(_))
        ));
        assert!(matches!(
            import_europass(b"%PDF-1.7 broken", &mut ids()),
            Err(ImportError::Malformed(_))
        ));
        assert_eq!(
            import_europass(&vec![b' '; MAX_INPUT + 1], &mut ids()),
            Err(ImportError::TooLarge)
        );
    }
}
