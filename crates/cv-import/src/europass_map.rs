//! Turns Europass records into a [`CvPartial`] with fresh IDs.

use cv_core::{
    Bullet, CalendarDate, Entry, EntrySection, Id, LanguageItem, LanguagesSection, Link,
    PartialDate, Period, PersonalDetails, PersonalField, Section,
};

use base64::Engine as _;

use crate::europass_xml::{EuropassCv, EuropassLanguage, EuropassPerson, EuropassPhoto};
use crate::partial::{CvPartial, ImportResult, ImportWarning, MAX_PHOTO, is_jpeg_or_png};

/// Maps a parsed Europass CV. `fresh` makes a new ID for every item.
pub fn to_partial(cv: &EuropassCv, fresh: &mut dyn FnMut() -> Id) -> ImportResult {
    let mut warnings = Vec::new();
    let p = &cv.person;
    let name = format!("{} {}", p.given_name, p.family_name);
    let mut partial = CvPartial {
        full_name: Some(name.trim().to_owned()).filter(|n| !n.is_empty()),
        email: p.emails.first().cloned(),
        phone: p.phones.first().cloned(),
        location: place(&p.city, &p.country),
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
    if let Some(photo) = &cv.photo {
        match photo_bytes(photo) {
            Ok(bytes) => partial.photo = Some(bytes),
            Err(why) => warnings.push(ImportWarning::new(why)),
        }
    }
    if p.emails.len() > 1 {
        warnings.push(ImportWarning::new(
            "More than one email address was found; the first one was used.",
        ));
    }
    if p.phones.len() > 1 {
        warnings.push(ImportWarning::new(
            "More than one phone number was found; the first one was used.",
        ));
    }

    partial.personal = personal(p, &mut warnings);
    if PersonalField::ALL
        .iter()
        .any(|f| partial.personal.is_filled(*f))
    {
        warnings.push(ImportWarning::new(
            "Personal details such as the date of birth or nationality were imported. Each prints \
             only where its Show switch is on, so check the switches under Personal details \
             before you export.",
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
            location: place(&job.city, &job.country),
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

/// Nationality, birth date, gender, postal address and driving licence.
/// Europass has no place of birth, marital status or right to work.
fn personal(p: &EuropassPerson, warnings: &mut Vec<ImportWarning>) -> PersonalDetails {
    let birth_date = p.birth_date.as_deref().and_then(|value| {
        // `YYYY-MM-DDThh:mm:ss` is allowed too; the time is dropped.
        let parsed = CalendarDate::parse(value.split('T').next().unwrap_or(value));
        if parsed.is_none() {
            warnings.push(ImportWarning::new(format!(
                "The date of birth \"{value}\" could not be read and was left empty."
            )));
        }
        parsed
    });
    let mut nationality: Vec<String> = Vec::new();
    for code in &p.nationalities {
        let name = match nationality_name(code) {
            Some(n) => n.to_owned(),
            None => {
                warnings.push(ImportWarning::new(format!(
                    "The nationality code \"{code}\" is not one PinaxCV knows; it was kept as written."
                )));
                code.to_uppercase()
            }
        };
        if !nationality.contains(&name) {
            nationality.push(name);
        }
    }
    let mut driving_licence: Vec<String> = Vec::new();
    for code in &p.licences {
        let category = code.trim().to_uppercase();
        if is_licence_category(&category) && !driving_licence.contains(&category) {
            driving_licence.push(category);
        }
    }
    PersonalDetails {
        nationality,
        birth_date,
        gender: p
            .gender
            .as_deref()
            .and_then(|g| match g.trim().to_ascii_lowercase().as_str() {
                "male" | "m" => Some("Male".to_owned()),
                "female" | "f" => Some("Female".to_owned()),
                // NotKnown, NotSpecified and anything else.
                _ => None,
            }),
        address: (!p.address.is_empty()).then(|| p.address.join("\n")),
        driving_licence,
        ..PersonalDetails::default()
    }
}

/// The driving licence categories of EU Directive 2006/126/EC.
fn is_licence_category(c: &str) -> bool {
    matches!(
        c,
        "AM" | "A1"
            | "A2"
            | "A"
            | "B1"
            | "B"
            | "BE"
            | "C1"
            | "C1E"
            | "C"
            | "CE"
            | "D1"
            | "D1E"
            | "D"
            | "DE"
    )
}

/// English nationality for an ISO 3166-1 alpha-2 country code: the EU and
/// EEA countries, Switzerland, the UK and other common ones.
fn nationality_name(code: &str) -> Option<&'static str> {
    Some(match code.trim().to_ascii_lowercase().as_str() {
        "at" => "Austrian",
        "be" => "Belgian",
        "bg" => "Bulgarian",
        "hr" => "Croatian",
        "cy" => "Cypriot",
        "cz" => "Czech",
        "dk" => "Danish",
        "ee" => "Estonian",
        "fi" => "Finnish",
        "fr" => "French",
        "de" => "German",
        "gr" | "el" => "Greek",
        "hu" => "Hungarian",
        "ie" => "Irish",
        "it" => "Italian",
        "lv" => "Latvian",
        "lt" => "Lithuanian",
        "lu" => "Luxembourgish",
        "mt" => "Maltese",
        "nl" => "Dutch",
        "pl" => "Polish",
        "pt" => "Portuguese",
        "ro" => "Romanian",
        "sk" => "Slovak",
        "si" => "Slovenian",
        "es" => "Spanish",
        "se" => "Swedish",
        "is" => "Icelandic",
        "li" => "Liechtensteiner",
        "no" => "Norwegian",
        "ch" => "Swiss",
        "gb" | "uk" => "British",
        "al" => "Albanian",
        "ba" => "Bosnian",
        "me" => "Montenegrin",
        "mk" => "North Macedonian",
        "rs" => "Serbian",
        "md" => "Moldovan",
        "ua" => "Ukrainian",
        "tr" => "Turkish",
        "us" => "American",
        "ca" => "Canadian",
        "au" => "Australian",
        "nz" => "New Zealander",
        "br" => "Brazilian",
        "ar" => "Argentinian",
        "mx" => "Mexican",
        "in" => "Indian",
        "cn" => "Chinese",
        "jp" => "Japanese",
        _ => return None,
    })
}

/// "City, CC", or whichever of the two is known.
fn place(city: &Option<String>, country: &Option<String>) -> Option<String> {
    match (city, country) {
        (Some(c), Some(k)) => Some(format!("{c}, {}", k.to_uppercase())),
        (c, k) => c.clone().or_else(|| k.as_ref().map(|k| k.to_uppercase())),
    }
}

/// Exactly `YYYY`, `YYYY-MM` or `YYYY-MM-DD`; the day is checked, then dropped.
pub(crate) fn parse_date(value: &str) -> Option<PartialDate> {
    let parts: Vec<&str> = value.trim().split('-').collect();
    let number = |part: &str, len: usize| -> Option<u16> {
        (part.len() == len && part.bytes().all(|b| b.is_ascii_digit()))
            .then(|| part.parse().ok())?
    };
    let year = number(parts.first()?, 4)?;
    if !(1900..=2100).contains(&year) || parts.len() > 3 {
        return None;
    }
    let month = match parts.get(1) {
        None => None,
        Some(m) => Some(
            u8::try_from(number(m, 2)?)
                .ok()
                .filter(|m| (1..=12).contains(m))?,
        ),
    };
    if let Some(d) = parts.get(2) {
        number(d, 2).filter(|d| (1..=31).contains(d))?;
    }
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

/// The photo's bytes, or why it was left out. The base64 text is measured
/// before decoding, so an oversized photo is never decoded.
fn photo_bytes(photo: &EuropassPhoto) -> Result<Vec<u8>, &'static str> {
    const TOO_LARGE: &str = "The profile photo is larger than 5 MB and was not imported.";
    const NOT_AN_IMAGE: &str = "The profile photo is not a JPEG or PNG image and was not imported.";
    let text: Vec<u8> = photo
        .base64
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    if text.len() > MAX_PHOTO.div_ceil(3) * 4 {
        return Err(TOO_LARGE);
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&text)
        .map_err(|_| NOT_AN_IMAGE)?;
    if bytes.len() > MAX_PHOTO {
        return Err(TOO_LARGE);
    }
    if !is_jpeg_or_png(&bytes) {
        return Err(NOT_AN_IMAGE);
    }
    Ok(bytes)
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
                phones: vec!["+49 301234567".into(), "+49 307654321".into()],
                web: vec!["https://example.com/".into()],
                city: Some("Berlin".into()),
                country: Some("de".into()),
                summary: Some("Engineer.".into()),
                ..EuropassPerson::default()
            },
            positions: vec![EuropassPosition {
                organisation: "Acme GmbH".into(),
                title: "Senior Engineer".into(),
                city: Some("Munich".into()),
                country: Some("de".into()),
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
            photo: None,
        }
    }

    #[test]
    fn maps_personal_details_without_showing_them() {
        let mut cv = sample();
        cv.person.nationalities = vec!["it".into(), "IT".into(), "zz".into()];
        cv.person.birth_date = Some("1990-03-12T00:00:00".into());
        cv.person.gender = Some("NotSpecified".into());
        cv.person.address = vec!["Via Esempio 1".into(), "10100 Torino".into()];
        cv.person.licences = vec!["b".into(), "B".into(), "Forklift".into(), "C1E".into()];
        let r = to_partial(&cv, &mut ids());
        let p = &r.cv_partial.personal;
        assert_eq!(p.nationality, ["Italian", "ZZ"]);
        assert_eq!(p.birth_date, CalendarDate::parse("1990-03-12"));
        assert_eq!(p.gender, None);
        assert_eq!(p.address.as_deref(), Some("Via Esempio 1\n10100 Torino"));
        assert_eq!(p.driving_licence, ["B", "C1E"]);
        assert_eq!(p.birth_place, None);
        let messages: Vec<&str> = r.warnings.iter().map(|w| w.message.as_str()).collect();
        assert!(
            messages.iter().any(|m| m.contains("\"zz\"")),
            "{messages:?}"
        );
        assert!(
            messages.iter().any(|m| m.contains("Show switch")),
            "{messages:?}"
        );

        cv.person.gender = Some("Female".into());
        cv.person.birth_date = Some("1990-02-30".into());
        let r = to_partial(&cv, &mut ids());
        assert_eq!(r.cv_partial.personal.gender.as_deref(), Some("Female"));
        assert_eq!(r.cv_partial.personal.birth_date, None);
        assert!(r.warnings.iter().any(|w| w.message.contains("1990-02-30")));
        // Current exports write the code in lower case.
        for (code, want) in [
            ("male", Some("Male")),
            ("F", Some("Female")),
            ("NotKnown", None),
        ] {
            cv.person.gender = Some(code.into());
            let r = to_partial(&cv, &mut ids());
            assert_eq!(r.cv_partial.personal.gender.as_deref(), want, "{code}");
        }

        // Nothing personal: no note about the switches.
        let r = to_partial(&sample(), &mut ids());
        assert!(!r.warnings.iter().any(|w| w.message.contains("Show switch")));
    }

    #[test]
    fn maps_person_sections_and_reports_guesses() {
        let r = to_partial(&sample(), &mut ids());
        let p = &r.cv_partial;
        assert_eq!(p.full_name.as_deref(), Some("Jane Doe"));
        assert_eq!(p.email.as_deref(), Some("jane@example.com"));
        assert_eq!(p.phone.as_deref(), Some("+49 301234567"));
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
        assert_eq!(job.location.as_deref(), Some("Munich, DE"));
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
        // Two emails, two phones and one unreadable date.
        assert_eq!(r.warnings.len(), 3, "{:?}", r.warnings);
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
    fn the_photo_is_decoded_or_refused_with_a_warning() {
        let png = b"\x89PNG\r\n\x1a\nrest";
        let encoded = base64::engine::general_purpose::STANDARD.encode(png);
        let with = |base64: String| EuropassCv {
            photo: Some(EuropassPhoto {
                mime: Some("image/png".into()),
                base64,
            }),
            ..EuropassCv::default()
        };
        let wrapped = format!("\n  {}\n  {}\n", &encoded[..8], &encoded[8..]);
        let r = to_partial(&with(wrapped), &mut ids());
        assert_eq!(r.cv_partial.photo.as_deref(), Some(&png[..]));
        assert!(r.warnings.is_empty());

        let gif = base64::engine::general_purpose::STANDARD.encode(b"GIF89a....");
        for bad in [gif, "not base64!".into()] {
            let r = to_partial(&with(bad), &mut ids());
            assert_eq!(r.cv_partial.photo, None);
            assert!(r.warnings[0].message.contains("not a JPEG or PNG"));
        }

        let mut big = vec![0u8; MAX_PHOTO + 1];
        big[..3].copy_from_slice(&[0xFF, 0xD8, 0xFF]);
        let big = base64::engine::general_purpose::STANDARD.encode(&big);
        let r = to_partial(&with(big), &mut ids());
        assert_eq!(r.cv_partial.photo, None);
        assert!(r.warnings[0].message.contains("larger than 5 MB"));
        // Exactly 5 MB is accepted.
        let mut max = vec![0u8; MAX_PHOTO];
        max[..3].copy_from_slice(&[0xFF, 0xD8, 0xFF]);
        let max = base64::engine::general_purpose::STANDARD.encode(&max);
        assert!(
            to_partial(&with(max), &mut ids())
                .cv_partial
                .photo
                .is_some()
        );
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
        for bad in [
            "2024-02-invalid",
            "2024-02-32",
            "2024-02-01-05",
            "2024-2",
            "+2024",
        ] {
            assert_eq!(parse_date(bad), None, "{bad}");
        }
        assert_eq!(
            parse_date("2024-02-29"),
            Some(PartialDate {
                year: 2024,
                month: Some(2)
            })
        );
        assert_eq!(place(&None, &Some("it".into())).as_deref(), Some("IT"));
        assert_eq!(place(&Some("Rome".into()), &None).as_deref(), Some("Rome"));
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
