//! Europass. The Europass editor's PDF export embeds the structured CV as an
//! XML attachment in the `Candidate` format; this module finds it so the CV
//! can be imported exactly instead of guessing from the page text.

use lopdf::{Document, Object};

/// Namespace of the current Europass `Candidate` XML.
pub const EUROPASS_NS: &str = "http://www.europass.eu/1.0";

/// Largest attachment read (spec section 9: 10 MB per embedded entry).
pub const MAX_ATTACHMENT: usize = 10 * 1024 * 1024;

/// Why no Europass XML could be taken from a PDF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentError {
    /// The bytes are not a readable PDF.
    NotPdf,
    /// The PDF has no embedded Europass XML (for example a printed CV).
    NoEuropassXml,
    /// The embedded XML is larger than [`MAX_ATTACHMENT`].
    TooLarge,
}

/// Returns the embedded Europass `Candidate` XML of a Europass PDF.
///
/// Every embedded file is checked, and the first one that declares the
/// Europass namespace is returned, whatever its file name.
pub fn embedded_xml(pdf: &[u8]) -> Result<Vec<u8>, AttachmentError> {
    let doc = Document::load_mem(pdf).map_err(|_| AttachmentError::NotPdf)?;
    let mut too_large = false;
    for object in doc.objects.values() {
        let Object::Stream(stream) = object else {
            continue;
        };
        let is_file = stream
            .dict
            .get(b"Type")
            .and_then(Object::as_name)
            .is_ok_and(|n| n == b"EmbeddedFile");
        if !is_file {
            continue;
        }
        // The declared length bounds the work before decompressing.
        let declared = stream
            .dict
            .get(b"Params")
            .and_then(Object::as_dict)
            .and_then(|p| p.get(b"Size"))
            .and_then(Object::as_i64)
            .unwrap_or(0);
        if usize::try_from(declared).is_ok_and(|n| n > MAX_ATTACHMENT) {
            too_large = true;
            continue;
        }
        let Ok(content) = stream.decompressed_content().or_else(|_| {
            // Uncompressed streams have no filter to undo.
            if stream.dict.get(b"Filter").is_err() {
                Ok(stream.content.clone())
            } else {
                Err(())
            }
        }) else {
            continue;
        };
        if content.len() > MAX_ATTACHMENT {
            too_large = true;
            continue;
        }
        if declares_europass(&content) {
            return Ok(content);
        }
    }
    Err(if too_large {
        AttachmentError::TooLarge
    } else {
        AttachmentError::NoEuropassXml
    })
}

/// True when the start of `xml` declares the Europass namespace on a
/// `Candidate` root.
fn declares_europass(xml: &[u8]) -> bool {
    let head = &xml[..xml.len().min(4096)];
    let head = String::from_utf8_lossy(head);
    head.contains("<Candidate") && head.contains(EUROPASS_NS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Dictionary, Stream, dictionary};

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Candidate xmlns="http://www.europass.eu/1.0" xmlns:hr="http://www.hr-xml.org/3">
  <CandidatePerson><PersonName><GivenName>Jane</GivenName><FamilyName>Doe</FamilyName></PersonName></CandidatePerson>
</Candidate>"#;

    /// A one-page PDF with `files` embedded as (name, content, compressed).
    fn pdf_with(files: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let mut names = Vec::new();
        for (name, content, compress) in files {
            let mut stream = Stream::new(
                dictionary! { "Type" => "EmbeddedFile", "Subtype" => "text#2Fxml" },
                content.to_vec(),
            );
            if *compress {
                stream.compress().expect("compress");
            }
            let file_id = doc.add_object(stream);
            let spec_id = doc.add_object(dictionary! {
                "Type" => "Filespec",
                "F" => Object::string_literal(*name),
                "EF" => dictionary! { "F" => file_id },
            });
            names.push(Object::string_literal(*name));
            names.push(spec_id.into());
        }
        let catalog = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "Names" => dictionary! {
                "EmbeddedFiles" => dictionary! { "Names" => names },
            },
        });
        doc.trailer.set("Root", catalog);
        let mut out = Vec::new();
        doc.save_to(&mut out).expect("save");
        out
    }

    #[test]
    fn finds_compressed_and_plain_europass_xml() {
        for compress in [true, false] {
            let pdf = pdf_with(&[("attachment.xml", XML.as_bytes(), compress)]);
            assert_eq!(embedded_xml(&pdf).expect("xml"), XML.as_bytes());
        }
    }

    #[test]
    fn skips_other_attachments() {
        let pdf = pdf_with(&[
            ("notes.xml", b"<notes/>", true),
            ("cv.xml", XML.as_bytes(), true),
        ]);
        assert_eq!(embedded_xml(&pdf).expect("xml"), XML.as_bytes());
    }

    #[test]
    fn printed_cv_and_garbage_are_refused() {
        assert_eq!(
            embedded_xml(&pdf_with(&[])),
            Err(AttachmentError::NoEuropassXml)
        );
        assert_eq!(
            embedded_xml(&pdf_with(&[("other.xml", b"<Candidate/>", false)])),
            Err(AttachmentError::NoEuropassXml)
        );
        assert_eq!(embedded_xml(b"not a pdf"), Err(AttachmentError::NotPdf));
        assert_eq!(embedded_xml(&[]), Err(AttachmentError::NotPdf));
    }

    #[test]
    fn declared_oversize_attachment_is_refused() {
        let mut doc =
            Document::load_mem(&pdf_with(&[("cv.xml", XML.as_bytes(), true)])).expect("load");
        for object in doc.objects.values_mut() {
            if let Object::Stream(s) = object
                && s.dict
                    .get(b"Type")
                    .and_then(Object::as_name)
                    .is_ok_and(|n| n == b"EmbeddedFile")
            {
                let mut params = Dictionary::new();
                params.set("Size", (MAX_ATTACHMENT as i64) + 1);
                s.dict.set("Params", params);
            }
        }
        let mut out = Vec::new();
        doc.save_to(&mut out).expect("save");
        assert_eq!(embedded_xml(&out), Err(AttachmentError::TooLarge));
    }
}
