//! Europass. The Europass editor's PDF export embeds the structured CV as an
//! XML attachment in the `Candidate` format; this module finds it so the CV
//! can be imported exactly instead of guessing from the page text.

use lopdf::{Document, Object};

/// Namespace of the current Europass `Candidate` XML.
pub const EUROPASS_NS: &str = "http://www.europass.eu/1.0";

/// Largest PDF read (spec section 9: importer input is capped at 20 MB).
pub const MAX_INPUT: usize = 20 * 1024 * 1024;

/// Largest attachment read (spec section 9: 10 MB per embedded entry).
pub const MAX_ATTACHMENT: usize = 10 * 1024 * 1024;

/// Why no Europass XML could be taken from a PDF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentError {
    /// The bytes are not a readable PDF.
    NotPdf,
    /// The PDF has no embedded Europass XML (for example a printed CV).
    NoEuropassXml,
    /// The PDF is larger than [`MAX_INPUT`], or its embedded XML is larger
    /// than [`MAX_ATTACHMENT`].
    TooLarge,
}

/// Returns the embedded Europass `Candidate` XML of a Europass PDF.
///
/// Every embedded file is checked, and the first one that declares the
/// Europass namespace is returned, whatever its file name.
pub fn embedded_xml(pdf: &[u8]) -> Result<Vec<u8>, AttachmentError> {
    if pdf.len() > MAX_INPUT {
        return Err(AttachmentError::TooLarge);
    }
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
        // Decoding stops at the limit, so a forged or missing size cannot
        // inflate a small stream into memory (a decompression bomb).
        let content = match stream.get_plain_content_with_limit(MAX_ATTACHMENT) {
            Ok(content) => content,
            Err(lopdf::Error::Decompress(lopdf::DecompressError::MemoryLimitExceeded {
                ..
            })) => {
                too_large = true;
                continue;
            }
            Err(_) => continue,
        };
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

/// True when `xml` parses and its root element is `Candidate` in the
/// Europass namespace.
fn declares_europass(xml: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(xml) else {
        return false;
    };
    let Ok(doc) = roxmltree::Document::parse(text) else {
        return false;
    };
    let root = doc.root_element().tag_name();
    root.name() == "Candidate" && root.namespace() == Some(EUROPASS_NS)
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

    #[test]
    fn decompression_stops_at_the_attachment_limit() {
        // Compresses to a few KB but inflates past the limit, with no
        // declared size to catch it early.
        let mut bomb = Vec::with_capacity(MAX_ATTACHMENT + 1);
        bomb.extend_from_slice(XML.as_bytes());
        bomb.resize(MAX_ATTACHMENT + 1, b' ');
        let pdf = pdf_with(&[("cv.xml", &bomb, true)]);
        assert!(pdf.len() < MAX_ATTACHMENT / 100);
        assert_eq!(embedded_xml(&pdf), Err(AttachmentError::TooLarge));
    }

    #[test]
    fn oversize_pdf_is_refused_before_parsing() {
        let big = vec![b' '; MAX_INPUT + 1];
        assert_eq!(embedded_xml(&big), Err(AttachmentError::TooLarge));
    }

    #[test]
    fn only_a_candidate_root_in_the_europass_namespace_counts() {
        let other_root =
            br#"<Other xmlns="http://www.europass.eu/1.0"><!-- <Candidate --></Other>"#;
        let wrong_ns = br#"<Candidate xmlns="http://example.com/"/>"#;
        let prefixed = br#"<e:Candidate xmlns:e="http://www.europass.eu/1.0"/>"#;
        assert!(!declares_europass(other_root));
        assert!(!declares_europass(wrong_ns));
        assert!(!declares_europass(
            b"<Candidate xmlns=\"http://www.europass.eu/1.0\">"
        ));
        assert!(declares_europass(prefixed));
        assert!(declares_europass(XML.as_bytes()));
        let pdf = pdf_with(&[("a.xml", other_root, true), ("b.xml", XML.as_bytes(), true)]);
        assert_eq!(embedded_xml(&pdf).expect("xml"), XML.as_bytes());
    }
}
