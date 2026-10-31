//! The materialized document produced by resolving a variant (spec section 4.4).
//!
//! `ResolvedCv` is the only input to renderers, exporters, lint and the AI payload
//! builder. It keeps every entity ID so lint findings, jump-to-field, AI targets and
//! fit-to-length can refer back to the base CV; the Typst data contract omits them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    Basics, Bullet, Closing, Id, LanguageItem, Locale, Paper, Period, SkillGroup, TemplateId,
    TemplateOptions,
};

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ResolvedCv {
    /// ID of the base CV.
    pub cv_id: Id,
    /// ID of the applied variant, if any.
    pub variant_id: Option<Id>,
    pub locale: Locale,
    pub paper: Paper,
    pub template: TemplateId,
    pub options: TemplateOptions,
    /// The base CV's basics, except that `personal` keeps only the fields in
    /// `show_personal`, which is the variant's set (ADR-088).
    pub basics: Basics,
    /// Whether to show the CV's profile photo: the variant's switch, or the
    /// base CV's without a variant. The image is attached at export time.
    pub show_photo: bool,
    /// Base summary, or the variant's `summary_override`.
    pub summary: Option<String>,
    /// Visible sections in final order. Empty sections are omitted.
    pub sections: Vec<ResolvedSection>,
    /// The closing block with only the parts the variant (or, without one,
    /// the base CV) shows; `closing.show` is that set (ADR-089).
    pub closing: Closing,
}

/// A visible section. Serialized with an explicit `"kind"` tag.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResolvedSection {
    Experience(ResolvedEntrySection),
    Education(ResolvedEntrySection),
    Projects(ResolvedEntrySection),
    Certifications(ResolvedEntrySection),
    Custom(ResolvedEntrySection),
    Skills(ResolvedSkillsSection),
    Languages(ResolvedLanguagesSection),
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ResolvedEntrySection {
    pub id: Id,
    /// Custom title or `title_override`; `None` means the locale's default heading.
    pub heading: Option<String>,
    pub entries: Vec<ResolvedEntry>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ResolvedEntry {
    pub id: Id,
    pub title: String,
    pub org: String,
    pub location: Option<String>,
    pub period: Period,
    /// Visible bullets in final order, with overrides applied. Empty when collapsed.
    pub bullets: Vec<Bullet>,
    /// Description with override applied. `None` when collapsed.
    pub description: Option<String>,
    pub url: Option<String>,
    pub locked: bool,
    pub collapsed: bool,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ResolvedSkillsSection {
    pub id: Id,
    pub heading: Option<String>,
    pub groups: Vec<SkillGroup>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ResolvedLanguagesSection {
    pub id: Id,
    pub heading: Option<String>,
    pub items: Vec<LanguageItem>,
}

impl ResolvedSection {
    /// Returns the section's ID, whatever its kind.
    pub fn id(&self) -> &Id {
        match self {
            ResolvedSection::Experience(s)
            | ResolvedSection::Education(s)
            | ResolvedSection::Projects(s)
            | ResolvedSection::Certifications(s)
            | ResolvedSection::Custom(s) => &s.id,
            ResolvedSection::Skills(s) => &s.id,
            ResolvedSection::Languages(s) => &s.id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_ids::id;

    #[test]
    fn resolved_section_id_and_kind_tag() {
        let section = ResolvedSection::Custom(ResolvedEntrySection {
            id: id(1),
            heading: Some("Volunteering".into()),
            entries: vec![],
        });
        assert_eq!(section.id(), &id(1));
        let value = serde_json::to_value(&section).expect("serialize");
        assert_eq!(value["kind"], "custom");

        let skills = ResolvedSection::Skills(ResolvedSkillsSection {
            id: id(2),
            heading: None,
            groups: vec![],
        });
        assert_eq!(skills.id(), &id(2));
        let languages = ResolvedSection::Languages(ResolvedLanguagesSection {
            id: id(3),
            heading: None,
            items: vec![],
        });
        assert_eq!(languages.id(), &id(3));
    }
}
