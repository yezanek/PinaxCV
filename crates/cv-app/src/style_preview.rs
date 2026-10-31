//! Style previews (ADR-084): the HTML exporter run on a built-in sample CV
//! with a style applied, so a style can be looked at or shared without any
//! of the user's content. Export only; the XML style file is what loads back.

use cv_api::{ApiError, ErrorCode, StyleSave, StyleSaved};
use cv_core::{
    Basics, Bullet, CV_SCHEMA_VERSION, CustomSection, Cv, Entry, EntrySection, Id, LanguageItem,
    LanguagesSection, Link, Locale, PartialDate, Period, Section, SectionKind, Skill, SkillGroup,
    SkillsSection, Style, StyleBlock, TemplateId,
};
use cv_render::{ExportOptions, Exporter, HtmlExporter};

use crate::{App, ExportKind, Result, render_error, templates};

/// Fixed IDs for the sample: ULIDs whose last digits are `n`.
fn id(n: u32) -> Id {
    // A valid ULID for every n below 10^7, so this cannot fail.
    Id::parse(&format!("01JB5TY1E0SAMP1ECV{n:08}")).unwrap_or_else(|_| unreachable!())
}

fn month(year: u16, month: u8) -> Option<PartialDate> {
    Some(PartialDate {
        year,
        month: Some(month),
    })
}

fn entry(n: u32, title: &str, org: &str, period: Period, bullets: &[&str]) -> Entry {
    Entry {
        id: id(n),
        title: title.into(),
        org: org.into(),
        location: Some("Springfield".into()),
        period,
        bullets: bullets
            .iter()
            .enumerate()
            .map(|(i, text)| Bullet {
                id: id(n * 10 + u32::try_from(i).unwrap_or(0)),
                text: (*text).into(),
            })
            .collect(),
        description: None,
        url: None,
        locked: false,
    }
}

/// The synthetic sample CV: every built-in section kind, no real person.
pub fn sample_cv() -> Cv {
    let span = |start, end| Period {
        start,
        end,
        current: false,
    };
    Cv {
        schema_version: CV_SCHEMA_VERSION,
        id: id(1),
        title: "Style preview".into(),
        locale: Locale::En,
        paper: Default::default(),
        template: TemplateId::default(),
        options: Default::default(),
        basics: Basics {
            full_name: "Alex Example".into(),
            headline: Some("Product Engineer".into()),
            email: Some("alex@example.com".into()),
            phone: Some("+1 555 0100".into()),
            location: Some("Springfield".into()),
            links: vec![Link {
                id: id(2),
                label: "example.com".into(),
                url: "https://example.com".into(),
            }],
            show_photo: false,
            personal: Default::default(),
            show_personal: Default::default(),
        },
        summary: Some(
            "Engineer who turns rough ideas into dependable products. Sample text for previewing a style."
                .into(),
        ),
        sections: vec![
            Section::Experience(EntrySection {
                id: id(10),
                title_override: None,
                entries: vec![
                    entry(
                        11,
                        "Senior Engineer",
                        "Example Labs",
                        Period {
                            start: month(2021, 3),
                            end: None,
                            current: true,
                        },
                        &[
                            "Led a team of four building the sample product.",
                            "Cut page load time by half with careful caching.",
                        ],
                    ),
                    entry(
                        12,
                        "Engineer",
                        "Placeholder Inc.",
                        span(month(2017, 9), month(2021, 2)),
                        &["Built and ran the internal tools platform."],
                    ),
                ],
            }),
            Section::Projects(EntrySection {
                id: id(20),
                title_override: None,
                entries: vec![entry(
                    21,
                    "Sample Project",
                    "Open source",
                    span(month(2022, 1), month(2023, 6)),
                    &["A small library used here as an example."],
                )],
            }),
            Section::Education(EntrySection {
                id: id(30),
                title_override: None,
                entries: vec![entry(
                    31,
                    "BSc Computer Science",
                    "Example University",
                    span(month(2013, 9), month(2017, 6)),
                    &[],
                )],
            }),
            Section::Certifications(EntrySection {
                id: id(40),
                title_override: None,
                entries: vec![entry(
                    41,
                    "Sample Certificate",
                    "Example Institute",
                    span(month(2020, 5), None),
                    &[],
                )],
            }),
            Section::Skills(SkillsSection {
                id: id(50),
                title_override: None,
                groups: vec![SkillGroup {
                    id: id(51),
                    label: Some("Tools".into()),
                    skills: ["Rust", "TypeScript", "SQL"]
                        .iter()
                        .enumerate()
                        .map(|(i, name)| Skill {
                            id: id(52 + u32::try_from(i).unwrap_or(0)),
                            name: (*name).into(),
                        })
                        .collect(),
                }],
            }),
            Section::Languages(LanguagesSection {
                id: id(60),
                title_override: None,
                items: vec![
                    LanguageItem {
                        id: id(61),
                        name: "English".into(),
                        level: Some("Native".into()),
                    },
                    LanguageItem {
                        id: id(62),
                        name: "Spanish".into(),
                        level: Some("B2".into()),
                    },
                ],
            }),
        ],
        layout: Default::default(),
        closing: Default::default(),
    }
}

/// The sample CV with `style` applied. Custom sections the style names get
/// a sample section with their title, so they show in the preview.
pub fn preview_cv(style: &Style) -> Cv {
    let mut cv = sample_cv();
    let mut n = 70;
    for b in &style.blocks {
        if let StyleBlock::Section {
            section: SectionKind::Custom,
            title,
        } = &b.block
        {
            cv.sections.push(Section::Custom(CustomSection {
                id: id(n),
                title: title.clone().unwrap_or_else(|| "Additional section".into()),
                entries: vec![entry(
                    n + 1,
                    "Sample entry",
                    "Example Organisation",
                    Period {
                        start: month(2019, 1),
                        end: None,
                        current: false,
                    },
                    &[],
                )],
            }));
            n += 2;
        }
    }
    cv.apply_style(style);
    cv
}

/// `<title>-style-preview.html`, with only letters and digits from the title.
fn suggested_preview_name(title: &str) -> String {
    let mut stem = String::new();
    for c in title.chars() {
        if c.is_alphanumeric() {
            stem.push(c);
        } else if !stem.is_empty() && !stem.ends_with('-') {
            stem.push('-');
        }
    }
    let stem = stem.trim_end_matches('-');
    if stem.is_empty() {
        "style-preview.html".to_owned()
    } else {
        format!("{stem}-style-preview.html")
    }
}

impl App {
    /// Asks where to save and writes an HTML page of the sample CV in
    /// `req.style`. Nothing of the open CV is used but its style and, for
    /// the suggested file name, its title.
    pub fn style_preview(&self, req: StyleSave) -> Result<StyleSaved> {
        let mut cv = preview_cv(&req.style);
        if !templates()?.iter().any(|m| m.id == cv.template) {
            cv.template = TemplateId::default();
        }
        let resolved = cv.resolve(None);
        let options = ExportOptions {
            include_email: true,
            include_phone: true,
            photo: None,
            signature: None,
            date: None,
        };
        let doc = HtmlExporter
            .export(&resolved, &options)
            .map_err(render_error)?;
        let path = self
            .save_file(
                &suggested_preview_name(&req.name),
                ExportKind::Html,
                &doc.bytes,
            )
            .map_err(|e| match e.code {
                ErrorCode::Cancelled => ApiError::new(ErrorCode::Cancelled, "Preview cancelled."),
                _ => e,
            })?;
        Ok(StyleSaved {
            path: path.display().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cv_core::{Column, Columns, PlacedBlock, TopBlock};

    #[test]
    fn the_sample_is_valid() {
        let mut cv = sample_cv();
        let issues = cv_core::validate::for_save(&mut cv);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn the_preview_uses_the_style_and_the_sample_text_only() {
        // A CV with its own text and a two-column style.
        let mut mine = sample_cv();
        mine.basics.full_name = "Morgan Mine".into();
        mine.summary = Some("My own words.".into());
        mine.sections = vec![Section::Custom(CustomSection {
            id: id(900),
            title: "Talks".into(),
            entries: vec![entry(
                901,
                "Talk at a private meetup",
                "Secret Org",
                Period {
                    start: None,
                    end: None,
                    current: false,
                },
                &[],
            )],
        })];
        mine.layout.columns = Columns::Two;
        mine.layout.sidebar = vec![cv_core::BlockRef::Top {
            block: TopBlock::Name,
        }];
        let style = Style::from_cv(&mine);

        let cv = preview_cv(&style);
        assert_eq!(cv.layout.columns, Columns::Two);
        assert_eq!(
            cv.layout.first_column(),
            Column::Sidebar,
            "the name sits in the sidebar"
        );
        let html = String::from_utf8(
            HtmlExporter
                .export(
                    &cv.resolve(None),
                    &ExportOptions {
                        include_email: true,
                        include_phone: true,
                        photo: None,
                        signature: None,
                        date: None,
                    },
                )
                .expect("export")
                .bytes,
        )
        .expect("utf-8");
        for sample in ["Alex Example", "Example Labs", "Talks", "Sample entry"] {
            assert!(html.contains(sample), "missing {sample}");
        }
        for own in [
            "Morgan Mine",
            "My own words",
            "private meetup",
            "Secret Org",
        ] {
            assert!(!html.contains(own), "leaked {own}");
        }
    }

    #[test]
    fn custom_blocks_without_a_title_get_a_placeholder() {
        let style = Style {
            blocks: vec![PlacedBlock {
                block: StyleBlock::Section {
                    section: SectionKind::Custom,
                    title: None,
                },
                column: Column::Main,
            }],
            ..Style::from_cv(&sample_cv())
        };
        let cv = preview_cv(&style);
        assert!(
            cv.sections
                .iter()
                .any(|s| matches!(s, Section::Custom(c) if c.title == "Additional section"))
        );
    }

    #[test]
    fn preview_file_names_are_portable() {
        assert_eq!(
            suggested_preview_name("Acme / Senior"),
            "Acme-Senior-style-preview.html"
        );
        assert_eq!(suggested_preview_name(""), "style-preview.html");
    }
}
