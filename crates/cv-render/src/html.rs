//! The standalone HTML exporter (spec section 6.0).
//!
//! One UTF-8 file with embedded CSS, no scripts and no external resources,
//! meant for upload to a personal website. It is built from the same data as
//! the PDF, never from the PDF itself, and every piece of user text is
//! escaped. Only `https:` and `mailto:` links are clickable.

use std::fmt::Write as _;

use cv_core::ResolvedCv;

use crate::data::{
    BlockData, EntriesData, EntryData, LinkData, SectionData, TypstData, to_typst_data,
};
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
            warnings: Vec::new(),
        })
    }
}

/// Renders `cv` as a complete HTML document. `options` are the user's export
/// choices; callers without one should pass [`ExportOptions::html_default`].
pub fn to_html(cv: &ResolvedCv, options: &ExportOptions) -> String {
    let d = to_typst_data(cv, options);
    let mut out = String::with_capacity(8 * 1024);
    head(&mut out, &d);
    let photo = options.photo.as_deref().filter(|_| d.basics.photo);
    let l = &d.layout;
    if l.two {
        // Two columns (ADR-084): the DOM is in reading order, the name's
        // column first, and that column is also the one drawn on the left.
        let side = u16::from(l.sidebar_width_pct);
        let (first, second) = if l.sidebar_first {
            (side, 100 - side)
        } else {
            (100 - side, side)
        };
        let _ = writeln!(
            out,
            "<div class=\"columns\" style=\"--cols: {first}fr {second}fr\">"
        );
        let sidebar = |out: &mut String| {
            out.push_str("<div class=\"sidebar\">\n");
            write_blocks(out, &d, &l.sidebar, photo);
            out.push_str("</div>\n");
        };
        let main = |out: &mut String| {
            out.push_str("<main>\n");
            write_blocks(out, &d, &l.main, photo);
            out.push_str("</main>\n");
        };
        if l.sidebar_first {
            sidebar(&mut out);
            main(&mut out);
        } else {
            main(&mut out);
            sidebar(&mut out);
        }
        out.push_str("</div>\n");
    } else {
        // One column: a leading name and contact block is the page header,
        // outside `<main>`.
        let lead = header_len(&l.main);
        write_blocks(&mut out, &d, &l.main[..lead], photo);
        out.push_str("<main>\n");
        write_blocks(&mut out, &d, &l.main[lead..], photo);
        out.push_str("</main>\n");
    }
    out.push_str("</body>\n</html>\n");
    out
}

/// How many of `blocks` form the leading header: the name, with the
/// contact block when it follows directly.
fn header_len(blocks: &[BlockData]) -> usize {
    match blocks {
        [BlockData::Name, BlockData::Contact, ..] => 2,
        [BlockData::Name, ..] => 1,
        _ => 0,
    }
}

/// Writes one column's blocks in order. A name followed by the contact
/// block shares one `<header>` with the photo.
fn write_blocks(out: &mut String, d: &TypstData, blocks: &[BlockData], photo: Option<&[u8]>) {
    let mut i = 0;
    while i < blocks.len() {
        match blocks[i] {
            BlockData::Name => {
                let with_contact = matches!(blocks.get(i + 1), Some(BlockData::Contact));
                header(out, d, photo, with_contact);
                if with_contact {
                    i += 1;
                }
            }
            BlockData::Contact => contact(out, d),
            BlockData::Summary => {
                if let Some(summary) = &d.summary {
                    let _ = writeln!(
                        out,
                        "<section class=\"summary\">\n<h2>{}</h2>\n<p>{}</p>\n</section>",
                        esc(d.labels.summary),
                        esc(summary)
                    );
                }
            }
            BlockData::Section { index } => write_section(out, &d.sections[index]),
        }
        i += 1;
    }
}

fn head(out: &mut String, d: &TypstData) {
    let _ = write!(
        out,
        "<!DOCTYPE html>\n<html lang=\"{lang}\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta name=\"author\" content=\"{author}\">\n<meta name=\"generator\" content=\"PinaxCV\">\n\
         <title>{title}</title>\n<style>\n:root {{ --accent: {accent}; }}\n{CSS}</style>\n\
         </head>\n<body{body_class}>\n",
        lang = esc(d.locale),
        author = esc(&d.meta.author),
        title = esc(&d.meta.title),
        accent = accent(d.opts.accent.as_deref()),
        body_class = if d.layout.two { " class=\"two\"" } else { "" },
    );
}

/// Standard base64 with padding, for the photo's `data:` URI.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
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

fn header(out: &mut String, d: &TypstData, photo: Option<&[u8]>, with_contact: bool) {
    let b = &d.basics;
    out.push_str("<header>\n");
    // Decorative (empty alt), so the page's text is the same with and
    // without the photo (ADR-083).
    if let Some(jpeg) = photo {
        let _ = writeln!(
            out,
            "<img class=\"photo\" src=\"data:image/jpeg;base64,{}\" alt=\"\">",
            base64(jpeg)
        );
    }
    let _ = writeln!(out, "<h1>{}</h1>", esc(&b.full_name));
    if let Some(h) = &b.headline {
        let _ = writeln!(out, "<p class=\"headline\">{}</p>", esc(h));
    }
    if with_contact {
        contact(out, d);
    }
    out.push_str("</header>\n");
}

fn contact(out: &mut String, d: &TypstData) {
    let b = &d.basics;
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
    // Untitled sections have no h2, so their entries move up a level and
    // heading navigation never skips one.
    let level = if s.heading.is_empty() {
        2
    } else {
        let _ = writeln!(out, "<h2>{}</h2>", esc(&s.heading));
        3
    };
    for e in &s.entries {
        entry(out, e, level);
    }
    out.push_str("</section>\n");
}

fn entry(out: &mut String, e: &EntryData, level: u8) {
    out.push_str("<article class=\"entry\">\n");
    let head = match (&e.title, &e.org) {
        (Some(t), Some(o)) => Some(format!("{}<span class=\"org\">, {}</span>", esc(t), esc(o))),
        (Some(t), None) => Some(esc(t)),
        (None, Some(o)) => Some(esc(o)),
        (None, None) => None,
    };
    if let Some(head) = head {
        let _ = writeln!(out, "<h{level}>{head}</h{level}>");
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
            &ExportOptions::html_default(),
        )
    }

    /// Every resource the document would load. Tags are parsed: `<link>`,
    /// `<script>`, `<img>`, `<iframe>`, `<object>`, `<embed>`, `<source>`,
    /// `<video>` and `<audio>`; any `src`, `srcset`, `poster` or `data`
    /// attribute; and `@import` or non-`data:` `url(` inside `<style>` or a
    /// `style` attribute. An `<img>` whose `src` is a `data:` URI (the
    /// photo) loads nothing and is allowed. Text between tags is never
    /// inspected, and `<a href>` is navigation and allowed.
    fn external_resources(doc: &str) -> Vec<String> {
        const LOADING_TAGS: [&str; 9] = [
            "link", "script", "img", "iframe", "object", "embed", "source", "video", "audio",
        ];
        const LOADING_ATTRS: [&str; 4] = ["src", "srcset", "poster", "data"];
        let mut found = Vec::new();
        let mut rest = doc;
        while let Some(open) = rest.find('<') {
            rest = &rest[open + 1..];
            let Some(close) = rest.find('>') else { break };
            let tag = &rest[..close];
            rest = &rest[close + 1..];
            let (name, attrs) = parse_tag(tag);
            let embedded = |attr: &str, value: &str| {
                name == "img" && attr == "src" && value.to_ascii_lowercase().starts_with("data:")
            };
            let embedded_image = attrs.iter().any(|(a, v)| embedded(a, v));
            if LOADING_TAGS.contains(&name.as_str()) && !embedded_image {
                found.push(format!("<{name}>"));
            }
            for (attr, value) in &attrs {
                if embedded(attr, value) {
                    continue;
                }
                if LOADING_ATTRS.contains(&attr.as_str()) {
                    found.push(format!("{attr}={value}"));
                } else if attr == "style" {
                    found.extend(css_resources(value));
                }
            }
            if name == "style" {
                let end = rest
                    .to_ascii_lowercase()
                    .find("</style")
                    .unwrap_or(rest.len());
                found.extend(css_resources(&rest[..end]));
                rest = &rest[end..];
            }
        }
        found
    }

    /// The lowercased tag name and its `(name, value)` attributes, allowing
    /// whitespace around `=` and quoted or bare values.
    fn parse_tag(tag: &str) -> (String, Vec<(String, String)>) {
        let tag = tag.trim_start_matches('/');
        let name_end = tag
            .find(|c: char| c.is_whitespace() || c == '/')
            .unwrap_or(tag.len());
        let name = tag[..name_end].to_ascii_lowercase();
        let mut attrs = Vec::new();
        let chars: Vec<char> = tag[name_end..].chars().collect();
        let mut i = 0;
        while i < chars.len() {
            while i < chars.len() && (chars[i].is_whitespace() || chars[i] == '/') {
                i += 1;
            }
            let start = i;
            while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '=' && chars[i] != '/'
            {
                i += 1;
            }
            if start == i {
                break;
            }
            let attr: String = chars[start..i]
                .iter()
                .collect::<String>()
                .to_ascii_lowercase();
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            let mut value = String::new();
            if i < chars.len() && chars[i] == '=' {
                i += 1;
                while i < chars.len() && chars[i].is_whitespace() {
                    i += 1;
                }
                if i < chars.len() && (chars[i] == '"' || chars[i] == '\'') {
                    let quote = chars[i];
                    i += 1;
                    while i < chars.len() && chars[i] != quote {
                        value.push(chars[i]);
                        i += 1;
                    }
                    i += 1;
                } else {
                    while i < chars.len() && !chars[i].is_whitespace() {
                        value.push(chars[i]);
                        i += 1;
                    }
                }
            }
            attrs.push((attr, value));
        }
        (name, attrs)
    }

    /// `@import` and every `url(` that is not a `data:` URI in a stylesheet.
    fn css_resources(css: &str) -> Vec<String> {
        let lower = css.to_ascii_lowercase();
        let mut found = Vec::new();
        if lower.contains("@import") {
            found.push("@import".to_owned());
        }
        for (i, m) in lower.match_indices("url(") {
            let arg = lower[i + m.len()..].trim_start_matches(['"', '\'', ' ']);
            if !arg.starts_with("data:") {
                found.push(lower[i..lower.len().min(i + 40)].to_owned());
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
            "<p src = 'https://x'>",
            "<IMG SRC=https://x>",
            "<link rel=stylesheet href=x>",
            "<script>1</script>",
            "<style>@import 'x.css';</style>",
            "<style>a { background: url(https://x) }</style>",
            "<p style=\"background: url( 'https://x' )\">x</p>",
        ] {
            assert!(!external_resources(bad).is_empty(), "{bad}");
        }
        for fine in [
            "<style>a { background: url(data:image/png;base64,AA) }</style>",
            "<a href=\"https://x\">x</a>",
            "<p>Set src=x and url(y) and @import in text</p>",
        ] {
            assert!(external_resources(fine).is_empty(), "{fine}");
        }
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
    fn base64_matches_the_standard_alphabet_and_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(&[0xfb, 0xff, 0xbf]), "+/+/");
    }

    #[test]
    fn a_shown_photo_is_an_embedded_decorative_image() {
        let jpeg: std::sync::Arc<[u8]> = vec![0xff, 0xd8, 0xff, 0xd9].into();
        let options = ExportOptions {
            photo: Some(jpeg),
            ..ExportOptions::html_default()
        };
        let mut cv = testdata::fixture("typical").resolve(None);
        assert!(!to_html(&cv, &options).contains("<img"), "not shown");
        let plain = to_html(&cv, &ExportOptions::html_default());
        cv.show_photo = true;
        let doc = to_html(&cv, &options);
        let img = "<img class=\"photo\" src=\"data:image/jpeg;base64,/9j/2Q==\" alt=\"\">\n";
        assert!(doc.contains(img), "{doc}");
        assert_eq!(doc.replace(img, ""), plain, "the photo adds nothing else");
        assert_eq!(external_resources(&doc), Vec::<String>::new());
        // A remote image is still caught, and so is a srcset beside a data: src.
        assert_eq!(
            external_resources("<img src=\"https://x.test/a.png\">"),
            ["<img>", "src=https://x.test/a.png"]
        );
        assert_eq!(
            external_resources(
                "<img src=\"data:image/png;base64,AA\" srcset=\"https://x.test/b.png 2x\">"
            ),
            ["srcset=https://x.test/b.png 2x"]
        );
    }

    #[test]
    fn contact_options_control_email_and_phone() {
        let cv = testdata::fixture("typical").resolve(None);
        let default = to_html(&cv, &ExportOptions::html_default());
        assert!(default.contains("mailto:jane@example.com") && !default.contains("+49 30"));
        let both = to_html(
            &cv,
            &ExportOptions {
                include_email: true,
                include_phone: true,
                photo: None,
            },
        );
        assert!(both.contains("mailto:jane@example.com") && both.contains("+49 30 1234 5678"));
        let neither = to_html(
            &cv,
            &ExportOptions {
                include_email: false,
                include_phone: false,
                photo: None,
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
    fn entries_in_untitled_sections_do_not_skip_a_heading_level() {
        let cv = testdata::fixture("typical").resolve(None);
        let mut d = to_typst_data(&cv, &ExportOptions::html_default());
        let SectionData::Experience(exp) = &mut d.sections[0] else {
            panic!("experience")
        };
        let mut titled = String::new();
        entries_section(&mut titled, exp);
        assert!(
            titled.contains("<h2>") && titled.contains("<h3>"),
            "{titled}"
        );

        exp.heading = String::new();
        let mut out = String::new();
        entries_section(&mut out, exp);
        assert!(!out.contains("<h3>") && out.contains("<h2>"), "{out}");
    }

    #[test]
    fn two_columns_keep_the_name_column_first_in_the_dom() {
        let doc = html("two_column");
        assert!(doc.contains("<body class=\"two\">"));
        assert!(doc.contains("--cols: 32fr 68fr"));
        let at = |s: &str| doc.find(s).unwrap_or_else(|| panic!("{s} missing"));
        assert!(at("<div class=\"sidebar\">") < at("<h1>"));
        assert!(at("<h1>") < at("<h2>Skills</h2>"));
        assert!(at("<h2>Languages</h2>") < at("<main>"));
        assert!(at("<main>") < at("<h2>Summary</h2>"));

        let doc = html("two_column_long");
        assert!(doc.contains("--cols: 68fr 32fr"), "main is first and wide");
        assert!(doc.find("<main>") < doc.find("<div class=\"sidebar\">"));
        assert!(!html("typical").contains("<div class=\"columns\""));
    }

    #[test]
    fn header_takes_a_leading_name_and_contact() {
        use BlockData::*;
        assert_eq!(header_len(&[Name, Contact, Summary]), 2);
        assert_eq!(header_len(&[Name, Summary, Contact]), 1);
        assert_eq!(header_len(&[Summary, Name, Contact]), 0);
        assert_eq!(header_len(&[]), 0);
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
            .export(&cv, &ExportOptions::html_default())
            .expect("exports");
        assert_eq!(doc.media_type, "text/html; charset=utf-8");
        assert!(doc.suggested_file_name.ends_with("-CV.html"));
        assert_eq!(doc.page_count, None);
        let text = String::from_utf8(doc.bytes).expect("utf-8");
        assert!(text.contains(cv.basics.full_name.trim()));
    }
}
