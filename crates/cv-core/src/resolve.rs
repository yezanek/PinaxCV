//! Variant resolution and pruning (spec section 4.4).
//!
//! [`Cv::resolve`] materializes the base CV with an optional variant overlay.
//! It is pure and total: unknown IDs in the variant are ignored, and the
//! output only ever contains IDs that exist in the base.

use std::collections::BTreeSet;

use crate::{
    Bullet, Cv, Entry, EntrySection, Id, LanguagesSection, ResolvedCv, ResolvedEntry,
    ResolvedEntrySection, ResolvedLanguagesSection, ResolvedSection, ResolvedSkillsSection,
    Section, SkillGroup, SkillsSection, Variant,
};

impl Cv {
    /// Resolves the base CV with `variant` applied (spec section 4.4).
    ///
    /// Hidden IDs are removed, collapsed entries lose their bullets and
    /// description, `order` lists are applied per parent (listed children
    /// first, then the rest in base order), and text overrides replace bullet
    /// text and entry descriptions. Sections, and skill groups, with no
    /// visible children are omitted.
    pub fn resolve(&self, variant: Option<&Variant>) -> ResolvedCv {
        let overlay = Overlay(variant);
        let sections: Vec<ResolvedSection> = overlay
            .ordered(&self.id, self.sections.iter(), |s| s.id())
            .filter(|s| !overlay.is_hidden(s.id()))
            .filter_map(|s| overlay.section(s))
            .collect();
        // Prune against what the variant shows, so the sidebar never names an
        // omitted section.
        let mut layout = self.layout.clone();
        let section_ids: Vec<Id> = sections.iter().map(|s| s.id().clone()).collect();
        layout.normalize(&section_ids);
        ResolvedCv {
            layout,
            cv_id: self.id.clone(),
            variant_id: variant.map(|v| v.id.clone()),
            locale: self.locale,
            paper: self.paper,
            template: self.template.clone(),
            options: self.options.clone(),
            basics: self.basics.clone(),
            show_photo: variant.map_or(self.basics.show_photo, |v| v.show_photo),
            summary: variant
                .and_then(|v| v.summary_override.clone())
                .or_else(|| self.summary.clone()),
            sections,
        }
    }

    /// Returns the IDs of every section, entry, bullet, skill group, skill,
    /// language item and link, in document order. The CV's own ID is not included.
    pub fn entity_ids(&self) -> Vec<Id> {
        let mut ids: Vec<Id> = self.basics.links.iter().map(|l| l.id.clone()).collect();
        for section in &self.sections {
            ids.push(section.id().clone());
            match section {
                Section::Experience(s)
                | Section::Education(s)
                | Section::Projects(s)
                | Section::Certifications(s) => push_entry_ids(&s.entries, &mut ids),
                Section::Custom(s) => push_entry_ids(&s.entries, &mut ids),
                Section::Skills(s) => {
                    for group in &s.groups {
                        ids.push(group.id.clone());
                        ids.extend(group.skills.iter().map(|k| k.id.clone()));
                    }
                }
                Section::Languages(s) => ids.extend(s.items.iter().map(|i| i.id.clone())),
            }
        }
        ids
    }
}

fn push_entry_ids(entries: &[Entry], ids: &mut Vec<Id>) {
    for entry in entries {
        ids.push(entry.id.clone());
        ids.extend(entry.bullets.iter().map(|b| b.id.clone()));
    }
}

impl Variant {
    /// Removes every reference to content that is not in `cv`, or that points
    /// at the wrong kind of entity, and returns the removed IDs, sorted and
    /// without duplicates.
    ///
    /// The UI calls this after base edits. It removes unknown IDs from
    /// `hidden`, non-entry IDs from `collapsed`, `order` lists whose parent is
    /// unknown and children that do not belong to their parent, and text
    /// overrides that target neither a bullet nor an entry. Suggestions are
    /// left alone; their targets are checked when they are applied.
    pub fn prune(&mut self, cv: &Cv) -> Vec<Id> {
        let index = Index::of(cv);
        let mut removed = BTreeSet::new();

        self.hidden
            .retain(|id| keep(index.hideable.contains(id), id, &mut removed));
        self.collapsed
            .retain(|id| keep(index.entries.contains(id), id, &mut removed));
        self.text_overrides.retain(|id, _| {
            let target = index.entries.contains(id) || index.bullets.contains(id);
            keep(target, id, &mut removed)
        });
        self.order.retain(|parent, children| {
            let Some(own) = index.children_of(cv, parent) else {
                removed.insert(parent.clone());
                removed.extend(children.iter().cloned());
                return false;
            };
            children.retain(|c| keep(own.contains(c), c, &mut removed));
            true
        });
        removed.into_iter().collect()
    }
}

fn keep(known: bool, id: &Id, removed: &mut BTreeSet<Id>) -> bool {
    if !known {
        removed.insert(id.clone());
    }
    known
}

/// ID sets used by [`Variant::prune`].
struct Index {
    /// IDs that `hidden` may reference: everything except links, which
    /// `resolve` never hides.
    hideable: BTreeSet<Id>,
    entries: BTreeSet<Id>,
    bullets: BTreeSet<Id>,
}

impl Index {
    fn of(cv: &Cv) -> Self {
        let mut entries = BTreeSet::new();
        let mut bullets = BTreeSet::new();
        for entry in cv.sections.iter().flat_map(section_entries) {
            entries.insert(entry.id.clone());
            bullets.extend(entry.bullets.iter().map(|b| b.id.clone()));
        }
        let links: BTreeSet<&Id> = cv.basics.links.iter().map(|l| &l.id).collect();
        Self {
            hideable: cv
                .entity_ids()
                .into_iter()
                .filter(|id| !links.contains(id))
                .collect(),
            entries,
            bullets,
        }
    }

    /// The direct children of `parent` that `order` may list, or `None` when
    /// `parent` cannot be ordered.
    fn children_of(&self, cv: &Cv, parent: &Id) -> Option<BTreeSet<Id>> {
        if parent == &cv.id {
            return Some(cv.sections.iter().map(|s| s.id().clone()).collect());
        }
        for section in &cv.sections {
            if section.id() == parent {
                return Some(match section {
                    Section::Skills(s) => s.groups.iter().map(|g| g.id.clone()).collect(),
                    Section::Languages(s) => s.items.iter().map(|i| i.id.clone()).collect(),
                    _ => section_entries(section).map(|e| e.id.clone()).collect(),
                });
            }
            if let Section::Skills(s) = section
                && let Some(g) = s.groups.iter().find(|g| &g.id == parent)
            {
                return Some(g.skills.iter().map(|k| k.id.clone()).collect());
            }
            if let Some(e) = section_entries(section).find(|e| &e.id == parent) {
                return Some(e.bullets.iter().map(|b| b.id.clone()).collect());
            }
        }
        None
    }
}

fn section_entries(section: &Section) -> impl Iterator<Item = &Entry> {
    let entries: &[Entry] = match section {
        Section::Experience(s)
        | Section::Education(s)
        | Section::Projects(s)
        | Section::Certifications(s) => &s.entries,
        Section::Custom(s) => &s.entries,
        Section::Skills(_) | Section::Languages(_) => &[],
    };
    entries.iter()
}

/// The optional variant, with the per-rule lookups resolve needs.
#[derive(Clone, Copy)]
struct Overlay<'a>(Option<&'a Variant>);

impl<'a> Overlay<'a> {
    fn is_hidden(&self, id: &Id) -> bool {
        self.0.is_some_and(|v| v.hidden.contains(id))
    }

    fn is_collapsed(&self, id: &Id) -> bool {
        self.0.is_some_and(|v| v.collapsed.contains(id))
    }

    fn text(&self, id: &Id) -> Option<&'a String> {
        self.0.and_then(|v| v.text_overrides.get(id))
    }

    /// Orders `items` by the variant's list for `parent`: listed items first
    /// in the given order (unknown and repeated IDs skipped), then the rest
    /// in base order.
    fn ordered<T, I>(&self, parent: &Id, items: I, id_of: fn(&T) -> &Id) -> impl Iterator<Item = T>
    where
        I: Iterator<Item = T>,
    {
        let mut rest: Vec<Option<T>> = items.map(Some).collect();
        let mut out = Vec::with_capacity(rest.len());
        if let Some(list) = self.0.and_then(|v| v.order.get(parent)) {
            for wanted in list {
                if let Some(slot) = rest
                    .iter_mut()
                    .find(|slot| slot.as_ref().is_some_and(|item| id_of(item) == wanted))
                {
                    out.extend(slot.take());
                }
            }
        }
        out.extend(rest.into_iter().flatten());
        out.into_iter()
    }

    fn section(&self, section: &Section) -> Option<ResolvedSection> {
        let resolved = match section {
            Section::Experience(s) => ResolvedSection::Experience(self.entries(s)),
            Section::Education(s) => ResolvedSection::Education(self.entries(s)),
            Section::Projects(s) => ResolvedSection::Projects(self.entries(s)),
            Section::Certifications(s) => ResolvedSection::Certifications(self.entries(s)),
            Section::Custom(s) => ResolvedSection::Custom(ResolvedEntrySection {
                id: s.id.clone(),
                heading: Some(s.title.clone()),
                entries: self.entry_list(&s.id, &s.entries),
            }),
            Section::Skills(s) => ResolvedSection::Skills(self.skills(s)),
            Section::Languages(s) => ResolvedSection::Languages(self.languages(s)),
        };
        let empty = match &resolved {
            ResolvedSection::Experience(s)
            | ResolvedSection::Education(s)
            | ResolvedSection::Projects(s)
            | ResolvedSection::Certifications(s)
            | ResolvedSection::Custom(s) => s.entries.is_empty(),
            ResolvedSection::Skills(s) => s.groups.is_empty(),
            ResolvedSection::Languages(s) => s.items.is_empty(),
        };
        (!empty).then_some(resolved)
    }

    fn entries(&self, s: &EntrySection) -> ResolvedEntrySection {
        ResolvedEntrySection {
            id: s.id.clone(),
            heading: s.title_override.clone(),
            entries: self.entry_list(&s.id, &s.entries),
        }
    }

    fn entry_list(&self, section_id: &Id, entries: &[Entry]) -> Vec<ResolvedEntry> {
        self.ordered(section_id, entries.iter(), |e| &e.id)
            .filter(|e| !self.is_hidden(&e.id))
            .map(|e| self.entry(e))
            .collect()
    }

    fn entry(&self, e: &Entry) -> ResolvedEntry {
        let collapsed = self.is_collapsed(&e.id);
        let (bullets, description) = if collapsed {
            (Vec::new(), None)
        } else {
            let bullets = self
                .ordered(&e.id, e.bullets.iter(), |b| &b.id)
                .filter(|b| !self.is_hidden(&b.id))
                .map(|b| Bullet {
                    id: b.id.clone(),
                    text: self.text(&b.id).unwrap_or(&b.text).clone(),
                })
                .collect();
            let description = self.text(&e.id).or(e.description.as_ref()).cloned();
            (bullets, description)
        };
        ResolvedEntry {
            id: e.id.clone(),
            title: e.title.clone(),
            org: e.org.clone(),
            location: e.location.clone(),
            period: e.period.clone(),
            bullets,
            description,
            url: e.url.clone(),
            locked: e.locked,
            collapsed,
        }
    }

    fn skills(&self, s: &SkillsSection) -> ResolvedSkillsSection {
        let groups = self
            .ordered(&s.id, s.groups.iter(), |g| &g.id)
            .filter(|g| !self.is_hidden(&g.id))
            .map(|g| SkillGroup {
                id: g.id.clone(),
                label: g.label.clone(),
                skills: self
                    .ordered(&g.id, g.skills.iter(), |k| &k.id)
                    .filter(|k| !self.is_hidden(&k.id))
                    .cloned()
                    .collect(),
            })
            .filter(|g| !g.skills.is_empty())
            .collect();
        ResolvedSkillsSection {
            id: s.id.clone(),
            heading: s.title_override.clone(),
            groups,
        }
    }

    fn languages(&self, s: &LanguagesSection) -> ResolvedLanguagesSection {
        ResolvedLanguagesSection {
            id: s.id.clone(),
            heading: s.title_override.clone(),
            items: self
                .ordered(&s.id, s.items.iter(), |i| &i.id)
                .filter(|i| !self.is_hidden(&i.id))
                .cloned()
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::sample_cv;
    use crate::test_ids::id;
    use crate::{CustomSection, LanguageItem, Period, Skill};

    /// The sample CV plus a second experience entry (9, bullets 10 and 11),
    /// a second skill (12) and a languages section (13, items 14 and 15).
    fn cv() -> Cv {
        let mut cv = sample_cv();
        if let Section::Experience(s) = &mut cv.sections[0] {
            s.entries.push(Entry {
                id: id(9),
                title: "Intern".into(),
                org: "Initech".into(),
                location: None,
                period: Period {
                    start: None,
                    end: None,
                    current: false,
                },
                bullets: vec![
                    Bullet {
                        id: id(10),
                        text: "Fixed printers".into(),
                    },
                    Bullet {
                        id: id(11),
                        text: "Wrote reports".into(),
                    },
                ],
                description: Some("Summer job".into()),
                url: None,
                locked: false,
            });
        }
        if let Section::Skills(s) = &mut cv.sections[1] {
            s.groups[0].skills.push(Skill {
                id: id(12),
                name: "SQL".into(),
            });
        }
        cv.sections.push(Section::Languages(LanguagesSection {
            id: id(13),
            title_override: None,
            items: vec![
                LanguageItem {
                    id: id(14),
                    name: "English".into(),
                    level: Some("Native".into()),
                },
                LanguageItem {
                    id: id(15),
                    name: "German".into(),
                    level: Some("B2".into()),
                },
            ],
        }));
        cv
    }

    fn variant() -> Variant {
        Variant::empty(id(100), id(1), "Acme")
    }

    fn experience(r: &ResolvedCv) -> &ResolvedEntrySection {
        match &r.sections[0] {
            ResolvedSection::Experience(s) => s,
            other => panic!("expected experience, got {other:?}"),
        }
    }

    fn bullet_ids(e: &ResolvedEntry) -> Vec<Id> {
        e.bullets.iter().map(|b| b.id.clone()).collect()
    }

    #[test]
    fn layout_is_carried_and_pruned() {
        let mut cv = cv();
        cv.layout.columns = crate::Columns::Two;
        cv.layout.sidebar = vec![
            crate::BlockRef::Section { id: id(13) },
            crate::BlockRef::Section { id: id(77) },
        ];
        let r = cv.resolve(None);
        assert_eq!(r.layout.columns, crate::Columns::Two);
        assert_eq!(r.layout.sidebar, [crate::BlockRef::Section { id: id(13) }]);
        assert_eq!(
            r.layout.column_of(&crate::BlockRef::Section { id: id(3) }),
            crate::Column::Main
        );
    }

    #[test]
    fn layout_drops_sections_the_variant_hides() {
        let mut cv = cv();
        cv.layout.columns = crate::Columns::Two;
        cv.layout.sidebar = vec![
            crate::BlockRef::Section { id: id(6) },
            crate::BlockRef::Section { id: id(13) },
        ];
        let mut v = variant();
        v.hidden.insert(id(13));
        let r = cv.resolve(Some(&v));
        assert_eq!(r.layout.sidebar, [crate::BlockRef::Section { id: id(6) }]);
    }

    #[test]
    fn no_variant_keeps_base_content_and_order() {
        let cv = cv();
        let r = cv.resolve(None);
        assert_eq!(r.cv_id, id(1));
        assert_eq!(r.variant_id, None);
        assert_eq!(r.basics, cv.basics);
        let section_ids: Vec<_> = r.sections.iter().map(|s| s.id().clone()).collect();
        assert_eq!(section_ids, [id(3), id(6), id(13)]);
        let exp = experience(&r);
        assert_eq!(bullet_ids(&exp.entries[1]), [id(10), id(11)]);
        assert_eq!(exp.entries[1].description.as_deref(), Some("Summer job"));
        assert!(!exp.entries[1].collapsed);
    }

    #[test]
    fn empty_variant_matches_no_variant_except_variant_id() {
        let cv = cv();
        let mut with = cv.resolve(Some(&variant()));
        assert_eq!(with.variant_id, Some(id(100)));
        with.variant_id = None;
        assert_eq!(with, cv.resolve(None));
    }

    #[test]
    fn hidden_ids_are_removed_at_every_level() {
        let cv = cv();
        let mut v = variant();
        v.hidden.extend([id(10), id(4), id(12), id(14)]);
        let r = cv.resolve(Some(&v));
        let exp = experience(&r);
        assert_eq!(exp.entries.len(), 1);
        assert_eq!(bullet_ids(&exp.entries[0]), [id(11)]);
        let ResolvedSection::Skills(skills) = &r.sections[1] else {
            panic!("skills")
        };
        assert_eq!(skills.groups[0].skills.len(), 1);
        let ResolvedSection::Languages(langs) = &r.sections[2] else {
            panic!("languages")
        };
        assert_eq!(langs.items[0].id, id(15));
    }

    #[test]
    fn sections_and_groups_without_visible_children_are_omitted() {
        let cv = cv();
        let mut v = variant();
        v.hidden.extend([id(4), id(9), id(8), id(12)]);
        let r = cv.resolve(Some(&v));
        let section_ids: Vec<_> = r.sections.iter().map(|s| s.id().clone()).collect();
        assert_eq!(section_ids, [id(13)]);

        let mut cv = cv;
        cv.sections.push(Section::Custom(CustomSection {
            id: id(16),
            title: "Volunteering".into(),
            entries: vec![],
        }));
        assert_eq!(cv.resolve(None).sections.len(), 3, "empty base section");
    }

    #[test]
    fn hiding_a_section_or_group_hides_its_children() {
        let cv = cv();
        let mut v = variant();
        v.hidden.extend([id(3), id(7)]);
        let r = cv.resolve(Some(&v));
        let section_ids: Vec<_> = r.sections.iter().map(|s| s.id().clone()).collect();
        assert_eq!(section_ids, [id(13)]);
    }

    #[test]
    fn collapsed_entries_keep_header_only() {
        let cv = cv();
        let mut v = variant();
        v.collapsed.insert(id(9));
        v.text_overrides
            .insert(id(9), "Ignored when collapsed".into());
        let r = cv.resolve(Some(&v));
        let e = &experience(&r).entries[1];
        assert!(e.collapsed);
        assert!(e.bullets.is_empty());
        assert_eq!(e.description, None);
        assert_eq!((e.title.as_str(), e.org.as_str()), ("Intern", "Initech"));
    }

    #[test]
    fn order_lists_children_first_then_rest_in_base_order() {
        let cv = cv();
        let mut v = variant();
        v.order.insert(id(1), vec![id(13), id(999), id(13)]);
        v.order.insert(id(3), vec![id(9)]);
        v.order.insert(id(9), vec![id(11), id(10)]);
        v.order.insert(id(7), vec![id(12)]);
        v.order.insert(id(13), vec![id(15)]);
        let r = cv.resolve(Some(&v));
        let section_ids: Vec<_> = r.sections.iter().map(|s| s.id().clone()).collect();
        assert_eq!(section_ids, [id(13), id(3), id(6)]);
        let base: Vec<Id> = cv.sections.iter().map(|s| s.id().clone()).collect();
        assert_eq!(v.children_order(&id(1), &base), section_ids, "same rule");
        let ResolvedSection::Experience(exp) = &r.sections[1] else {
            panic!("experience")
        };
        assert_eq!(exp.entries[0].id, id(9));
        assert_eq!(bullet_ids(&exp.entries[0]), [id(11), id(10)]);
        let ResolvedSection::Skills(skills) = &r.sections[2] else {
            panic!("skills")
        };
        assert_eq!(skills.groups[0].skills[0].id, id(12));
        let ResolvedSection::Languages(langs) = &r.sections[0] else {
            panic!("languages")
        };
        assert_eq!(langs.items[0].id, id(15));
    }

    #[test]
    fn text_overrides_replace_bullets_and_descriptions_only() {
        let cv = cv();
        let mut v = variant();
        v.text_overrides.insert(id(10), "Fixed 40 printers".into());
        v.text_overrides.insert(id(9), "Paid internship".into());
        v.text_overrides.insert(id(14), "Ignored".into());
        let r = cv.resolve(Some(&v));
        let e = &experience(&r).entries[1];
        assert_eq!(e.bullets[0].text, "Fixed 40 printers");
        assert_eq!(e.bullets[1].text, "Wrote reports");
        assert_eq!(e.description.as_deref(), Some("Paid internship"));
        assert_eq!(e.title, "Intern");
    }

    #[test]
    fn summary_override_wins_over_base_summary() {
        let mut cv = cv();
        cv.summary = Some("Base".into());
        let mut v = variant();
        assert_eq!(cv.resolve(Some(&v)).summary.as_deref(), Some("Base"));
        v.summary_override = Some("Tailored".into());
        assert_eq!(cv.resolve(Some(&v)).summary.as_deref(), Some("Tailored"));
    }

    #[test]
    fn entity_ids_lists_everything_in_document_order() {
        assert_eq!(
            cv().entity_ids(),
            [2, 3, 4, 5, 9, 10, 11, 6, 7, 8, 12, 13, 14, 15].map(id)
        );
    }

    #[test]
    fn prune_removes_unknown_and_misplaced_references() {
        let cv = cv();
        let mut v = variant();
        v.hidden.extend([id(5), id(900)]);
        v.collapsed.extend([id(9), id(5)]); // 5 is a bullet, not an entry
        v.text_overrides.insert(id(10), "ok".into());
        v.text_overrides.insert(id(14), "language item".into());
        v.order.insert(id(1), vec![id(13), id(4)]); // 4 is an entry, not a section
        v.order.insert(id(901), vec![id(902)]);
        v.order.insert(id(9), vec![id(11)]);

        let removed = v.prune(&cv);
        assert_eq!(removed, [4, 5, 14, 900, 901, 902].map(id));
        assert_eq!(v.hidden, [id(5)].into());
        assert_eq!(v.collapsed, [id(9)].into());
        assert_eq!(
            v.text_overrides.keys().cloned().collect::<Vec<_>>(),
            [id(10)]
        );
        assert_eq!(v.order.get(&id(1)), Some(&vec![id(13)]));
        assert_eq!(v.order.get(&id(9)), Some(&vec![id(11)]));
        assert!(!v.order.contains_key(&id(901)));

        assert_eq!(v.prune(&cv), vec![], "pruning twice removes nothing");
    }

    #[test]
    fn prune_removes_hidden_links() {
        let cv = cv();
        let link = cv.basics.links[0].id.clone();
        let mut v = variant();
        v.hidden.insert(link.clone());
        assert_eq!(v.prune(&cv), [link]);
        assert!(v.hidden.is_empty());
    }

    #[test]
    fn prune_accepts_group_and_section_parents() {
        let cv = cv();
        let mut v = variant();
        v.order.insert(id(6), vec![id(7)]);
        v.order.insert(id(7), vec![id(12), id(8)]);
        v.order.insert(id(13), vec![id(15)]);
        v.order.insert(id(3), vec![id(9)]);
        let before = v.clone();
        assert_eq!(v.prune(&cv), vec![]);
        assert_eq!(v, before);
    }
}

#[cfg(test)]
mod properties {
    //! Property tests required by spec section 4.4.

    use proptest::prelude::*;

    use super::*;
    use crate::test_fixtures::sample_cv;
    use crate::test_ids::id;
    use crate::{CustomSection, LanguageItem, Period, Skill};

    /// Shape of one section: kind (0..7) and child counts.
    type SectionShape = (u8, Vec<Vec<u8>>);

    fn shapes() -> impl Strategy<Value = Vec<SectionShape>> {
        prop::collection::vec(
            (
                0u8..7,
                prop::collection::vec(prop::collection::vec(0u8..3, 0..4), 0..4),
            ),
            0..5,
        )
    }

    /// Builds a CV whose IDs are numbered from 2 upwards in document order.
    fn build(shapes: &[SectionShape]) -> Cv {
        let mut next = 1;
        let mut fresh = || {
            next += 1;
            id(next)
        };
        let mut cv = sample_cv();
        cv.basics.links.clear();
        cv.summary = Some("Base summary".into());
        cv.sections = shapes
            .iter()
            .map(|(kind, children)| {
                let section_id = fresh();
                let mut entries = || -> Vec<Entry> {
                    children
                        .iter()
                        .map(|bullets| Entry {
                            id: fresh(),
                            title: "T".into(),
                            org: "O".into(),
                            location: None,
                            period: Period {
                                start: None,
                                end: None,
                                current: false,
                            },
                            bullets: bullets
                                .iter()
                                .map(|n| Bullet {
                                    id: fresh(),
                                    text: format!("bullet {n}"),
                                })
                                .collect(),
                            description: Some("D".into()),
                            url: None,
                            locked: false,
                        })
                        .collect()
                };
                let section = |entries| EntrySection {
                    id: section_id.clone(),
                    title_override: None,
                    entries,
                };
                match kind {
                    0 => Section::Experience(section(entries())),
                    1 => Section::Education(section(entries())),
                    2 => Section::Projects(section(entries())),
                    3 => Section::Certifications(section(entries())),
                    4 => Section::Custom(CustomSection {
                        id: section_id.clone(),
                        title: "Custom".into(),
                        entries: entries(),
                    }),
                    5 => Section::Skills(SkillsSection {
                        id: section_id.clone(),
                        title_override: None,
                        groups: children
                            .iter()
                            .map(|skills| SkillGroup {
                                id: fresh(),
                                label: None,
                                skills: skills
                                    .iter()
                                    .map(|n| Skill {
                                        id: fresh(),
                                        name: format!("skill {n}"),
                                    })
                                    .collect(),
                            })
                            .collect(),
                    }),
                    _ => Section::Languages(LanguagesSection {
                        id: section_id.clone(),
                        title_override: None,
                        items: children
                            .iter()
                            .map(|_| LanguageItem {
                                id: fresh(),
                                name: "Lang".into(),
                                level: None,
                            })
                            .collect(),
                    }),
                }
            })
            .collect();
        cv
    }

    /// IDs 1..=60 cover the CV's own IDs and some that do not exist.
    fn any_id() -> impl Strategy<Value = Id> {
        (1u32..=60).prop_map(id)
    }

    fn variants() -> impl Strategy<Value = Variant> {
        (
            prop::collection::btree_set(any_id(), 0..12),
            prop::collection::btree_set(any_id(), 0..6),
            prop::collection::btree_map(any_id(), prop::collection::vec(any_id(), 0..6), 0..6),
            prop::collection::btree_map(any_id(), "[a-z]{1,8}", 0..6),
            prop::option::of("[a-z]{1,8}"),
        )
            .prop_map(|(hidden, collapsed, order, text_overrides, summary)| {
                let mut v = Variant::empty(id(99_999), id(1), "V");
                v.hidden = hidden;
                v.collapsed = collapsed;
                v.order = order;
                v.text_overrides = text_overrides;
                v.summary_override = summary;
                v
            })
    }

    fn resolved_ids(r: &ResolvedCv) -> Vec<Id> {
        let mut ids = Vec::new();
        for section in &r.sections {
            ids.push(section.id().clone());
            match section {
                ResolvedSection::Experience(s)
                | ResolvedSection::Education(s)
                | ResolvedSection::Projects(s)
                | ResolvedSection::Certifications(s)
                | ResolvedSection::Custom(s) => {
                    for e in &s.entries {
                        ids.push(e.id.clone());
                        ids.extend(e.bullets.iter().map(|b| b.id.clone()));
                    }
                }
                ResolvedSection::Skills(s) => {
                    for g in &s.groups {
                        ids.push(g.id.clone());
                        ids.extend(g.skills.iter().map(|k| k.id.clone()));
                    }
                }
                ResolvedSection::Languages(s) => ids.extend(s.items.iter().map(|i| i.id.clone())),
            }
        }
        ids
    }

    /// Turns a resolved CV back into a base CV with the same visible content.
    fn as_base(cv: &Cv, r: &ResolvedCv) -> Cv {
        let entries = |s: &ResolvedEntrySection| -> Vec<Entry> {
            s.entries
                .iter()
                .map(|e| Entry {
                    id: e.id.clone(),
                    title: e.title.clone(),
                    org: e.org.clone(),
                    location: e.location.clone(),
                    period: e.period.clone(),
                    bullets: e.bullets.clone(),
                    description: e.description.clone(),
                    url: e.url.clone(),
                    locked: e.locked,
                })
                .collect()
        };
        let section = |s: &ResolvedEntrySection| EntrySection {
            id: s.id.clone(),
            title_override: s.heading.clone(),
            entries: entries(s),
        };
        Cv {
            summary: r.summary.clone(),
            sections: r
                .sections
                .iter()
                .map(|s| match s {
                    ResolvedSection::Experience(s) => Section::Experience(section(s)),
                    ResolvedSection::Education(s) => Section::Education(section(s)),
                    ResolvedSection::Projects(s) => Section::Projects(section(s)),
                    ResolvedSection::Certifications(s) => Section::Certifications(section(s)),
                    ResolvedSection::Custom(s) => Section::Custom(CustomSection {
                        id: s.id.clone(),
                        title: s.heading.clone().unwrap_or_default(),
                        entries: entries(s),
                    }),
                    ResolvedSection::Skills(s) => Section::Skills(SkillsSection {
                        id: s.id.clone(),
                        title_override: s.heading.clone(),
                        groups: s.groups.clone(),
                    }),
                    ResolvedSection::Languages(s) => Section::Languages(LanguagesSection {
                        id: s.id.clone(),
                        title_override: s.heading.clone(),
                        items: s.items.clone(),
                    }),
                })
                .collect(),
            ..cv.clone()
        }
    }

    fn clear_collapsed(mut r: ResolvedCv) -> ResolvedCv {
        for section in &mut r.sections {
            if let ResolvedSection::Experience(s)
            | ResolvedSection::Education(s)
            | ResolvedSection::Projects(s)
            | ResolvedSection::Certifications(s)
            | ResolvedSection::Custom(s) = section
            {
                for e in &mut s.entries {
                    e.collapsed = false;
                }
            }
        }
        r
    }

    /// IDs of base sections and skill groups that have no children.
    fn empty_in_base(cv: &Cv) -> BTreeSet<Id> {
        let mut empty = BTreeSet::new();
        for section in &cv.sections {
            let no_children = match section {
                Section::Skills(s) => {
                    for g in s.groups.iter().filter(|g| g.skills.is_empty()) {
                        empty.insert(g.id.clone());
                    }
                    s.groups.iter().all(|g| g.skills.is_empty())
                }
                Section::Languages(s) => s.items.is_empty(),
                _ => section_entries(section).next().is_none(),
            };
            if no_children {
                empty.insert(section.id().clone());
            }
        }
        empty
    }

    proptest! {
        /// (a) Without a variant the output is the base content: every ID of
        /// a non-empty section or group in base order, with base text.
        #[test]
        fn no_variant_is_base_content(shape in shapes()) {
            let cv = build(&shape);
            let r = cv.resolve(None);
            prop_assert_eq!(r.summary.as_deref(), Some("Base summary"));
            let empty = empty_in_base(&cv);
            let expected: Vec<Id> = cv.entity_ids().into_iter()
                .filter(|i| !empty.contains(i)).collect();
            prop_assert_eq!(resolved_ids(&r), expected);
            prop_assert_eq!(as_base(&cv, &r).resolve(None), r);
        }

        /// (b) Hiding then unhiding an ID is the identity.
        #[test]
        fn hide_then_unhide_is_identity(shape in shapes(), v in variants(), x in any_id()) {
            let cv = build(&shape);
            prop_assume!(!v.hidden.contains(&x));
            let mut toggled = v.clone();
            toggled.hidden.insert(x.clone());
            toggled.hidden.remove(&x);
            prop_assert_eq!(cv.resolve(Some(&toggled)), cv.resolve(Some(&v)));
        }

        /// (c) Every ID in the output exists in the base, at most once.
        #[test]
        fn output_ids_exist_in_base(shape in shapes(), v in variants()) {
            let cv = build(&shape);
            let base: BTreeSet<Id> = cv.entity_ids().into_iter().collect();
            let ids = resolved_ids(&cv.resolve(Some(&v)));
            let unique: BTreeSet<Id> = ids.iter().cloned().collect();
            prop_assert_eq!(unique.len(), ids.len());
            prop_assert!(unique.is_subset(&base));
        }

        /// (d) Re-resolving the output with an empty variant changes nothing
        /// (collapse is a rendering flag, so it is compared without it).
        #[test]
        fn resolve_is_idempotent(shape in shapes(), v in variants()) {
            let cv = build(&shape);
            let once = cv.resolve(Some(&v));
            let mut twice = as_base(&cv, &once).resolve(Some(&Variant::empty(id(99_999), id(1), "E")));
            twice.variant_id = once.variant_id.clone();
            prop_assert_eq!(twice, clear_collapsed(once));
        }

        /// Pruning never changes the resolved output, and is idempotent.
        #[test]
        fn prune_preserves_output(shape in shapes(), v in variants()) {
            let cv = build(&shape);
            let mut pruned = v.clone();
            pruned.prune(&cv);
            prop_assert_eq!(cv.resolve(Some(&pruned)), cv.resolve(Some(&v)));
            prop_assert_eq!(pruned.prune(&cv), Vec::<Id>::new());
        }
    }
}
