//! JSON Resume (spec section 9.1, <https://jsonresume.org/schema>) into a
//! [`CvPartial`]. The file is read as untyped JSON so unknown fields and
//! values of the wrong type are skipped rather than failing the import.

use cv_core::{
    Bullet, CustomSection, Entry, EntrySection, Id, LanguageItem, LanguagesSection, Link,
    PartialDate, Period, Section, Skill, SkillGroup, SkillsSection,
};
use serde_json::{Map, Value};
use unicode_normalization::UnicodeNormalization;

use crate::europass_map::parse_date;
use crate::import::{ImportError, MAX_INPUT};
use crate::partial::{CvPartial, ImportResult, ImportWarning};

/// Items kept per list (entries in a section, bullets in an entry, skills in
/// a group); the rest are dropped with a warning.
pub const MAX_ITEMS: usize = 200;
/// Characters kept per text value.
pub const MAX_TEXT: usize = 10_000;

/// Top-level keys of the schema; a JSON object with none of them is not a
/// JSON Resume.
const KEYS: &[&str] = &[
    "basics",
    "work",
    "volunteer",
    "education",
    "awards",
    "certificates",
    "publications",
    "skills",
    "languages",
    "interests",
    "references",
    "projects",
];

/// True when `bytes` look like JSON (an object, after any BOM and spaces).
pub fn looks_like_json(bytes: &[u8]) -> bool {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{')
}

/// Parses a JSON Resume file. `fresh` makes a new ID for every item.
pub fn import_json_resume(
    bytes: &[u8],
    fresh: &mut dyn FnMut() -> Id,
) -> Result<ImportResult, ImportError> {
    if bytes.len() > MAX_INPUT {
        return Err(ImportError::TooLarge);
    }
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let root: Value = serde_json::from_slice(bytes)
        .map_err(|_| ImportError::Malformed("The JSON file could not be read.".into()))?;
    let Some(root) = root
        .as_object()
        .filter(|o| KEYS.iter().any(|k| o.contains_key(*k)))
    else {
        return Err(ImportError::UnsupportedFormat(
            "The JSON file doesn't look like a JSON Resume. Expected keys such as \"basics\" and \"work\"."
                .into(),
        ));
    };
    let mut m = Mapper {
        fresh,
        warnings: Vec::new(),
    };
    Ok(m.map(root))
}

struct Mapper<'a> {
    fresh: &'a mut dyn FnMut() -> Id,
    warnings: Vec<ImportWarning>,
}

impl Mapper<'_> {
    fn id(&mut self) -> Id {
        (self.fresh)()
    }

    fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(ImportWarning::new(message));
    }

    fn map(&mut self, root: &Map<String, Value>) -> ImportResult {
        let mut partial = CvPartial::default();
        if let Some(b) = root.get("basics").and_then(Value::as_object) {
            self.basics(b, &mut partial);
        }
        let work = self.entries(root, "work", "work", |m, o| m.work(o));
        let education = self.entries(root, "education", "education", |m, o| m.education(o));
        let projects = self.entries(root, "projects", "project", |m, o| m.project(o));
        let certificates =
            self.entries(root, "certificates", "certificate", |m, o| m.certificate(o));
        let volunteer = self.entries(root, "volunteer", "volunteer", |m, o| m.work(o));
        for (entries, make) in [
            (work, Section::Experience as fn(EntrySection) -> Section),
            (education, Section::Education),
            (projects, Section::Projects),
            (certificates, Section::Certifications),
        ] {
            if !entries.is_empty() {
                partial.sections.push(make(EntrySection {
                    id: self.id(),
                    title_override: None,
                    entries,
                }));
            }
        }
        if !volunteer.is_empty() {
            partial.sections.push(Section::Custom(CustomSection {
                id: self.id(),
                title: "Volunteering".into(),
                entries: volunteer,
            }));
        }
        if let Some(skills) = self.skills(root) {
            partial.sections.push(skills);
        }
        if let Some(languages) = self.languages(root) {
            partial.sections.push(languages);
        }
        for (key, what) in [
            ("awards", "Awards"),
            ("publications", "Publications"),
            ("interests", "Interests"),
            ("references", "References"),
        ] {
            if root
                .get(key)
                .and_then(Value::as_array)
                .is_some_and(|a| !a.is_empty())
            {
                self.warn(format!("{what} were not imported."));
            }
        }
        ImportResult {
            cv_partial: partial,
            warnings: std::mem::take(&mut self.warnings),
        }
    }

    fn basics(&mut self, b: &Map<String, Value>, p: &mut CvPartial) {
        p.full_name = line(b, "name");
        p.headline = line(b, "label");
        p.email = line(b, "email");
        p.phone = line(b, "phone");
        p.summary = text(b, "summary");
        if let Some(loc) = b.get("location").and_then(Value::as_object) {
            let parts: Vec<String> = ["city", "region", "countryCode"]
                .into_iter()
                .filter_map(|k| line(loc, k))
                .collect();
            p.location = (!parts.is_empty()).then(|| parts.join(", "));
        }
        if let Some(url) = line(b, "url") {
            let label = host_label(&url);
            p.links.push(Link {
                id: self.id(),
                label,
                url,
            });
        }
        for profile in self.cap(objects(b.get("profiles")), "profiles") {
            let Some(url) = line(profile, "url") else {
                continue;
            };
            let label = line(profile, "network")
                .or_else(|| line(profile, "username"))
                .unwrap_or_else(|| host_label(&url));
            p.links.push(Link {
                id: self.id(),
                label,
                url,
            });
        }
    }

    /// The first [`MAX_ITEMS`] of `items`, with a warning when some are left
    /// out.
    fn cap<T>(&mut self, items: impl Iterator<Item = T>, what: &str) -> Vec<T> {
        let mut all: Vec<T> = items.collect();
        if all.len() > MAX_ITEMS {
            self.warn(format!("Only the first {MAX_ITEMS} {what} were imported."));
            all.truncate(MAX_ITEMS);
        }
        all
    }

    /// Maps each object in `root[key]` with `f`, at most [`MAX_ITEMS`].
    /// Entries with neither a title nor an organization are skipped with a
    /// warning.
    fn entries(
        &mut self,
        root: &Map<String, Value>,
        key: &str,
        what: &str,
        mut f: impl FnMut(&mut Self, &Map<String, Value>) -> Entry,
    ) -> Vec<Entry> {
        let all: Vec<&Map<String, Value>> = objects(root.get(key)).collect();
        if all.len() > MAX_ITEMS {
            self.warn(format!(
                "Only the first {MAX_ITEMS} {what} entries were imported."
            ));
        }
        let mut out = Vec::new();
        for o in all.into_iter().take(MAX_ITEMS) {
            let entry = f(self, o);
            if entry.title.is_empty() && entry.org.is_empty() {
                self.warn(format!(
                    "Skipped an entry in \"{key}\": it has no title or organization."
                ));
                continue;
            }
            out.push(entry);
        }
        out
    }

    fn work(&mut self, o: &Map<String, Value>) -> Entry {
        let title = line(o, "position").unwrap_or_default();
        // `name` is the current schema; `company` and `organization` are
        // older or the volunteer section's.
        let org = line(o, "name")
            .or_else(|| line(o, "company"))
            .or_else(|| line(o, "organization"))
            .unwrap_or_default();
        let what = describe(&title, &org);
        Entry {
            id: self.id(),
            location: line(o, "location"),
            period: self.period(o, &what),
            bullets: self.bullets(o, "highlights", &what),
            description: text(o, "summary").or_else(|| text(o, "description")),
            url: line(o, "url"),
            locked: false,
            title,
            org,
        }
    }

    fn education(&mut self, o: &Map<String, Value>) -> Entry {
        let title = [line(o, "studyType"), line(o, "area")]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        let org = line(o, "institution").unwrap_or_default();
        let what = describe(&title, &org);
        let score = line(o, "score");
        Entry {
            id: self.id(),
            location: None,
            period: self.period(o, &what),
            bullets: self.bullets(o, "courses", &what),
            description: score,
            url: line(o, "url"),
            locked: false,
            title,
            org,
        }
    }

    fn project(&mut self, o: &Map<String, Value>) -> Entry {
        let title = line(o, "name").unwrap_or_default();
        let org = line(o, "entity").unwrap_or_default();
        let what = describe(&title, &org);
        Entry {
            id: self.id(),
            location: None,
            period: self.period(o, &what),
            bullets: self.bullets(o, "highlights", &what),
            description: text(o, "description"),
            url: line(o, "url"),
            locked: false,
            title,
            org,
        }
    }

    fn certificate(&mut self, o: &Map<String, Value>) -> Entry {
        let title = line(o, "name").unwrap_or_default();
        let org = line(o, "issuer").unwrap_or_default();
        let what = describe(&title, &org);
        Entry {
            id: self.id(),
            location: None,
            period: Period {
                start: self.date(o, "date", &what),
                end: None,
                current: false,
            },
            bullets: Vec::new(),
            description: None,
            url: line(o, "url"),
            locked: false,
            title,
            org,
        }
    }

    /// A start without an end means the entry is current (spec section 9.1).
    fn period(&mut self, o: &Map<String, Value>, what: &str) -> Period {
        let start = self.date(o, "startDate", what);
        let end = self.date(o, "endDate", what);
        let current = start.is_some() && end.is_none() && line(o, "endDate").is_none();
        if current {
            self.warn(format!(
                "{what} has no end date, so it was marked as current."
            ));
        }
        Period {
            start,
            end,
            current,
        }
    }

    fn date(&mut self, o: &Map<String, Value>, key: &str, what: &str) -> Option<PartialDate> {
        let value = line(o, key)?;
        let parsed = parse_date(&value);
        if parsed.is_none() {
            self.warn(format!(
                "The date \"{value}\" of {what} could not be read and was left empty."
            ));
        }
        parsed
    }

    fn bullets(&mut self, o: &Map<String, Value>, key: &str, what: &str) -> Vec<Bullet> {
        let all: Vec<String> = strings(o.get(key)).collect();
        if all.len() > MAX_ITEMS {
            self.warn(format!(
                "Only the first {MAX_ITEMS} bullets of {what} were imported."
            ));
        }
        all.into_iter()
            .take(MAX_ITEMS)
            .map(|text| Bullet {
                id: self.id(),
                text,
            })
            .collect()
    }

    fn skills(&mut self, root: &Map<String, Value>) -> Option<Section> {
        let mut groups = Vec::new();
        let mut loose = Vec::new();
        for o in self.cap(objects(root.get("skills")), "skills") {
            let name = line(o, "name");
            let keywords = self.cap(strings(o.get("keywords")), "keywords of a skill");
            if keywords.is_empty() {
                // A skill without keywords is a skill, not a group label.
                loose.extend(name);
                continue;
            }
            let skills = keywords
                .into_iter()
                .map(|name| Skill {
                    id: self.id(),
                    name,
                })
                .collect();
            groups.push(SkillGroup {
                id: self.id(),
                label: name,
                skills,
            });
        }
        if !loose.is_empty() {
            let skills = loose
                .into_iter()
                .map(|name| Skill {
                    id: self.id(),
                    name,
                })
                .collect();
            groups.push(SkillGroup {
                id: self.id(),
                label: None,
                skills,
            });
        }
        (!groups.is_empty()).then(|| {
            Section::Skills(SkillsSection {
                id: self.id(),
                title_override: None,
                groups,
            })
        })
    }

    fn languages(&mut self, root: &Map<String, Value>) -> Option<Section> {
        let items: Vec<LanguageItem> = self
            .cap(objects(root.get("languages")), "languages")
            .into_iter()
            .filter_map(|o| {
                let name = line(o, "language")?;
                Some(LanguageItem {
                    id: self.id(),
                    name,
                    level: line(o, "fluency"),
                })
            })
            .collect();
        (!items.is_empty()).then(|| {
            Section::Languages(LanguagesSection {
                id: self.id(),
                title_override: None,
                items,
            })
        })
    }
}

/// The objects in an array value; anything else is skipped.
fn objects(v: Option<&Value>) -> impl Iterator<Item = &Map<String, Value>> {
    v.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
}

/// The non-empty one-line strings in an array value.
fn strings(v: Option<&Value>) -> impl Iterator<Item = String> + '_ {
    v.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|s| clean(s, false))
}

/// A one-line text field.
fn line(o: &Map<String, Value>, key: &str) -> Option<String> {
    clean(o.get(key)?.as_str()?, false)
}

/// A multi-line text field; line breaks are kept.
fn text(o: &Map<String, Value>, key: &str) -> Option<String> {
    clean(o.get(key)?.as_str()?, true)
}

/// NFC, with whitespace controls as spaces (line breaks kept with
/// `keep_lines`), other control characters dropped, runs of spaces collapsed, at most
/// [`MAX_TEXT`] characters. `None` when nothing is left.
fn clean(s: &str, keep_lines: bool) -> Option<String> {
    let nfc: String = s
        .nfc()
        .filter_map(|c| match c {
            '\n' if keep_lines => Some('\n'),
            c if c.is_whitespace() => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .take(MAX_TEXT)
        .collect();
    let out = if keep_lines {
        nfc.lines()
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_owned()
    } else {
        nfc.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    (!out.is_empty()).then_some(out)
}

/// `"Engineer at Acme"`-style text for warnings.
fn describe(title: &str, org: &str) -> String {
    match (title.is_empty(), org.is_empty()) {
        (false, false) => format!("\"{title}\" ({org})"),
        (false, true) => format!("\"{title}\""),
        (true, false) => format!("\"{org}\""),
        (true, true) => "an entry".into(),
    }
}

/// `example.com/me` for `https://example.com/me/`.
fn host_label(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> impl FnMut() -> Id {
        let mut n = 0u32;
        move || {
            n += 1;
            Id::parse(&format!("01JAAAAAAAAAAAAAAAAAA{n:05}")).expect("id")
        }
    }

    fn import(json: &str) -> Result<ImportResult, ImportError> {
        import_json_resume(json.as_bytes(), &mut ids())
    }

    #[test]
    fn detects_json_objects_only() {
        assert!(looks_like_json(b"\xEF\xBB\xBF  {\"basics\":{}}"));
        assert!(!looks_like_json(b"%PDF-1.7"));
        assert!(!looks_like_json(b"[1]"));
    }

    #[test]
    fn other_json_is_unsupported_and_broken_json_is_malformed() {
        assert!(matches!(
            import(r#"{"name":"x"}"#),
            Err(ImportError::UnsupportedFormat(_))
        ));
        assert!(matches!(
            import(r#"{"basics": "#),
            Err(ImportError::Malformed(_))
        ));
    }

    #[test]
    fn wrong_types_are_skipped_not_fatal() {
        let r = import(
            r#"{"basics": {"name": 7, "email": ["x"]}, "work": "none", "skills": [1, null, {"name": ["x"]}],
                "education": [{"institution": {"a": 1}, "area": "Physics", "startDate": 2020}]}"#,
        )
        .expect("imports");
        assert_eq!(r.cv_partial.full_name, None);
        assert_eq!(r.cv_partial.sections.len(), 1, "only the education entry");
    }

    #[test]
    fn text_is_normalized_and_capped() {
        assert_eq!(clean("  a\u{0}b \t c ", false).as_deref(), Some("ab c"));
        assert_eq!(clean("e\u{301}", false).as_deref(), Some("\u{e9}"));
        assert_eq!(clean("one\n\n two ", true).as_deref(), Some("one\n\ntwo"));
        assert_eq!(clean(" \u{7} ", false), None);
        assert_eq!(clean("a\tb\nc", false).as_deref(), Some("a b c"));
        assert_eq!(clean("a\tb\r\nc", true).as_deref(), Some("a b\nc"));
        let long = "x".repeat(MAX_TEXT + 5);
        assert_eq!(
            clean(&long, false).map(|s| s.chars().count()),
            Some(MAX_TEXT)
        );
    }

    #[test]
    fn entries_without_title_or_org_are_skipped_with_a_warning() {
        let r = import(r#"{"work": [{"summary": "x"}, {"name": "Acme"}]}"#).expect("imports");
        let Section::Experience(x) = &r.cv_partial.sections[0] else {
            panic!("experience");
        };
        assert_eq!(x.entries.len(), 1);
        assert!(
            r.warnings
                .iter()
                .any(|w| w.message.contains("no title or organization"))
        );
    }

    #[test]
    fn input_over_the_size_cap_is_too_large() {
        let mut big = br#"{"basics": {"summary": ""#.to_vec();
        big.resize(MAX_INPUT + 1, b'x');
        assert!(matches!(
            import_json_resume(&big, &mut ids()),
            Err(ImportError::TooLarge)
        ));
    }

    #[test]
    fn every_list_cap_warns() {
        let n = MAX_ITEMS + 1;
        let profiles: Vec<String> = (0..n)
            .map(|i| format!(r#"{{"url": "https://e.com/{i}"}}"#))
            .collect();
        let langs: Vec<String> = (0..n)
            .map(|i| format!(r#"{{"language": "L{i}"}}"#))
            .collect();
        let words: Vec<String> = (0..n).map(|i| format!(r#""k{i}""#)).collect();
        let json = format!(
            r#"{{"basics": {{"profiles": [{}]}}, "languages": [{}], "skills": [{{"name": "G", "keywords": [{}]}}]}}"#,
            profiles.join(","),
            langs.join(","),
            words.join(",")
        );
        let r = import(&json).expect("imports");
        for what in ["profiles", "languages", "keywords of a skill"] {
            assert!(
                r.warnings
                    .iter()
                    .any(|w| w.message.contains(&format!("first {MAX_ITEMS} {what}"))),
                "{what}"
            );
        }
        assert_eq!(r.cv_partial.links.len(), MAX_ITEMS);
    }

    #[test]
    fn long_lists_are_capped_with_a_warning() {
        let work: Vec<String> = (0..MAX_ITEMS + 3)
            .map(|i| format!(r#"{{"name": "Org {i}"}}"#))
            .collect();
        let r = import(&format!(r#"{{"work": [{}]}}"#, work.join(","))).expect("imports");
        let Section::Experience(x) = &r.cv_partial.sections[0] else {
            panic!("experience");
        };
        assert_eq!(x.entries.len(), MAX_ITEMS);
        assert!(r.warnings.iter().any(|w| w.message.contains("first 200")));
    }

    #[test]
    fn deeply_nested_json_fails_cleanly() {
        let deep = format!(
            r#"{{"basics": {}{}}}"#,
            "[".repeat(10_000),
            "]".repeat(10_000)
        );
        assert!(import(&deep).is_err());
    }
}
