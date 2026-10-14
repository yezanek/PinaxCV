//! Turns Europass records into a [`CvPartial`] with fresh IDs.

use cv_core::{
    Bullet, Entry, EntrySection, Id, LanguageItem, LanguagesSection, Link, PartialDate, Period,
    Section,
};

use crate::europass_xml::{EuropassCv, EuropassLanguage};
use crate::partial::{CvPartial, ImportResult, ImportWarning};

/// Maps a parsed Europass CV. `fresh` makes a new ID for every item.
pub fn to_partial(cv: &EuropassCv, fresh: &mut dyn FnMut() -> Id) -> ImportResult {
    let mut warnings = Vec::new();
    let p = &cv.person;
    let name = format!("{} {}", p.given_name, p.family_name);
    let mut partial = CvPartial {
        full_name: Some(name.trim().to_owned()).filter(|n| !n.is_empty()),
        email: p.emails.first().cloned(),
        phone: p.phones.first().cloned(),
        location: match (&p.city, &p.country) {
            (Some(c), Some(k)) => Some(format!("{c}, {}", k.to_uppercase())),
            (c, k) => c.clone().or_else(|| k.as_ref().map(|k| k.to_uppercase())),
        },
        summary: p.summary.clone(),
        links: p
            .web
            .iter()
            .map(|url| Link {
                id: fresh(),
                label: url
                    .trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .trim_end_matches('/')
                    .to_owned(),
                url: url.clone(),
            })
            .collect(),
        ..CvPartial::default()
    };
    if p.emails.len() > 1 {
        warnings.push(ImportWarning::new(
            "More than one email address was found; the first one was used.",
        ));
    }

    let date = |value: &Option<String>, what: &str, warnings: &mut Vec<ImportWarning>| {
        let value = value.as_deref()?;
        let parsed = parse_date(value);
        if parsed.is_none() {
            warnings.push(ImportWarning::new(format!(
                "The date \"{value}\" of {what} could not be read and was left empty."
            )));
        }
        parsed
    };

    let mut experience = Vec::new();
    for job in &cv.positions {
        let what = format!("\"{}\"", job.title);
        experience.push(Entry {
            id: fresh(),
            title: job.title.clone(),
            org: job.organisation.clone(),
            location: job.city.clone(),
            period: Period {
                start: date(&job.start, &what, &mut warnings),
                end: date(&job.end, &what, &mut warnings),
                current: job.current,
            },
            bullets: job
                .bullets
                .iter()
                .map(|text| Bullet {
                    id: fresh(),
                    text: text.clone(),
                })
                .collect(),
            description: job.description.clone(),
            url: None,
            locked: false,
        });
    }
    if !experience.is_empty() {
        partial.sections.push(Section::Experience(EntrySection {
            id: fresh(),
            title_override: None,
            entries: experience,
        }));
    }

    let mut education = Vec::new();
    for e in &cv.education {
        let what = format!("\"{}\"", e.degree);
        education.push(Entry {
            id: fresh(),
            title: e.degree.clone(),
            org: e.organisation.clone(),
            location: e.city.clone(),
            period: Period {
                start: date(&e.start, &what, &mut warnings),
                end: date(&e.end, &what, &mut warnings),
                current: e.ongoing,
            },
            bullets: Vec::new(),
            description: None,
            url: None,
            locked: false,
        });
    }
    if !education.is_empty() {
        partial.sections.push(Section::Education(EntrySection {
            id: fresh(),
            title_override: None,
            entries: education,
        }));
    }

    if !cv.languages.is_empty() {
        let items = cv
            .languages
            .iter()
            .map(|l| LanguageItem {
                id: fresh(),
                name: language_name(&l.code),
                level: level(l),
            })
            .collect();
        partial.sections.push(Section::Languages(LanguagesSection {
            id: fresh(),
            title_override: None,
            items,
        }));
    }

    ImportResult {
        cv_partial: partial,
        warnings,
    }
}

/// `YYYY`, `YYYY-MM` or `YYYY-MM-DD`; the day is dropped.
fn parse_date(value: &str) -> Option<PartialDate> {
    let mut parts = value.trim().split('-');
    let year: u16 = parts.next()?.parse().ok()?;
    if !(1900..=2100).contains(&year) {
        return None;
    }
    let month = match parts.next() {
        None => None,
        Some(m) => {
            let m: u8 = m.parse().ok()?;
            (1..=12).contains(&m).then_some(m)?;
            Some(m)
        }
    };
    Some(PartialDate { year, month })
}

/// The most common CEFR score, or the highest on a tie.
fn level(l: &EuropassLanguage) -> Option<String> {
    let mut best: Option<(usize, &str)> = None;
    for (_, score) in &l.scores {
        let count = l.scores.iter().filter(|(_, s)| s == score).count();
        let better = match best {
            None => true,
            Some((n, s)) => count > n || (count == n && score.as_str() > s),
        };
        if better {
            best = Some((count, score.as_str()));
        }
    }
    best.map(|(_, s)| s.to_owned())
}

/// English names for common ISO 639 codes; other codes are kept as written.
fn language_name(code: &str) -> String {
    let name = match code.to_ascii_lowercase().as_str() {
        "en" | "eng" => "English",
        "de" | "deu" | "ger" => "German",
        "fr" | "fra" | "fre" => "French",
        "it" | "ita" => "Italian",
        "es" | "spa" => "Spanish",
        "pt" | "por" => "Portuguese",
        "nl" | "nld" | "dut" => "Dutch",
        "pl" | "pol" => "Polish",
        _ => return code.to_owned(),
    };
    name.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::europass_xml::{EuropassEducation, EuropassPerson, EuropassPosition};

    fn ids() -> impl FnMut() -> Id {
        let mut n = 0u8;
        move || {
            n += 1;
            Id::parse(&format!("01HZX3M8Q4V7K2N9P5R6T0W1{n:02}")).expect("id")
        }
    }

    fn sample() -> EuropassCv {
        EuropassCv {
            person: EuropassPerson {
                given_name: "Jane".into(),
                family_name: "Doe".into(),
                emails: vec!["jane@example.com".into(), "other@example.com".into()],
                phones: vec!["+49 301234567".into()],
                web: vec!["https://example.com/".into()],
                city: Some("Berlin".into()),
                country: Some("de".into()),
                summary: Some("Engineer.".into()),
            },
            positions: vec![EuropassPosition {
                organisation: "Acme GmbH".into(),
                title: "Senior Engineer".into(),
                start: Some("2021-03".into()),
                end: Some("2023-13".into()),
                bullets: vec!["Cut latency.".into()],
                ..EuropassPosition::default()
            }],
            education: vec![EuropassEducation {
                organisation: "TU Berlin".into(),
                degree: "MSc".into(),
                end: Some("2014-09-30".into()),
                ..EuropassEducation::default()
            }],
            languages: vec![EuropassLanguage {
                code: "eng".into(),
                scores: vec![
                    ("a".into(), "B2".into()),
                    ("b".into(), "C1".into()),
                    ("c".into(), "C1".into()),
                ],
            }],
        }
    }

    #[test]
    fn maps_person_sections_and_reports_guesses() {
        let r = to_partial(&sample(), &mut ids());
        let p = &r.cv_partial;
        assert_eq!(p.full_name.as_deref(), Some("Jane Doe"));
        assert_eq!(p.email.as_deref(), Some("jane@example.com"));
        assert_eq!(p.location.as_deref(), Some("Berlin, DE"));
        assert_eq!(p.links[0].label, "example.com");
        assert_eq!(p.sections.len(), 3);
        let Section::Experience(x) = &p.sections[0] else {
            panic!("experience")
        };
        let job = &x.entries[0];
        assert_eq!(
            job.period.start,
            Some(PartialDate {
                year: 2021,
                month: Some(3)
            })
        );
        assert_eq!(job.period.end, None);
        assert_eq!(job.bullets[0].text, "Cut latency.");
        let Section::Education(e) = &p.sections[1] else {
            panic!("education")
        };
        assert_eq!(
            e.entries[0].period.end,
            Some(PartialDate {
                year: 2014,
                month: Some(9)
            })
        );
        let Section::Languages(l) = &p.sections[2] else {
            panic!("languages")
        };
        assert_eq!(l.items[0].name, "English");
        assert_eq!(l.items[0].level.as_deref(), Some("C1"));
        // Two emails and one unreadable date.
        assert_eq!(r.warnings.len(), 2, "{:?}", r.warnings);
    }

    #[test]
    fn every_item_gets_a_distinct_fresh_id() {
        let r = to_partial(&sample(), &mut ids());
        let mut seen = std::collections::HashSet::new();
        for s in &r.cv_partial.sections {
            assert!(seen.insert(s.id().clone()));
        }
        assert!(seen.insert(r.cv_partial.links[0].id.clone()));
    }

    #[test]
    fn empty_records_give_an_empty_partial() {
        let r = to_partial(&EuropassCv::default(), &mut ids());
        assert_eq!(r.cv_partial, CvPartial::default());
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn dates_levels_and_language_names() {
        assert_eq!(
            parse_date("2020"),
            Some(PartialDate {
                year: 2020,
                month: None
            })
        );
        assert_eq!(parse_date("1800-01"), None);
        assert_eq!(parse_date("x"), None);
        let tie = EuropassLanguage {
            code: "xx".into(),
            scores: vec![("a".into(), "A2".into()), ("b".into(), "B1".into())],
        };
        assert_eq!(level(&tie).as_deref(), Some("B1"));
        assert_eq!(level(&EuropassLanguage::default()), None);
        assert_eq!(language_name("DEU"), "German");
        assert_eq!(language_name("xx"), "xx");
    }
}
