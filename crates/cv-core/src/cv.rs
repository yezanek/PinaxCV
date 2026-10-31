//! The base CV document (spec section 4.2).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use std::collections::BTreeSet;

use crate::{Id, IdSource, PersonalDetails, PersonalField, SuggestionTarget};

/// Current version of the [`Cv`] JSON shape. Bumped together with a migration step.
pub const CV_SCHEMA_VERSION: u32 = 1;

/// The canonical base CV. Variants are overlays on top of it and never mutate it.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct Cv {
    pub schema_version: u32,
    pub id: Id,
    /// Internal name shown in the CV list.
    pub title: String,
    /// Content language; drives section headings and date formatting.
    pub locale: Locale,
    pub paper: Paper,
    pub template: TemplateId,
    pub options: TemplateOptions,
    pub basics: Basics,
    /// Professional summary, plain text.
    pub summary: Option<String>,
    /// Sections in display order.
    pub sections: Vec<Section>,
}

impl Cv {
    /// Applies an accepted text suggestion to the base CV ("Apply to base
    /// CV", spec section 4.5): `text` replaces a bullet's text, an entry's
    /// description or the summary, but only while the current text is still
    /// `before`. Returns whether anything changed.
    pub fn apply_text(&mut self, target: &SuggestionTarget, before: &str, text: &str) -> bool {
        // A CV without a summary gets one when the review proposed it.
        if matches!(target, SuggestionTarget::Summary) && self.summary.is_none() {
            if before.is_empty() && !text.is_empty() {
                self.summary = Some(text.to_owned());
                return true;
            }
            return false;
        }
        let slot: Option<&mut String> = match target {
            SuggestionTarget::Summary => self.summary.as_mut(),
            SuggestionTarget::Bullet { id } => self
                .entries_mut()
                .flat_map(|e| e.bullets.iter_mut())
                .find(|b| b.id == *id)
                .map(|b| &mut b.text),
            SuggestionTarget::Entry { id } => self
                .entries_mut()
                .find(|e| e.id == *id)
                .and_then(|e| e.description.as_mut()),
            _ => None,
        };
        match slot {
            Some(current) if current == before && current != text => {
                *current = text.to_owned();
                true
            }
            _ => false,
        }
    }

    fn entries_mut(&mut self) -> impl Iterator<Item = &mut Entry> {
        self.sections.iter_mut().flat_map(|s| match s {
            Section::Experience(s)
            | Section::Education(s)
            | Section::Projects(s)
            | Section::Certifications(s) => s.entries.iter_mut(),
            Section::Custom(s) => s.entries.iter_mut(),
            Section::Skills(_) | Section::Languages(_) => [].iter_mut(),
        })
    }

    /// Returns a copy in which the CV and every entity in it has a new ID
    /// from `ids`. Used when duplicating a CV, so overlays and suggestions of
    /// the original never match the copy.
    pub fn with_fresh_ids(&self, ids: &mut dyn IdSource) -> Cv {
        let mut cv = self.clone();
        cv.id = ids.next_id();
        for link in &mut cv.basics.links {
            link.id = ids.next_id();
        }
        for section in &mut cv.sections {
            match section {
                Section::Experience(s)
                | Section::Education(s)
                | Section::Projects(s)
                | Section::Certifications(s) => {
                    s.id = ids.next_id();
                    refresh_entries(&mut s.entries, ids);
                }
                Section::Custom(s) => {
                    s.id = ids.next_id();
                    refresh_entries(&mut s.entries, ids);
                }
                Section::Skills(s) => {
                    s.id = ids.next_id();
                    for group in &mut s.groups {
                        group.id = ids.next_id();
                        for skill in &mut group.skills {
                            skill.id = ids.next_id();
                        }
                    }
                }
                Section::Languages(s) => {
                    s.id = ids.next_id();
                    for item in &mut s.items {
                        item.id = ids.next_id();
                    }
                }
            }
        }
        cv
    }
}

fn refresh_entries(entries: &mut [Entry], ids: &mut dyn IdSource) {
    for entry in entries {
        entry.id = ids.next_id();
        for bullet in &mut entry.bullets {
            bullet.id = ids.next_id();
        }
    }
}

/// Name, headline and contact details.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct Basics {
    pub full_name: String,
    /// For example "Backend Engineer".
    pub headline: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    /// For example "Berlin, Germany".
    pub location: Option<String>,
    /// Printed visibly; only `https:` and `mailto:` URLs become clickable in exports.
    pub links: Vec<Link>,
    /// Whether the base CV shows the profile photo, when the CV has one. The
    /// photo itself is stored beside the CV, not in this document; each
    /// variant has its own switch (`Variant::show_photo`).
    #[serde(default)]
    pub show_photo: bool,
    /// Optional European personal details (ADR-088). Empty by default.
    #[serde(default)]
    pub personal: PersonalDetails,
    /// The personal details the base CV prints. Empty by default; each
    /// variant has its own set (`Variant::show_personal`).
    #[serde(default)]
    pub show_personal: BTreeSet<PersonalField>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct Link {
    pub id: Id,
    pub label: String,
    pub url: String,
}

/// A CV section. Serialized with an explicit `"type"` tag.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Section {
    Experience(EntrySection),
    Education(EntrySection),
    Projects(EntrySection),
    Skills(SkillsSection),
    Languages(LanguagesSection),
    Certifications(EntrySection),
    Custom(CustomSection),
}

impl Section {
    /// Returns the section's own ID, whatever its kind.
    pub fn id(&self) -> &Id {
        match self {
            Section::Experience(s)
            | Section::Education(s)
            | Section::Projects(s)
            | Section::Certifications(s) => &s.id,
            Section::Skills(s) => &s.id,
            Section::Languages(s) => &s.id,
            Section::Custom(s) => &s.id,
        }
    }
}

/// A section made of entries (jobs, degrees, projects, certifications).
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct EntrySection {
    pub id: Id,
    /// Replaces the default heading from the locale table.
    pub title_override: Option<String>,
    pub entries: Vec<Entry>,
}

/// A user-titled section with the same shape as [`EntrySection`].
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct CustomSection {
    pub id: Id,
    pub title: String,
    pub entries: Vec<Entry>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: Id,
    /// Job title, degree or project name.
    pub title: String,
    /// Employer, school or issuer.
    pub org: String,
    pub location: Option<String>,
    pub period: Period,
    pub bullets: Vec<Bullet>,
    /// Optional one-line or paragraph description.
    pub description: Option<String>,
    /// Optional link (certifications, projects). Licence numbers go in `description`.
    pub url: Option<String>,
    /// When true, AI may not modify title, org, period or location.
    pub locked: bool,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct Bullet {
    pub id: Id,
    pub text: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct Period {
    pub start: Option<PartialDate>,
    pub end: Option<PartialDate>,
    /// When true, `end` is ignored and rendered as "Present".
    pub current: bool,
}

/// A year with an optional month (1..=12).
#[derive(
    Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord,
)]
pub struct PartialDate {
    pub year: u16,
    pub month: Option<u8>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct SkillsSection {
    pub id: Id,
    pub title_override: Option<String>,
    pub groups: Vec<SkillGroup>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct SkillGroup {
    pub id: Id,
    pub label: Option<String>,
    pub skills: Vec<Skill>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct Skill {
    pub id: Id,
    pub name: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct LanguagesSection {
    pub id: Id,
    pub title_override: Option<String>,
    pub items: Vec<LanguageItem>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct LanguageItem {
    pub id: Id,
    pub name: String,
    /// Free text, for example "C1" or "Native".
    pub level: Option<String>,
}

/// CV content language. v1 ships English only; more locales are added here
/// together with their heading and date tables.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Locale {
    #[default]
    En,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Paper {
    #[default]
    A4,
    Letter,
}

/// Identifier of an embedded template. v1 has only `classic`.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq, Eq)]
#[serde(transparent)]
pub struct TemplateId(pub String);

impl Default for TemplateId {
    fn default() -> Self {
        Self("classic".to_owned())
    }
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DateStyle {
    /// "Mar 2021"
    #[default]
    MonthNameYear,
    /// "03/2021"
    NumericMonthYear,
    /// "2021"
    YearOnly,
}

/// Layout options. Out-of-range values are clamped at save time.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct TemplateOptions {
    pub font_size_pt: f32,
    pub line_spacing: f32,
    pub margin_mm: f32,
    /// `#RRGGBB` for heading rules and text only; `None` means black.
    pub accent: Option<String>,
    pub date_style: DateStyle,
}

impl TemplateOptions {
    pub const FONT_SIZE_PT: (f32, f32) = (9.0, 12.0);
    pub const LINE_SPACING: (f32, f32) = (0.9, 1.4);
    pub const MARGIN_MM: (f32, f32) = (12.0, 30.0);
}

impl Default for TemplateOptions {
    fn default() -> Self {
        Self {
            font_size_pt: 10.0,
            line_spacing: 1.1,
            margin_mm: 18.0,
            accent: None,
            date_style: DateStyle::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::sample_cv;
    use crate::test_ids::id;

    #[test]
    fn apply_text_needs_the_reviewed_text() {
        let mut cv = sample_cv();
        let bullet = SuggestionTarget::Bullet { id: id(5) };
        let before = match &cv.sections[0] {
            Section::Experience(s) => s.entries[0].bullets[0].text.clone(),
            _ => unreachable!(),
        };
        assert!(!cv.apply_text(&bullet, "Something else", "New"));
        assert!(cv.apply_text(&bullet, &before, "New"));
        assert!(!cv.apply_text(&bullet, &before, "Newer"));
        assert!(!cv.apply_text(&SuggestionTarget::Bullet { id: id(99) }, "", "x"));
        // No summary yet: a reviewed empty summary may be filled.
        assert!(cv.apply_text(&SuggestionTarget::Summary, "", "Engineer."));
        assert_eq!(cv.summary.as_deref(), Some("Engineer."));
        assert!(!cv.apply_text(&SuggestionTarget::Summary, "", "Other."));
    }

    #[test]
    fn cv_round_trips_through_json() {
        let cv = sample_cv();
        let json = serde_json::to_string(&cv).expect("serialize");
        let back: Cv = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, cv);
    }

    #[test]
    fn sections_are_tagged_and_enums_are_snake_case() {
        let value = serde_json::to_value(sample_cv()).expect("serialize");
        assert_eq!(value["sections"][0]["type"], "experience");
        assert_eq!(value["sections"][1]["type"], "skills");
        assert_eq!(value["locale"], "en");
        assert_eq!(value["paper"], "a4");
        assert_eq!(value["template"], "classic");
        assert_eq!(value["options"]["date_style"], "month_name_year");
    }

    #[test]
    fn section_id_returns_own_id_for_every_kind() {
        let cv = sample_cv();
        assert_eq!(cv.sections[0].id(), &id(3));
        assert_eq!(cv.sections[1].id(), &id(6));
        let custom = Section::Custom(CustomSection {
            id: id(9),
            title: "Volunteering".into(),
            entries: vec![],
        });
        assert_eq!(custom.id(), &id(9));
        let languages = Section::Languages(LanguagesSection {
            id: id(10),
            title_override: None,
            items: vec![],
        });
        assert_eq!(languages.id(), &id(10));
    }

    #[test]
    fn template_options_default_matches_spec() {
        let o = TemplateOptions::default();
        assert_eq!(
            (o.font_size_pt, o.line_spacing, o.margin_mm),
            (10.0, 1.1, 18.0)
        );
        assert_eq!(o.accent, None);
        assert_eq!(o.date_style, DateStyle::MonthNameYear);
        let (min, max) = TemplateOptions::FONT_SIZE_PT;
        assert!(min <= o.font_size_pt && o.font_size_pt <= max);
    }

    #[test]
    fn partial_dates_order_by_year_then_month() {
        let a = PartialDate {
            year: 2020,
            month: Some(12),
        };
        let b = PartialDate {
            year: 2021,
            month: None,
        };
        let c = PartialDate {
            year: 2021,
            month: Some(1),
        };
        assert!(a < b && b < c);
    }

    #[test]
    fn with_fresh_ids_replaces_every_id_and_keeps_content() {
        struct Counter(u32);
        impl IdSource for Counter {
            fn next_id(&mut self) -> Id {
                self.0 += 1;
                id(self.0)
            }
        }
        let cv = sample_cv();
        let copy = cv.with_fresh_ids(&mut Counter(500));
        assert_eq!(copy.id, id(501));
        let old: std::collections::BTreeSet<Id> = cv.entity_ids().into_iter().collect();
        let new = copy.entity_ids();
        assert_eq!(new.len(), old.len());
        assert!(new.iter().all(|i| !old.contains(i)));
        assert_eq!(copy.basics.full_name, cv.basics.full_name);
        assert_eq!(copy.title, cv.title);
    }
}
