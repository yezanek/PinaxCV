//! Style files (`.pinaxstyle.xml`, ADR-084): a CV's template, paper,
//! options and layout as XML, with no CV content. The format is described
//! in `docs/IMPORT_EXPORT.md`.

use std::fmt::Write as _;
use std::fs::File;
use std::io::Read;

use cv_api::{ApiError, ErrorCode, StyleOpened, StyleSave, StyleSaved};
use cv_core::{
    Column, Columns, DateStyle, Layout, Paper, PlacedBlock, SectionKind, Style, StyleBlock,
    TemplateId, TemplateOptions, TopBlock,
};

use crate::{App, ExportKind, Result, templates};

/// The style file namespace; the version is the `version` attribute.
pub const STYLE_NAMESPACE: &str = "urn:pinaxcv:style:1";
/// The newest format version this build reads and the one it writes.
pub const STYLE_VERSION: u32 = 1;
/// Largest style file accepted.
pub const MAX_STYLE_FILE: usize = 256 * 1024;
/// What the open dialog offers.
pub const STYLE_EXTENSIONS: &[&str] = &["xml"];
/// The file name ending of style files.
pub const STYLE_SUFFIX: &str = ".pinaxstyle.xml";

fn columns_name(c: Columns) -> &'static str {
    match c {
        Columns::One => "one",
        Columns::Two => "two",
    }
}

fn column_name(c: Column) -> &'static str {
    match c {
        Column::Sidebar => "sidebar",
        Column::Main => "main",
    }
}

fn paper_name(p: Paper) -> &'static str {
    match p {
        Paper::A4 => "a4",
        Paper::Letter => "letter",
    }
}

fn date_style_name(d: DateStyle) -> &'static str {
    match d {
        DateStyle::MonthNameYear => "month_name_year",
        DateStyle::NumericMonthYear => "numeric_month_year",
        DateStyle::YearOnly => "year_only",
    }
}

fn top_name(b: TopBlock) -> &'static str {
    match b {
        TopBlock::Name => "name",
        TopBlock::Contact => "contact",
        TopBlock::Summary => "summary",
    }
}

/// Escapes text for an attribute value in double quotes.
fn attr(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            // XML 1.0 cannot carry other control characters at all.
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Writes `style` as a style file.
pub fn style_to_xml(style: &Style) -> String {
    let o = &style.options;
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        out,
        "<pinaxcv-style xmlns=\"{STYLE_NAMESPACE}\" version=\"{STYLE_VERSION}\">"
    );
    let _ = writeln!(out, "  <template id=\"{}\"/>", attr(&style.template.0));
    let _ = writeln!(out, "  <paper size=\"{}\"/>", paper_name(style.paper));
    let _ = write!(
        out,
        "  <options font-size-pt=\"{}\" line-spacing=\"{}\" margin-mm=\"{}\" date-style=\"{}\"",
        o.font_size_pt,
        o.line_spacing,
        o.margin_mm,
        date_style_name(o.date_style)
    );
    if let Some(accent) = &o.accent {
        let _ = write!(out, " accent=\"{}\"", attr(accent));
    }
    out.push_str("/>\n");
    let _ = writeln!(
        out,
        "  <layout columns=\"{}\" sidebar-width-pct=\"{}\">",
        columns_name(style.columns),
        style.sidebar_width_pct
    );
    for b in &style.blocks {
        let column = column_name(b.column);
        match &b.block {
            StyleBlock::Top { block } => {
                let _ = writeln!(
                    out,
                    "    <block kind=\"{}\" column=\"{column}\"/>",
                    top_name(*block)
                );
            }
            StyleBlock::Section { section, title } => {
                let _ = write!(out, "    <block kind=\"{}\"", section.as_str());
                if let Some(title) = title {
                    let _ = write!(out, " title=\"{}\"", attr(title));
                }
                let _ = writeln!(out, " column=\"{column}\"/>");
            }
        }
    }
    out.push_str("  </layout>\n</pinaxcv-style>\n");
    out
}

fn invalid(message: &str) -> ApiError {
    ApiError::new(ErrorCode::UnsupportedFormat, message)
}

fn not_a_style() -> ApiError {
    invalid("This is not a PinaxCV style file.")
}

/// Reads a style file. Unknown elements and attributes are ignored so later
/// additions to version 1 still load; values out of range are clamped and
/// unknown values replaced by defaults, each with a warning.
pub fn style_from_xml(bytes: &[u8]) -> Result<(Style, Vec<String>)> {
    if bytes.len() > MAX_STYLE_FILE {
        return Err(too_large());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| not_a_style())?;
    // roxmltree refuses DTDs by default, so entities cannot expand.
    let doc = roxmltree::Document::parse(text).map_err(|_| not_a_style())?;
    let root = doc.root_element();
    if root.tag_name().name() != "pinaxcv-style"
        || root.tag_name().namespace() != Some(STYLE_NAMESPACE)
    {
        return Err(not_a_style());
    }
    match root.attribute("version").map(|v| v.trim().parse::<u32>()) {
        Some(Ok(v)) if v == STYLE_VERSION => {}
        Some(Ok(v)) if v > STYLE_VERSION => {
            return Err(invalid(
                "This style was saved by a newer version of PinaxCV. Update PinaxCV to load it.",
            ));
        }
        _ => return Err(not_a_style()),
    }

    let mut warnings = Vec::new();
    let child = |name: &str| {
        root.children().find(|n| {
            n.is_element()
                && n.tag_name().name() == name
                && n.tag_name().namespace() == Some(STYLE_NAMESPACE)
        })
    };
    let defaults = TemplateOptions::default();
    let template = child("template")
        .and_then(|n| n.attribute("id"))
        .map(|id| TemplateId(id.trim().to_owned()))
        .unwrap_or_default();
    let paper = match child("paper").and_then(|n| n.attribute("size")) {
        Some("a4") | None => Paper::A4,
        Some("letter") => Paper::Letter,
        Some(_) => {
            warnings.push("The paper size was not recognised; A4 is used.".to_owned());
            Paper::A4
        }
    };

    let options_node = child("options");
    let mut number = |name: &str, label: &str, (min, max): (f32, f32), default: f32| {
        let Some(raw) = options_node.and_then(|n| n.attribute(name)) else {
            return default;
        };
        match raw.trim().parse::<f32>() {
            Ok(v) if v.is_finite() => {
                let clamped = v.clamp(min, max);
                if clamped != v {
                    warnings.push(format!("The {label} was out of range and was adjusted."));
                }
                clamped
            }
            _ => {
                warnings.push(format!(
                    "The {label} was not a number; the default is used."
                ));
                default
            }
        }
    };
    let font_size_pt = number(
        "font-size-pt",
        "font size",
        TemplateOptions::FONT_SIZE_PT,
        defaults.font_size_pt,
    );
    let line_spacing = number(
        "line-spacing",
        "line spacing",
        TemplateOptions::LINE_SPACING,
        defaults.line_spacing,
    );
    let margin_mm = number(
        "margin-mm",
        "margin",
        TemplateOptions::MARGIN_MM,
        defaults.margin_mm,
    );
    let accent = match options_node.and_then(|n| n.attribute("accent")) {
        None => None,
        Some(a) if is_hex_colour(a.trim()) => Some(a.trim().to_owned()),
        Some(_) => {
            warnings.push("The accent colour was not like #1A2B3C; black is used.".to_owned());
            None
        }
    };
    let date_style = match options_node.and_then(|n| n.attribute("date-style")) {
        None | Some("month_name_year") => DateStyle::MonthNameYear,
        Some("numeric_month_year") => DateStyle::NumericMonthYear,
        Some("year_only") => DateStyle::YearOnly,
        Some(_) => {
            warnings.push("The date style was not recognised; the default is used.".to_owned());
            defaults.date_style
        }
    };

    let layout = child("layout");
    let columns = match layout.and_then(|n| n.attribute("columns")) {
        None | Some("one") => Columns::One,
        Some("two") => Columns::Two,
        Some(_) => {
            warnings
                .push("The number of columns was not recognised; one column is used.".to_owned());
            Columns::One
        }
    };
    let (min, max) = Layout::SIDEBAR_WIDTH_PCT;
    let sidebar_width_pct = match layout
        .and_then(|n| n.attribute("sidebar-width-pct"))
        .map(|w| w.trim().parse::<u32>())
    {
        None => Layout::default().sidebar_width_pct,
        Some(Ok(w)) => {
            let clamped = w.clamp(u32::from(min), u32::from(max));
            if clamped != w {
                warnings.push("The sidebar width was out of range and was adjusted.".to_owned());
            }
            u8::try_from(clamped).unwrap_or(max)
        }
        Some(Err(_)) => {
            warnings.push("The sidebar width was not a number; the default is used.".to_owned());
            Layout::default().sidebar_width_pct
        }
    };

    let mut blocks = Vec::new();
    let mut skipped = 0usize;
    for node in layout.into_iter().flat_map(|n| n.children()).filter(|n| {
        n.is_element()
            && n.tag_name().name() == "block"
            && n.tag_name().namespace() == Some(STYLE_NAMESPACE)
    }) {
        let column = match node.attribute("column") {
            Some("sidebar") => Column::Sidebar,
            _ => Column::Main,
        };
        let kind = node.attribute("kind").unwrap_or_default();
        let block = match kind {
            "name" => StyleBlock::Top {
                block: TopBlock::Name,
            },
            "contact" => StyleBlock::Top {
                block: TopBlock::Contact,
            },
            "summary" => StyleBlock::Top {
                block: TopBlock::Summary,
            },
            _ => match SectionKind::ALL.iter().find(|k| k.as_str() == kind) {
                Some(&section) => StyleBlock::Section {
                    section,
                    title: (section == SectionKind::Custom)
                        .then(|| node.attribute("title").map(|t| t.trim().to_owned()))
                        .flatten()
                        .filter(|t| !t.is_empty()),
                },
                None => {
                    skipped += 1;
                    continue;
                }
            },
        };
        if matches!(block, StyleBlock::Top { .. })
            && blocks.iter().any(|b: &PlacedBlock| b.block == block)
        {
            continue;
        }
        blocks.push(PlacedBlock { block, column });
    }
    if skipped > 0 {
        warnings.push(format!(
            "{skipped} block(s) of a kind this version does not know were skipped."
        ));
    }

    let style = Style {
        template,
        paper,
        options: TemplateOptions {
            font_size_pt,
            line_spacing,
            margin_mm,
            accent,
            date_style,
        },
        columns,
        sidebar_width_pct,
        blocks,
    };
    Ok((style, warnings))
}

fn is_hex_colour(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

fn too_large() -> ApiError {
    ApiError::new(
        ErrorCode::Validation,
        "That file is larger than 256 KB, so it is not a style file.",
    )
}

/// `<title>.pinaxstyle.xml`, with only letters and digits from the title.
fn suggested_style_name(title: &str) -> String {
    let mut stem = String::new();
    for c in title.chars() {
        if c.is_alphanumeric() {
            stem.push(c);
        } else if !stem.is_empty() && !stem.ends_with('-') {
            stem.push('-');
        }
    }
    let stem = stem.trim_end_matches('-');
    let stem = if stem.is_empty() { "style" } else { stem };
    format!("{stem}{STYLE_SUFFIX}")
}

impl App {
    /// Asks where to save and writes `req.style` as a style file.
    pub fn style_save(&self, req: StyleSave) -> Result<StyleSaved> {
        let xml = style_to_xml(&req.style);
        let path = self.save_file(
            &suggested_style_name(&req.name),
            ExportKind::Style,
            xml.as_bytes(),
        )?;
        Ok(StyleSaved {
            path: path.display().to_string(),
        })
    }

    /// Asks for a style file and reads it. The UI applies it to the CV as
    /// one edit; an unknown template is reported, and the UI keeps the CV's.
    pub fn style_open(&self) -> Result<StyleOpened> {
        let path = self
            .dialog
            .choose_open_path(STYLE_EXTENSIONS)
            .ok_or_else(|| ApiError::new(ErrorCode::Cancelled, "No style chosen."))?;
        // The io::Error text can carry the path, which holds the user's name.
        let unreadable = |_| {
            ApiError::new(
                ErrorCode::Io,
                "Could not read the file. Check that it still exists and you can open it.",
            )
        };
        let mut bytes = Vec::new();
        File::open(&path)
            .map_err(unreadable)?
            .take(MAX_STYLE_FILE as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(unreadable)?;
        let (style, mut warnings) = style_from_xml(&bytes)?;
        let known = templates()?.iter().any(|m| m.id == style.template);
        if !known {
            warnings.insert(
                0,
                "The style's template is not in this version of PinaxCV; the CV keeps its template."
                    .to_owned(),
            );
        }
        Ok(StyleOpened {
            style,
            warnings,
            keep_template: !known,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Style {
        Style {
            template: TemplateId::default(),
            paper: Paper::Letter,
            options: TemplateOptions {
                accent: Some("#2B5CAB".into()),
                ..TemplateOptions::default()
            },
            columns: Columns::Two,
            sidebar_width_pct: 30,
            blocks: vec![
                PlacedBlock {
                    block: StyleBlock::Top {
                        block: TopBlock::Name,
                    },
                    column: Column::Sidebar,
                },
                PlacedBlock {
                    block: StyleBlock::Top {
                        block: TopBlock::Contact,
                    },
                    column: Column::Sidebar,
                },
                PlacedBlock {
                    block: StyleBlock::Top {
                        block: TopBlock::Summary,
                    },
                    column: Column::Main,
                },
                PlacedBlock {
                    block: StyleBlock::Section {
                        section: SectionKind::Experience,
                        title: None,
                    },
                    column: Column::Main,
                },
                PlacedBlock {
                    block: StyleBlock::Section {
                        section: SectionKind::Custom,
                        title: Some("Talks & \"panels\" <2026>".into()),
                    },
                    column: Column::Sidebar,
                },
            ],
        }
    }

    fn wrap(body: &str) -> String {
        format!("<pinaxcv-style xmlns=\"{STYLE_NAMESPACE}\" version=\"1\">{body}</pinaxcv-style>")
    }

    #[test]
    fn a_style_round_trips_through_xml() {
        let style = sample();
        let xml = style_to_xml(&style);
        let (back, warnings) = style_from_xml(xml.as_bytes()).expect("parses");
        assert_eq!(back, style);
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn the_written_file_has_the_documented_shape() {
        let xml = style_to_xml(&sample());
        assert!(xml.contains("<pinaxcv-style xmlns=\"urn:pinaxcv:style:1\" version=\"1\">"));
        assert!(xml.contains("<layout columns=\"two\" sidebar-width-pct=\"30\">"));
        assert!(xml.contains("<block kind=\"name\" column=\"sidebar\"/>"));
        assert!(xml.contains(
            "<block kind=\"custom\" title=\"Talks &amp; &quot;panels&quot; &lt;2026&gt;\" column=\"sidebar\"/>"
        ));
    }

    #[test]
    fn other_files_are_refused() {
        for xml in [
            "not xml",
            "<pinaxcv-style version=\"1\"/>",
            "<other xmlns=\"urn:pinaxcv:style:1\" version=\"1\"/>",
            "<pinaxcv-style xmlns=\"urn:pinaxcv:style:1\"/>",
            "<pinaxcv-style xmlns=\"urn:pinaxcv:style:1\" version=\"x\"/>",
        ] {
            let e = style_from_xml(xml.as_bytes()).expect_err(xml);
            assert_eq!(e.code, ErrorCode::UnsupportedFormat, "{xml}");
        }
        assert!(style_from_xml(&[0xff, 0xfe]).is_err());
    }

    #[test]
    fn newer_versions_are_refused_clearly() {
        let xml = "<pinaxcv-style xmlns=\"urn:pinaxcv:style:1\" version=\"2\"/>";
        let e = style_from_xml(xml.as_bytes()).expect_err("newer");
        assert!(e.message.contains("newer version"), "{}", e.message);
    }

    #[test]
    fn dtds_are_refused() {
        let xml = format!(
            "<!DOCTYPE pinaxcv-style [<!ENTITY a \"aaaa\">]>{}",
            wrap("<template id=\"&a;\"/>")
        );
        assert!(style_from_xml(xml.as_bytes()).is_err());
    }

    #[test]
    fn oversized_files_are_refused() {
        let xml = wrap(&" ".repeat(MAX_STYLE_FILE));
        let e = style_from_xml(xml.as_bytes()).expect_err("too large");
        assert_eq!(e.code, ErrorCode::Validation);
    }

    #[test]
    fn bad_values_are_clamped_or_defaulted_with_warnings() {
        let xml = wrap(
            "<paper size=\"a3\"/>\
             <options font-size-pt=\"40\" line-spacing=\"wide\" accent=\"blue\" date-style=\"iso\"/>\
             <layout columns=\"three\" sidebar-width-pct=\"90\">\
               <block kind=\"name\" column=\"sidebar\"/>\
               <block kind=\"name\" column=\"main\"/>\
               <block kind=\"hobbies\"/>\
               <block kind=\"skills\" title=\"ignored\" column=\"nowhere\"/>\
             </layout><extra/>",
        );
        let (style, warnings) = style_from_xml(xml.as_bytes()).expect("parses");
        assert_eq!(style.paper, Paper::A4);
        assert_eq!(style.options.font_size_pt, TemplateOptions::FONT_SIZE_PT.1);
        assert_eq!(
            style.options.line_spacing,
            TemplateOptions::default().line_spacing
        );
        assert_eq!(style.options.accent, None);
        assert_eq!(style.columns, Columns::One);
        assert_eq!(style.sidebar_width_pct, Layout::SIDEBAR_WIDTH_PCT.1);
        assert_eq!(
            style.blocks,
            [
                PlacedBlock {
                    block: StyleBlock::Top {
                        block: TopBlock::Name
                    },
                    column: Column::Sidebar,
                },
                PlacedBlock {
                    block: StyleBlock::Section {
                        section: SectionKind::Skills,
                        title: None,
                    },
                    column: Column::Main,
                },
            ]
        );
        assert_eq!(warnings.len(), 8, "{warnings:?}");
    }

    #[test]
    fn missing_parts_use_defaults() {
        let (style, warnings) = style_from_xml(wrap("").as_bytes()).expect("parses");
        assert!(warnings.is_empty());
        assert_eq!(style.template, TemplateId::default());
        assert_eq!(style.options, TemplateOptions::default());
        assert_eq!(style.columns, Columns::One);
        assert!(style.blocks.is_empty());
    }

    #[test]
    fn the_documented_example_loads() {
        // Windows checkouts may turn the newlines into CRLF.
        let doc = include_str!("../../../docs/IMPORT_EXPORT.md").replace("\r\n", "\n");
        let xml = doc
            .split("```xml\n")
            .nth(1)
            .and_then(|rest| rest.split("```").next())
            .expect("example in IMPORT_EXPORT.md");
        let (style, warnings) = style_from_xml(xml.as_bytes()).expect("parses");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(style.columns, Columns::Two);
        assert_eq!(style.blocks.len(), 7);
    }

    #[test]
    fn file_names_are_portable() {
        assert_eq!(
            suggested_style_name("Acme / Senior"),
            "Acme-Senior.pinaxstyle.xml"
        );
        assert_eq!(suggested_style_name("../"), "style.pinaxstyle.xml");
    }
}
