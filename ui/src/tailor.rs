//! The tailor panel in the Variants tab (spec section 12.2 (4), M7a): the
//! job title, company and description saved on the variant, "Analyze and
//! tailor", and the job's progress with Cancel. The suggestions it saves are
//! all pending; nothing changes until the user accepts one.

use cv_api::{
    AiDone, AiFailed, AiProgress, AiStage, AiStatus, AiTailor, DiscardedRewrite, ErrorCode, JobId,
    TailorResult,
};
use cv_core::StoredVariant;
use cv_core::selection::TailorOptions;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Serialize;

use crate::fit::EditFn;
use crate::i18n::{error_message, fill, t};
use crate::ipc::{Listener, NoArgs, ReqArg, call, listen};
use crate::settings::ConsentPanel;

/// Event names, as the shell emits them.
const PROGRESS: &str = "ai://progress";
const DONE: &str = "ai://done";
const ERROR: &str = "ai://error";

/// What the progress line says for `stage`.
pub fn stage_label(stage: AiStage) -> &'static str {
    let s = t();
    match stage {
        AiStage::Starting => s.stage_starting,
        AiStage::JobParse => s.stage_job_parse,
        AiStage::Relevance => s.stage_relevance,
        AiStage::Selection => s.stage_selection,
        AiStage::Fit => s.stage_fit,
        AiStage::Rewrite => s.stage_rewrite,
        AiStage::Summary => s.stage_summary,
        AiStage::Gaps => s.stage_gaps,
        AiStage::Review => s.stage_review,
        AiStage::Import => s.stage_working,
    }
}

/// The status line after a run.
pub fn result_message(r: &TailorResult) -> String {
    let mut text = match r.suggestions {
        0 => t().tailor_nothing.to_owned(),
        1 => t().tailor_done_one.to_owned(),
        n => fill(t().tailor_done, &[("n", &n.to_string())]),
    };
    for (stage, why) in [
        (AiStage::Rewrite, t().rewrite_failed),
        (AiStage::Summary, t().summary_failed),
        (AiStage::Gaps, t().gaps_failed),
    ] {
        if r.failed_stages.contains(&stage) {
            text.push(' ');
            text.push_str(why);
        }
    }
    if r.fits == Some(false) {
        text.push(' ');
        text.push_str(&fill(
            t().fit_impossible,
            &[("pages", &r.pages.unwrap_or_default().to_string())],
        ));
    }
    text
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JobArg<'a> {
    job_id: &'a str,
}

/// How a job ended.
#[derive(Clone)]
enum Ended {
    Done(Box<TailorResult>),
    Failed(cv_api::ApiError),
}

#[component]
pub fn TailorPanel(
    variant: RwSignal<Option<StoredVariant>>,
    error: RwSignal<Option<String>>,
    changed: Callback<()>,
    /// True while overlay edits are saving; tailoring waits for them.
    busy: RwSignal<bool>,
    /// True while tailoring runs; the rest of the overlay is locked meanwhile,
    /// since the job saves the variant when it ends.
    tailoring: RwSignal<bool>,
    edit: Callback<EditFn>,
) -> impl IntoView {
    let status: RwSignal<Option<AiStatus>> = RwSignal::new(None);
    let progress: RwSignal<Option<(AiStage, u8)>> = RwSignal::new(None);
    let message: RwSignal<Option<String>> = RwSignal::new(None);
    // Rewrites the last run's safety checks dropped; not saved.
    let discarded: RwSignal<Vec<DiscardedRewrite>> = RwSignal::new(Vec::new());
    let job: RwSignal<Option<JobId>> = RwSignal::new(None);
    let reorder = RwSignal::new(false);
    // A job can end before `ai_tailor` answers with its ID: the end is kept
    // here until the ID is known.
    let early: StoredValue<Option<(JobId, Ended)>> = StoredValue::new(None);
    let listeners: StoredValue<Vec<Listener>, LocalStorage> = StoredValue::new_local(Vec::new());

    spawn_local(async move {
        match call::<_, AiStatus>("ai_status", &NoArgs {}).await {
            Ok(s) => {
                let _ = status.try_set(Some(s));
            }
            Err(e) => {
                let _ = error.try_set(Some(error_message(&e)));
            }
        }
    });

    let finish = move |ended: Ended| {
        job.set(None);
        progress.set(None);
        tailoring.set(false);
        match ended {
            Ended::Done(result) => {
                message.set(Some(result_message(&result)));
                discarded.set(result.discarded.clone());
                let same = variant.with_untracked(|v| {
                    v.as_ref()
                        .is_some_and(|v| v.variant.id == result.variant.variant.id)
                });
                if same {
                    variant.set(Some(result.variant));
                    changed.run(());
                }
            }
            Ended::Failed(e) if e.code == ErrorCode::AiCancelled => {
                message.set(Some(t().tailor_cancelled.into()));
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
        } else if tailoring.try_get_untracked() == Some(true) {
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
                if let Ok(r) = serde_json::from_value::<TailorResult>(d.result) {
                    ended(d.job_id, Ended::Done(Box::new(r)));
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
                // Dropped with the panel, which unsubscribes.
                Ok(l) => {
                    listeners.try_update_value(|v| v.push(l));
                }
                Err(e) => {
                    let _ = error.try_set(Some(error_message(&e)));
                }
            }
        }
    });

    let job_field = move |f: fn(&cv_core::JobTarget) -> String| {
        move || variant.with(|v| v.as_ref().map(|v| f(&v.variant.job)).unwrap_or_default())
    };
    let set_job = move |f: fn(&mut cv_core::JobTarget, String), value: String| {
        edit.run(Box::new(move |v| f(&mut v.job, value)));
    };
    let has_description = move || {
        variant.with(|v| {
            v.as_ref()
                .is_some_and(|v| !v.variant.job.description_text.trim().is_empty())
        })
    };

    let start = move |_| {
        let Some(stored) = variant.get_untracked() else {
            return;
        };
        let req = AiTailor {
            variant_id: stored.variant.id.clone(),
            options: TailorOptions {
                page_target: stored.variant.page_target,
                reorder_entries_by_relevance: reorder.get_untracked(),
                ..TailorOptions::default()
            },
        };
        tailoring.set(true);
        message.set(None);
        discarded.set(Vec::new());
        error.set(None);
        progress.set(Some((AiStage::Starting, 0)));
        early.set_value(None);
        spawn_local(async move {
            let result = call::<_, JobId>("ai_tailor", &ReqArg { req: &req }).await;
            if tailoring.try_get_untracked().is_none() {
                return; // The editor was closed.
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

    let ready = move || status.with(|s| s.as_ref().is_some_and(|s| s.configured && s.consent_ok));
    let needs_consent = move || {
        status.with(|s| {
            s.as_ref()
                .filter(|s| s.configured && !s.consent_ok)
                .cloned()
        })
    };
    let locked = move || busy.get() || tailoring.get();

    view! {
        <fieldset class="tailor" prop:disabled=move || tailoring.get()>
            <legend>{t().tailor_heading}</legend>
            <label>{t().job_title}
                <input prop:value=job_field(|j| j.title.clone().unwrap_or_default())
                    on:change=move |ev| set_job(|j, v| j.title = Some(v.trim().to_owned()).filter(|v| !v.is_empty()), event_target_value(&ev)) />
            </label>
            <label>{t().company}
                <input prop:value=job_field(|j| j.company.clone().unwrap_or_default())
                    on:change=move |ev| set_job(|j, v| j.company = Some(v.trim().to_owned()).filter(|v| !v.is_empty()), event_target_value(&ev)) />
            </label>
            <label>{t().job_description}
                <textarea rows="8" prop:value=job_field(|j| j.description_text.clone())
                    on:change=move |ev| set_job(|j, v| j.description_text = v, event_target_value(&ev))></textarea>
            </label>
            <label class="check"><input type="checkbox" prop:checked=move || reorder.get()
                on:change=move |ev| reorder.set(event_target_checked(&ev)) />{t().reorder_by_relevance}</label>
        </fieldset>
        <div class="tailor-run">
            {move || match (status.get(), needs_consent()) {
                (None, _) => None,
                (Some(s), _) if !s.configured => Some(view! { <p class="hint">{t().tailor_needs_ai}</p> }.into_any()),
                (_, Some(s)) => Some(view! {
                    <ConsentPanel status=s accepted=Callback::new(move |s: AiStatus| status.set(Some(s))) />
                }.into_any()),
                _ => None,
            }}
            <Show when=ready>
                <button prop:disabled=move || locked() || !has_description() on:click=start>{t().analyze_and_tailor}</button>
            </Show>
            {move || progress.get().map(|(stage, pct)| view! {
                <div class="tailor-progress">
                    <progress max="100" prop:value=pct aria-label=t().tailor_progress></progress>
                    <span>{stage_label(stage)}</span>
                    <button prop:disabled=move || job.with(Option::is_none) on:click=cancel>{t().cancel}</button>
                </div>
            })}
            <p role="status">{move || message.get()}</p>
            <DiscardedDetails items=discarded.into() />
        </div>
    }
}

/// "N suggestions discarded by safety checks", with what each said.
#[component]
pub fn DiscardedDetails(items: Signal<Vec<DiscardedRewrite>>) -> impl IntoView {
    view! {
        <Show when=move || items.with(|d| !d.is_empty())>
            <details class="discarded">
                <summary>{move || fill(t().discarded_count, &[("n", &items.with(Vec::len).to_string())])}</summary>
                <ul>{move || items.get().into_iter().map(|d| view! {
                    <li>
                        <span>{t().discarded_before}": "{d.before}</span>
                        <span>{t().discarded_after}": "{d.after}</span>
                        {d.warnings.iter().map(|w| view! {
                            <span class="badge">{fill(crate::review::badge(w.kind), &[("detail", &w.detail)])}</span>
                        }).collect_view()}
                    </li>
                }).collect_view()}</ul>
            </details>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cv_core::{Id, Timestamp, Variant};

    fn result(suggestions: usize, fits: Option<bool>) -> TailorResult {
        let id = Id::parse("01J02000000000000000000001").expect("id");
        TailorResult {
            variant: StoredVariant {
                variant: Variant::empty(id.clone(), id, "V"),
                revision: 1,
                created_at: Timestamp("2026-10-24T00:00:00Z".into()),
                updated_at: Timestamp("2026-10-24T00:00:00Z".into()),
            },
            suggestions,
            pages: fits.map(|_| 2),
            fits,
            discarded: Vec::new(),
            failed_stages: Vec::new(),
        }
    }

    #[test]
    fn messages_count_suggestions_and_say_when_it_cannot_fit() {
        assert_eq!(result_message(&result(0, None)), t().tailor_nothing);
        assert_eq!(result_message(&result(1, None)), t().tailor_done_one);
        assert!(result_message(&result(3, None)).contains('3'));
        let over = result_message(&result(3, Some(false)));
        assert!(over.contains('3') && over.contains('2'), "{over}");
        let mut partial = result(3, None);
        partial.failed_stages.push(AiStage::Rewrite);
        assert!(result_message(&partial).contains(t().rewrite_failed));
    }

    #[test]
    fn every_stage_has_a_label() {
        for stage in [
            AiStage::Starting,
            AiStage::JobParse,
            AiStage::Relevance,
            AiStage::Selection,
            AiStage::Rewrite,
            AiStage::Fit,
        ] {
            assert!(!stage_label(stage).is_empty());
        }
    }
}
