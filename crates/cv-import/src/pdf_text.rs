//! First step of the generic PDF importer (ADR-047): the page text in reading
//! order, split into sections at standard CV headings, the way Workday-style
//! parsers read a PDF. Entry detection inside sections comes next.

use lopdf::Document;

/// Largest decompressed text stream accepted per page.
const MAX_STREAM: usize = 10 * 1024 * 1024;

/// A section heading recognised in the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heading {
    /// Lines before the first heading: usually the name and headline.
    Top,
    Contact,
    Summary,
    Experience,
    Education,
    Skills,
    Languages,
    Certifications,
}

/// The PDF could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotReadable;

/// Extracts the text of every page in order.
pub fn page_text(pdf: &[u8]) -> Result<String, NotReadable> {
    let doc = Document::load_mem(pdf).map_err(|_| NotReadable)?;
    let pages: Vec<u32> = doc.get_pages().keys().copied().collect();
    doc.extract_text_with_limit(&pages, MAX_STREAM)
        .map_err(|_| NotReadable)
}

fn heading(line: &str) -> Option<Heading> {
    let l = line.trim().trim_end_matches(':').to_lowercase();
    Some(match l.as_str() {
        "contact" | "contact information" | "contact details" => Heading::Contact,
        "summary" | "profile" | "about" | "about me" | "professional summary" => Heading::Summary,
        "experience"
        | "work experience"
        | "employment"
        | "employment history"
        | "professional experience" => Heading::Experience,
        "education" | "education and training" => Heading::Education,
        "skills" | "top skills" | "technical skills" => Heading::Skills,
        "languages" | "language skills" => Heading::Languages,
        "certifications" | "licenses & certifications" | "certificates" => Heading::Certifications,
        _ => return None,
    })
}

/// A "Page 2 of 5" footer, which interrupts entries across page breaks.
fn is_page_footer(line: &str) -> bool {
    let mut w = line.split_whitespace();
    matches!(
        (w.next(), w.next(), w.next(), w.next(), w.next()),
        (Some("Page"), Some(a), Some("of"), Some(b), None)
            if a.parse::<u32>().is_ok() && b.parse::<u32>().is_ok()
    )
}

/// Splits text into sections at recognised headings, dropping blank lines and
/// page footers. A heading that appears twice continues the same section.
pub fn split_sections(text: &str) -> Vec<(Heading, Vec<String>)> {
    let mut out: Vec<(Heading, Vec<String>)> = vec![(Heading::Top, Vec::new())];
    let mut current = 0;
    for line in text.lines().map(str::trim) {
        if line.is_empty() || is_page_footer(line) {
            continue;
        }
        if let Some(h) = heading(line) {
            current = match out.iter().position(|(k, _)| *k == h) {
                Some(i) => i,
                None => {
                    out.push((h, Vec::new()));
                    out.len() - 1
                }
            };
            continue;
        }
        out[current].1.push(line.to_owned());
    }
    out.retain(|(h, lines)| *h != Heading::Top || !lines.is_empty());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::{Object, Stream, dictionary};

    fn pdf(lines: &[&str]) -> Vec<u8> {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let font = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let mut ops = vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 11.into()]),
            Operation::new("Td", vec![50.into(), 800.into()]),
        ];
        for l in lines {
            ops.push(Operation::new("Tj", vec![Object::string_literal(*l)]));
            ops.push(Operation::new("Td", vec![0.into(), (-14).into()]));
        }
        ops.push(Operation::new("ET", vec![]));
        let content = Content { operations: ops };
        let content_id = doc.add_object(Stream::new(
            dictionary! {},
            content.encode().expect("encode"),
        ));
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1,
            }),
        );
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        let mut out = Vec::new();
        doc.save_to(&mut out).expect("save");
        out
    }

    #[test]
    fn extracts_page_text() {
        let text =
            page_text(&pdf(&["Alex Sample", "Experience", "Platform Engineer"])).expect("text");
        assert!(text.contains("Alex Sample"));
        assert!(text.contains("Platform Engineer"));
        assert_eq!(page_text(b"not a pdf"), Err(NotReadable));
    }

    #[test]
    fn splits_at_headings_and_drops_page_footers() {
        let text = "Alex Sample\nPlatform Engineer\n\nContact\nalex@example.org\nTop Skills\nRust\nExperience\nExample Works\nPage 1 of 2\nPlatform Engineer\nEducation:\nExample University\nSKILLS\nSQL";
        let s = split_sections(text);
        let kinds: Vec<Heading> = s.iter().map(|(h, _)| *h).collect();
        assert_eq!(
            kinds,
            [
                Heading::Top,
                Heading::Contact,
                Heading::Skills,
                Heading::Experience,
                Heading::Education
            ]
        );
        assert_eq!(s[0].1, ["Alex Sample", "Platform Engineer"]);
        assert_eq!(s[2].1, ["Rust", "SQL"]);
        assert_eq!(s[3].1, ["Example Works", "Platform Engineer"]);
    }

    #[test]
    fn footer_detection_is_strict() {
        assert!(is_page_footer("Page 3 of 5"));
        assert!(!is_page_footer("Page three of five"));
        assert!(!is_page_footer("Page 3 of 5 extra"));
        assert!(split_sections("").is_empty());
    }
}
