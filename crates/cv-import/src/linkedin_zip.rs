//! LinkedIn's data export ZIP (spec section 9.3) into a [`CvPartial`].
//!
//! Only the profile CSVs listed in the spec are read, found by file name
//! through an alias table, and their columns are found by header name. The
//! archive is never extracted to disk. It is refused outright when an entry
//! has an unsafe path (zip slip), when there are too many entries, or when a
//! file unpacks past its cap (zip bomb): sizes are enforced while reading,
//! not taken from the archive's own headers.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use cv_core::{
    Bullet, Entry, EntrySection, Id, LanguageItem, LanguagesSection, Link, PartialDate, Period,
    Section, Skill, SkillGroup, SkillsSection,
};
use unicode_normalization::UnicodeNormalization;
use zip::ZipArchive;

use crate::import::ImportError;
use crate::partial::{CvPartial, ImportResult, ImportWarning};
use crate::pdf_map::parse_date;

/// Entries allowed in the archive (a full export has a few dozen).
pub const MAX_ENTRIES: usize = 2_000;
/// Largest unpacked size of one CSV that is read.
pub const MAX_CSV: u64 = 5 * 1024 * 1024;
/// Largest unpacked size of all CSVs read together.
pub const MAX_TOTAL: u64 = 20 * 1024 * 1024;
/// Rows kept per CSV, and characters per value.
pub const MAX_ROWS: usize = 500;
pub const MAX_TEXT: usize = 10_000;
/// A one-paragraph description longer than this stays a description.
const LONG_PARAGRAPH: usize = 300;
/// File names listed in the "not used" warning.
const LISTED_UNUSED: usize = 10;
/// Characters of a file name shown in a warning.
const NAME_IN_WARNING: usize = 100;
/// Rows searched for the header; LinkedIn's notes preamble is a few lines.
const HEADER_SEARCH: usize = 20;

/// The CSVs the importer reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    Profile,
    Positions,
    Education,
    Skills,
    Languages,
    Certifications,
    Projects,
    Emails,
    Phones,
}

/// Lower-case file names, without folders, for each kind.
const FILES: &[(&str, Kind)] = &[
    ("profile.csv", Kind::Profile),
    ("positions.csv", Kind::Positions),
    ("education.csv", Kind::Education),
    ("skills.csv", Kind::Skills),
    ("languages.csv", Kind::Languages),
    ("certifications.csv", Kind::Certifications),
    ("projects.csv", Kind::Projects),
    ("email addresses.csv", Kind::Emails),
    ("email_addresses.csv", Kind::Emails),
    ("emailaddresses.csv", Kind::Emails),
    ("phonenumbers.csv", Kind::Phones),
    ("phone numbers.csv", Kind::Phones),
    ("phone_numbers.csv", Kind::Phones),
];

/// True for a ZIP file's signature.
pub fn looks_like_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06")
}

/// Reads a LinkedIn data export. `fresh` makes a new ID for every item.
pub fn import_linkedin_zip(
    bytes: &[u8],
    fresh: &mut dyn FnMut() -> Id,
) -> Result<ImportResult, ImportError> {
    let files = read_csvs(bytes)?;
    let mut warnings = Vec::new();
    if !files.unused.is_empty() {
        let mut names: Vec<&str> = files
            .unused
            .iter()
            .take(LISTED_UNUSED)
            .map(String::as_str)
            .collect();
        if files.unused.len() > LISTED_UNUSED {
            names.push("…");
        }
        warnings.push(ImportWarning::new(format!(
            "Other files in the export were not used: {}.",
            names.join(", ")
        )));
    }
    let mut m = Mapper {
        fresh,
        warnings,
        tables: files.tables,
    };
    Ok(m.map())
}

struct Csvs {
    tables: HashMap<Kind, Table>,
    /// Names of other files in the archive, for the warning.
    unused: Vec<String>,
}

fn malformed(message: &str) -> ImportError {
    ImportError::Malformed(message.into())
}

/// Opens the archive and reads the known CSVs within the caps.
fn read_csvs(bytes: &[u8]) -> Result<Csvs, ImportError> {
    if bytes.len() > crate::import::MAX_INPUT {
        return Err(ImportError::TooLarge);
    }
    let mut zip = ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| malformed("The ZIP file could not be read."))?;
    if zip.len() > MAX_ENTRIES {
        return Err(malformed(
            "The ZIP file has too many files to be a LinkedIn export.",
        ));
    }
    let mut tables = HashMap::new();
    let mut unused = Vec::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        // Names are checked without decompressing, so a file that is never
        // read can't fail the import with a codec this build doesn't have.
        let (name, is_dir) = {
            let file = zip
                .by_index_raw(i)
                .map_err(|_| malformed("A file inside the ZIP could not be read."))?;
            let name = file
                .name()
                .map_err(|_| malformed("The ZIP file has a file name that could not be read."))?
                .into_owned();
            (name, file.is_dir())
        };
        if !safe_path(&name) {
            return Err(malformed(
                "The ZIP file contains a file with an unsafe path, so it was not opened.",
            ));
        }
        if is_dir {
            continue;
        }
        let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
        let Some(kind) = FILES
            .iter()
            .find(|(f, _)| base.eq_ignore_ascii_case(f))
            .map(|(_, k)| *k)
        else {
            unused.push(name_for_warning(base));
            continue;
        };
        if tables.contains_key(&kind) {
            continue;
        }
        let mut file = zip
            .by_index(i)
            .map_err(|_| malformed("A file inside the ZIP could not be read."))?;
        let mut data = Vec::new();
        (&mut file)
            .take(MAX_CSV + 1)
            .read_to_end(&mut data)
            .map_err(|_| malformed("A file inside the ZIP could not be unpacked."))?;
        total += data.len() as u64;
        if data.len() as u64 > MAX_CSV || total > MAX_TOTAL {
            return Err(malformed(
                "The ZIP file unpacks to far more data than a LinkedIn export, so it was not opened.",
            ));
        }
        tables.insert(kind, Table::parse(&data, kind));
    }
    if tables.is_empty() {
        return Err(ImportError::UnsupportedFormat(
            "The ZIP file doesn't look like a LinkedIn data export. Expected files such as Profile.csv and Positions.csv."
                .into(),
        ));
    }
    Ok(Csvs { tables, unused })
}

/// A file name from the archive, cleaned and shortened for a warning.
fn name_for_warning(name: &str) -> String {
    clean(name, false)
        .map(|n| n.chars().take(NAME_IN_WARNING).collect())
        .unwrap_or_else(|| "(unnamed file)".into())
}

/// Relative, without `..`, drive letters or NUL.
fn safe_path(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(['/', '\\'])
        && !name.contains(['\0', ':'])
        && !name.split(['/', '\\']).any(|part| part == "..")
}

/// A CSV's rows by lower-case header name.
#[derive(Default)]
struct Table {
    rows: Vec<HashMap<String, String>>,
    /// Rows past [`MAX_ROWS`] were dropped.
    truncated: bool,
    /// Reading stopped at a row the CSV parser could not read.
    unreadable: bool,
    /// The file had rows, but none of them looked like a header.
    no_header: bool,
}

impl Table {
    /// The header is the row, among the first [`HEADER_SEARCH`], with the
    /// most columns this kind expects, the earliest on a tie. LinkedIn puts
    /// a notes preamble before it in some files, and a preamble line that
    /// happens to match one column must not win over the real header.
    fn parse(data: &[u8], kind: Kind) -> Self {
        let data = data.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(data);
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_reader(data);
        let mut table = Self::default();
        let mut records: Vec<Vec<String>> = Vec::new();
        let mut hit_cap = false;
        for record in reader.byte_records() {
            let Ok(record) = record else {
                table.unreadable = true;
                break;
            };
            if records.len() == HEADER_SEARCH + MAX_ROWS {
                hit_cap = true;
                break;
            }
            records.push(
                record
                    .iter()
                    .map(|c| String::from_utf8_lossy(c).into_owned())
                    .collect(),
            );
        }
        let header_name = |c: &String| c.trim().trim_start_matches('\u{feff}').to_lowercase();
        let best = records
            .iter()
            .take(HEADER_SEARCH)
            .enumerate()
            .map(|(i, cells)| {
                let known = cells
                    .iter()
                    .filter(|c| known_column(kind, &header_name(c)))
                    .count();
                (known, i)
            })
            .filter(|(known, _)| *known > 0)
            .max_by_key(|(known, i)| (*known, std::cmp::Reverse(*i)));
        let Some((_, at)) = best else {
            table.no_header = !records.is_empty();
            return table;
        };
        let mut records = records.into_iter().skip(at);
        let names: Vec<String> = records
            .next()
            .unwrap_or_default()
            .iter()
            .map(header_name)
            .collect();
        let data: Vec<Vec<String>> = records.collect();
        table.truncated = hit_cap || data.len() > MAX_ROWS;
        table.rows = data
            .into_iter()
            .take(MAX_ROWS)
            .map(|cells| names.iter().cloned().zip(cells).collect())
            .collect();
        table
    }
}

/// The file name shown in warnings for each kind.
fn file_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Profile => "Profile.csv",
        Kind::Positions => "Positions.csv",
        Kind::Education => "Education.csv",
        Kind::Skills => "Skills.csv",
        Kind::Languages => "Languages.csv",
        Kind::Certifications => "Certifications.csv",
        Kind::Projects => "Projects.csv",
        Kind::Emails => "Email Addresses.csv",
        Kind::Phones => "PhoneNumbers.csv",
    }
}

/// Header names (lower case) for each kind, first the spec's, then aliases.
fn columns(kind: Kind) -> &'static [&'static str] {
    match kind {
        Kind::Profile => &[
            "first name",
            "last name",
            "headline",
            "summary",
            "geo location",
            "websites",
        ],
        Kind::Positions => &[
            "company name",
            "title",
            "description",
            "location",
            "started on",
            "finished on",
        ],
        Kind::Education => &[
            "school name",
            "start date",
            "end date",
            "notes",
            "degree name",
            "activities",
        ],
        Kind::Skills => &["name"],
        Kind::Languages => &["name", "proficiency"],
        Kind::Certifications => &[
            "name",
            "url",
            "authority",
            "started on",
            "finished on",
            "license number",
        ],
        Kind::Projects => &["title", "description", "url", "started on", "finished on"],
        Kind::Emails => &["email address", "confirmed", "primary"],
        Kind::Phones => &["extension", "number", "type"],
    }
}

fn known_column(kind: Kind, name: &str) -> bool {
    columns(kind).contains(&name) || ALIASES.iter().any(|(alias, _)| *alias == name)
}

/// Other header names seen in exports, and the name they stand for.
const ALIASES: &[(&str, &str)] = &[
    ("company", "company name"),
    ("position", "title"),
    ("school", "school name"),
    ("degree", "degree name"),
    ("start", "started on"),
    ("end", "finished on"),
    ("location name", "geo location"),
    ("email", "email address"),
    ("phone number", "number"),
];

/// A cleaned value from `row` by column name or one of its aliases.
fn get(row: &HashMap<String, String>, column: &str) -> Option<String> {
    row.get(column)
        .or_else(|| {
            ALIASES
                .iter()
                .filter(|(_, to)| *to == column)
                .find_map(|(alias, _)| row.get(*alias))
        })
        .and_then(|v| clean(v, false))
}

fn get_text(row: &HashMap<String, String>, column: &str) -> Option<String> {
    row.get(column).and_then(|v| clean(v, true))
}

/// NFC, with whitespace (other than kept line breaks) turned into spaces,
/// without other control characters, with runs of spaces collapsed, at most
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

/// Splits a free-text description into bullets on lines and on leading
/// `-`, `•` or `*`. One long paragraph stays a description.
pub fn split_description(text: &str) -> (Vec<String>, Option<String>) {
    let mut bullets = Vec::new();
    for line in text.lines() {
        for part in split_markers(line) {
            bullets.push(part);
        }
    }
    if bullets.len() == 1 && bullets[0].chars().count() > LONG_PARAGRAPH {
        return (Vec::new(), bullets.pop());
    }
    (bullets, None)
}

/// "- a • b" → ["a", "b"]; a line without markers is one item.
fn split_markers(line: &str) -> Vec<String> {
    let line = line.trim();
    let starts_with_marker = line.starts_with(['-', '•', '*']);
    let parts: Vec<&str> = if starts_with_marker || line.contains(" • ") {
        line.split(['•', '*'])
            .flat_map(|p| {
                let p = p.trim();
                match p.strip_prefix('-') {
                    Some(rest) => vec![rest],
                    None => vec![p],
                }
            })
            .collect()
    } else {
        vec![line]
    };
    parts
        .into_iter()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Links from Profile.csv's `Websites`, written like
/// `[PORTFOLIO:https://a.example,PERSONAL:https://b.example]`.
pub fn websites(value: &str) -> Vec<(String, String)> {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .filter_map(|item| {
            let item = item.trim();
            let at = item.find("http://").or_else(|| item.find("https://"))?;
            let url = item[at..].trim().to_owned();
            let label = item[..at].trim().trim_end_matches(':').trim();
            let label = if label.is_empty() {
                url.trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .trim_end_matches('/')
                    .to_owned()
            } else {
                title_case(label)
            };
            Some((label, url))
        })
        .collect()
}

/// "PERSONAL" → "Personal".
fn title_case(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut chars = lower.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn yes(v: Option<String>) -> bool {
    v.is_some_and(|v| v.eq_ignore_ascii_case("yes") || v.eq_ignore_ascii_case("true"))
}

struct Mapper<'a> {
    fresh: &'a mut dyn FnMut() -> Id,
    warnings: Vec<ImportWarning>,
    tables: HashMap<Kind, Table>,
}

impl Mapper<'_> {
    fn id(&mut self) -> Id {
        (self.fresh)()
    }

    fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(ImportWarning::new(message));
    }

    /// The rows of one CSV, warning once for anything that was skipped.
    fn rows(&mut self, kind: Kind) -> Vec<HashMap<String, String>> {
        let Some(table) = self.tables.remove(&kind) else {
            return Vec::new();
        };
        let file = file_name(kind);
        if table.no_header {
            self.warn(format!(
                "{file} was skipped because its column headers were not found."
            ));
        }
        if table.unreadable {
            self.warn(format!(
                "Part of {file} could not be read, so the rows after it were skipped."
            ));
        }
        if table.truncated {
            self.warn(format!(
                "Only the first {MAX_ROWS} rows of {file} were imported."
            ));
        }
        table.rows
    }

    fn map(&mut self) -> ImportResult {
        let mut p = CvPartial::default();
        if let Some(row) = self.rows(Kind::Profile).into_iter().next() {
            let name = [get(&row, "first name"), get(&row, "last name")]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            p.full_name = (!name.is_empty()).then_some(name);
            p.headline = get(&row, "headline");
            p.summary = get_text(&row, "summary");
            p.location = get(&row, "geo location");
            for (label, url) in get(&row, "websites")
                .map(|w| websites(&w))
                .unwrap_or_default()
            {
                p.links.push(Link {
                    id: self.id(),
                    label,
                    url,
                });
            }
        }
        p.email = self.email();
        p.phone = self
            .rows(Kind::Phones)
            .iter()
            .find_map(|r| get(r, "number"));

        let mut positions = self.positions();
        // Newest first; entries without a start date keep their place at the end.
        positions.sort_by_key(|e| std::cmp::Reverse(e.period.start));
        let education = self.education();
        let projects = self.projects();
        let certifications = self.certifications();
        for (entries, make) in [
            (
                positions,
                Section::Experience as fn(EntrySection) -> Section,
            ),
            (education, Section::Education),
            (projects, Section::Projects),
            (certifications, Section::Certifications),
        ] {
            if !entries.is_empty() {
                p.sections.push(make(EntrySection {
                    id: self.id(),
                    title_override: None,
                    entries,
                }));
            }
        }
        if let Some(s) = self.skills() {
            p.sections.push(s);
        }
        if let Some(s) = self.languages() {
            p.sections.push(s);
        }
        ImportResult {
            cv_partial: p,
            warnings: std::mem::take(&mut self.warnings),
        }
    }

    /// The primary confirmed address, else the first confirmed one. An
    /// unconfirmed address may be stale, so it is never used.
    fn email(&mut self) -> Option<String> {
        let rows = self.rows(Kind::Emails);
        let pick = |want_primary: bool, want_confirmed: bool| {
            rows.iter().find_map(|r| {
                let ok = (!want_primary || yes(get(r, "primary")))
                    && (!want_confirmed || yes(get(r, "confirmed")));
                ok.then(|| get(r, "email address")).flatten()
            })
        };
        pick(true, true).or_else(|| pick(false, true))
    }

    fn date(
        &mut self,
        row: &HashMap<String, String>,
        column: &str,
        what: &str,
    ) -> Option<PartialDate> {
        let value = get(row, column)?;
        let parsed = parse_date(&value);
        if parsed.is_none() {
            self.warn(format!(
                "The date \"{value}\" of {what} could not be read and was left empty."
            ));
        }
        parsed
    }

    /// Bullets and description from a free-text column.
    fn described(
        &mut self,
        row: &HashMap<String, String>,
        what: &str,
    ) -> (Vec<Bullet>, Option<String>) {
        let Some(text) = get_text(row, "description") else {
            return (Vec::new(), None);
        };
        let (bullets, description) = split_description(&text);
        if description.is_some() {
            self.warn(format!(
                "Review {what}: its description is one long paragraph. Consider splitting it into bullets."
            ));
        }
        let bullets = bullets
            .into_iter()
            .map(|text| Bullet {
                id: self.id(),
                text,
            })
            .collect();
        (bullets, description)
    }

    fn positions(&mut self) -> Vec<Entry> {
        let mut out = Vec::new();
        for row in self.rows(Kind::Positions) {
            let title = get(&row, "title").unwrap_or_default();
            let org = get(&row, "company name").unwrap_or_default();
            let what = describe(&title, &org);
            let start = self.date(&row, "started on", &what);
            let end = self.date(&row, "finished on", &what);
            let (bullets, description) = self.described(&row, &what);
            out.push(Entry {
                id: self.id(),
                location: get(&row, "location"),
                period: Period {
                    start,
                    end,
                    // An empty "Finished On" means the job is current.
                    current: get(&row, "finished on").is_none(),
                },
                bullets,
                description,
                url: None,
                locked: false,
                title,
                org,
            });
        }
        out
    }

    fn education(&mut self) -> Vec<Entry> {
        let mut out = Vec::new();
        for row in self.rows(Kind::Education) {
            let title = get(&row, "degree name").unwrap_or_default();
            let org = get(&row, "school name").unwrap_or_default();
            let what = describe(&title, &org);
            let start = self.date(&row, "start date", &what);
            let end = self.date(&row, "end date", &what);
            let description = [get_text(&row, "notes"), get_text(&row, "activities")]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join("\n");
            out.push(Entry {
                id: self.id(),
                location: None,
                period: Period {
                    start,
                    end,
                    current: false,
                },
                bullets: Vec::new(),
                description: (!description.is_empty()).then_some(description),
                url: None,
                locked: false,
                title,
                org,
            });
        }
        out
    }

    fn projects(&mut self) -> Vec<Entry> {
        let mut out = Vec::new();
        for row in self.rows(Kind::Projects) {
            let title = get(&row, "title").unwrap_or_default();
            let what = describe(&title, "");
            let start = self.date(&row, "started on", &what);
            let end = self.date(&row, "finished on", &what);
            let (bullets, description) = self.described(&row, &what);
            out.push(Entry {
                id: self.id(),
                org: String::new(),
                location: None,
                period: Period {
                    start,
                    end,
                    current: false,
                },
                bullets,
                description,
                url: get(&row, "url"),
                locked: false,
                title,
            });
        }
        out
    }

    fn certifications(&mut self) -> Vec<Entry> {
        let mut out = Vec::new();
        for row in self.rows(Kind::Certifications) {
            let title = get(&row, "name").unwrap_or_default();
            let org = get(&row, "authority").unwrap_or_default();
            let what = describe(&title, &org);
            let start = self.date(&row, "started on", &what);
            let end = self.date(&row, "finished on", &what);
            out.push(Entry {
                id: self.id(),
                location: None,
                period: Period {
                    start,
                    end,
                    current: false,
                },
                bullets: Vec::new(),
                description: get(&row, "license number").map(|n| format!("License number {n}")),
                url: get(&row, "url"),
                locked: false,
                title,
                org,
            });
        }
        out
    }

    /// One unlabelled group: the section heading already says "Skills".
    fn skills(&mut self) -> Option<Section> {
        let names: Vec<String> = self
            .rows(Kind::Skills)
            .iter()
            .filter_map(|r| get(r, "name"))
            .collect();
        if names.is_empty() {
            return None;
        }
        let skills = names
            .into_iter()
            .map(|name| Skill {
                id: self.id(),
                name,
            })
            .collect();
        let group = SkillGroup {
            id: self.id(),
            label: None,
            skills,
        };
        Some(Section::Skills(SkillsSection {
            id: self.id(),
            title_override: None,
            groups: vec![group],
        }))
    }

    fn languages(&mut self) -> Option<Section> {
        let rows = self.rows(Kind::Languages);
        let items: Vec<LanguageItem> = rows
            .iter()
            .filter_map(|r| {
                Some(LanguageItem {
                    id: self.id(),
                    name: get(r, "name")?,
                    level: get(r, "proficiency"),
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

/// `"Title" (Org)`-style text for warnings.
fn describe(title: &str, org: &str) -> String {
    match (title.is_empty(), org.is_empty()) {
        (false, false) => format!("\"{title}\" ({org})"),
        (false, true) => format!("\"{title}\""),
        (true, false) => format!("\"{org}\""),
        (true, true) => "an entry".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_must_stay_inside_the_archive() {
        assert!(safe_path("Profile.csv"));
        assert!(safe_path("export/Positions.csv"));
        assert!(!safe_path("../Profile.csv"));
        assert!(!safe_path("a/../../b.csv"));
        assert!(!safe_path("/etc/passwd"));
        assert!(!safe_path("\\evil.csv"));
        assert!(!safe_path("C:evil.csv"));
        assert!(!safe_path(""));
    }

    #[test]
    fn descriptions_split_into_bullets() {
        assert_eq!(
            split_description("- Led a team\n• Shipped v2\n\nCut costs"),
            (
                vec!["Led a team".into(), "Shipped v2".into(), "Cut costs".into()],
                None
            )
        );
        assert_eq!(
            split_description("Built things • Ran things"),
            (vec!["Built things".into(), "Ran things".into()], None)
        );
        let long = "word ".repeat(80);
        let (bullets, description) = split_description(long.trim());
        assert!(bullets.is_empty());
        assert_eq!(description.as_deref(), Some(long.trim()));
    }

    #[test]
    fn websites_keep_their_labels() {
        assert_eq!(
            websites("[PORTFOLIO:https://a.example,https://b.example/]"),
            [
                ("Portfolio".to_owned(), "https://a.example".to_owned()),
                ("b.example".to_owned(), "https://b.example/".to_owned()),
            ]
        );
        assert!(websites("").is_empty());
    }

    #[test]
    fn headers_are_found_after_a_preamble_and_matched_by_name() {
        let csv = b"\xEF\xBB\xBFNotes:\n\"Some text, quoted\"\n\nCompany Name,Title,Started On\nAcme,Engineer,Jan 2020\n";
        let t = Table::parse(csv, Kind::Positions);
        assert_eq!(t.rows.len(), 1);
        assert_eq!(get(&t.rows[0], "company name").as_deref(), Some("Acme"));
        assert_eq!(get(&t.rows[0], "finished on"), None);
        let aliased = Table::parse(b"Company,Position\nAcme,Engineer\n", Kind::Positions);
        assert_eq!(get(&aliased.rows[0], "title").as_deref(), Some("Engineer"));
    }

    #[test]
    fn the_header_with_the_most_known_columns_wins() {
        let t = Table::parse(b"Name\nName,Proficiency\nFrench,Native\n", Kind::Languages);
        assert_eq!(t.rows.len(), 1);
        assert_eq!(get(&t.rows[0], "name").as_deref(), Some("French"));
        assert_eq!(get(&t.rows[0], "proficiency").as_deref(), Some("Native"));
        let skills = Table::parse(b"Name\nRust\nName\n", Kind::Skills);
        assert_eq!(skills.rows.len(), 2);
    }

    #[test]
    fn skipped_rows_and_missing_headers_are_flagged() {
        let mut csv = b"Name\n".to_vec();
        for i in 0..=MAX_ROWS {
            csv.extend_from_slice(format!("Skill {i}\n").as_bytes());
        }
        let t = Table::parse(&csv, Kind::Skills);
        assert_eq!(t.rows.len(), MAX_ROWS);
        assert!(t.truncated);
        let exact = Table::parse(&csv[..csv.len() - "Skill 500\n".len()], Kind::Skills);
        assert_eq!(exact.rows.len(), MAX_ROWS);
        assert!(!exact.truncated);
        let none = Table::parse(b"foo,bar\n1,2\n", Kind::Profile);
        assert!(none.no_header && none.rows.is_empty());
        assert!(!Table::parse(b"", Kind::Profile).no_header);
    }

    #[test]
    fn whitespace_controls_separate_words() {
        assert_eq!(
            clean("Site\tReliability", false).as_deref(),
            Some("Site Reliability")
        );
        assert_eq!(clean("a\nb\u{7}c", false).as_deref(), Some("a bc"));
        assert_eq!(clean("a\tb\r\nc", true).as_deref(), Some("a b\nc"));
    }

    #[test]
    fn file_names_in_warnings_are_cleaned_and_short() {
        assert_eq!(name_for_warning("bad\u{1b}[31mname.txt"), "bad[31mname.txt");
        assert_eq!(
            name_for_warning(&"x".repeat(5000)).chars().count(),
            NAME_IN_WARNING
        );
        assert_eq!(name_for_warning("\u{7}"), "(unnamed file)");
    }

    #[test]
    fn not_a_zip_is_malformed() {
        assert!(matches!(
            import_linkedin_zip(b"PK\x03\x04garbage", &mut || Id::parse(
                "01JAAAAAAAAAAAAAAAAAAAAA01"
            )
            .expect("id")),
            Err(ImportError::Malformed(_))
        ));
    }
}
