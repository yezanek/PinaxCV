//! The standalone HTML exporter (spec section 6.0).
//!
//! One UTF-8 file with embedded CSS, no scripts and no external resources,
//! meant for upload to a personal website. It is built from the same data as
//! the PDF, never from the PDF itself, and every piece of user text is
//! escaped. Only `https:` and `mailto:` links are clickable.

use std::fmt::Write as _;

use cv_core::ResolvedCv;

use crate::data::{EntriesData, EntryData, LinkData, SectionData, TypstData, to_typst_data};
use crate::{ExportOptions, ExportedDocument, Exporter, RenderError, suggested_file_name};

/// Single column, system fonts, automatic dark mode and a print stylesheet.
const CSS: &str = include_str!("../templates/html/cv.css");

/// Exports the CV as one self-contained HTML file.
#[derive(Clone, Copy, Debug, Default)]
pub struct HtmlExporter;

impl Exporter for HtmlExporter {
    fn export(
        &self,
        cv: &ResolvedCv,
        options: &ExportOptions,
    ) -> Result<ExportedDocument, RenderError> {
        Ok(ExportedDocument {
            bytes: to_html(cv, options).into_bytes(),
            media_type: "text/html; charset=utf-8",
            suggested_file_name: suggested_file_name(&cv.basics.full_name, "html"),
            page_count: None,
        })
    }
}

/// Renders `cv` as a complete HTML document.
pub fn to_html(cv: &ResolvedCv, options: &ExportOptions) -> String {
    let d = to_typst_data(cv, options);
    let mut out = String::with_capacity(8 * 1024);
    head(&mut out, &d);
    header(&mut out, &d);
    out.push_str("<main>\n");
    if let Some(summary) = &d.summary {
        let _ = writeln!(
            out,
            "<section class=\"summary\">\n<h2>{}</h2>\n<p>{}</p>\n</section>",
            esc(d.labels.summary),
            esc(summary)
        );
    }
    for section in &d.sections {
        write_section(&mut out, section);
    }
    out.push_str("</main>\n</body>\n</html>\n");
    out
}

fn head(out: &mut String, d: &TypstData) {
    let _ = write!(
        out,
        "<!DOCTYPE html>\n<html lang=\"{lang}\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta name=\"author\" content=\"{author}\">\n<meta name=\"generator\" content=\"PinaxCV\">\n\
         <title>{title}</title>\n<style>\n:root {{ --accent: {accent}; }}\n{CSS}</style>\n\
         </head>\n<body>\n",
        lang = esc(d.locale),
        author = esc(&d.meta.author),
        title = esc(&d.meta.title),
        accent = accent(d.opts.accent.as_deref()),
    );
}

/// A `#RRGGBB` accent, or the text colour when unset or malformed, so a
/// stored value can never inject CSS.
fn accent(value: Option<&str>) -> String {
    match value {
        Some(v)
            if v.len() == 7
                && v.starts_with('#')
                && v[1..].chars().all(|c| c.is_ascii_hexdigit()) =>
        {
            v.to_owned()
        }
        _ => "currentColor".to_owned(),
    }
}

fn header(out: &mut String, d: &TypstData) {
    let b = &d.basics;
    out.push_str("<header>\n");
    let _ = writeln!(out, "<h1>{}</h1>", esc(&b.full_name));
    if let Some(h) = &b.headline {
        let _ = writeln!(out, "<p class=\"headline\">{}</p>", esc(h));
    }
    let mut items = Vec::new();
    if let Some(email) = &b.email {
        items.push(format!("<a href=\"mailto:{0}\">{0}</a>", esc(email)));
    }
    if let Some(phone) = &b.phone {
        items.push(esc(phone));
    }
    if let Some(location) = &b.location {
        items.push(esc(location));
    }
    for l in &b.links {
        items.push(match &l.label {
            Some(label) => format!("{}: {}", esc(label), link_html(l)),
            None => link_html(l),
        });
    }
    if !items.is_empty() {
        out.push_str("<ul class=\"contact\">\n");
        for item in items {
            let _ = writeln!(out, "<li>{item}</li>");
        }
        out.push_str("</ul>\n");
    }
    out.push_str("</header>\n");
}

fn write_section(out: &mut String, section: &SectionData) {
    match section {
        SectionData::Experience(s)
        | SectionData::Education(s)
        | SectionData::Projects(s)
        | SectionData::Certifications(s)
        | SectionData::Custom(s) => entries_section(out, s),
        SectionData::Skills { heading, groups } => {
            let _ = writeln!(
                out,
                "<section>\n<h2>{}</h2>\n<ul class=\"skills\">",
                esc(heading)
            );
            for g in groups {
                let skills = g
                    .skills
                    .iter()
                    .map(|s| esc(s))
                    .collect::<Vec<_>>()
                    .join(", ");
                match &g.label {
                    Some(label) => {
                        let _ = writeln!(out, "<li><strong>{}:</strong> {skills}</li>", esc(label));
                    }
                    None => {
                        let _ = writeln!(out, "<li>{skills}</li>");
                    }
                }
            }
            out.push_str("</ul>\n</section>\n");
        }
        SectionData::Languages { heading, items } => {
            let _ = writeln!(
                out,
                "<section>\n<h2>{}</h2>\n<ul class=\"languages\">",
                esc(heading)
            );
            for i in items {
                match &i.level {
                    Some(level) => {
                        let _ = writeln!(out, "<li>{} ({})</li>", esc(&i.name), esc(level));
                    }
                    None => {
                        let _ = writeln!(out, "<li>{}</li>", esc(&i.name));
                    }
                }
            }
            out.push_str("</ul>\n</section>\n");
        }
    }
}

fn entries_section(out: &mut String, s: &EntriesData) {
    out.push_str("<section>\n");
    if !s.heading.is_empty() {
        let _ = writeln!(out, "<h2>{}</h2>", esc(&s.heading));
    }
    for e in &s.entries {
        entry(out, e);
    }
    out.push_str("</section>\n");
}

fn entry(out: &mut String, e: &EntryData) {
    out.push_str("<article class=\"entry\">\n");
    let head = match (&e.title, &e.org) {
        (Some(t), Some(o)) => Some(format!("{}<span class=\"org\">, {}</span>", esc(t), esc(o))),
        (Some(t), None) => Some(esc(t)),
        (None, Some(o)) => Some(esc(o)),
        (None, None) => None,
    };
    if let Some(head) = head {
        let _ = writeln!(out, "<h3>{head}</h3>");
    }
    let meta: Vec<String> = [&e.period, &e.location]
        .into_iter()
        .flatten()
        .map(|s| esc(s))
        .collect();
    if !meta.is_empty() {
        let _ = writeln!(out, "<p class=\"meta\">{}</p>", meta.join(" | "));
    }
    if let Some(desc) = &e.description {
        let _ = writeln!(out, "<p>{}</p>", esc(desc));
    }
    if let Some(link) = &e.link {
        let _ = writeln!(out, "<p>{}</p>", link_html(link));
    }
    if !e.bullets.is_empty() {
        out.push_str("<ul>\n");
        for b in &e.bullets {
            let _ = writeln!(out, "<li>{}</li>", esc(b));
        }
        out.push_str("</ul>\n");
    }
    out.push_str("</article>\n");
}

/// A link for clickable URLs, plain text for every other scheme.
fn link_html(l: &LinkData) -> String {
    if l.clickable {
        format!("<a href=\"{0}\">{0}</a>", esc(&l.url))
    } else {
        esc(&l.url)
    }
}

/// Escapes text for element content and quoted attribute values.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdata::{self, FIXTURES};

    fn html(name: &str) -> String {
        to_html(
            &testdata::fixture(name).resolve(None),
            &ExportOptions::default(),
        )
    }

    /// Every resource the document would load: `src=`, `<link>`, `<script>`,
    /// `@import` and CSS `url(` that is not a `data:` URI. `<a href>` is
    /// navigation and allowed.
    fn external_resources(doc: &str) -> Vec<String> {
        let lower = doc.to_lowercase();
        let mut found = Vec::new();
        for needle in ["src=", "<link", "<script", "@import", "url("] {
            for (i, _) in lower.match_indices(needle) {
                let rest = lower[i + needle.len()..].trim_start_matches(['"', '\'', ' ']);
                if !(needle == "url(" && rest.starts_with("data:")) {
                    found.push(lower[i..lower.len().min(i + 40)].to_owned());
                }
            }
        }
        found
    }

    fn hrefs(doc: &str) -> Vec<String> {
        doc.match_indices("href=\"")
            .map(|(i, m)| {
                let rest = &doc[i + m.len()..];
                rest[..rest.find('"').expect("closing quote")].to_owned()
            })
            .collect()
    }

    #[test]
    fn every_fixture_is_a_standalone_document() {
        for name in FIXTURES {
            let doc = html(name);
            assert!(
                doc.starts_with("<!DOCTYPE html>\n<html lang=\"en\">"),
                "{name}"
            );
            assert!(doc.contains("<meta charset=\"utf-8\">"), "{name}");
            assert_eq!(doc.matches("<style>").count(), 1, "{name}");
            assert!(doc.ends_with("</html>\n"), "{name}");
            assert_eq!(external_resources(&doc), Vec::<String>::new(), "{name}");
        }
    }

    #[test]
    fn resource_detector_catches_loads() {
        for bad in [
            "<img src=\"https://x\">",
            "<link rel=stylesheet href=x>",
            "<script>1</script>",
            "@import 'x.css';",
            "a { background: url(https://x) }",
        ] {
            assert!(!external_resources(bad).is_empty(), "{bad}");
        }
        assert!(external_resources("a { background: url(data:image/png;base64,AA) }").is_empty());
        assert!(external_resources("<a href=\"https://x\">x</a>").is_empty());
    }

    #[test]
    fn user_text_is_escaped() {
        let doc = html("special_chars");
        assert!(!doc.contains("<script>alert"), "{doc}");
        assert!(doc.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(doc.contains("Jane &quot;JD&quot; O&#39;Doe &amp; Co"));
        assert_eq!(
            esc("<a href=\"x\">&'"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
    }

    #[test]
    fn only_https_and_mailto_are_links() {
        for name in FIXTURES {
            for href in hrefs(&html(name)) {
                assert!(
                    href.starts_with("https:") || href.starts_with("mailto:"),
                    "{name}: {href}"
                );
            }
        }
        let doc = html("unsafe_links");
        assert!(doc.contains("<a href=\"https:"));
        for scheme in ["javascript:", "data:", "http:", "tel:"] {
            assert!(!doc.contains(&format!("href=\"{scheme}")), "{scheme}");
            assert!(doc.contains(scheme), "{scheme} is still printed as text");
        }
    }

    #[test]
    fn contact_options_control_email_and_phone() {
        let cv = testdata::fixture("typical").resolve(None);
        let both = to_html(&cv, &ExportOptions::default());
        assert!(both.contains("mailto:jane@example.com") && both.contains("+49 30 1234 5678"));
        let neither = to_html(
            &cv,
            &ExportOptions {
                include_email: false,
                include_phone: false,
            },
        );
        assert!(!neither.contains("jane@example.com") && !neither.contains("+49 30"));
    }

    #[test]
    fn semantic_structure_and_styles() {
        let doc = html("typical");
        for tag in [
            "<header>",
            "<main>",
            "<section>",
            "<h1>Jane Doe</h1>",
            "<h2>Work Experience</h2>",
            "<h3>",
            "<ul>",
        ] {
            assert!(doc.contains(tag), "{tag}");
        }
        assert!(doc.contains("<title>Jane Doe, CV</title>"));
        assert!(doc.contains("prefers-color-scheme: dark") && doc.contains("@media print"));
        assert!(doc.contains("font-family: system-ui"));
    }

    #[test]
    fn accent_is_sanitised() {
        assert_eq!(accent(Some("#1A2b3C")), "#1A2b3C");
        for bad in [
            None,
            Some("red"),
            Some("#12345"),
            Some("#123456; } body { x"),
            Some("#gggggg"),
        ] {
            assert_eq!(accent(bad), "currentColor", "{bad:?}");
        }
    }

    #[test]
    fn exporter_returns_named_utf8_document() {
        let cv = testdata::fixture("unicode").resolve(None);
        let doc = HtmlExporter
            .export(&cv, &ExportOptions::default())
            .expect("exports");
        assert_eq!(doc.media_type, "text/html; charset=utf-8");
        assert!(doc.suggested_file_name.ends_with("-CV.html"));
        assert_eq!(doc.page_count, None);
        let text = String::from_utf8(doc.bytes).expect("utf-8");
        assert!(text.contains(cv.basics.full_name.trim()));
    }
}
