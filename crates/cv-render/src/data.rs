//! The JSON contract between `ResolvedCv` and the Typst template (spec section 6.3).
//!
//! User text reaches Typst only as data in `data.json`, never as markup.
//! Absent or blank optional fields are `null`, so the template never prints
//! stray separators. Dates and headings are formatted here, not in Typst.

use cv_core::lint::is_clickable_url;
use cv_core::locale::{self, Labels};
use cv_core::{
    BlockRef, ClosingPart, Column, Columns, Locale, Paper, ResolvedCv, ResolvedEntry,
    ResolvedSection, TopBlock,
};
use serde::Serialize;

use crate::ExportOptions;

#[derive(Serialize, Debug, PartialEq)]
pub struct TypstData {
    pub meta: MetaData,
    pub locale: &'static str,
    pub labels: LabelsData,
    pub paper: &'static str,
    pub opts: OptsData,
    pub basics: BasicsData,
    pub summary: Option<String>,
    pub sections: Vec<SectionData>,
    /// The closing block after the last section (ADR-089), or `null` when
    /// it prints nothing.
    pub closing: Option<ClosingData>,
    pub layout: LayoutData,
}

/// The shown parts of the closing block, as printed.
#[derive(Serialize, Debug, PartialEq)]
pub struct ClosingData {
    pub consent: Option<String>,
    /// "Berlin, 30 Oct 2026": the place and the export date in the CV's
    /// date style, or whichever of the two is known.
    pub place_date: Option<String>,
}

/// Which blocks each column draws, in order (ADR-084). With one column every
/// block is in `main`. Sections are indices into `TypstData::sections`.
#[derive(Serialize, Debug, PartialEq)]
pub struct LayoutData {
    pub two: bool,
    pub sidebar_width_pct: u8,
    /// Whether the sidebar is drawn on the left and read first: it holds
    /// the name.
    pub sidebar_first: bool,
    pub sidebar: Vec<BlockData>,
    pub main: Vec<BlockData>,
}

impl LayoutData {
    /// The columns in reading order: the left one (holding the name), then
    /// the right one. With one column the second is empty.
    pub fn reading_columns(&self) -> [&[BlockData]; 2] {
        if self.sidebar_first {
            [&self.sidebar, &self.main]
        } else {
            [&self.main, &self.sidebar]
        }
    }
}

#[derive(Serialize, Debug, PartialEq, Clone, Copy)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BlockData {
    Name,
    Contact,
    Summary,
    Section { index: usize },
}

/// PDF metadata: title "<Full name>, CV" and author = full name.
#[derive(Serialize, Debug, PartialEq)]
pub struct MetaData {
    pub title: String,
    pub author: String,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct LabelsData {
    pub experience: &'static str,
    pub education: &'static str,
    pub skills: &'static str,
    pub languages: &'static str,
    pub certifications: &'static str,
    pub projects: &'static str,
    pub summary: &'static str,
    pub present: &'static str,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct OptsData {
    pub font_size_pt: f32,
    pub line_spacing: f32,
    pub margin_mm: f32,
    pub accent: Option<String>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct BasicsData {
    pub full_name: String,
    pub headline: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub location: Option<String>,
    pub links: Vec<LinkData>,
    /// Whether the template draws `photo.jpg`: the CV shows its photo and
    /// the export was given one.
    pub photo: bool,
    /// Personal details switched on for this CV or variant (ADR-088), in
    /// print order, one line each after the contact details.
    pub personal: Vec<PersonalLineData>,
}

/// One personal detail as printed: "Nationality: Italian".
#[derive(Serialize, Debug, PartialEq)]
pub struct PersonalLineData {
    pub label: &'static str,
    pub value: String,
}

/// `clickable` is true only for `https:` and `mailto:` URLs.
#[derive(Serialize, Debug, PartialEq)]
pub struct LinkData {
    pub label: Option<String>,
    pub url: String,
    pub clickable: bool,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SectionData {
    Experience(EntriesData),
    Education(EntriesData),
    Projects(EntriesData),
    Certifications(EntriesData),
    Custom(EntriesData),
    Skills {
        heading: String,
        groups: Vec<GroupData>,
    },
    Languages {
        heading: String,
        items: Vec<LanguageData>,
    },
}

#[derive(Serialize, Debug, PartialEq)]
pub struct EntriesData {
    pub heading: String,
    pub entries: Vec<EntryData>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct EntryData {
    pub title: Option<String>,
    pub org: Option<String>,
    pub location: Option<String>,
    pub period: Option<String>,
    pub description: Option<String>,
    pub link: Option<LinkData>,
    pub bullets: Vec<String>,
    pub collapsed: bool,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct GroupData {
    pub label: Option<String>,
    pub skills: Vec<String>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct LanguageData {
    pub name: String,
    pub level: Option<String>,
}

/// Builds the template data for `cv`. `options` decides whether email and
/// phone are printed.
pub fn to_typst_data(cv: &ResolvedCv, options: &ExportOptions) -> TypstData {
    let labels = locale::labels(cv.locale);
    let b = &cv.basics;
    let sections: Vec<(Column, SectionData)> = cv
        .sections
        .iter()
        .filter_map(|s| {
            let column = cv
                .layout
                .column_of(&BlockRef::Section { id: s.id().clone() });
            section(s, cv).map(|data| (column, data))
        })
        .collect();
    let columns: Vec<Column> = sections.iter().map(|(c, _)| *c).collect();
    let layout = layout_data(cv, &columns);
    let name = b.full_name.trim();
    TypstData {
        meta: MetaData {
            title: if name.is_empty() {
                "CV".to_owned()
            } else {
                format!("{name}, CV")
            },
            author: name.to_owned(),
        },
        locale: locale_code(cv.locale),
        labels: labels_data(labels),
        paper: match cv.paper {
            Paper::A4 => "a4",
            Paper::Letter => "us-letter",
        },
        opts: OptsData {
            font_size_pt: cv.options.font_size_pt,
            line_spacing: cv.options.line_spacing,
            margin_mm: cv.options.margin_mm,
            accent: text(&cv.options.accent),
        },
        basics: BasicsData {
            full_name: b.full_name.trim().to_owned(),
            headline: text(&b.headline),
            email: text(&b.email).filter(|_| options.include_email),
            phone: text(&b.phone).filter(|_| options.include_phone),
            location: text(&b.location),
            links: b
                .links
                .iter()
                .filter_map(|l| link(Some(&l.label), &l.url))
                .collect(),
            photo: cv.show_photo && options.photo.is_some(),
            personal: b
                .personal
                .lines(cv.options.date_style, cv.locale)
                .into_iter()
                .map(|(field, value)| PersonalLineData {
                    label: locale::personal_label(field, cv.locale),
                    value,
                })
                .collect(),
        },
        summary: text(&cv.summary),
        sections: sections.into_iter().map(|(_, s)| s).collect(),
        layout,
        closing: closing(cv, options),
    }
}

fn closing(cv: &ResolvedCv, options: &ExportOptions) -> Option<ClosingData> {
    let c = &cv.closing;
    let place_date = c.show.contains(&ClosingPart::PlaceDate).then(|| {
        let date = options
            .date
            .map(|d| locale::format_day(d, cv.options.date_style, cv.locale));
        [c.place_text().map(str::to_owned), date]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", ")
    });
    let data = ClosingData {
        consent: c.consent_text().map(str::to_owned),
        place_date: place_date.filter(|l| !l.is_empty()),
    };
    (data.consent.is_some() || data.place_date.is_some()).then_some(data)
}

/// Puts the top blocks and the printed sections into their columns.
/// `sections` pairs each printed section with its column.
fn layout_data(cv: &ResolvedCv, columns: &[Column]) -> LayoutData {
    let l = &cv.layout;
    let mut data = LayoutData {
        two: l.columns == Columns::Two,
        sidebar_width_pct: l.sidebar_width_pct,
        sidebar_first: l.first_column() == Column::Sidebar,
        sidebar: Vec::new(),
        main: Vec::new(),
    };
    for column in [Column::Sidebar, Column::Main] {
        let blocks = match column {
            Column::Sidebar => &mut data.sidebar,
            Column::Main => &mut data.main,
        };
        blocks.extend(l.top_in(column).into_iter().map(|b| match b {
            TopBlock::Name => BlockData::Name,
            TopBlock::Contact => BlockData::Contact,
            TopBlock::Summary => BlockData::Summary,
        }));
        blocks.extend(
            columns
                .iter()
                .enumerate()
                .filter(|(_, c)| **c == column)
                .map(|(index, _)| BlockData::Section { index }),
        );
    }
    data
}

/// The Typst `lang` code for `locale`.
pub fn locale_code(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "en",
    }
}

fn labels_data(l: Labels) -> LabelsData {
    LabelsData {
        experience: l.experience,
        education: l.education,
        skills: l.skills,
        languages: l.languages,
        certifications: l.certifications,
        projects: l.projects,
        summary: l.summary,
        present: l.present,
    }
}

/// Trims `s`; blank becomes `None`.
fn text(s: &Option<String>) -> Option<String> {
    s.as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
}

fn link(label: Option<&String>, url: &str) -> Option<LinkData> {
    let url = url.trim();
    (!url.is_empty()).then(|| LinkData {
        label: text(&label.cloned()),
        url: url.to_owned(),
        clickable: is_clickable_url(url),
    })
}

fn section(s: &ResolvedSection, cv: &ResolvedCv) -> Option<SectionData> {
    let heading = locale::heading(s, cv.locale);
    let entries = |entries: &[ResolvedEntry]| EntriesData {
        heading: heading.clone(),
        entries: entries.iter().map(|e| entry(e, cv)).collect(),
    };
    let data = match s {
        ResolvedSection::Experience(x) => SectionData::Experience(entries(&x.entries)),
        ResolvedSection::Education(x) => SectionData::Education(entries(&x.entries)),
        ResolvedSection::Projects(x) => SectionData::Projects(entries(&x.entries)),
        ResolvedSection::Certifications(x) => SectionData::Certifications(entries(&x.entries)),
        ResolvedSection::Custom(x) => SectionData::Custom(entries(&x.entries)),
        ResolvedSection::Skills(x) => SectionData::Skills {
            heading: heading.clone(),
            groups: x
                .groups
                .iter()
                .map(|g| GroupData {
                    label: text(&g.label),
                    skills: g
                        .skills
                        .iter()
                        .filter_map(|k| text(&Some(k.name.clone())))
                        .collect(),
                })
                .filter(|g| !g.skills.is_empty())
                .collect(),
        },
        ResolvedSection::Languages(x) => SectionData::Languages {
            heading: heading.clone(),
            items: x
                .items
                .iter()
                .filter_map(|i| {
                    text(&Some(i.name.clone())).map(|name| LanguageData {
                        name,
                        level: text(&i.level),
                    })
                })
                .collect(),
        },
    };
    let empty = match &data {
        SectionData::Experience(e)
        | SectionData::Education(e)
        | SectionData::Projects(e)
        | SectionData::Certifications(e)
        | SectionData::Custom(e) => e.entries.is_empty(),
        SectionData::Skills { groups, .. } => groups.is_empty(),
        SectionData::Languages { items, .. } => items.is_empty(),
    };
    (!empty).then_some(data)
}

fn entry(e: &ResolvedEntry, cv: &ResolvedCv) -> EntryData {
    EntryData {
        title: text(&Some(e.title.clone())),
        org: text(&Some(e.org.clone())),
        location: text(&e.location),
        period: locale::format_period(&e.period, cv.options.date_style, cv.locale),
        description: if e.collapsed {
            None
        } else {
            text(&e.description)
        },
        link: if e.collapsed {
            None
        } else {
            e.url.as_deref().and_then(|u| link(None, u))
        },
        bullets: if e.collapsed {
            Vec::new()
        } else {
            e.bullets
                .iter()
                .filter_map(|b| text(&Some(b.text.clone())))
                .collect()
        },
        collapsed: e.collapsed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdata;

    fn data(name: &str) -> serde_json::Value {
        let cv = testdata::fixture(name).resolve(None);
        serde_json::to_value(to_typst_data(&cv, &ExportOptions::default())).expect("json")
    }

    #[test]
    fn one_column_puts_every_block_in_main() {
        let cv = testdata::fixture("typical").resolve(None);
        let d = to_typst_data(&cv, &ExportOptions::default());
        assert!(!d.layout.two);
        assert!(d.layout.sidebar.is_empty());
        assert_eq!(
            d.layout.main,
            [
                BlockData::Name,
                BlockData::Contact,
                BlockData::Summary,
                BlockData::Section { index: 0 },
                BlockData::Section { index: 1 },
                BlockData::Section { index: 2 },
                BlockData::Section { index: 3 },
            ]
        );
        let [first, second] = d.layout.reading_columns();
        assert_eq!(first.len(), 7);
        assert!(second.is_empty());
    }

    #[test]
    fn two_columns_split_blocks_and_read_the_name_column_first() {
        let cv = testdata::fixture("two_column").resolve(None);
        let d = to_typst_data(&cv, &ExportOptions::default());
        assert!(d.layout.two && d.layout.sidebar_first);
        assert_eq!(d.layout.sidebar_width_pct, 32);
        // Skills and languages (sections 2 and 3) are in the sidebar.
        assert_eq!(
            d.layout.sidebar,
            [
                BlockData::Name,
                BlockData::Contact,
                BlockData::Section { index: 2 },
                BlockData::Section { index: 3 },
            ]
        );
        assert_eq!(
            d.layout.main,
            [
                BlockData::Summary,
                BlockData::Section { index: 0 },
                BlockData::Section { index: 1 },
            ]
        );
        assert_eq!(d.layout.reading_columns()[0], d.layout.sidebar.as_slice());

        let cv = testdata::fixture("two_column_long").resolve(None);
        let d = to_typst_data(&cv, &ExportOptions::default());
        assert!(!d.layout.sidebar_first, "the name stays in main");
        assert_eq!(d.layout.reading_columns()[0], d.layout.main.as_slice());
        let v = serde_json::to_value(&d.layout).expect("json");
        assert_eq!(v["main"][0]["kind"], "name");
        assert_eq!(v["sidebar"][0]["kind"], "section");
        assert!(v["sidebar"][0]["index"].is_u64());
    }

    #[test]
    fn typical_matches_contract_shape() {
        let d = data("typical");
        assert_eq!(d["locale"], "en");
        assert_eq!(d["paper"], "a4");
        assert_eq!(d["labels"]["present"], "Present");
        assert_eq!(d["basics"]["full_name"], "Jane Doe");
        assert_eq!(d["meta"]["title"], "Jane Doe, CV");
        assert_eq!(d["meta"]["author"], "Jane Doe");
        let exp = &d["sections"][0];
        assert_eq!(exp["kind"], "experience");
        assert_eq!(exp["heading"], "Work Experience");
        assert_eq!(exp["entries"][0]["period"], "Mar 2021 – Present");
        assert!(
            exp["entries"][0]["bullets"]
                .as_array()
                .is_some_and(|b| !b.is_empty())
        );
        let kinds: Vec<_> = d["sections"]
            .as_array()
            .expect("sections")
            .iter()
            .map(|s| s["kind"].as_str().expect("kind").to_owned())
            .collect();
        assert_eq!(kinds, ["experience", "education", "skills", "languages"]);
    }

    #[test]
    fn blank_optionals_are_null_never_empty_strings() {
        let d = data("minimal");
        for key in ["headline", "phone", "location"] {
            assert!(d["basics"][key].is_null(), "{key}");
        }
        assert!(d["summary"].is_null());
        assert_eq!(d["sections"], serde_json::json!([]));
    }

    #[test]
    fn empty_sections_are_dropped() {
        let d = data("empty_sections");
        assert!(d["sections"].as_array().is_some_and(|s| s.len() == 1));
    }

    #[test]
    fn links_are_clickable_only_for_https_and_mailto() {
        let d = data("unsafe_links");
        let clickable: Vec<(String, bool)> = d["basics"]["links"]
            .as_array()
            .expect("links")
            .iter()
            .map(|l| {
                (
                    l["url"].as_str().expect("url").to_owned(),
                    l["clickable"] == true,
                )
            })
            .collect();
        assert!(clickable.iter().any(|(_, c)| *c), "{clickable:?}");
        for (url, c) in &clickable {
            assert_eq!(
                *c,
                url.starts_with("https:") || url.starts_with("mailto:"),
                "{url}"
            );
        }
    }

    #[test]
    fn contact_options_hide_email_and_phone() {
        let cv = testdata::fixture("typical").resolve(None);
        let opts = ExportOptions {
            include_email: false,
            include_phone: false,
            photo: None,
            date: None,
        };
        let d = to_typst_data(&cv, &opts);
        assert_eq!((d.basics.email, d.basics.phone), (None, None));
    }

    #[test]
    fn personal_details_follow_the_switches() {
        let d = data("european");
        let lines: Vec<String> = d["basics"]["personal"]
            .as_array()
            .expect("personal")
            .iter()
            .map(|l| {
                format!(
                    "{}: {}",
                    l["label"].as_str().expect("label"),
                    l["value"].as_str().expect("value")
                )
            })
            .collect();
        assert_eq!(
            lines,
            [
                "Nationality: Italian, German",
                "Date of birth: 12 Mar 1990",
                "Place of birth: Turin",
                "Gender: Female",
                "Marital status: Married",
                "Address: Via Esempio 1, 10100 Torino",
                "Driving licence: B, C1",
                "Right to work: EU citizen",
            ]
        );
        // A variant that switches nothing on prints none of them.
        let cv = testdata::fixture("european");
        let v = cv_core::Variant::empty(cv.id.clone(), cv.id.clone(), "UK");
        let d = to_typst_data(&cv.resolve(Some(&v)), &ExportOptions::default());
        assert!(d.basics.personal.is_empty());
        assert!(
            data("typical")["basics"]["personal"]
                .as_array()
                .is_some_and(Vec::is_empty)
        );
    }

    #[test]
    fn closing_prints_the_shown_parts_with_the_export_date() {
        use cv_core::{CalendarDate, ClosingPart};
        let mut cv = testdata::fixture("typical");
        assert_eq!(data("typical")["closing"], serde_json::Value::Null);
        cv.closing.consent = Some(" I consent. ".into());
        cv.closing.place = Some("Berlin".into());
        cv.closing.show = [ClosingPart::Consent, ClosingPart::PlaceDate].into();
        let mut opts = ExportOptions {
            date: CalendarDate::parse("2026-10-30"),
            ..ExportOptions::default()
        };
        let d = to_typst_data(&cv.resolve(None), &opts)
            .closing
            .expect("closing");
        assert_eq!(d.consent.as_deref(), Some("I consent."));
        assert_eq!(d.place_date.as_deref(), Some("Berlin, 30 Oct 2026"));

        cv.options.date_style = cv_core::DateStyle::NumericMonthYear;
        cv.closing.place = None;
        let d = to_typst_data(&cv.resolve(None), &opts)
            .closing
            .expect("closing");
        assert_eq!(d.place_date.as_deref(), Some("30/10/2026"));

        // Without a place or a date, the line prints nothing.
        opts.date = None;
        cv.closing.show = [ClosingPart::PlaceDate].into();
        assert_eq!(to_typst_data(&cv.resolve(None), &opts).closing, None);
    }

    #[test]
    fn blank_accent_is_null() {
        let mut cv = testdata::fixture("minimal");
        cv.options.accent = Some("  ".to_owned());
        let d = serde_json::to_value(to_typst_data(&cv.resolve(None), &ExportOptions::default()))
            .expect("json");
        assert!(d["options"]["accent"].is_null());
    }

    #[test]
    fn letter_paper_maps_to_typst_name() {
        let mut cv = testdata::fixture("minimal");
        cv.paper = Paper::Letter;
        assert_eq!(
            to_typst_data(&cv.resolve(None), &ExportOptions::default()).paper,
            "us-letter"
        );
        assert_eq!(locale_code(Locale::En), "en");
    }

    #[test]
    fn collapsed_entries_drop_body() {
        let cv = testdata::fixture("typical");
        let mut v = cv_core::Variant::empty(cv.id.clone(), cv.id.clone(), "V");
        let first_entry = cv.resolve(None);
        let ResolvedSection::Experience(exp) = &first_entry.sections[0] else {
            panic!("experience")
        };
        v.collapsed.insert(exp.entries[0].id.clone());
        let d = to_typst_data(&cv.resolve(Some(&v)), &ExportOptions::default());
        let SectionData::Experience(e) = &d.sections[0] else {
            panic!("experience")
        };
        assert!(e.entries[0].collapsed && e.entries[0].bullets.is_empty());
        assert!(e.entries[0].title.is_some() && e.entries[0].period.is_some());
    }
}
