//! AI advice outside the suggestion list: the gap questions a tailoring run
//! saves on the variant (spec section 12.6), and "Review CV with AI" in the
//! Checks tab (spec section 11.11), whose accepted rewrites change the base
//! CV. Gap questions are never CV content: "Add to CV…" only opens the
//! editor.

use std::time::Duration;

use cv_api::{
    AiDone, AiFailed, AiProgress, AiReview, AiStatus, ErrorCode, JobId, ReviewDecide,
    ReviewDecided, ReviewNoteCategory, ReviewSession,
};
use cv_core::{Cv, GapQuestion, GapSeverity, Id, StoredCv, StoredVariant, SuggestionStatus};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Serialize;

use crate::checks::{Spot, focus_spot};
use crate::fit::EditFn;
use crate::i18n::{error_message, fill, t};
use crate::ipc::{Listener, NoArgs, ReqArg, call, listen};
use crate::review::{badges_view, bullet_entries, describe, diff_view, labels, needs_attention};
use crate::settings::ConsentPanel;
use crate::tailor::{DiscardedDetails, stage_label};

/// The gap questions still open, must-haves first (the order they were
/// saved in).
pub fn open_gaps(gaps: &[GapQuestion]) -> Vec<GapQuestion> {
    gaps.iter().filter(|g| !g.dismissed).cloned().collect()
}

#[component]
pub fn GapList(
    cv: RwSignal<Option<Cv>>,
    variant: RwSignal<Option<StoredVariant>>,
    busy: RwSignal<bool>,
    edit: Callback<EditFn>,
) -> impl IntoView {
    let gaps = Memo::new(move |_| {
        variant.with(|v| {
            v.as_ref()
                .map(|v| open_gaps(&v.variant.gaps))
                .unwrap_or_default()
        })
    });
    // The CV's first section: where "Add to CV…" puts the cursor.
    let add = move |_| {
        let Some(first) = cv.with_untracked(|c| {
            c.as_ref()
                .and_then(|c| c.sections.first().map(|s| s.id().clone()))
        }) else {
            return;
        };
        set_timeout(move || focus_spot(&Spot::Item(first)), Duration::ZERO);
    };
    let dismiss = move |requirement_id: String| {
        edit.run(Box::new(move |v| {
            v.dismiss_gap(&requirement_id);
        }))
    };

    view! {
        <Show when=move || gaps.with(|g| !g.is_empty())>
            <section class="gaps" aria-label=t().gaps_heading>
                <h3>{t().gaps_heading}</h3>
                <p class="hint">{t().gaps_hint}</p>
                <ul>
                    {move || gaps.get().into_iter().map(|g| {
                        let severity = match g.severity {
                            GapSeverity::Missing => t().gap_missing,
                            GapSeverity::Weak => t().gap_weak,
                        };
                        let need = if g.must_have { t().gap_must_have } else { t().gap_nice_to_have };
                        let where_to = (!g.where_to_add_hint.is_empty())
                            .then(|| fill(t().gap_where, &[("hint", &g.where_to_add_hint)]));
                        let id = g.requirement_id.clone();
                        view! {
                            <li class="card">
                                <div class="card-text">
                                    <strong>{g.requirement.clone()}</strong>
                                    <span class="badge" class:warn=g.must_have class:info=!g.must_have>{need}</span>
                                    <span class="badge">{severity}</span>
                                    <p>{g.question.clone()}</p>
                                    {where_to.map(|w| view! { <small>{w}</small> })}
                                </div>
                                <button on:click=add>{t().add_to_cv}</button>
                                <button prop:disabled=move || busy.get()
                                    on:click=move |_| dismiss(id.clone())>{t().dismiss}</button>
                            </li>
                        }
                    }).collect_view()}
                </ul>
            </section>
        </Show>
    }
}

/// Event names, as the shell emits them.
const PROGRESS: &str = "ai://progress";
const DONE: &str = "ai://done";
const ERROR: &str = "ai://error";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CvIdArg<'a> {
    cv_id: &'a Id,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JobArg<'a> {
    job_id: &'a str,
}

fn note_label(c: ReviewNoteCategory) -> &'static str {
    let s = t();
    match c {
        ReviewNoteCategory::Impact => s.note_impact,
        ReviewNoteCategory::Clarity => s.note_clarity,
        ReviewNoteCategory::Structure => s.note_structure,
        ReviewNoteCategory::Consistency => s.note_consistency,
    }
}

/// "2026-10-13 12:00" from a stored timestamp.
pub fn when(at: &str) -> String {
    at.get(..16).unwrap_or(at).replacen('T', " ", 1)
}

/// The status line after a review.
pub fn review_message(session: &ReviewSession) -> String {
    match session.suggestions.len() {
        0 => t().ai_review_nothing.to_owned(),
        1 => t().ai_review_done_one.to_owned(),
        n => fill(t().ai_review_done, &[("n", &n.to_string())]),
    }
}

#[derive(Clone)]
enum Ended {
    Done(Box<ReviewSession>),
    Failed(cv_api::ApiError),
}

#[component]
pub fn AiReviewPanel(
    cv_id: Id,
    cv: RwSignal<Option<Cv>>,
    /// True while CV edits are unsaved; decisions wait for them.
    dirty: Signal<bool>,
    /// Called with the CV an accepted suggestion saved.
    applied: Callback<StoredCv>,
) -> impl IntoView {
    let status: RwSignal<Option<AiStatus>> = RwSignal::new(None);
    let session: RwSignal<Option<ReviewSession>> = RwSignal::new(None);
    let progress = RwSignal::new(None);
    let message: RwSignal<Option<String>> = RwSignal::new(None);
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let job: RwSignal<Option<JobId>> = RwSignal::new(None);
    let running = RwSignal::new(false);
    let deciding = RwSignal::new(false);
    let editing: RwSignal<Option<(Id, String)>> = RwSignal::new(None);
    let early: StoredValue<Option<(JobId, Ended)>> = StoredValue::new(None);
    let listeners: StoredValue<Vec<Listener>, LocalStorage> = StoredValue::new_local(Vec::new());
    let cv_id = StoredValue::new(cv_id);

    spawn_local(async move {
        let id = cv_id.get_value();
        let (s, last) = (
            call::<_, AiStatus>("ai_status", &NoArgs {}).await,
            call::<_, Option<ReviewSession>>("get_review", &CvIdArg { cv_id: &id }).await,
        );
        match (s, last) {
            (Ok(s), Ok(last)) => {
                let _ = status.try_set(Some(s));
                let _ = session.try_set(last);
            }
            (Err(e), _) | (_, Err(e)) => {
                let _ = error.try_set(Some(error_message(&e)));
            }
        }
    });

    let finish = move |ended: Ended| {
        job.set(None);
        progress.set(None);
        running.set(false);
        match ended {
            Ended::Done(s) => {
                message.set(Some(review_message(&s)));
                session.set(Some(*s));
            }
            Ended::Failed(e) if e.code == ErrorCode::AiCancelled => {
                message.set(Some(t().ai_review_cancelled.into()));
            }
            Ended::Failed(e) => {
                message.set(None);
                error.set(Some(error_message(&e)));
            }
        }
    };
    let ended = move |id: JobId, how: Ended| {
        if job.try_get_untracked().flatten().as_deref() == Some(id.as_str()) {
            finish(how);
        } else if running.try_get_untracked() == Some(true) {
            early.set_value(Some((id, how)));
        }
    };
    spawn_local(async move {
        let subscriptions = [
            listen(PROGRESS, move |p: AiProgress| {
                if job.try_get_untracked().flatten().as_deref() == Some(p.job_id.as_str()) {
                    let _ = progress.try_set(Some((p.stage, p.pct)));
                }
            })
            .await,
            listen(DONE, move |d: AiDone| {
                if let Ok(s) = serde_json::from_value::<ReviewSession>(d.result) {
                    ended(d.job_id, Ended::Done(Box::new(s)));
                }
            })
            .await,
            listen(ERROR, move |f: AiFailed| {
                ended(f.job_id, Ended::Failed(f.error))
            })
            .await,
        ];
        for s in subscriptions {
            match s {
                Ok(l) => {
                    listeners.try_update_value(|v| v.push(l));
                }
                Err(e) => {
                    let _ = error.try_set(Some(error_message(&e)));
                }
            }
        }
    });

    let start = move |_| {
        let req = AiReview {
            cv_id: cv_id.get_value(),
        };
        running.set(true);
        message.set(None);
        error.set(None);
        progress.set(Some((cv_api::AiStage::Starting, 0)));
        early.set_value(None);
        spawn_local(async move {
            let result = call::<_, JobId>("ai_review", &ReqArg { req: &req }).await;
            if running.try_get_untracked().is_none() {
                return;
            }
            match result {
                Ok(id) => {
                    let done = early
                        .try_update_value(|e| e.take().filter(|(early_id, _)| *early_id == id))
                        .flatten();
                    job.set(Some(id));
                    if let Some((_, how)) = done {
                        finish(how);
                    }
                }
                Err(e) => finish(Ended::Failed(e)),
            }
        });
    };
    let cancel = move |_| {
        let Some(id) = job.get_untracked() else {
            return;
        };
        spawn_local(async move {
            if let Err(e) = call::<_, ()>("ai_cancel", &JobArg { job_id: &id }).await {
                let _ = error.try_set(Some(error_message(&e)));
            }
        });
    };
    let decide = move |suggestion_id: Id, accept: bool, text: Option<String>| {
        if accept && dirty.get_untracked() {
            error.set(Some(t().wait_for_save.into()));
            return;
        }
        let req = ReviewDecide {
            cv_id: cv_id.get_value(),
            suggestion_id,
            accept,
            text,
        };
        deciding.set(true);
        error.set(None);
        spawn_local(async move {
            let result = call::<_, ReviewDecided>("review_decide", &ReqArg { req: &req }).await;
            if deciding.try_set(false).is_some() {
                return;
            }
            match result {
                Ok(d) => {
                    editing.set(None);
                    session.set(Some(d.session));
                    if let Some(saved) = d.cv {
                        applied.run(saved);
                    }
                }
                Err(e) => error.set(Some(error_message(&e))),
            }
        });
    };

    let ready = move || status.with(|s| s.as_ref().is_some_and(|s| s.configured && s.consent_ok));
    let needs_consent = move || {
        status.with(|s| {
            s.as_ref()
                .filter(|s| s.configured && !s.consent_ok)
                .cloned()
        })
    };
    let item_labels = Memo::new(move |_| cv.with(|c| c.as_ref().map(labels).unwrap_or_default()));
    let entries =
        Memo::new(move |_| cv.with(|c| c.as_ref().map(bullet_entries).unwrap_or_default()));
    let locked = move || running.get() || deciding.get();

    view! {
        <section class="ai-review" aria-label=t().ai_review_heading>
            <h2>{t().ai_review_heading}</h2>
            <p class="hint">{t().ai_review_hint}</p>
            {move || match (status.get(), needs_consent()) {
                (None, _) => None,
                (Some(s), _) if !s.configured => Some(view! { <p class="hint">{t().tailor_needs_ai}</p> }.into_any()),
                (_, Some(s)) => Some(view! {
                    <ConsentPanel status=s accepted=Callback::new(move |s: AiStatus| status.set(Some(s))) />
                }.into_any()),
                _ => None,
            }}
            <Show when=ready>
                <button prop:disabled=locked on:click=start>{t().review_cv_with_ai}</button>
            </Show>
            {move || progress.get().map(|(stage, pct)| view! {
                <div class="tailor-progress">
                    <progress max="100" prop:value=pct aria-label=t().ai_review_progress></progress>
                    <span>{stage_label(stage)}</span>
                    <button prop:disabled=move || job.with(Option::is_none) on:click=cancel>{t().cancel}</button>
                </div>
            })}
            <p role="status">{move || message.get()}</p>
            {move || error.get().map(|e| view! { <p role="alert">{e}</p> })}
            {move || session.get().map(|s| {
                let (labels, entries) = (item_labels.get(), entries.get());
                let notes = s.notes.iter().map(|n| {
                    let about = n.target.as_ref()
                        .and_then(|id| labels.get(id).cloned())
                        .unwrap_or_else(|| t().whole_cv.into());
                    view! {
                        <li>
                            <span class="badge" class:info=true>{note_label(n.category)}</span>
                            <strong>{about}</strong>": "{n.message.clone()}
                        </li>
                    }
                }).collect_view();
                let cards = s.suggestions.iter()
                    .filter(|s| s.status == SuggestionStatus::Pending)
                    .map(|s| {
                        let text = describe(s, &labels, &entries);
                        let rewrite = s.after.as_deref().map(|a| diff_view(s.before.as_deref().unwrap_or_default(), a));
                        let attention = needs_attention(&s.warnings);
                        let (a, r, e) = (s.id.clone(), s.id.clone(), s.id.clone());
                        let after = s.after.clone().unwrap_or_default();
                        let id = s.id.clone();
                        let is_editing = move || editing.with(|e| e.as_ref().is_some_and(|(i, _)| *i == id));
                        view! {
                            <li class="card" class:attention=attention>
                                <div class="card-text">
                                    <strong>{text}</strong>
                                    {attention.then(|| view! { <span class="needs-attention">{t().needs_attention}</span> })}
                                    {rewrite}
                                    <small>{s.reason.clone()}</small>
                                    {badges_view(&s.warnings)}
                                    <Show when=is_editing.clone()>
                                        <label>{t().edited_text}
                                            <textarea rows="3"
                                                prop:value=move || editing.with(|e| e.as_ref().map(|(_, t)| t.clone()).unwrap_or_default())
                                                on:input=move |ev| editing.update(|e| if let Some((_, t)) = e { *t = event_target_value(&ev) })></textarea>
                                        </label>
                                    </Show>
                                </div>
                                <Show when=is_editing.clone()
                                    fallback=move || {
                                        let (a, r, e, after) = (a.clone(), r.clone(), e.clone(), after.clone());
                                        view! {
                                            <button prop:disabled=locked on:click=move |_| decide(a.clone(), true, None)>{t().accept}</button>
                                            <button prop:disabled=locked
                                                on:click=move |_| editing.set(Some((e.clone(), after.clone())))>{t().edit_then_accept}</button>
                                            <button prop:disabled=locked on:click=move |_| decide(r.clone(), false, None)>{t().reject}</button>
                                        }
                                    }>
                                    <button prop:disabled=move || locked() || editing.with(|e| e.as_ref().is_none_or(|(_, t)| t.trim().is_empty()))
                                        on:click=move |_| {
                                            if let Some((id, text)) = editing.get_untracked() {
                                                decide(id, true, Some(text.trim().to_owned()));
                                            }
                                        }>{t().accept}</button>
                                    <button on:click=move |_| editing.set(None)>{t().cancel}</button>
                                </Show>
                            </li>
                        }
                    }).collect_view();
                let discarded = Signal::stored(s.discarded.clone());
                view! {
                    <p class="hint">{fill(t().ai_review_last, &[("at", &when(&s.created_at.0))])}</p>
                    {(!s.notes.is_empty()).then(|| view! {
                        <h3>{t().ai_review_notes}</h3>
                        <ul class="notes">{notes}</ul>
                    })}
                    <ul>{cards}</ul>
                    <DiscardedDetails items=discarded />
                }
            })}
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gap(id: &str, dismissed: bool) -> GapQuestion {
        GapQuestion {
            requirement_id: id.into(),
            requirement: "Kubernetes".into(),
            must_have: true,
            severity: GapSeverity::Missing,
            question: "Have you used Kubernetes?".into(),
            where_to_add_hint: String::new(),
            dismissed,
        }
    }

    #[test]
    fn dismissed_gaps_are_not_listed() {
        let open = open_gaps(&[gap("r1", true), gap("r2", false)]);
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].requirement_id, "r2");
    }

    #[test]
    fn review_times_show_minutes() {
        assert_eq!(when("2026-10-13T12:00:05.000Z"), "2026-10-13 12:00");
        assert_eq!(when("bad"), "bad");
    }
}
