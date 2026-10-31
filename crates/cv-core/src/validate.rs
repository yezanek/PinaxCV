//! Save-time and export-time validation (spec section 4.3).
//!
//! Save-time checks are structural only: incomplete content is allowed, so a
//! half-written CV always saves. Export-time checks add the content an
//! exported CV cannot do without.

use std::collections::BTreeMap;

use crate::{
    CV_SCHEMA_VERSION, Cv, Entry, Id, Layout, Section, Severity, TemplateOptions, ValidationIssue,
};

/// Stable codes for [`ValidationIssue::code`].
pub mod codes {
    /// The document's `schema_version` is not the current one; it must be migrated first.
    pub const SCHEMA_VERSION: &str = "schema_version";
    /// Two entities share an ID.
    pub const DUPLICATE_ID: &str = "duplicate_id";
    /// A partial date has a month outside 1..=12.
    pub const INVALID_MONTH: &str = "invalid_month";
    /// The birth date is not a real calendar date (ADR-088).
    pub const INVALID_DATE: &str = "invalid_date";
    /// A template option was out of range and has been clamped.
    pub const OPTION_CLAMPED: &str = "option_clamped";
    /// The accent colour was not `#RRGGBB` and has been reset to black.
    pub const ACCENT_RESET: &str = "accent_reset";
    /// Export needs `basics.full_name`.
    pub const MISSING_NAME: &str = "missing_name";
    /// Export needs an email address or a phone number.
    pub const MISSING_CONTACT: &str = "missing_contact";
}

/// Normalizes `cv` and checks its structure before it is saved.
///
/// Out-of-range template options are clamped (a non-finite value takes its
/// default) and an invalid accent is reset to black; each change is reported
/// as a [`Severity::Warning`]. Duplicate IDs, invalid months and a stale
/// schema version are reported as [`Severity::Error`]: the caller must not
/// save a document with errors.
pub fn for_save(cv: &mut Cv) -> Vec<ValidationIssue> {
    let mut issues = normalize_options(&mut cv.options);
    let section_ids: Vec<Id> = cv.sections.iter().map(|s| s.id().clone()).collect();
    if cv.layout.normalize(&section_ids) {
        let (min, max) = Layout::SIDEBAR_WIDTH_PCT;
        issues.push(warning(
            codes::OPTION_CLAMPED,
            "/layout/sidebar_width_pct",
            &format!(
                "sidebar_width_pct must be between {min} and {max}; it was set to {}.",
                cv.layout.sidebar_width_pct
            ),
        ));
    }
    issues.extend(structure(cv));
    issues
}

/// Checks `cv` before export without changing it.
///
/// Returns the structural findings of [`for_save`] (except clamping, which
/// export does not need) plus the required content: a full name and at least
/// one of email or phone.
pub fn for_export(cv: &Cv) -> Vec<ValidationIssue> {
    let mut issues = structure(cv);
    if is_blank(Some(&cv.basics.full_name)) {
        issues.push(error(
            codes::MISSING_NAME,
            "/basics/full_name",
            "Add your full name before exporting.",
        ));
    }
    if is_blank(cv.basics.email.as_deref()) && is_blank(cv.basics.phone.as_deref()) {
        issues.push(error(
            codes::MISSING_CONTACT,
            "/basics/email",
            "Add an email address or a phone number before exporting.",
        ));
    }
    issues
}

/// Returns true when `issues` contains a [`Severity::Error`].
pub fn has_errors(issues: &[ValidationIssue]) -> bool {
    issues.iter().any(|i| i.severity == Severity::Error)
}

fn structure(cv: &Cv) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    if cv.schema_version != CV_SCHEMA_VERSION {
        issues.push(error(
            codes::SCHEMA_VERSION,
            "/schema_version",
            &format!(
                "Schema version {} is not the current version {CV_SCHEMA_VERSION}.",
                cv.schema_version
            ),
        ));
    }

    if let Some(date) = cv.basics.personal.birth_date
        && !date.is_valid()
    {
        issues.push(error(
            codes::INVALID_DATE,
            "/basics/personal/birth_date",
            &format!("The date of birth {} is not a real date.", date.iso()),
        ));
    }

    let mut ids = IdCollector::default();
    ids.add(&cv.id, "/id".to_owned());
    for (i, link) in cv.basics.links.iter().enumerate() {
        ids.add(&link.id, format!("/basics/links/{i}/id"));
    }
    for (s, section) in cv.sections.iter().enumerate() {
        let base = format!("/sections/{s}");
        ids.add(section.id(), format!("{base}/id"));
        match section {
            Section::Experience(sec)
            | Section::Education(sec)
            | Section::Projects(sec)
            | Section::Certifications(sec) => {
                check_entries(&sec.entries, &base, &mut ids, &mut issues);
            }
            Section::Custom(sec) => check_entries(&sec.entries, &base, &mut ids, &mut issues),
            Section::Skills(sec) => {
                for (g, group) in sec.groups.iter().enumerate() {
                    let gpath = format!("{base}/groups/{g}");
                    ids.add(&group.id, format!("{gpath}/id"));
                    for (k, skill) in group.skills.iter().enumerate() {
                        ids.add(&skill.id, format!("{gpath}/skills/{k}/id"));
                    }
                }
            }
            Section::Languages(sec) => {
                for (k, item) in sec.items.iter().enumerate() {
                    ids.add(&item.id, format!("{base}/items/{k}/id"));
                }
            }
        }
    }
    issues.extend(ids.duplicates());
    issues
}

fn check_entries(
    entries: &[Entry],
    section_path: &str,
    ids: &mut IdCollector,
    issues: &mut Vec<ValidationIssue>,
) {
    for (e, entry) in entries.iter().enumerate() {
        let epath = format!("{section_path}/entries/{e}");
        ids.add(&entry.id, format!("{epath}/id"));
        for (b, bullet) in entry.bullets.iter().enumerate() {
            ids.add(&bullet.id, format!("{epath}/bullets/{b}/id"));
        }
        for (field, date) in [("start", &entry.period.start), ("end", &entry.period.end)] {
            if let Some(month) = date.and_then(|d| d.month)
                && !(1..=12).contains(&month)
            {
                issues.push(error(
                    codes::INVALID_MONTH,
                    &format!("{epath}/period/{field}/month"),
                    &format!("Month {month} is not between 1 and 12."),
                ));
            }
        }
    }
}

/// Records the first path of every ID and reports later ones as duplicates.
#[derive(Default)]
struct IdCollector {
    first: BTreeMap<Id, String>,
    duplicates: Vec<(String, String)>,
}

impl IdCollector {
    fn add(&mut self, id: &Id, path: String) {
        match self.first.get(id) {
            Some(first) => self.duplicates.push((path, first.clone())),
            None => {
                self.first.insert(id.clone(), path);
            }
        }
    }

    fn duplicates(self) -> impl Iterator<Item = ValidationIssue> {
        self.duplicates.into_iter().map(|(path, first)| {
            error(
                codes::DUPLICATE_ID,
                &path,
                &format!("This ID is already used at {first}."),
            )
        })
    }
}

fn normalize_options(o: &mut TemplateOptions) -> Vec<ValidationIssue> {
    let defaults = TemplateOptions::default();
    let mut issues = Vec::new();
    for (name, value, (min, max), default) in [
        (
            "font_size_pt",
            &mut o.font_size_pt,
            TemplateOptions::FONT_SIZE_PT,
            defaults.font_size_pt,
        ),
        (
            "line_spacing",
            &mut o.line_spacing,
            TemplateOptions::LINE_SPACING,
            defaults.line_spacing,
        ),
        (
            "margin_mm",
            &mut o.margin_mm,
            TemplateOptions::MARGIN_MM,
            defaults.margin_mm,
        ),
    ] {
        let fixed = if value.is_finite() {
            value.clamp(min, max)
        } else {
            default
        };
        // Compare bits so NaN (never equal to itself) is always reported.
        if fixed.to_bits() != value.to_bits() {
            issues.push(warning(
                codes::OPTION_CLAMPED,
                &format!("/options/{name}"),
                &format!("{name} must be between {min} and {max}; it was set to {fixed}."),
            ));
            *value = fixed;
        }
    }
    if let Some(accent) = &o.accent
        && !is_hex_colour(accent)
    {
        issues.push(warning(
            codes::ACCENT_RESET,
            "/options/accent",
            "The accent colour must look like #1A2B3C; it was reset to black.",
        ));
        o.accent = None;
    }
    issues
}

fn is_hex_colour(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_blank(s: Option<&str>) -> bool {
    s.is_none_or(|s| s.trim().is_empty())
}

fn error(code: &str, path: &str, message: &str) -> ValidationIssue {
    issue(Severity::Error, code, path, message)
}

fn warning(code: &str, path: &str, message: &str) -> ValidationIssue {
    issue(Severity::Warning, code, path, message)
}

fn issue(severity: Severity, code: &str, path: &str, message: &str) -> ValidationIssue {
    ValidationIssue {
        code: code.to_owned(),
        severity,
        path: path.to_owned(),
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::sample_cv;
    use crate::test_ids::id;
    use crate::{PartialDate, Section};

    fn codes_of(issues: &[ValidationIssue]) -> Vec<(&str, &str)> {
        issues
            .iter()
            .map(|i| (i.code.as_str(), i.path.as_str()))
            .collect()
    }

    #[test]
    fn valid_cv_has_no_issues() {
        let mut cv = sample_cv();
        assert_eq!(for_save(&mut cv), vec![]);
        assert_eq!(for_export(&cv), vec![]);
        assert_eq!(cv, sample_cv());
    }

    #[test]
    fn incomplete_content_still_saves() {
        let mut cv = sample_cv();
        cv.basics.full_name.clear();
        cv.basics.email = None;
        cv.sections.clear();
        let issues = for_save(&mut cv);
        assert!(!has_errors(&issues), "{issues:?}");
    }

    #[test]
    fn duplicate_ids_are_errors_pointing_at_the_second_use() {
        let mut cv = sample_cv();
        if let Section::Experience(sec) = &mut cv.sections[0] {
            sec.entries[0].bullets[0].id = id(4); // the entry's own ID
        }
        let issues = for_save(&mut cv);
        assert_eq!(
            codes_of(&issues),
            [(codes::DUPLICATE_ID, "/sections/0/entries/0/bullets/0/id")]
        );
        assert!(issues[0].message.contains("/sections/0/entries/0/id"));
        assert!(has_errors(&issues));
    }

    #[test]
    fn ids_are_unique_across_link_skill_and_language_kinds() {
        let mut cv = sample_cv();
        cv.basics.links[0].id = id(8); // same as the skill
        let issues = for_export(&cv);
        assert_eq!(
            codes_of(&issues),
            [(codes::DUPLICATE_ID, "/sections/1/groups/0/skills/0/id")]
        );
    }

    #[test]
    fn months_outside_1_to_12_are_errors() {
        let mut cv = sample_cv();
        if let Section::Experience(sec) = &mut cv.sections[0] {
            sec.entries[0].period.start = Some(PartialDate {
                year: 2021,
                month: Some(0),
            });
            sec.entries[0].period.end = Some(PartialDate {
                year: 2022,
                month: Some(13),
            });
        }
        let issues = for_save(&mut cv);
        assert_eq!(
            codes_of(&issues),
            [
                (
                    codes::INVALID_MONTH,
                    "/sections/0/entries/0/period/start/month"
                ),
                (
                    codes::INVALID_MONTH,
                    "/sections/0/entries/0/period/end/month"
                ),
            ]
        );
    }

    #[test]
    fn impossible_birth_dates_are_errors() {
        let mut cv = sample_cv();
        cv.basics.personal.birth_date = Some(crate::CalendarDate {
            year: 1990,
            month: Some(2),
            day: Some(30),
        });
        let issues = for_save(&mut cv);
        assert_eq!(
            codes_of(&issues),
            [(codes::INVALID_DATE, "/basics/personal/birth_date")]
        );
        cv.basics.personal.birth_date = crate::CalendarDate::parse("1990-02-28");
        assert!(for_save(&mut cv).is_empty());
    }

    #[test]
    fn stale_schema_version_is_an_error() {
        let mut cv = sample_cv();
        cv.schema_version = 0;
        assert_eq!(
            codes_of(&for_save(&mut cv)),
            [(codes::SCHEMA_VERSION, "/schema_version")]
        );
    }

    #[test]
    fn layout_is_repaired_and_a_clamped_width_reported() {
        let mut cv = sample_cv();
        cv.layout.sidebar_width_pct = 80;
        cv.layout.sidebar = vec![
            crate::BlockRef::Section { id: id(3) },
            crate::BlockRef::Section { id: id(99) },
        ];
        assert_eq!(
            codes_of(&for_save(&mut cv)),
            [(codes::OPTION_CLAMPED, "/layout/sidebar_width_pct")]
        );
        assert_eq!(cv.layout.sidebar_width_pct, 40);
        assert_eq!(cv.layout.sidebar, [crate::BlockRef::Section { id: id(3) }]);
        assert_eq!(for_save(&mut cv), vec![], "repair is idempotent");
    }

    #[test]
    fn out_of_range_options_are_clamped_with_warnings() {
        let mut cv = sample_cv();
        cv.options.font_size_pt = 20.0;
        cv.options.line_spacing = 0.1;
        cv.options.margin_mm = f32::NAN;
        let issues = for_save(&mut cv);
        assert_eq!(
            codes_of(&issues),
            [
                (codes::OPTION_CLAMPED, "/options/font_size_pt"),
                (codes::OPTION_CLAMPED, "/options/line_spacing"),
                (codes::OPTION_CLAMPED, "/options/margin_mm"),
            ]
        );
        assert!(!has_errors(&issues));
        assert_eq!(cv.options.font_size_pt, 12.0);
        assert_eq!(cv.options.line_spacing, 0.9);
        assert_eq!(cv.options.margin_mm, 18.0);
        assert_eq!(for_save(&mut cv), vec![], "clamping is idempotent");
    }

    #[test]
    fn invalid_accent_is_reset_and_valid_one_kept() {
        let mut cv = sample_cv();
        cv.options.accent = Some("#12345G".into());
        assert_eq!(
            codes_of(&for_save(&mut cv)),
            [(codes::ACCENT_RESET, "/options/accent")]
        );
        assert_eq!(cv.options.accent, None);

        cv.options.accent = Some("#1a2B3c".into());
        assert_eq!(for_save(&mut cv), vec![]);
        assert_eq!(cv.options.accent.as_deref(), Some("#1a2B3c"));
    }

    #[test]
    fn is_hex_colour_rejects_wrong_length_and_multibyte_input() {
        assert!(is_hex_colour("#000000"));
        assert!(!is_hex_colour("000000"));
        assert!(!is_hex_colour("#00000"));
        assert!(!is_hex_colour("#ééé"));
    }

    #[test]
    fn export_requires_name_and_one_contact() {
        let mut cv = sample_cv();
        cv.basics.full_name = "  ".into();
        cv.basics.email = Some(String::new());
        cv.basics.phone = None;
        assert_eq!(
            codes_of(&for_export(&cv)),
            [
                (codes::MISSING_NAME, "/basics/full_name"),
                (codes::MISSING_CONTACT, "/basics/email"),
            ]
        );

        cv.basics.full_name = "Jane Doe".into();
        cv.basics.phone = Some("+49 30 1234567".into());
        assert_eq!(for_export(&cv), vec![]);
    }

    #[test]
    fn has_errors_ignores_warnings() {
        let w = warning(codes::ACCENT_RESET, "/options/accent", "m");
        let e = error(codes::MISSING_NAME, "/basics/full_name", "m");
        assert!(!has_errors(std::slice::from_ref(&w)));
        assert!(has_errors(&[w, e]));
    }
}
