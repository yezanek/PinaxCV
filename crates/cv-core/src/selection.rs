//! Stage C of tailoring (spec section 11.6): deterministic hide, collapse
//! and order suggestions from relevance scores. No LLM.
//!
//! [`plan_selection`] only proposes; every suggestion is `Pending` until the
//! user accepts it. Unscored items get no suggestion.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::fit::RelevanceScores;
use crate::{
    Id, IdSource, ResolvedCv, ResolvedEntry, ResolvedSection, Suggestion, SuggestionAction,
    SuggestionKind, SuggestionSource, SuggestionStatus, SuggestionTarget,
};

/// Tailoring thresholds (spec section 11.6). Scores are 0 to 100.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct TailorOptions {
    /// 1 or 2: run fit-to-length after selection.
    pub page_target: Option<u8>,
    /// Entries below this keep all their bullets: trimming and reordering
    /// bullets is only worth it in entries that stay.
    pub min_entry_score: u8,
    /// The most recent Experience entries never hidden or collapsed.
    pub keep_recent_entries: u8,
    pub collapse_below: u8,
    pub hide_below: u8,
    pub max_bullets_per_entry: u8,
    /// Suggest ordering Experience by relevance instead of chronology.
    pub reorder_entries_by_relevance: bool,
}

impl Default for TailorOptions {
    fn default() -> Self {
        Self {
            page_target: None,
            min_entry_score: 35,
            keep_recent_entries: 1,
            collapse_below: 35,
            hide_below: 20,
            max_bullets_per_entry: 6,
            reorder_entries_by_relevance: false,
        }
    }
}

/// The model's one-line reason per entry ID, shown with the suggestion.
pub type Reasons = BTreeMap<Id, String>;

/// Proposes hide, collapse and order suggestions for `cv` from `scores`.
pub fn plan_selection(
    cv: &ResolvedCv,
    scores: &RelevanceScores,
    reasons: &Reasons,
    options: &TailorOptions,
    ids: &mut dyn IdSource,
) -> Vec<Suggestion> {
    let recent = recent_experience(cv, usize::from(options.keep_recent_entries));
    let mut out = Vec::new();
    for section in &cv.sections {
        let (entries, is_experience, is_education) = match section {
            ResolvedSection::Experience(s) => (&s.entries, true, false),
            ResolvedSection::Education(s) => (&s.entries, false, true),
            ResolvedSection::Projects(s)
            | ResolvedSection::Certifications(s)
            | ResolvedSection::Custom(s) => (&s.entries, false, false),
            ResolvedSection::Skills(s) => {
                for group in &s.groups {
                    let current: Vec<Id> = group.skills.iter().map(|k| k.id.clone()).collect();
                    if let Some(order) = by_score(&current, &scores.skills) {
                        out.push(suggestion(
                            ids,
                            SuggestionKind::SkillOrder,
                            SuggestionTarget::Skills,
                            Some(SuggestionAction::Reorder { order }),
                            "Most relevant skills first".into(),
                        ));
                    }
                }
                continue;
            }
            ResolvedSection::Languages(_) => continue,
        };
        for e in entries {
            let Some(&score) = scores.entries.get(&e.id) else {
                continue;
            };
            let why = |tag: String| match reasons.get(&e.id) {
                Some(r) if !r.trim().is_empty() => format!("{tag}. {}", r.trim()),
                _ => tag,
            };
            let protected = e.locked || recent.contains(&&e.id);
            if !protected && !e.collapsed && score < options.collapse_below {
                let hide = score < options.hide_below && !is_education;
                let (kind, action, tag) = if hide {
                    (
                        SuggestionKind::Hide,
                        SuggestionAction::Hide,
                        "Low relevance",
                    )
                } else {
                    (
                        SuggestionKind::Collapse,
                        SuggestionAction::Collapse,
                        "Limited relevance",
                    )
                };
                out.push(suggestion(
                    ids,
                    kind,
                    SuggestionTarget::Entry { id: e.id.clone() },
                    Some(action),
                    why(format!("{tag} (score {score})")),
                ));
                continue;
            }
            if score >= options.min_entry_score {
                bullets(e, scores, options, ids, &mut out);
            }
        }
        if is_experience && options.reorder_entries_by_relevance {
            let current: Vec<Id> = entries.iter().map(|e| e.id.clone()).collect();
            if let Some(order) = by_score(&current, &scores.entries) {
                out.push(suggestion(
                    ids,
                    SuggestionKind::Reorder,
                    SuggestionTarget::Section {
                        id: section.id().clone(),
                    },
                    Some(SuggestionAction::Reorder { order }),
                    "Most relevant experience first".into(),
                ));
            }
        }
    }
    out
}

/// Bullet order and trimming inside an entry that stays.
fn bullets(
    e: &ResolvedEntry,
    scores: &RelevanceScores,
    options: &TailorOptions,
    ids: &mut dyn IdSource,
    out: &mut Vec<Suggestion>,
) {
    let current: Vec<Id> = e.bullets.iter().map(|b| b.id.clone()).collect();
    let ranked = by_score(&current, &scores.bullets);
    if let Some(order) = &ranked {
        out.push(suggestion(
            ids,
            SuggestionKind::Reorder,
            SuggestionTarget::Entry { id: e.id.clone() },
            Some(SuggestionAction::Reorder {
                order: order.clone(),
            }),
            "Most relevant bullets first".into(),
        ));
    }
    let ranked = ranked.unwrap_or(current);
    let keep = usize::from(options.max_bullets_per_entry);
    for id in ranked.iter().skip(keep) {
        let Some(&score) = scores.bullets.get(id) else {
            continue;
        };
        let before = e
            .bullets
            .iter()
            .find(|b| &b.id == id)
            .map(|b| b.text.clone());
        let mut s = suggestion(
            ids,
            SuggestionKind::Hide,
            SuggestionTarget::Bullet { id: id.clone() },
            Some(SuggestionAction::Hide),
            format!("More than {keep} bullets; one of the least relevant (score {score})"),
        );
        s.before = before;
        out.push(s);
    }
}

/// `current` sorted by score, highest first, ties and unscored items in
/// their current order after the scored ones; `None` when nothing moves.
fn by_score(current: &[Id], scores: &BTreeMap<Id, u8>) -> Option<Vec<Id>> {
    let mut ranked: Vec<(usize, &Id)> = current.iter().enumerate().collect();
    ranked.sort_by_key(|(i, id)| {
        (
            std::cmp::Reverse(scores.get(*id).map_or(-1, |&s| i16::from(s))),
            *i,
        )
    });
    let order: Vec<Id> = ranked.into_iter().map(|(_, id)| id.clone()).collect();
    (order != current).then_some(order)
}

/// The `n` most recent Experience entries: current first, then by start.
fn recent_experience(cv: &ResolvedCv, n: usize) -> Vec<&Id> {
    let mut all: Vec<&ResolvedEntry> = cv
        .sections
        .iter()
        .filter_map(|s| match s {
            ResolvedSection::Experience(s) => Some(s.entries.iter()),
            _ => None,
        })
        .flatten()
        .collect();
    // Stable: document order breaks ties.
    all.sort_by_key(|e| std::cmp::Reverse((e.period.current, e.period.end, e.period.start)));
    all.into_iter().take(n).map(|e| &e.id).collect()
}

fn suggestion(
    ids: &mut dyn IdSource,
    kind: SuggestionKind,
    target: SuggestionTarget,
    action: Option<SuggestionAction>,
    reason: String,
) -> Suggestion {
    Suggestion {
        id: ids.next_id(),
        kind,
        target,
        before: None,
        after: None,
        action,
        reason,
        confidence: 0.7,
        warnings: Vec::new(),
        status: SuggestionStatus::Pending,
        source: SuggestionSource::Tailor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_ids::id;
    use crate::{
        Bullet, Cv, Entry, EntrySection, PartialDate, Period, Section, Skill, SkillGroup,
        SkillsSection,
    };

    struct Ids(u32);

    impl IdSource for Ids {
        fn next_id(&mut self) -> Id {
            self.0 += 1;
            id(1000 + self.0)
        }
    }

    fn entry(n: u32, start: u16, bullets: u32) -> Entry {
        Entry {
            id: id(n),
            title: format!("Role {n}"),
            org: "Org".into(),
            location: None,
            period: Period {
                start: Some(PartialDate {
                    year: start,
                    month: None,
                }),
                end: Some(PartialDate {
                    year: start + 1,
                    month: None,
                }),
                current: false,
            },
            bullets: (1..=bullets)
                .map(|b| Bullet {
                    id: id(n * 10 + b),
                    text: format!("Bullet {b}"),
                })
                .collect(),
            description: None,
            url: None,
            locked: false,
        }
    }

    /// Experience 1 (2022), 2 (2018), 3 (2015, locked); Education 4; skills 7, 8, 9.
    fn cv() -> Cv {
        let mut cv = crate::test_fixtures::sample_cv();
        let mut locked = entry(3, 2015, 1);
        locked.locked = true;
        cv.sections = vec![
            Section::Experience(EntrySection {
                id: id(100),
                title_override: None,
                entries: vec![entry(1, 2022, 8), entry(2, 2018, 2), locked],
            }),
            Section::Education(EntrySection {
                id: id(101),
                title_override: None,
                entries: vec![entry(4, 2010, 0)],
            }),
            Section::Skills(SkillsSection {
                id: id(102),
                title_override: None,
                groups: vec![SkillGroup {
                    id: id(103),
                    label: None,
                    skills: [7, 8, 9]
                        .map(|n| Skill {
                            id: id(n),
                            name: format!("Skill {n}"),
                        })
                        .into(),
                }],
            }),
        ];
        cv
    }

    fn scores(
        entries: &[(u32, u8)],
        bullets: &[(u32, u8)],
        skills: &[(u32, u8)],
    ) -> RelevanceScores {
        let map = |v: &[(u32, u8)]| v.iter().map(|&(n, s)| (id(n), s)).collect();
        RelevanceScores {
            entries: map(entries),
            bullets: map(bullets),
            skills: map(skills),
        }
    }

    fn plan(scores: &RelevanceScores, options: &TailorOptions) -> Vec<Suggestion> {
        let mut reasons = Reasons::new();
        reasons.insert(id(2), "No backend work.".into());
        plan_selection(&cv().resolve(None), scores, &reasons, options, &mut Ids(0))
    }

    fn kinds(s: &[Suggestion]) -> Vec<(SuggestionKind, SuggestionTarget)> {
        s.iter().map(|s| (s.kind, s.target.clone())).collect()
    }

    #[test]
    fn low_entries_are_hidden_or_collapsed_but_protected_ones_stay() {
        // Entry 1 is the most recent; 3 is locked; 4 is education.
        let s = plan(
            &scores(&[(1, 5), (2, 10), (3, 0), (4, 0)], &[], &[]),
            &TailorOptions::default(),
        );
        assert_eq!(
            kinds(&s),
            [
                (SuggestionKind::Hide, SuggestionTarget::Entry { id: id(2) }),
                (
                    SuggestionKind::Collapse,
                    SuggestionTarget::Entry { id: id(4) }
                ),
            ]
        );
        assert_eq!(s[0].reason, "Low relevance (score 10). No backend work.");
        assert!(
            s.iter()
                .all(|s| s.status == SuggestionStatus::Pending
                    && s.source == SuggestionSource::Tailor)
        );
    }

    #[test]
    fn middle_scores_collapse() {
        let s = plan(&scores(&[(2, 25)], &[], &[]), &TailorOptions::default());
        assert_eq!(
            kinds(&s),
            [(
                SuggestionKind::Collapse,
                SuggestionTarget::Entry { id: id(2) }
            )]
        );
    }

    #[test]
    fn bullets_are_ordered_and_trimmed() {
        // Entry 1 has 8 bullets (ids 11..=18); the last one scores highest.
        let bullet_scores: Vec<(u32, u8)> = (11..=18)
            .map(|b| (b, if b == 18 { 99 } else { 50 }))
            .collect();
        let s = plan(
            &scores(&[(1, 90)], &bullet_scores, &[]),
            &TailorOptions::default(),
        );
        let SuggestionAction::Reorder { order } = s[0].action.clone().expect("action") else {
            panic!("not a reorder");
        };
        assert_eq!(order[0], id(18));
        assert_eq!(order[1], id(11));
        let hidden: Vec<_> = s[1..]
            .iter()
            .map(|s| (s.kind, s.target.clone(), s.before.clone()))
            .collect();
        assert_eq!(
            hidden,
            [16, 17].map(|b| (
                SuggestionKind::Hide,
                SuggestionTarget::Bullet { id: id(b) },
                Some(format!("Bullet {}", b - 10))
            ))
        );
    }

    #[test]
    fn skills_and_opt_in_entry_order() {
        let sc = scores(&[(1, 40), (2, 90), (3, 60)], &[], &[(7, 10), (8, 80)]);
        let s = plan(&sc, &TailorOptions::default());
        assert_eq!(
            kinds(&s),
            [(SuggestionKind::SkillOrder, SuggestionTarget::Skills)]
        );
        assert_eq!(
            s[0].action,
            Some(SuggestionAction::Reorder {
                order: vec![id(8), id(7), id(9)]
            })
        );
        let s = plan(
            &sc,
            &TailorOptions {
                reorder_entries_by_relevance: true,
                ..TailorOptions::default()
            },
        );
        assert!(
            s.iter()
                .any(|s| s.target == SuggestionTarget::Section { id: id(100) }
                    && s.action
                        == Some(SuggestionAction::Reorder {
                            order: vec![id(2), id(3), id(1)]
                        }))
        );
    }

    #[test]
    fn unscored_items_get_nothing() {
        assert!(plan(&RelevanceScores::default(), &TailorOptions::default()).is_empty());
    }

    #[test]
    fn options_deserialize_with_defaults() {
        let o: TailorOptions = serde_json::from_str(r#"{"page_target": 1}"#).expect("json");
        assert_eq!(o.page_target, Some(1));
        assert_eq!(o.hide_below, 20);
    }
}
