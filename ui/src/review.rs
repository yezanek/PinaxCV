//! The suggestion review list (spec section 12.6, M7a): tailoring and
//! review suggestions grouped by what they do, each with Accept and Reject,
//! and the bulk actions. Nothing is applied without the user's approval;
//! fit-to-length cuts keep their own list in the fit panel.

use std::collections::BTreeMap;

use cv_core::{
    Cv, Id, Section, StoredVariant, Suggestion, SuggestionAction, SuggestionKind, SuggestionSource,
    SuggestionStatus, SuggestionTarget, VerificationKind,
};
use leptos::prelude::*;

use crate::fit::EditFn;
use crate::i18n::{fill, t};
use crate::variants::{entry_label, section_label, short};

/// The sources this list reviews.
pub const SOURCES: [SuggestionSource; 2] = [SuggestionSource::Tailor, SuggestionSource::Review];

/// The groups, in the order they are listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Selection,
    Order,
    Rewrites,
    Summary,
    SkillsOrder,
}

impl Group {
    fn heading(self) -> &'static str {
        let s = t();
        match self {
            Group::Selection => s.group_selection,
            Group::Order => s.group_order,
            Group::Rewrites => s.group_rewrites,
            Group::Summary => s.group_summary,
            Group::SkillsOrder => s.group_skills_order,
        }
    }
}

/// The group `s` is listed under.
pub fn group_of(s: &Suggestion) -> Group {
    match s.kind {
        SuggestionKind::Hide | SuggestionKind::Collapse | SuggestionKind::LengthCut => {
            Group::Selection
        }
        SuggestionKind::Reorder => Group::Order,
        SuggestionKind::Rewrite => Group::Rewrites,
        SuggestionKind::Summary => Group::Summary,
        SuggestionKind::SkillOrder => Group::SkillsOrder,
    }
}

/// The pending suggestions this list reviews, by group.
pub fn pending(suggestions: &[Suggestion]) -> BTreeMap<Group, Vec<Suggestion>> {
    let mut groups: BTreeMap<Group, Vec<Suggestion>> = BTreeMap::new();
    for s in suggestions
        .iter()
        .filter(|s| s.status == SuggestionStatus::Pending && SOURCES.contains(&s.source))
    {
        groups.entry(group_of(s)).or_default().push(s.clone());
    }
    groups
}

/// One-line labels for every section, entry, bullet, skill group and skill.
pub fn labels(cv: &Cv) -> BTreeMap<Id, String> {
    let mut out = BTreeMap::new();
    for section in &cv.sections {
        out.insert(section.id().clone(), section_label(section));
        let entries = match section {
            Section::Experience(s)
            | Section::Education(s)
            | Section::Projects(s)
            | Section::Certifications(s) => s.entries.as_slice(),
            Section::Custom(c) => c.entries.as_slice(),
            Section::Skills(s) => {
                for g in &s.groups {
                    let names: Vec<&str> = g.skills.iter().map(|k| k.name.as_str()).collect();
                    let label = g.label.clone().filter(|l| !l.trim().is_empty());
                    out.insert(
                        g.id.clone(),
                        label.unwrap_or_else(|| short(&names.join(", "))),
                    );
                    for k in &g.skills {
                        out.insert(k.id.clone(), k.name.clone());
                    }
                }
                continue;
            }
            Section::Languages(_) => continue,
        };
        for e in entries {
            out.insert(e.id.clone(), entry_label(e));
            for b in &e.bullets {
                out.insert(b.id.clone(), short(&b.text));
            }
        }
    }
    out
}

/// What accepting `s` does, in words.
pub fn describe(s: &Suggestion, labels: &BTreeMap<Id, String>) -> String {
    let name = |id: &Id| {
        labels
            .get(id)
            .cloned()
            .unwrap_or_else(|| t().removed_item.into())
    };
    let tr = t();
    match (&s.target, &s.action) {
        (SuggestionTarget::Summary, _) => tr.rewrite_summary.into(),
        (target, Some(SuggestionAction::Reorder { order })) => {
            let list: Vec<String> = order.iter().map(name).collect();
            let what = match target {
                SuggestionTarget::Entry { id } => {
                    fill(tr.reorder_bullets_of, &[("item", &name(id))])
                }
                SuggestionTarget::Section { id } => {
                    fill(tr.reorder_entries_of, &[("item", &name(id))])
                }
                SuggestionTarget::SkillGroup { id } => {
                    fill(tr.reorder_skills_of, &[("item", &name(id))])
                }
                _ => tr.reorder_items.into(),
            };
            format!("{what}: {}", list.join(" · "))
        }
        (
            SuggestionTarget::Bullet { id }
            | SuggestionTarget::Entry { id }
            | SuggestionTarget::Section { id }
            | SuggestionTarget::SkillGroup { id },
            action,
        ) => {
            let template = match action {
                Some(SuggestionAction::Hide) => tr.suggest_hide,
                Some(SuggestionAction::Collapse) => tr.suggest_collapse,
                Some(SuggestionAction::Show) => tr.suggest_show,
                _ => tr.suggest_rewrite,
            };
            fill(template, &[("item", &name(id))])
        }
        (SuggestionTarget::Skills, _) => tr.reorder_items.into(),
    }
}

fn badge(kind: VerificationKind) -> &'static str {
    let s = t();
    match kind {
        VerificationKind::NewNumber => s.badge_new_number,
        VerificationKind::NewTerm => s.badge_new_term,
        VerificationKind::TerminologyChanged => s.badge_terminology,
        VerificationKind::LengthOutOfRange => s.badge_length,
        VerificationKind::LanguageChanged => s.badge_language,
    }
}

#[component]
pub fn SuggestionReview(
    cv: RwSignal<Option<Cv>>,
    variant: RwSignal<Option<StoredVariant>>,
    /// True while overlay edits are saving.
    busy: RwSignal<bool>,
    edit: Callback<EditFn>,
) -> impl IntoView {
    let groups = Memo::new(move |_| {
        variant.with(|v| {
            v.as_ref()
                .map(|v| pending(&v.variant.suggestions))
                .unwrap_or_default()
        })
    });
    let labels = Memo::new(move |_| cv.with(|c| c.as_ref().map(labels).unwrap_or_default()));
    let any_warnings =
        move || groups.with(|g| g.values().flatten().any(|s| !s.warnings.is_empty()));
    // "Accept all" asks first when any suggestion carries a warning.
    let confirming = RwSignal::new(false);

    let accept = move |id: Id| {
        edit.run(Box::new(move |v| {
            v.accept_suggestion(&id);
        }))
    };
    let reject = move |id: Id| {
        edit.run(Box::new(move |v| {
            v.reject_suggestion(&id);
        }))
    };
    let accept_all = move |safe_only: bool| {
        confirming.set(false);
        edit.run(Box::new(move |v| {
            v.accept_all(&SOURCES, safe_only);
        }))
    };
    let reject_all = move |_| {
        confirming.set(false);
        edit.run(Box::new(|v| {
            v.reject_all(&SOURCES);
        }))
    };

    view! {
        <Show when=move || groups.with(|g| !g.is_empty())>
            <section class="review" aria-label=t().review_heading>
                <h3>{t().review_heading}</h3>
                <p class="hint">{t().review_hint}</p>
                <div class="review-bulk">
                    <button prop:disabled=move || busy.get() on:click=move |_| accept_all(true)>{t().accept_all_safe}</button>
                    <button prop:disabled=move || busy.get() on:click=reject_all>{t().reject_all}</button>
                    <Show when=move || !confirming.get()
                        fallback=move || view! {
                            <span>{t().accept_all_confirm}</span>
                            <button prop:disabled=move || busy.get() on:click=move |_| accept_all(false)>{t().accept_all}</button>
                            <button on:click=move |_| confirming.set(false)>{t().cancel}</button>
                        }>
                        <button prop:disabled=move || busy.get()
                            on:click=move |_| if any_warnings() { confirming.set(true) } else { accept_all(false) }>
                            {t().accept_all}
                        </button>
                    </Show>
                </div>
                {move || {
                    let labels = labels.get();
                    groups.get().into_iter().map(|(group, items)| {
                        let cards = items.into_iter().map(|s| {
                            let text = describe(&s, &labels);
                            let (a, r) = (s.id.clone(), s.id.clone());
                            let rewrite = s.after.clone().map(|after| view! {
                                <div class="rewrite">
                                    <del>{s.before.clone().unwrap_or_default()}</del>
                                    <ins>{after}</ins>
                                </div>
                            });
                            let badges = s.warnings.iter().map(|w| view! {
                                <span class="badge" class:warn=w.severity != cv_core::Severity::Info>
                                    {fill(badge(w.kind), &[("detail", &w.detail)])}
                                </span>
                            }).collect_view();
                            view! {
                                <li class="card">
                                    <div class="card-text">
                                        <strong>{text}</strong>
                                        {rewrite}
                                        <small>{s.reason.clone()}</small>
                                        {badges}
                                    </div>
                                    <button prop:disabled=move || busy.get() on:click=move |_| accept(a.clone())>{t().accept}</button>
                                    <button prop:disabled=move || busy.get() on:click=move |_| reject(r.clone())>{t().reject}</button>
                                </li>
                            }
                        }).collect_view();
                        view! {
                            <div class="review-group">
                                <h4>{group.heading()}</h4>
                                <ul>{cards}</ul>
                            </div>
                        }
                    }).collect_view()
                }}
            </section>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cv() -> Cv {
        serde_json::from_str(include_str!("../../testdata/cvs/typical.json")).expect("fixture")
    }

    fn id(s: &str) -> Id {
        Id::parse(s).expect("id")
    }

    fn suggestion(
        kind: SuggestionKind,
        target: SuggestionTarget,
        action: Option<SuggestionAction>,
    ) -> Suggestion {
        Suggestion {
            id: id("01J0200000000000000000S001"),
            kind,
            target,
            before: None,
            after: None,
            action,
            reason: "Low relevance (score 10)".into(),
            confidence: 0.7,
            warnings: Vec::new(),
            status: SuggestionStatus::Pending,
            source: SuggestionSource::Tailor,
        }
    }

    #[test]
    fn suggestions_are_grouped_and_fit_cuts_left_out() {
        let hide = suggestion(
            SuggestionKind::Hide,
            SuggestionTarget::Entry {
                id: id("01J02000000000000000000007"),
            },
            Some(SuggestionAction::Hide),
        );
        let mut cut = hide.clone();
        cut.source = SuggestionSource::Fit;
        let mut done = hide.clone();
        done.status = SuggestionStatus::Accepted;
        let order = suggestion(
            SuggestionKind::SkillOrder,
            SuggestionTarget::SkillGroup {
                id: id("01J02000000000000000000014"),
            },
            None,
        );
        let g = pending(&[hide, cut, done, order]);
        assert_eq!(
            g.keys().copied().collect::<Vec<_>>(),
            [Group::Selection, Group::SkillsOrder]
        );
        assert_eq!(g[&Group::Selection].len(), 1);
    }

    #[test]
    fn descriptions_name_the_items() {
        let labels = labels(&cv());
        let hide = suggestion(
            SuggestionKind::Hide,
            SuggestionTarget::Entry {
                id: id("01J02000000000000000000007"),
            },
            Some(SuggestionAction::Hide),
        );
        assert!(
            describe(&hide, &labels).contains("Backend Engineer, Example AG"),
            "{}",
            describe(&hide, &labels)
        );
        let order = suggestion(
            SuggestionKind::SkillOrder,
            SuggestionTarget::SkillGroup {
                id: id("01J02000000000000000000014"),
            },
            Some(SuggestionAction::Reorder {
                order: vec![
                    id("01J02000000000000000000017"),
                    id("01J02000000000000000000015"),
                ],
            }),
        );
        let text = describe(&order, &labels);
        assert!(text.ends_with("SQL · Rust"), "{text}");
        let gone = suggestion(
            SuggestionKind::Hide,
            SuggestionTarget::Bullet {
                id: id("01J0200000000000000000ZZZZ"),
            },
            Some(SuggestionAction::Hide),
        );
        assert!(describe(&gone, &labels).contains(t().removed_item));
    }
}
