//! Fit to N pages for a variant (spec section 4.6, M5): the page target, a
//! button that asks the backend for cuts, and the review of those cuts.
//! Nothing is hidden until the user accepts a cut.

use cv_api::{FitOutcome, FitVariant};
use cv_core::{
    Cv, Entry, Id, Section, StoredVariant, Suggestion, SuggestionAction, SuggestionSource,
    SuggestionStatus, SuggestionTarget, Variant,
};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::i18n::{error_message, fill, t};
use crate::ipc::{ReqArg, call};
use crate::variants::entry_label;

/// Applies a change to the selected variant's overlay and queues its save.
pub type EditFn = Box<dyn FnOnce(&mut Variant)>;

/// The pending cuts from fit-to-length, in the order they were proposed.
pub fn pending_cuts(v: &Variant) -> Vec<Suggestion> {
    v.suggestions
        .iter()
        .filter(|s| s.source == SuggestionSource::Fit && s.status == SuggestionStatus::Pending)
        .cloned()
        .collect()
}

fn find_entry<'a>(cv: &'a Cv, id: &Id) -> Option<&'a Entry> {
    cv.sections
        .iter()
        .flat_map(|s| match s {
            Section::Experience(x)
            | Section::Education(x)
            | Section::Projects(x)
            | Section::Certifications(x) => x.entries.as_slice(),
            Section::Custom(c) => c.entries.as_slice(),
            Section::Skills(_) | Section::Languages(_) => &[],
        })
        .find(|e| e.id == *id)
}

/// What accepting `s` does, in words.
pub fn cut_label(cv: &Cv, s: &Suggestion) -> String {
    match (&s.target, &s.action) {
        (SuggestionTarget::Entry { id }, Some(SuggestionAction::Collapse)) => {
            let entry = find_entry(cv, id).map_or_else(|| t().untitled_entry.into(), entry_label);
            fill(t().cut_collapse_entry, &[("entry", &entry)])
        }
        _ => fill(
            t().cut_hide_bullet,
            &[("text", s.before.as_deref().unwrap_or_default())],
        ),
    }
}

/// The result of a fit, as a status line.
pub fn outcome_message(o: &FitOutcome, cuts: usize) -> String {
    match (o.fits, cuts) {
        (false, _) => fill(t().fit_impossible, &[("pages", &o.pages.to_string())]),
        (true, 0) => t().fit_already.into(),
        (true, n) => fill(t().fit_proposed, &[("n", &n.to_string())]),
    }
}

#[component]
pub fn FitPanel(
    cv: RwSignal<Option<Cv>>,
    variant: RwSignal<Option<StoredVariant>>,
    error: RwSignal<Option<String>>,
    changed: Callback<()>,
    /// True while overlay edits are saving; a fit counts as one.
    busy: RwSignal<bool>,
    settled: Callback<bool>,
    /// True while a fit runs; the rest of the overlay is locked meanwhile.
    fitting: RwSignal<bool>,
    edit: Callback<EditFn>,
) -> impl IntoView {
    let message: RwSignal<Option<String>> = RwSignal::new(None);
    let target = move || variant.with(|v| v.as_ref().and_then(|v| v.variant.page_target));
    let set_target = move |value: String| {
        let pages = value.parse::<u8>().ok().filter(|p| (1..=2).contains(p));
        message.set(None);
        edit.run(Box::new(move |v| v.page_target = pages));
    };
    let run_fit = move |_| {
        let Some(stored) = variant.get_untracked() else {
            return;
        };
        let Some(page_target) = stored.variant.page_target else {
            return;
        };
        let req = FitVariant {
            variant: stored.variant,
            expected_revision: stored.revision,
            page_target,
        };
        busy.set(true);
        fitting.set(true);
        message.set(Some(t().fitting.into()));
        spawn_local(async move {
            let result = call::<_, FitOutcome>("fit_variant", &ReqArg { req: &req }).await;
            if busy.try_set(false).is_some() || fitting.try_set(false).is_some() {
                return; // The editor was closed.
            }
            match result {
                Ok(outcome) => {
                    let cuts = pending_cuts(&outcome.variant.variant).len();
                    message.set(Some(outcome_message(&outcome, cuts)));
                    // A different variant may have been chosen meanwhile;
                    // the selector is disabled while busy, so this is rare.
                    let same = variant.with_untracked(|v| {
                        v.as_ref()
                            .is_some_and(|v| v.variant.id == outcome.variant.variant.id)
                    });
                    if same {
                        variant.set(Some(outcome.variant));
                        changed.run(());
                    }
                    settled.run(true);
                }
                Err(e) => {
                    message.set(None);
                    error.set(Some(error_message(&e)));
                    settled.run(false);
                }
            }
        });
    };
    let cuts = move || {
        variant.with(|v| {
            v.as_ref()
                .map(|v| pending_cuts(&v.variant))
                .unwrap_or_default()
        })
    };
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
    let accept_all = move |_| {
        edit.run(Box::new(|v| {
            for s in pending_cuts(v) {
                v.accept_suggestion(&s.id);
            }
        }))
    };

    view! {
        <div class="fit">
            <label>{t().page_target}" "
                <select prop:disabled=move || busy.get() || fitting.get()
                    on:change=move |ev| set_target(event_target_value(&ev))>
                    <option value="" selected=move || target().is_none()>{t().pages_auto}</option>
                    <option value="1" selected=move || target() == Some(1)>{t().pages_one}</option>
                    <option value="2" selected=move || target() == Some(2)>{t().pages_two}</option>
                </select>
            </label>
            <Show when=move || target().is_some()>
                <button prop:disabled=move || busy.get() on:click=run_fit>
                    {move || if target() == Some(1) { t().fit_to_one } else { t().fit_to_two }}
                </button>
            </Show>
            <span role="status">{move || message.get()}</span>
        </div>
        <Show when=move || !cuts().is_empty()>
            <div class="cuts">
                <strong>{t().proposed_cuts}</strong>
                <button prop:disabled=move || busy.get() on:click=accept_all>{t().accept_all}</button>
                <ul>
                    <For each=cuts key=|s| s.id.clone() let:s>
                        {
                            let label = cv.with_untracked(|c| c.as_ref().map(|c| cut_label(c, &s)).unwrap_or_default());
                            let (a, r) = (s.id.clone(), s.id.clone());
                            view! {
                                <li>
                                    <span>{label}</span>
                                    <button on:click=move |_| accept(a.clone())>{t().accept}</button>
                                    <button on:click=move |_| reject(r.clone())>{t().reject}</button>
                                </li>
                            }
                        }
                    </For>
                </ul>
            </div>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cv_core::{SuggestionKind, Timestamp};

    fn cv() -> Cv {
        serde_json::from_str(include_str!("../../testdata/cvs/typical.json")).expect("cv")
    }

    fn cut(id: Id, target: SuggestionTarget, action: SuggestionAction) -> Suggestion {
        Suggestion {
            id,
            kind: SuggestionKind::LengthCut,
            target,
            before: Some("Ran CI".into()),
            after: None,
            action: Some(action),
            reason: String::new(),
            confidence: 1.0,
            warnings: Vec::new(),
            status: SuggestionStatus::Pending,
            source: SuggestionSource::Fit,
        }
    }

    #[test]
    fn cuts_are_described_in_words() {
        let cv = cv();
        let Section::Experience(x) = &cv.sections[0] else {
            panic!("experience first")
        };
        let e = &x.entries[0];
        let collapse = cut(
            x.id.clone(),
            SuggestionTarget::Entry { id: e.id.clone() },
            SuggestionAction::Collapse,
        );
        assert!(cut_label(&cv, &collapse).contains(&entry_label(e)));
        let hide = cut(
            x.id.clone(),
            SuggestionTarget::Bullet {
                id: e.bullets[0].id.clone(),
            },
            SuggestionAction::Hide,
        );
        assert!(cut_label(&cv, &hide).contains("Ran CI"));
    }

    #[test]
    fn only_pending_fit_cuts_are_listed() {
        let cv = cv();
        let mut v = Variant::empty(cv.id.clone(), cv.id.clone(), "v");
        let b = SuggestionTarget::Bullet { id: cv.id.clone() };
        v.suggestions = vec![
            cut(
                cv.sections[0].id().clone(),
                b.clone(),
                SuggestionAction::Hide,
            ),
            cut(
                cv.sections[1].id().clone(),
                b.clone(),
                SuggestionAction::Hide,
            ),
            cut(cv.sections[2].id().clone(), b, SuggestionAction::Hide),
        ];
        v.suggestions[1].status = SuggestionStatus::Accepted;
        v.suggestions[2].source = SuggestionSource::Tailor;
        assert_eq!(pending_cuts(&v).len(), 1);
    }

    #[test]
    fn outcomes_read_as_sentences() {
        let stored = StoredVariant {
            variant: Variant::empty(cv().id, cv().id, "v"),
            revision: 1,
            created_at: Timestamp("2026-10-24T10:00:00Z".into()),
            updated_at: Timestamp("2026-10-24T10:00:00Z".into()),
        };
        let o = |fits, pages| FitOutcome {
            variant: stored.clone(),
            pages,
            fits,
        };
        assert_eq!(outcome_message(&o(true, 1), 0), t().fit_already);
        assert!(outcome_message(&o(true, 1), 3).contains('3'));
        assert!(outcome_message(&o(false, 3), 0).contains('3'));
    }
}
