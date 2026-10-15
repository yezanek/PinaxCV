//! Job-specific overlays on a base CV (spec section 4.4).

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Id, JobRequirements, Suggestion};

/// Current version of the [`Variant`] JSON shape.
pub const VARIANT_SCHEMA_VERSION: u32 = 1;

/// An overlay on a base CV. Resolving it never mutates the base.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct Variant {
    pub schema_version: u32,
    pub id: Id,
    pub cv_id: Id,
    /// For example "Acme, Senior Backend".
    pub name: String,
    pub job: JobTarget,
    /// Sections, entries, bullets, skills or language items hidden entirely.
    pub hidden: BTreeSet<Id>,
    /// Entries rendered as a single line (title, org, period).
    pub collapsed: BTreeSet<Id>,
    /// Parent id to ordered child ids. The parent is the CV id (section order),
    /// a section id (entries or items), an entry id (bullets) or a skill group id (skills).
    pub order: BTreeMap<Id, Vec<Id>>,
    /// Bullet id to bullet text, or entry id to entry description.
    pub text_overrides: BTreeMap<Id, String>,
    pub summary_override: Option<String>,
    /// 1 or 2 pages.
    pub page_target: Option<u8>,
    pub suggestions: Vec<Suggestion>,
    /// Mirrors the storage revision; the store's column is authoritative.
    pub revision: u64,
}

/// The job a variant is tailored to.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq, Default)]
pub struct JobTarget {
    pub title: Option<String>,
    pub company: Option<String>,
    /// Pasted job description.
    pub description_text: String,
    /// SHA-256 hex of `description_text`.
    pub description_hash: Option<String>,
    /// Cached AI parse.
    pub requirements: Option<JobRequirements>,
}

impl Variant {
    /// Creates an empty overlay for `cv_id`: resolving it yields the base content.
    pub fn empty(id: Id, cv_id: Id, name: impl Into<String>) -> Self {
        Self {
            schema_version: VARIANT_SCHEMA_VERSION,
            id,
            cv_id,
            name: name.into(),
            job: JobTarget::default(),
            hidden: BTreeSet::new(),
            collapsed: BTreeSet::new(),
            order: BTreeMap::new(),
            text_overrides: BTreeMap::new(),
            summary_override: None,
            page_target: None,
            suggestions: Vec::new(),
            revision: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_ids::id;

    #[test]
    fn empty_variant_has_no_overlay() {
        let v = Variant::empty(id(1), id(2), "Acme");
        assert_eq!(v.schema_version, VARIANT_SCHEMA_VERSION);
        assert_eq!(
            (v.id.clone(), v.cv_id.clone(), v.name.as_str()),
            (id(1), id(2), "Acme")
        );
        assert!(v.hidden.is_empty() && v.collapsed.is_empty() && v.order.is_empty());
        assert!(v.text_overrides.is_empty() && v.summary_override.is_none());
        assert!(v.suggestions.is_empty());
    }

    #[test]
    fn variant_round_trips_through_json() {
        let mut v = Variant::empty(id(1), id(2), "Acme");
        v.hidden.insert(id(3));
        v.order.insert(id(2), vec![id(5), id(4)]);
        v.text_overrides.insert(id(6), "Shorter text".into());
        let json = serde_json::to_string(&v).expect("serialize");
        let back: Variant = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, v);
    }
}
