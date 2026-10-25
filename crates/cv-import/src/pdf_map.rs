//! Second step of the generic PDF importer (ADR-047): turns the sections from
//! [`split_sections`] into a [`CvPartial`]. Every guess is reported as a
//! warning, and the user reviews the result in the merge step.

use cv_core::{
    Entry, EntrySection, Id, LanguageItem, LanguagesSection, PartialDate, Period, Section, Skill,
    SkillGroup, SkillsSection,
};

use crate::partial::{CvPartial, ImportResult, ImportWarning};
use crate::pdf_text::{Heading, split_sections};

const EMPTY: Period = Period {
    start: None,
    end: None,
    current: false,
};

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

/// A date such as "2021", "03/2021", "2021-03", "Mar 2021" or "March 2021".
fn parse_date(s: &str) -> Option<PartialDate> {
    let s = s.trim();
    let year_ok =
        |y: &str| y.len() == 4 && y.parse::<u16>().is_ok_and(|y| (1900..=2100).contains(&y));
    if year_ok(s) {
        return Some(PartialDate {
            year: s.parse().ok()?,
            month: None,
        });
    }
    let (a, b) = s
        .split_once(['/', '-', ' '])
        .map(|(a, b)| (a.trim(), b.trim()))?;
    let (month, year) = if year_ok(a) { (b, a) } else { (a, b) };
    if !year_ok(year) {
        return None;
    }
    let month = match month.parse::<u8>() {
        Ok(m) if (1..=12).contains(&m) => m,
        Ok(_) => return None,
        Err(_) => {
            let m = month.to_lowercase();
            let i = MONTHS
                .iter()
                .position(|p| m.len() >= 3 && m.starts_with(p))?;
            u8::try_from(i + 1).ok()?
        }
    };
    Some(PartialDate {
        year: year.parse().ok()?,
        month: Some(month),
    })
}

/// A date range line such as "March 2021 - Present (3 years)" or
/// "(2010 - 2014)". Returns the period when the whole range reads.
pub fn parse_range(line: &str) -> Option<Period> {
    // LinkedIn puts no-break spaces around the dash.
    let line = line.replace('\u{a0}', " ");
    let core = line.split('(').find(|p| !p.trim().is_empty())?;
    let core = core.split(')').next()?.trim();
    let (a, b) = core
        .split_once(" - ")
        .or_else(|| core.split_once(" – "))
        .or_else(|| core.split_once(" to "))?;
    let start = parse_date(a)?;
    let b = b.trim();
    if b.eq_ignore_ascii_case("present") || b.eq_ignore_ascii_case("current") {
        return Some(Period {
            start: Some(start),
            end: None,
            current: true,
        });
    }
    Some(Period {
        start: Some(start),
        end: Some(parse_date(b)?),
        current: false,
    })
}

/// Experience-like lines: each entry is "organisation, title, date range",
/// followed by a location and description lines until the next entry.
fn entries(lines: &[String], fresh: &mut dyn FnMut() -> Id) -> Vec<Entry> {
    // A date line needs two header lines before it.
    let dates: Vec<usize> = (2..lines.len())
        .filter(|&i| parse_range(&lines[i]).is_some())
        .collect();
    let mut out = Vec::new();
    for (n, &d) in dates.iter().enumerate() {
        let end = dates
            .get(n + 1)
            .map_or(lines.len(), |&next| next.saturating_sub(2));
        let mut body: Vec<&str> = lines[d + 1..end.max(d + 1)]
            .iter()
            .map(String::as_str)
            .collect();
        // A duration such as "(3 years)" may sit on its own line.
        if body
            .first()
            .is_some_and(|l| l.starts_with('(') && l.ends_with(')'))
        {
            body.remove(0);
        }
        // A short first line without a full stop is usually the location.
        let location = match body.first() {
            Some(l) if l.len() <= 60 && !l.ends_with('.') && body.len() > 1 => {
                Some(body.remove(0).to_owned())
            }
            _ => None,
        };
        out.push(Entry {
            id: fresh(),
            title: lines[d - 1].clone(),
            org: lines[d - 2].clone(),
            location,
            period: parse_range(&lines[d]).unwrap_or(EMPTY),
            bullets: Vec::new(),
            description: (!body.is_empty()).then(|| body.join("\n")),
            url: None,
            locked: false,
        });
    }
    out
}

/// Education lines: "school" then "degree · (2010 - 2014)" or "degree, 2010 - 2014".
fn education(lines: &[String], fresh: &mut dyn FnMut() -> Id) -> Vec<Entry> {
    lines
        .chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| {
            let (degree, period) = match c[1].rfind(['(', '·']) {
                Some(i) => (
                    c[1][..i].trim().trim_end_matches(['·', ',']).trim(),
                    parse_range(c[1][i..].trim_start_matches('·')),
                ),
                None => (c[1].as_str(), None),
            };
            Entry {
                id: fresh(),
                title: degree.to_owned(),
                org: c[0].clone(),
                location: None,
                period: period.unwrap_or(EMPTY),
                bullets: Vec::new(),
                description: None,
                url: None,
                locked: false,
            }
        })
        .collect()
}

/// "English (Native or Bilingual)" becomes name and level.
fn language(line: &str, fresh: &mut dyn FnMut() -> Id) -> LanguageItem {
    let (name, level) = match line.split_once('(') {
        Some((n, l)) => (n.trim(), Some(l.trim_end_matches(')').trim().to_owned())),
        None => (line.trim(), None),
    };
    LanguageItem {
        id: fresh(),
        name: name.to_owned(),
        level,
    }
}

/// Skill lines: "Label: a, b" becomes a labelled group; other lines are
/// split on commas, bullets, pipes and semicolons into one unlabelled group.
/// A line that is only "Label:" heads the lines below it, up to the next
/// label. Repeats (ignoring case) are dropped. Also returns a warning for
/// each label that ends up with no skills.
fn skill_groups(
    lines: &[String],
    fresh: &mut dyn FnMut() -> Id,
) -> (Vec<SkillGroup>, Vec<ImportWarning>) {
    let mut seen = std::collections::HashSet::new();
    let mut split = |text: &str| -> Vec<String> {
        text.split([',', '·', '•', '|', ';'])
            .map(str::trim)
            .filter(|s| !s.is_empty() && seen.insert(s.to_lowercase()))
            .map(str::to_owned)
            .collect()
    };
    let mut labelled: Vec<(String, Vec<String>)> = Vec::new();
    let mut loose = Vec::new();
    // Set by a bare "Label:" line; the lines below it join that group.
    let mut heading: Option<usize> = None;
    for line in lines {
        match line.split_once(':') {
            // "Languages: Rust, Go"; a colon in a URL-like word is not a label.
            Some((label, rest)) if !label.trim().is_empty() && !rest.starts_with("//") => {
                labelled.push((label.trim().to_owned(), split(rest)));
                heading = rest.trim().is_empty().then(|| labelled.len() - 1);
            }
            _ => match heading {
                Some(i) => labelled[i].1.extend(split(line)),
                None => loose.extend(split(line)),
            },
        }
    }
    let mut warnings = Vec::new();
    let mut groups: Vec<(Option<String>, Vec<String>)> = Vec::new();
    if !loose.is_empty() {
        groups.push((None, loose));
    }
    for (label, names) in labelled {
        if names.is_empty() {
            warnings.push(ImportWarning::new(format!(
                "The skill heading \"{label}\" had no skills under it, so it was skipped."
            )));
        } else {
            groups.push((Some(label), names));
        }
    }
    let groups = groups
        .into_iter()
        .map(|(label, names)| SkillGroup {
            id: fresh(),
            label,
            skills: names
                .into_iter()
                .map(|name| Skill { id: fresh(), name })
                .collect(),
        })
        .collect();
    (groups, warnings)
}

/// A trailing date such as "(2021)", ", Mar 2021" or " 2021-03", and the
/// text before it.
fn trailing_date(line: &str) -> (String, Option<PartialDate>) {
    let line = line.trim();
    let trimmed = line.trim_end_matches(')');
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    for take in [2, 1] {
        if words.len() < take {
            continue;
        }
        let tail = words[words.len() - take..]
            .join(" ")
            .trim_start_matches('(')
            .to_owned();
        if let Some(date) = parse_date(&tail) {
            let head = words[..words.len() - take]
                .join(" ")
                .trim_end_matches(['(', ',', '-', '–', '·', ' '])
                .trim()
                .to_owned();
            return (head, Some(date));
        }
    }
    (line.to_owned(), None)
}

/// Certification lines: "Name", "Name - Issuer" or "Name, Issuer (2021)".
/// A line that is only a date belongs to the certification above it; one
/// with no dateless certification above it is skipped with a warning.
fn certifications(
    lines: &[String],
    fresh: &mut dyn FnMut() -> Id,
) -> (Vec<Entry>, Vec<ImportWarning>) {
    let mut out: Vec<Entry> = Vec::new();
    let mut warnings = Vec::new();
    for line in lines {
        let (text, date) = trailing_date(line);
        if text.is_empty() {
            match (out.last_mut(), date) {
                (Some(last), Some(date)) if last.period.start.is_none() => {
                    last.period.start = Some(date);
                }
                _ if !line.trim().is_empty() => warnings.push(ImportWarning::new(format!(
                    "The date \"{}\" in Certifications didn't match a certification, so it was skipped.",
                    line.trim()
                ))),
                _ => {}
            }
            continue;
        }
        let (title, org) = match text
            .split_once(" - ")
            .or_else(|| text.split_once(" – "))
            .or_else(|| text.split_once(" | "))
        {
            Some((t, o)) => (t.trim().to_owned(), o.trim().to_owned()),
            None => (text, String::new()),
        };
        out.push(Entry {
            id: fresh(),
            title,
            org,
            location: None,
            period: Period {
                start: date,
                end: None,
                current: false,
            },
            bullets: Vec::new(),
            description: None,
            url: None,
            locked: false,
        });
    }
    (out, warnings)
}

fn is_sidebar(h: Heading) -> bool {
    matches!(
        h,
        Heading::Contact | Heading::Skills | Heading::Languages | Heading::Certifications
    )
}

/// Builds a partial CV from PDF text.
pub fn text_to_partial(text: &str, fresh: &mut dyn FnMut() -> Id) -> ImportResult {
    let mut sections = split_sections(text);
    let mut warnings = Vec::new();
    let mut partial = CvPartial::default();
    let mut later = Vec::new();

    // LinkedIn puts its sidebar first, so the name, headline and location end
    // the last sidebar section before the main column starts. A section with
    // fewer than three lines is left alone, so its own content stays put.
    if sections.first().is_some_and(|(h, _)| *h != Heading::Top)
        && let Some(main) = sections.iter().position(|(h, _)| !is_sidebar(*h))
        && main > 0
        && sections[main - 1].1.len() >= 3
    {
        let lines = &mut sections[main - 1].1;
        let top = lines.split_off(lines.len() - 3);
        sections.insert(0, (Heading::Top, top));
        warnings.push(ImportWarning::new(
            "The name and headline were taken from a sidebar layout. Check them.".to_owned(),
        ));
    }

    for (heading, lines) in &sections {
        match heading {
            Heading::Top => {
                let mut it = lines.iter();
                partial.full_name = it.next().cloned();
                partial.headline = it.next().cloned();
                partial.location = it.next().cloned();
            }
            Heading::Contact => {
                for l in lines {
                    if l.contains('@') && !l.contains(' ') {
                        partial.email.get_or_insert_with(|| l.clone());
                    } else if l.chars().filter(char::is_ascii_digit).count() >= 7
                        && l.chars()
                            .all(|c| c.is_ascii_digit() || " +-()./".contains(c))
                    {
                        partial.phone.get_or_insert_with(|| l.clone());
                    }
                }
            }
            Heading::Summary => {
                partial.summary = (!lines.is_empty()).then(|| lines.join(" "));
            }
            Heading::Experience => {
                let found = entries(lines, fresh);
                if found.is_empty() && !lines.is_empty() {
                    warnings.push(ImportWarning::new(
                        "No dated jobs were found under Experience. Add them by hand.".to_owned(),
                    ));
                } else if !found.is_empty() {
                    partial.sections.push(Section::Experience(EntrySection {
                        id: fresh(),
                        title_override: None,
                        entries: found,
                    }));
                }
            }
            Heading::Education => {
                let found = education(lines, fresh);
                if !found.is_empty() {
                    partial.sections.push(Section::Education(EntrySection {
                        id: fresh(),
                        title_override: None,
                        entries: found,
                    }));
                }
            }
            Heading::Languages => {
                let items: Vec<LanguageItem> = lines.iter().map(|l| language(l, fresh)).collect();
                if !items.is_empty() {
                    later.push(Section::Languages(LanguagesSection {
                        id: fresh(),
                        title_override: None,
                        items,
                    }));
                }
            }
            Heading::Skills => {
                let (groups, skipped) = skill_groups(lines, fresh);
                warnings.extend(skipped);
                if !groups.is_empty() {
                    later.push(Section::Skills(SkillsSection {
                        id: fresh(),
                        title_override: None,
                        groups,
                    }));
                }
            }
            Heading::Certifications => {
                let (found, skipped) = certifications(lines, fresh);
                warnings.extend(skipped);
                if !found.is_empty() {
                    later.push(Section::Certifications(EntrySection {
                        id: fresh(),
                        title_override: None,
                        entries: found,
                    }));
                }
            }
        }
    }
    // Skills, languages and certifications (LinkedIn's sidebar) come after
    // the main sections, as a CV usually orders them.
    partial.sections.extend(later);
    ImportResult {
        cv_partial: partial,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> impl FnMut() -> Id {
        let mut n = 0u8;
        move || {
            n += 1;
            Id::parse(&format!("01HZX3M8Q4V7K2N9P5R6T0W3{n:02}")).expect("id")
        }
    }

    fn d(year: u16, month: Option<u8>) -> Option<PartialDate> {
        Some(PartialDate { year, month })
    }

    #[test]
    fn reads_date_ranges() {
        let p = parse_range("March 2021 - Present (3 years 7 months)").expect("range");
        assert_eq!((p.start, p.current), (d(2021, Some(3)), true));
        let p = parse_range("(2010 - 2014)").expect("range");
        assert_eq!((p.start, p.end), (d(2010, None), d(2014, None)));
        let p = parse_range("03/2019 to 2022-01").expect("range");
        assert_eq!((p.start, p.end), (d(2019, Some(3)), d(2022, Some(1))));
        let p = parse_range("May 2016\u{a0}-\u{a0}June 2017").expect("no-break spaces");
        assert_eq!(p.end, d(2017, Some(6)));
        assert_eq!(parse_range("Example Works"), None);
        assert_eq!(parse_range("13/2019 - 2020"), None);
    }

    const LINKEDIN_LIKE: &str = "Contact\nalex@example.org\n+49 30 1234567\nTop Skills\nRust\nLanguages\nEnglish (Native or Bilingual)\nGerman\nCertifications\nCloud Basics\nAlex Sample\nPlatform Engineer at Example Works\nBerlin, Germany\nSummary\nI build reliable systems.\nExperience\nExample Works\nPlatform Engineer\nMarch 2021 - Present (3 years)\nBerlin, Germany\nRan the build farm.\nPage 1 of 2\nOther Co\nDeveloper\nJanuary 2018 - February 2021\n(3 years 2 months)\nShipped things.\nEducation\nExample University\nBSc, Computer Science · (2014 - 2017)";

    #[test]
    fn maps_a_sidebar_layout() {
        let r = text_to_partial(LINKEDIN_LIKE, &mut ids());
        let p = &r.cv_partial;
        assert_eq!(p.full_name.as_deref(), Some("Alex Sample"));
        assert_eq!(
            p.headline.as_deref(),
            Some("Platform Engineer at Example Works")
        );
        assert_eq!(p.location.as_deref(), Some("Berlin, Germany"));
        assert_eq!(p.email.as_deref(), Some("alex@example.org"));
        assert_eq!(p.phone.as_deref(), Some("+49 30 1234567"));
        assert_eq!(p.summary.as_deref(), Some("I build reliable systems."));
        // The sidebar's sections follow the main column's.
        let Section::Languages(l) = &p.sections[3] else {
            panic!("languages")
        };
        assert_eq!(l.items[0].level.as_deref(), Some("Native or Bilingual"));
        assert_eq!(l.items[1].level, None);
        let Section::Experience(x) = &p.sections[0] else {
            panic!("experience")
        };
        assert_eq!(x.entries.len(), 2);
        assert_eq!(
            (x.entries[0].org.as_str(), x.entries[0].title.as_str()),
            ("Example Works", "Platform Engineer")
        );
        assert_eq!(x.entries[0].location.as_deref(), Some("Berlin, Germany"));
        assert_eq!(
            x.entries[0].description.as_deref(),
            Some("Ran the build farm.")
        );
        assert!(x.entries[0].period.current);
        assert_eq!(x.entries[1].period.end, d(2021, Some(2)));
        assert_eq!(x.entries[1].description.as_deref(), Some("Shipped things."));
        let Section::Education(e) = &p.sections[1] else {
            panic!("education")
        };
        assert_eq!(e.entries[0].title, "BSc, Computer Science");
        assert_eq!(e.entries[0].period.end, d(2017, None));
        assert!(r.warnings.iter().any(|w| w.message.contains("sidebar")));
        let Section::Skills(k) = &p.sections[2] else {
            panic!("skills")
        };
        assert_eq!(k.groups[0].skills[0].name, "Rust");
        let Section::Certifications(c) = &p.sections[4] else {
            panic!("certifications")
        };
        assert_eq!(c.entries[0].title, "Cloud Basics");
    }

    #[test]
    fn short_last_sidebar_section_is_not_split() {
        let r = text_to_partial(
            "Contact\nalex@example.org\nExperience\nno dates here",
            &mut ids(),
        );
        assert_eq!(r.cv_partial.email.as_deref(), Some("alex@example.org"));
        assert_eq!(r.cv_partial.full_name, None);
        assert!(!r.warnings.iter().any(|w| w.message.contains("sidebar")));
    }

    #[test]
    fn plain_layout_keeps_the_top_lines() {
        let r = text_to_partial(
            "Alex Sample\nEngineer\nExperience\nno dates here",
            &mut ids(),
        );
        assert_eq!(r.cv_partial.full_name.as_deref(), Some("Alex Sample"));
        assert!(r.cv_partial.sections.is_empty());
        assert_eq!(r.warnings.len(), 1);
    }

    #[test]
    fn skills_split_into_groups() {
        let lines: Vec<String> = [
            "Rust, Go · Python",
            "Cloud: AWS, GCP",
            "rust | Kubernetes",
            "https://example.com",
        ]
        .map(String::from)
        .to_vec();
        let (groups, warnings) = skill_groups(&lines, &mut ids());
        assert!(warnings.is_empty());
        let names = |g: &SkillGroup| g.skills.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
        assert_eq!(groups[0].label, None);
        assert_eq!(
            names(&groups[0]),
            ["Rust", "Go", "Python", "Kubernetes", "https://example.com"]
        );
        assert_eq!(groups[1].label.as_deref(), Some("Cloud"));
        assert_eq!(names(&groups[1]), ["AWS", "GCP"]);
    }

    #[test]
    fn a_bare_skill_label_heads_the_lines_below_it() {
        let lines: Vec<String> = ["Go", "Cloud:", "AWS, GCP", "Tools:", "Data: SQL"]
            .map(String::from)
            .to_vec();
        let (groups, warnings) = skill_groups(&lines, &mut ids());
        let names = |g: &SkillGroup| g.skills.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
        assert_eq!(groups.len(), 3);
        assert_eq!(
            (groups[0].label.as_deref(), names(&groups[0])),
            (None, vec!["Go".to_owned()])
        );
        assert_eq!(groups[1].label.as_deref(), Some("Cloud"));
        assert_eq!(names(&groups[1]), ["AWS", "GCP"]);
        assert_eq!(groups[2].label.as_deref(), Some("Data"));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("\"Tools\""), "{warnings:?}");
    }

    #[test]
    fn certifications_read_issuers_and_dates() {
        let lines: Vec<String> = [
            "Cloud Basics",
            "Kubernetes Admin - Example Foundation (2021)",
            "Data Engineer, Mar 2022",
            "2023",
            "Security+ | Example Org",
        ]
        .map(String::from)
        .to_vec();
        let (c, warnings) = certifications(&lines, &mut ids());
        // "2023" follows a certification that already has its date.
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(c.len(), 4);
        assert_eq!(
            (c[0].title.as_str(), c[0].period.start),
            ("Cloud Basics", None)
        );
        assert_eq!(
            (c[1].title.as_str(), c[1].org.as_str(), c[1].period.start),
            ("Kubernetes Admin", "Example Foundation", d(2021, None))
        );
        assert_eq!(
            (c[2].title.as_str(), c[2].period.start),
            ("Data Engineer", d(2022, Some(3)))
        );
        assert_eq!(c[3].org, "Example Org");
        let (lone, _) = certifications(&["Name".into(), "2019".into()], &mut ids());
        assert_eq!(lone[0].period.start, d(2019, None));
        let (stray, warnings) = certifications(
            &["2018".into(), "Name (2019)".into(), "2020".into()],
            &mut ids(),
        );
        assert_eq!(stray.len(), 1);
        assert_eq!(stray[0].period.start, d(2019, None));
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings[0].message.contains("\"2018\""));
    }
}
