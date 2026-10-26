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

    /// Hides `id` in this variant, or shows it again. Returns whether it is
    /// now hidden.
    pub fn toggle_hidden(&mut self, id: &Id) -> bool {
        toggle(&mut self.hidden, id)
    }

    /// Collapses entry `id` to one line, or expands it again. Returns whether
    /// it is now collapsed.
    pub fn toggle_collapsed(&mut self, id: &Id) -> bool {
        toggle(&mut self.collapsed, id)
    }

    /// Sets this variant's text for a bullet or entry description. Text equal
    /// to the base (`base`) removes the override, so the variant follows
    /// later base edits again.
    pub fn set_text(&mut self, id: &Id, text: String, base: &str) {
        if text == base {
            self.text_overrides.remove(id);
        } else {
            self.text_overrides.insert(id.clone(), text);
        }
    }

    /// "Revert to base" for a bullet or description. Returns whether an
    /// override was removed.
    pub fn revert_text(&mut self, id: &Id) -> bool {
        self.text_overrides.remove(id).is_some()
    }

    /// Sets this variant's summary. Text equal to the base summary (`base`,
    /// `None` when the CV has none) removes the override.
    pub fn set_summary(&mut self, text: String, base: Option<&str>) {
        self.summary_override = (text != base.unwrap_or_default()).then_some(text);
    }

    /// "Revert to base" for the summary. Returns whether an override was
    /// removed.
    pub fn revert_summary(&mut self) -> bool {
        self.summary_override.take().is_some()
    }

    /// The order this variant shows `base` (the children of `parent` in base
    /// order) in: the variant's list for `parent` first, skipping unknown and
    /// repeated IDs, then the rest in base order. Resolving uses the same rule.
    pub fn children_order(&self, parent: &Id, base: &[Id]) -> Vec<Id> {
        let mut out: Vec<Id> = Vec::with_capacity(base.len());
        for wanted in self.order.get(parent).into_iter().flatten() {
            if base.contains(wanted) && !out.contains(wanted) {
                out.push(wanted.clone());
            }
        }
        out.extend(
            base.iter()
                .filter(|b| !out.contains(b))
                .cloned()
                .collect::<Vec<_>>(),
        );
        out
    }

    /// Moves child `id` of `parent` to where `target` is now, as a drag does.
    /// `current` is the order the variant shows now. Returns false when either
    /// is missing or they are the same.
    pub fn move_child_to(&mut self, parent: &Id, current: &[Id], id: &Id, target: &Id) -> bool {
        let (Some(i), Some(j)) = (
            current.iter().position(|c| c == id),
            current.iter().position(|c| c == target),
        ) else {
            return false;
        };
        if i == j {
            return false;
        }
        let mut order = current.to_vec();
        let moved = order.remove(i);
        order.insert(j, moved);
        self.order.insert(parent.clone(), order);
        true
    }

    /// Moves child `id` of `parent` one step up or down. `current` is the
    /// order the variant shows now (base order with this variant's order
    /// applied); the result is stored as the variant's full order for
    /// `parent`. Returns false when the move is not possible.
    pub fn move_child(&mut self, parent: &Id, current: &[Id], id: &Id, up: bool) -> bool {
        let Some(i) = current.iter().position(|c| c == id) else {
            return false;
        };
        let j = match (up, i) {
            (true, 0) => return false,
            (true, i) => i - 1,
            (false, i) if i + 1 >= current.len() => return false,
            (false, i) => i + 1,
        };
        let mut order = current.to_vec();
        order.swap(i, j);
        self.order.insert(parent.clone(), order);
        true
    }
}

fn toggle(set: &mut BTreeSet<Id>, id: &Id) -> bool {
    if set.remove(id) {
        false
    } else {
        set.insert(id.clone());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_ids::id;

    #[test]
    fn summary_overrides_follow_the_base_when_equal() {
        let mut v = Variant::empty(id(1), id(2), "v");
        v.set_summary("Tailored".into(), Some("Base"));
        assert_eq!(v.summary_override.as_deref(), Some("Tailored"));
        v.set_summary("Base".into(), Some("Base"));
        assert_eq!(v.summary_override, None);
        v.set_summary(String::new(), None);
        assert_eq!(v.summary_override, None);
        v.set_summary("New".into(), None);
        assert!(v.revert_summary());
        assert!(!v.revert_summary());
    }

    #[test]
    fn children_order_lists_ordered_children_first() {
        let mut v = Variant::empty(id(1), id(2), "Acme");
        let base = [id(10), id(11), id(12)];
        assert_eq!(v.children_order(&id(5), &base), base);
        v.order.insert(id(5), vec![id(12), id(99), id(12), id(10)]);
        assert_eq!(v.children_order(&id(5), &base), [id(12), id(10), id(11)]);
    }

    #[test]
    fn move_child_to_places_the_item_at_the_target() {
        let mut v = Variant::empty(id(1), id(2), "Acme");
        let current = [id(10), id(11), id(12)];
        assert!(v.move_child_to(&id(5), &current, &id(10), &id(12)));
        assert_eq!(v.order[&id(5)], [id(11), id(12), id(10)]);
        assert!(v.move_child_to(&id(5), &v.order[&id(5)].clone(), &id(10), &id(11)));
        assert_eq!(v.order[&id(5)], [id(10), id(11), id(12)]);
        assert!(!v.move_child_to(&id(5), &current, &id(10), &id(10)));
        assert!(!v.move_child_to(&id(5), &current, &id(10), &id(99)));
    }

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
    fn toggles_hide_and_collapse() {
        let mut v = Variant::empty(id(1), id(2), "Acme");
        assert!(v.toggle_hidden(&id(3)));
        assert!(v.hidden.contains(&id(3)));
        assert!(!v.toggle_hidden(&id(3)));
        assert!(v.hidden.is_empty());
        assert!(v.toggle_collapsed(&id(4)));
        assert!(!v.toggle_collapsed(&id(4)));
        assert!(v.collapsed.is_empty());
    }

    #[test]
    fn text_overrides_follow_the_base_when_equal_and_revert() {
        let mut v = Variant::empty(id(1), id(2), "Acme");
        v.set_text(&id(5), "Short".into(), "Long base text");
        assert_eq!(
            v.text_overrides.get(&id(5)).map(String::as_str),
            Some("Short")
        );
        v.set_text(&id(5), "Long base text".into(), "Long base text");
        assert!(v.text_overrides.is_empty());
        v.set_text(&id(5), "Short".into(), "Long base text");
        assert!(v.revert_text(&id(5)));
        assert!(!v.revert_text(&id(5)));
    }

    #[test]
    fn moves_children_within_bounds() {
        let mut v = Variant::empty(id(1), id(2), "Acme");
        let current = [id(4), id(5), id(6)];
        assert!(v.move_child(&id(2), &current, &id(6), true));
        assert_eq!(v.order[&id(2)], [id(4), id(6), id(5)]);
        assert!(!v.move_child(&id(2), &current, &id(4), true));
        assert!(!v.move_child(&id(2), &current, &id(6), false));
        assert!(!v.move_child(&id(2), &current, &id(9), false));
        assert!(v.move_child(&id(2), &current, &id(4), false));
        assert_eq!(v.order[&id(2)], [id(5), id(4), id(6)]);
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
