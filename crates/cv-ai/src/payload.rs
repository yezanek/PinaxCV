//! Redaction and the CV payload sent to providers (spec section 11.3).
//!
//! Email, phone and links are never sent, not even when they appear inside
//! free text. Location is cut to city and country, and left out when
//! `send_location` is off. The full name is sent only with `send_name`.

use std::sync::LazyLock;

use cv_core::{Basics, PartialDate, Period, ResolvedCv, ResolvedEntry, ResolvedSection};
use regex::Regex;
use serde::Serialize;

/// The ID the summary is addressed by in suggestions.
pub const SUMMARY_ID: &str = "SUMMARY";

/// Replaces anything redacted inside free text.
pub const REDACTED: &str = "[redacted]";

/// What the user allows to be sent besides CV content (settings `ai.send_name`
/// and `ai.send_location`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PayloadOptions {
    pub send_name: bool,
    pub send_location: bool,
}

impl Default for PayloadOptions {
    /// The defaults of the settings: no name, city and country.
    fn default() -> Self {
        Self {
            send_name: false,
            send_location: true,
        }
    }
}

/// The CV as the model sees it.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct CvPayload {
    pub language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headline: Option<String>,
    pub summary_id: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub entries: Vec<PayloadEntry>,
    pub skills: Vec<PayloadSkill>,
    pub languages: Vec<PayloadLanguage>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct PayloadEntry {
    pub id: String,
    /// "experience", "education", "projects", "certifications" or "custom".
    pub section: &'static str,
    /// The section's own heading, for custom sections.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    pub title: String,
    pub org: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// "2021-03/present", "2019/2021-06", or absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period: Option<String>,
    pub locked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub bullets: Vec<PayloadBullet>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct PayloadBullet {
    pub id: String,
    pub text: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct PayloadSkill {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct PayloadLanguage {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
}

/// Builds the payload for `cv` (spec section 11.3).
pub fn build_cv_payload(cv: &ResolvedCv, opts: PayloadOptions) -> CvPayload {
    let r = Redactor::new(&cv.basics, opts);
    let place = |l: &Option<String>| {
        l.as_deref()
            .filter(|_| opts.send_location)
            .and_then(city_country)
            .map(|l| r.text(&l))
    };
    let mut payload = CvPayload {
        language: serde_json::to_value(cv.locale)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_else(|| "en".into()),
        name: Some(cv.basics.full_name.trim().to_owned())
            .filter(|n| opts.send_name && !n.is_empty()),
        location: place(&cv.basics.location),
        headline: r.opt(&cv.basics.headline),
        summary_id: SUMMARY_ID,
        summary: r.opt(&cv.summary),
        entries: Vec::new(),
        skills: Vec::new(),
        languages: Vec::new(),
    };
    for section in &cv.sections {
        let (kind, s) = match section {
            ResolvedSection::Experience(s) => ("experience", s),
            ResolvedSection::Education(s) => ("education", s),
            ResolvedSection::Projects(s) => ("projects", s),
            ResolvedSection::Certifications(s) => ("certifications", s),
            ResolvedSection::Custom(s) => ("custom", s),
            ResolvedSection::Skills(s) => {
                payload.skills.extend(s.groups.iter().flat_map(|g| {
                    g.skills.iter().map(|k| PayloadSkill {
                        id: k.id.to_string(),
                        name: r.text(&k.name),
                    })
                }));
                continue;
            }
            ResolvedSection::Languages(s) => {
                payload
                    .languages
                    .extend(s.items.iter().map(|l| PayloadLanguage {
                        id: l.id.to_string(),
                        name: r.text(&l.name),
                        level: r.opt(&l.level),
                    }));
                continue;
            }
        };
        let heading = if kind == "custom" {
            r.opt(&s.heading)
        } else {
            None
        };
        payload
            .entries
            .extend(s.entries.iter().map(|e| PayloadEntry {
                id: e.id.to_string(),
                section: kind,
                heading: heading.clone(),
                title: r.text(&e.title),
                org: r.text(&e.org),
                location: place(&e.location),
                period: period(&e.period),
                locked: e.locked,
                description: r.opt(&e.description),
                bullets: bullets(e, &r),
            }));
    }
    payload
}

fn bullets(e: &ResolvedEntry, r: &Redactor) -> Vec<PayloadBullet> {
    e.bullets
        .iter()
        .map(|b| PayloadBullet {
            id: b.id.to_string(),
            text: r.text(&b.text),
        })
        .collect()
}

/// "2021-03/present", "2019/2021-06", "2019", or `None` when no date is set.
pub fn period(p: &Period) -> Option<String> {
    let date = |d: &PartialDate| match d.month {
        Some(m) => format!("{:04}-{m:02}", d.year),
        None => format!("{:04}", d.year),
    };
    let end = if p.current {
        Some("present".to_owned())
    } else {
        p.end.as_ref().map(date)
    };
    match (p.start.as_ref().map(date), end) {
        (None, None) => None,
        (Some(s), None) => Some(s),
        (s, Some(e)) => Some(format!("{}/{e}", s.unwrap_or_default())),
    }
}

/// The city and country of a free-text location: its last two comma-separated
/// parts, without words holding digits (street numbers, postcodes).
pub fn city_country(location: &str) -> Option<String> {
    let parts: Vec<String> = location
        .split(',')
        .map(|part| {
            part.split_whitespace()
                .filter(|w| !w.chars().any(|c| c.is_ascii_digit()))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|p| !p.is_empty())
        .collect();
    let start = parts.len().saturating_sub(2);
    Some(parts[start..].join(", ")).filter(|l| !l.is_empty())
}

pub(crate) static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[a-z0-9._%+-]+@[a-z0-9-]+(\.[a-z0-9-]+)+").expect("email pattern")
});
pub(crate) static URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(https?://|www\.)[^\s<>]+").expect("url pattern"));
/// Phone-like runs; [`Redactor::text`] keeps those with fewer than 9 digits,
/// so dates and year ranges survive.
pub(crate) static PHONE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\+?\(?\d[\d\s()./-]{6,}\d").expect("phone pattern"));

/// Whether a [`PHONE`] match is a phone number: 9 digits or more, and not
/// only months, days and years ("01/2019 - 03/2021").
pub(crate) fn is_phone(m: &str) -> bool {
    let dates_only = m
        .split(|c: char| !c.is_ascii_digit())
        .filter(|g| !g.is_empty())
        .all(|g| g.len() <= 2 || (g.len() == 4 && (g.starts_with("19") || g.starts_with("20"))));
    m.chars().filter(char::is_ascii_digit).count() >= 9 && !dates_only
}

/// Removes contact details from free text: the CV's own email, phone, links
/// and (unless allowed) name, and anything shaped like an email address, a
/// URL or a phone number.
pub struct Redactor {
    known: Vec<String>,
}

impl Redactor {
    pub fn new(basics: &Basics, opts: PayloadOptions) -> Self {
        let mut known: Vec<String> = [&basics.email, &basics.phone]
            .into_iter()
            .flatten()
            .cloned()
            .chain(basics.links.iter().map(|l| l.url.clone()))
            .collect();
        if !opts.send_name {
            known.push(basics.full_name.clone());
        }
        if let Some(l) = &basics.location {
            // The full address, when it is more than city and country.
            if city_country(l).as_deref() != Some(l.trim()) || !opts.send_location {
                known.push(l.clone());
            }
        }
        known.retain(|k| k.trim().chars().count() >= 3);
        known.iter_mut().for_each(|k| *k = k.trim().to_owned());
        // Longest first, so a value containing another one goes whole.
        known.sort_by_key(|k| std::cmp::Reverse(k.len()));
        Self { known }
    }

    /// A redactor that knows no CV: it takes out only what is shaped like
    /// an email address, a URL or a phone number.
    pub fn patterns() -> Self {
        Self { known: Vec::new() }
    }

    /// `text` with contact details replaced by [`REDACTED`].
    pub fn text(&self, text: &str) -> String {
        let mut out = text.to_owned();
        for k in &self.known {
            out = replace_ignore_case(&out, k);
        }
        let out = EMAIL.replace_all(&out, REDACTED);
        let out = URL.replace_all(&out, REDACTED);
        PHONE
            .replace_all(&out, |c: &regex::Captures| {
                let m = &c[0];
                if is_phone(m) {
                    REDACTED.to_owned()
                } else {
                    m.to_owned()
                }
            })
            .into_owned()
    }

    fn opt(&self, text: &Option<String>) -> Option<String> {
        text.as_deref()
            .map(|t| self.text(t))
            .filter(|t| !t.trim().is_empty())
    }
}

/// Replaces every case-insensitive occurrence of `needle` in `hay`.
fn replace_ignore_case(hay: &str, needle: &str) -> String {
    let pattern = format!("(?i){}", regex::escape(needle));
    match Regex::new(&pattern) {
        Ok(re) => re.replace_all(hay, REDACTED).into_owned(),
        Err(_) => hay.replace(needle, REDACTED),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cv_core::Cv;

    fn typical() -> Cv {
        let json = include_str!("../../../testdata/cvs/typical.json");
        serde_json::from_str(json).expect("fixture")
    }

    #[test]
    fn locations_keep_city_and_country() {
        assert_eq!(
            city_country("Hauptstraße 5, 10115 Berlin, Germany").as_deref(),
            Some("Berlin, Germany")
        );
        assert_eq!(city_country("Berlin").as_deref(), Some("Berlin"));
        assert_eq!(city_country(" , 12345 ").as_deref(), None);
    }

    #[test]
    fn periods_are_compact() {
        let d = |year, month| Some(PartialDate { year, month });
        let p = |start, end, current| Period {
            start,
            end,
            current,
        };
        assert_eq!(
            period(&p(d(2021, Some(3)), None, true)).as_deref(),
            Some("2021-03/present")
        );
        assert_eq!(
            period(&p(d(2019, None), d(2021, Some(6)), false)).as_deref(),
            Some("2019/2021-06")
        );
        assert_eq!(
            period(&p(d(2019, None), None, false)).as_deref(),
            Some("2019")
        );
        assert_eq!(period(&p(None, None, false)), None);
    }

    #[test]
    fn free_text_loses_contact_details_but_keeps_dates() {
        let cv = typical();
        let r = Redactor::new(&cv.basics, PayloadOptions::default());
        let out = r.text(
            "Mail JANE@example.com, call +49 30 1234 5678 or (030) 555-12345, \
             see www.example.org/x and https://jane.example.com. Jane Doe led 2019-2021 work for 12 people.",
        );
        assert!(!out.to_lowercase().contains("jane@"), "{out}");
        assert!(
            !out.contains("1234 5678") && !out.contains("555-12345"),
            "{out}"
        );
        assert!(
            !out.contains("example.org") && !out.contains("jane.example"),
            "{out}"
        );
        assert!(!out.contains("Jane Doe"), "{out}");
        assert!(
            out.contains("2019-2021") && out.contains("12 people"),
            "{out}"
        );
    }

    #[test]
    fn payload_follows_the_options() {
        let mut cv = typical();
        cv.basics.location = Some("Hauptstraße 5, 10115 Berlin, Germany".into());
        let resolved = cv.resolve(None);
        let p = build_cv_payload(&resolved, PayloadOptions::default());
        assert_eq!(p.name, None);
        assert_eq!(p.location.as_deref(), Some("Berlin, Germany"));
        assert_eq!(p.language, "en");
        assert_eq!(p.summary_id, SUMMARY_ID);
        assert!(!p.entries.is_empty() && !p.skills.is_empty());
        let json = serde_json::to_string(&p).expect("json");
        assert!(!json.contains("Hauptstraße") && !json.contains("10115"));

        let p = build_cv_payload(
            &resolved,
            PayloadOptions {
                send_name: true,
                send_location: false,
            },
        );
        assert_eq!(p.name.as_deref(), Some("Jane Doe"));
        assert_eq!(p.location, None);
        assert!(p.entries.iter().all(|e| e.location.is_none()));
        let json = serde_json::to_string(&p).expect("json");
        assert!(!json.contains("Berlin"), "{json}");
    }
}
