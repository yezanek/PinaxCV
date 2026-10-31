//! Merging an import into an existing CV (spec section 9.5). The user decides
//! every conflict: each header field the two sides disagree on, and each
//! imported entry. Nothing in the base is deleted unless the user picks
//! `Replace` for that entry.

use std::collections::{HashMap, HashSet};

use cv_core::{
    Cv, Entry, EntrySection, Id, LanguagesSection, PersonalDetails, PersonalField, Section,
    SkillsSection,
};

use crate::partial::CvPartial;

/// A header field that can conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Field {
    FullName,
    Headline,
    Email,
    Phone,
    Location,
    Summary,
    /// One of the optional personal details (ADR-088).
    Personal(PersonalField),
}

/// A header field both sides fill with different values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldConflict {
    pub field: Field,
    pub mine: String,
    pub imported: String,
}

/// What to do with one imported entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryDecision {
    Skip,
    AddNew,
    /// Overwrite the content of this base entry, keeping its ID.
    Replace(Id),
}

/// The user's choices. Missing field choices keep the base value; missing
/// entry decisions add the entry as new.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergePlan {
    pub use_imported: Vec<Field>,
    pub entries: HashMap<Id, EntryDecision>,
}

/// A blank or whitespace-only value counts as empty.
fn filled(s: Option<&str>) -> Option<String> {
    s.filter(|v| !v.trim().is_empty()).map(str::to_owned)
}

/// A personal detail as text: lists joined with ", ", the birth date in
/// ISO 8601, the address with its line breaks.
fn personal_text(p: &PersonalDetails, f: PersonalField) -> Option<String> {
    let list = |items: &[String]| {
        let joined = items
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        filled(Some(&joined))
    };
    match f {
        PersonalField::Nationality => list(&p.nationality),
        PersonalField::BirthDate => p.birth_date.map(|d| d.iso()),
        PersonalField::BirthPlace => filled(p.birth_place.as_deref()),
        PersonalField::Gender => filled(p.gender.as_deref()),
        PersonalField::MaritalStatus => filled(p.marital_status.as_deref()),
        PersonalField::Address => filled(p.address.as_deref()),
        PersonalField::DrivingLicence => list(&p.driving_licence),
        PersonalField::WorkPermit => filled(p.work_permit.as_deref()),
    }
}

fn base_value(cv: &Cv, f: Field) -> Option<String> {
    let b = &cv.basics;
    match f {
        Field::FullName => filled(Some(b.full_name.as_str())),
        Field::Headline => filled(b.headline.as_deref()),
        Field::Email => filled(b.email.as_deref()),
        Field::Phone => filled(b.phone.as_deref()),
        Field::Location => filled(b.location.as_deref()),
        Field::Summary => filled(cv.summary.as_deref()),
        Field::Personal(p) => personal_text(&b.personal, p),
    }
}

fn imported_value(p: &CvPartial, f: Field) -> Option<String> {
    match f {
        Field::FullName => filled(p.full_name.as_deref()),
        Field::Headline => filled(p.headline.as_deref()),
        Field::Email => filled(p.email.as_deref()),
        Field::Phone => filled(p.phone.as_deref()),
        Field::Location => filled(p.location.as_deref()),
        Field::Summary => filled(p.summary.as_deref()),
        Field::Personal(f) => personal_text(&p.personal, f),
    }
}

/// Takes `f` from the import. Personal details are copied as they are, so
/// lists and the birth date keep their shape; their show switches are left
/// alone (ADR-088).
fn set_value(cv: &mut Cv, f: Field, incoming: &CvPartial, v: String) {
    let b = &mut cv.basics;
    match f {
        Field::FullName => b.full_name = v,
        Field::Headline => b.headline = Some(v),
        Field::Email => b.email = Some(v),
        Field::Phone => b.phone = Some(v),
        Field::Location => b.location = Some(v),
        Field::Summary => cv.summary = Some(v),
        Field::Personal(p) => {
            let from = &incoming.personal;
            let to = &mut b.personal;
            match p {
                PersonalField::Nationality => to.nationality.clone_from(&from.nationality),
                PersonalField::BirthDate => to.birth_date = from.birth_date,
                PersonalField::BirthPlace => to.birth_place.clone_from(&from.birth_place),
                PersonalField::Gender => to.gender.clone_from(&from.gender),
                PersonalField::MaritalStatus => to.marital_status.clone_from(&from.marital_status),
                PersonalField::Address => to.address.clone_from(&from.address),
                PersonalField::DrivingLicence => {
                    to.driving_licence.clone_from(&from.driving_licence);
                }
                PersonalField::WorkPermit => to.work_permit.clone_from(&from.work_permit),
            }
        }
    }
}

const FIELDS: [Field; 14] = [
    Field::FullName,
    Field::Headline,
    Field::Email,
    Field::Phone,
    Field::Location,
    Field::Personal(PersonalField::Nationality),
    Field::Personal(PersonalField::BirthDate),
    Field::Personal(PersonalField::BirthPlace),
    Field::Personal(PersonalField::Gender),
    Field::Personal(PersonalField::MaritalStatus),
    Field::Personal(PersonalField::Address),
    Field::Personal(PersonalField::DrivingLicence),
    Field::Personal(PersonalField::WorkPermit),
    Field::Summary,
];

/// Header fields that need the user's choice. Fields empty on one side are
/// not conflicts: the filled value is kept.
pub fn field_conflicts(base: &Cv, incoming: &CvPartial) -> Vec<FieldConflict> {
    FIELDS
        .iter()
        .filter_map(|&field| {
            let mine = base_value(base, field)?;
            let imported = imported_value(incoming, field)?;
            (norm(&mine) != norm(&imported)).then_some(FieldConflict {
                field,
                mine,
                imported,
            })
        })
        .collect()
}

fn norm(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn match_key(e: &Entry) -> (String, String, Option<u16>) {
    (norm(&e.org), norm(&e.title), e.period.start.map(|d| d.year))
}

fn entry_sections(sections: &[Section]) -> impl Iterator<Item = (u8, &EntrySection)> {
    sections.iter().filter_map(|s| match s {
        Section::Experience(x) => Some((0, x)),
        Section::Education(x) => Some((1, x)),
        Section::Projects(x) => Some((2, x)),
        Section::Certifications(x) => Some((3, x)),
        _ => None,
    })
}

/// Suggested matches, imported entry ID to base entry ID, by normalized
/// organisation, title and start year within the same kind of section.
pub fn suggest_matches(base: &Cv, incoming: &CvPartial) -> HashMap<Id, Id> {
    let mut known = HashMap::new();
    for (kind, s) in entry_sections(&base.sections) {
        for e in &s.entries {
            known
                .entry((kind, match_key(e)))
                .or_insert_with(|| e.id.clone());
        }
    }
    let mut out = HashMap::new();
    for (kind, s) in entry_sections(&incoming.sections) {
        for e in &s.entries {
            if let Some(b) = known.get(&(kind, match_key(e))) {
                out.insert(e.id.clone(), b.clone());
            }
        }
    }
    out
}

/// Applies `incoming` to `base` following `plan`. The caller snapshots the
/// base first (spec section 9.5).
pub fn merge(base: &mut Cv, incoming: CvPartial, plan: &MergePlan) {
    for &field in &FIELDS {
        if let Some(v) = imported_value(&incoming, field)
            && (base_value(base, field).is_none() || plan.use_imported.contains(&field))
        {
            set_value(base, field, &incoming, v);
        }
    }
    for link in incoming.links {
        if !base.basics.links.iter().any(|l| l.url == link.url) {
            base.basics.links.push(link);
        }
    }
    for section in incoming.sections {
        let (kind, x, make): (u8, EntrySection, fn(EntrySection) -> Section) = match section {
            Section::Experience(x) => (0, x, Section::Experience),
            Section::Education(x) => (1, x, Section::Education),
            Section::Projects(x) => (2, x, Section::Projects),
            Section::Certifications(x) => (3, x, Section::Certifications),
            Section::Skills(s) => {
                merge_skills(base, s);
                continue;
            }
            Section::Languages(l) => {
                merge_languages(base, l);
                continue;
            }
            custom @ Section::Custom(_) => {
                base.sections.push(custom);
                continue;
            }
        };
        let mut added = Vec::new();
        for e in x.entries {
            match plan.entries.get(&e.id).unwrap_or(&EntryDecision::AddNew) {
                EntryDecision::Skip => {}
                EntryDecision::AddNew => added.push(e),
                EntryDecision::Replace(id) => match entry_mut(base, id) {
                    Some(target) => {
                        let keep = target.id.clone();
                        *target = Entry { id: keep, ..e };
                    }
                    // The base entry is gone: keep the import rather than lose it.
                    None => added.push(e),
                },
            }
        }
        if added.is_empty() {
            continue;
        }
        let existing = base.sections.iter_mut().find_map(|s| match (kind, s) {
            (0, Section::Experience(t))
            | (1, Section::Education(t))
            | (2, Section::Projects(t))
            | (3, Section::Certifications(t)) => Some(t),
            _ => None,
        });
        match existing {
            Some(t) => t.entries.extend(added),
            None => base.sections.push(make(EntrySection {
                id: x.id,
                title_override: x.title_override,
                entries: added,
            })),
        }
    }
}

fn entry_mut<'a>(cv: &'a mut Cv, id: &Id) -> Option<&'a mut Entry> {
    cv.sections.iter_mut().find_map(|s| match s {
        Section::Experience(x)
        | Section::Education(x)
        | Section::Projects(x)
        | Section::Certifications(x) => x.entries.iter_mut().find(|e| &e.id == id),
        _ => None,
    })
}

/// Adds skills the base doesn't have yet (by case-insensitive name), into the
/// group with the same label, or as a new group. Incoming duplicates are
/// dropped first.
fn merge_skills(base: &mut Cv, mut incoming: SkillsSection) {
    let mut seen = HashSet::new();
    for group in &mut incoming.groups {
        group.skills.retain(|k| seen.insert(norm(&k.name)));
    }
    incoming.groups.retain(|g| !g.skills.is_empty());
    let Some(target) = base.sections.iter_mut().find_map(|s| match s {
        Section::Skills(x) => Some(x),
        _ => None,
    }) else {
        base.sections.push(Section::Skills(incoming));
        return;
    };
    for mut group in incoming.groups {
        group.skills.retain(|k| {
            !target
                .groups
                .iter()
                .flat_map(|g| &g.skills)
                .any(|b| norm(&b.name) == norm(&k.name))
        });
        if group.skills.is_empty() {
            continue;
        }
        let label = group.label.as_deref().map(norm);
        match target
            .groups
            .iter_mut()
            .find(|g| g.label.as_deref().map(norm) == label)
        {
            Some(g) => g.skills.extend(group.skills),
            None => target.groups.push(group),
        }
    }
}

/// Adds languages the base doesn't list, and fills in missing levels.
fn merge_languages(base: &mut Cv, incoming: LanguagesSection) {
    let Some(target) = base.sections.iter_mut().find_map(|s| match s {
        Section::Languages(x) => Some(x),
        _ => None,
    }) else {
        base.sections.push(Section::Languages(incoming));
        return;
    };
    for item in incoming.items {
        match target
            .items
            .iter_mut()
            .find(|b| norm(&b.name) == norm(&item.name))
        {
            Some(b) => {
                if b.level.is_none() {
                    b.level = item.level;
                }
            }
            None => target.items.push(item),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cv_core::{Bullet, LanguageItem, PartialDate, Period, Skill, SkillGroup};

    fn typical() -> Cv {
        serde_json::from_str(include_str!("../../../testdata/cvs/typical.json")).expect("fixture")
    }

    fn id(n: u8) -> Id {
        Id::parse(&format!("01JAAAAAAAAAAAAAAAAAAAAA{n:02}")).expect("id")
    }

    fn entry(n: u8, title: &str, org: &str, year: u16) -> Entry {
        Entry {
            id: id(n),
            title: title.into(),
            org: org.into(),
            location: None,
            period: Period {
                start: Some(PartialDate { year, month: None }),
                end: None,
                current: false,
            },
            bullets: vec![Bullet {
                id: id(n + 50),
                text: "Imported bullet.".into(),
            }],
            description: None,
            url: None,
            locked: false,
        }
    }

    fn incoming() -> CvPartial {
        CvPartial {
            full_name: Some("JANE  doe".into()),
            email: Some("new@example.com".into()),
            headline: Some("Staff Engineer".into()),
            sections: vec![
                Section::Experience(EntrySection {
                    id: id(1),
                    title_override: None,
                    entries: vec![
                        entry(2, " senior backend engineer", "ACME GmbH", 2021),
                        entry(3, "Intern", "Elsewhere", 2010),
                    ],
                }),
                Section::Skills(SkillsSection {
                    id: id(4),
                    title_override: None,
                    groups: vec![SkillGroup {
                        id: id(5),
                        label: Some("Brand new".into()),
                        skills: vec![Skill {
                            id: id(6),
                            name: "Zig".into(),
                        }],
                    }],
                }),
                Section::Languages(LanguagesSection {
                    id: id(7),
                    title_override: None,
                    items: vec![LanguageItem {
                        id: id(8),
                        name: "Klingon".into(),
                        level: Some("A1".into()),
                    }],
                }),
            ],
            ..CvPartial::default()
        }
    }

    #[test]
    fn personal_details_merge_per_field_and_stay_hidden() {
        let mut base = typical();
        base.basics.personal.nationality = vec!["Italian".into()];
        base.basics.personal.gender = Some("Female".into());
        let mut partial = CvPartial::default();
        partial.personal.nationality = vec!["Italian".into(), "German".into()];
        partial.personal.gender = Some(" female ".into());
        partial.personal.birth_date = cv_core::CalendarDate::parse("1990-03-12");

        let conflicts = field_conflicts(&base, &partial);
        assert_eq!(
            conflicts,
            [FieldConflict {
                field: Field::Personal(PersonalField::Nationality),
                mine: "Italian".into(),
                imported: "Italian, German".into(),
            }]
        );

        let mut kept = base.clone();
        merge(&mut kept, partial.clone(), &MergePlan::default());
        assert_eq!(kept.basics.personal.nationality, ["Italian"]);
        // Empty in the base: filled from the import.
        assert_eq!(kept.basics.personal.birth_date, partial.personal.birth_date);
        assert_eq!(kept.basics.personal.gender.as_deref(), Some("Female"));

        let plan = MergePlan {
            use_imported: vec![Field::Personal(PersonalField::Nationality)],
            ..MergePlan::default()
        };
        merge(&mut base, partial, &plan);
        assert_eq!(base.basics.personal.nationality, ["Italian", "German"]);
        assert!(base.basics.show_personal.is_empty(), "never switched on");
    }

    #[test]
    fn conflicts_ignore_case_and_spacing_and_empty_sides() {
        let base = typical();
        let fields: Vec<_> = field_conflicts(&base, &incoming())
            .into_iter()
            .map(|c| c.field)
            .collect();
        // Name differs only by case and spacing; headline may be empty in the base.
        assert!(!fields.contains(&Field::FullName));
        assert!(fields.contains(&Field::Email));
        assert!(!fields.contains(&Field::Phone));
    }

    #[test]
    fn suggests_the_matching_base_entry() {
        let base = typical();
        let matches = suggest_matches(&base, &incoming());
        let first = match &base.sections[0] {
            Section::Experience(x) => x.entries[0].id.clone(),
            _ => panic!("experience first"),
        };
        assert_eq!(matches.get(&id(2)), Some(&first));
        assert!(!matches.contains_key(&id(3)));
    }

    #[test]
    fn keeps_mine_by_default_and_adds_new_entries() {
        let mut base = typical();
        let before = base.clone();
        let matches = suggest_matches(&base, &incoming());
        let mut plan = MergePlan::default();
        for (imported, _) in matches {
            plan.entries.insert(imported, EntryDecision::Skip);
        }
        merge(&mut base, incoming(), &plan);
        assert_eq!(base.basics.email, before.basics.email);
        assert_eq!(base.basics.full_name, before.basics.full_name);
        let (Section::Experience(after), Section::Experience(old)) =
            (&base.sections[0], &before.sections[0])
        else {
            panic!("experience")
        };
        assert_eq!(after.entries.len(), old.entries.len() + 1);
        assert!(after.entries.iter().any(|e| e.title == "Intern"));
        // Nothing from the base was removed.
        for e in &old.entries {
            assert!(after.entries.iter().any(|a| a == e));
        }
    }

    #[test]
    fn replace_keeps_the_base_id_and_use_imported_takes_the_value() {
        let mut base = typical();
        let target = match &base.sections[0] {
            Section::Experience(x) => x.entries[0].id.clone(),
            _ => panic!("experience"),
        };
        let mut plan = MergePlan {
            use_imported: vec![Field::Email],
            ..MergePlan::default()
        };
        plan.entries
            .insert(id(2), EntryDecision::Replace(target.clone()));
        plan.entries.insert(id(3), EntryDecision::Skip);
        merge(&mut base, incoming(), &plan);
        assert_eq!(base.basics.email.as_deref(), Some("new@example.com"));
        let Section::Experience(x) = &base.sections[0] else {
            panic!("experience")
        };
        assert_eq!(x.entries[0].id, target);
        assert_eq!(x.entries[0].bullets[0].text, "Imported bullet.");
        assert!(!x.entries.iter().any(|e| e.title == "Intern"));
    }

    #[test]
    fn skills_and_languages_are_unioned() {
        let mut base = typical();
        merge(&mut base, incoming(), &MergePlan::default());
        let skills = base.sections.iter().find_map(|s| match s {
            Section::Skills(x) => Some(x),
            _ => None,
        });
        let skills = skills.expect("skills");
        assert!(
            skills
                .groups
                .iter()
                .flat_map(|g| &g.skills)
                .any(|k| k.name == "Zig")
        );
        let langs = base.sections.iter().find_map(|s| match s {
            Section::Languages(x) => Some(x),
            _ => None,
        });
        assert!(
            langs
                .expect("languages")
                .items
                .iter()
                .any(|l| l.name == "Klingon")
        );
        // Merging the same import again adds no duplicate skills.
        let count = |cv: &Cv| {
            cv.sections
                .iter()
                .filter_map(|s| match s {
                    Section::Skills(x) => {
                        Some(x.groups.iter().map(|g| g.skills.len()).sum::<usize>())
                    }
                    _ => None,
                })
                .sum::<usize>()
        };
        let once = count(&base);
        let mut again = incoming();
        again.sections.retain(|s| matches!(s, Section::Skills(_)));
        merge(&mut base, again, &MergePlan::default());
        assert_eq!(count(&base), once);
    }

    #[test]
    fn empty_base_fields_are_filled_without_asking() {
        let mut base = typical();
        base.basics.headline = None;
        merge(&mut base, incoming(), &MergePlan::default());
        assert_eq!(base.basics.headline.as_deref(), Some("Staff Engineer"));
    }

    #[test]
    fn blank_values_count_as_empty_on_both_sides() {
        let mut base = typical();
        base.basics.headline = Some("  ".into());
        let mut inc = incoming();
        inc.email = Some(" ".into());
        let email = base.basics.email.clone();
        assert!(
            !field_conflicts(&base, &inc)
                .iter()
                .any(|c| c.field == Field::Headline)
        );
        merge(&mut base, inc, &MergePlan::default());
        assert_eq!(base.basics.headline.as_deref(), Some("Staff Engineer"));
        assert_eq!(base.basics.email, email);
    }

    #[test]
    fn duplicate_incoming_skills_are_added_once() {
        let skill = |n: u8, name: &str| Skill {
            id: id(n),
            name: name.into(),
        };
        let mut base = typical();
        base.sections.retain(|s| !matches!(s, Section::Skills(_)));
        let section = SkillsSection {
            id: id(40),
            title_override: None,
            groups: vec![
                SkillGroup {
                    id: id(41),
                    label: None,
                    skills: vec![skill(42, "Zig"), skill(43, " zig ")],
                },
                SkillGroup {
                    id: id(44),
                    label: Some("Other".into()),
                    skills: vec![skill(45, "ZIG")],
                },
            ],
        };
        merge_skills(&mut base, section);
        let Some(Section::Skills(x)) = base.sections.last() else {
            panic!("skills")
        };
        assert_eq!(x.groups.len(), 1);
        assert_eq!(x.groups[0].skills.len(), 1);
    }
}
