//! Text import (spec section 9.4): paste a CV or choose a PDF or text file,
//! see exactly what would be sent, then read it with AI or without. The
//! result opens in the import wizard like a file import.

use cv_api::{
    AiDone, AiFailed, AiImport, AiProgress, AiStage, AiStatus, ErrorCode, ImportPreview,
    ImportText, ImportTextSource, JobId,
};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Serialize;

use crate::i18n::{error_message, fill, t};
use crate::ipc::{Listener, NoArgs, ReqArg, call, listen};
use crate::settings::ConsentPanel;
use crate::tailor::stage_label;

const PROGRESS: &str = "ai://progress";
const DONE: &str = "ai://done";
const ERROR: &str = "ai://error";

#[derive(Serialize)]
struct SourceArg<'a> {
    source: &'a ImportTextSource,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TextIdArg {
    text_id: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JobArg<'a> {
    job_id: &'a str,
}

/// What the note under the text says was taken out.
pub fn redacted_note(n: usize) -> String {
    match n {
        0 => t().text_import_nothing_redacted.to_owned(),
        n => fill(t().text_import_redacted, &[("n", &n.to_string())]),
    }
}

#[derive(Clone)]
enum Ended {
    Done(Box<ImportPreview>),
    Failed(cv_api::ApiError),
}

#[component]
pub fn TextImport(
    /// Called to leave without importing.
    close: Callback<()>,
    /// Called with the preview to review in the wizard.
    done: Callback<ImportPreview>,
) -> impl IntoView {
    let s = t();
    let pasted = RwSignal::new(String::new());
    let prepared: RwSignal<Option<ImportText>> = RwSignal::new(None);
    let status: RwSignal<Option<AiStatus>> = RwSignal::new(None);
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let busy = RwSignal::new(false);
    let running = RwSignal::new(false);
    let progress: RwSignal<Option<(AiStage, u8)>> = RwSignal::new(None);
    let job: RwSignal<Option<JobId>> = RwSignal::new(None);
    let early: StoredValue<Option<(JobId, Ended)>> = StoredValue::new(None);
    let listeners: StoredValue<Vec<Listener>, LocalStorage> = StoredValue::new_local(Vec::new());

    spawn_local(async move {
        match call::<_, AiStatus>("ai_status", &NoArgs {}).await {
            Ok(st) => {
                let _ = status.try_set(Some(st));
            }
            Err(e) => {
                let _ = error.try_set(Some(error_message(&e)));
            }
        }
    });

    let finish = move |ended: Ended| {
        job.set(None);
        progress.set(None);
        running.set(false);
        match ended {
            Ended::Done(p) => done.run(*p),
            Ended::Failed(e) if e.code == ErrorCode::AiCancelled => {}
            Ended::Failed(e) => error.set(Some(error_message(&e))),
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
                if let Ok(p) = serde_json::from_value::<ImportPreview>(d.result) {
                    ended(d.job_id, Ended::Done(Box::new(p)));
                }
            })
            .await,
            listen(ERROR, move |f: AiFailed| {
                ended(f.job_id, Ended::Failed(f.error))
            })
            .await,
        ];
        for sub in subscriptions {
            match sub {
                Ok(l) => {
                    listeners.try_update_value(|v| v.push(l));
                }
                Err(e) => {
                    let _ = error.try_set(Some(error_message(&e)));
                }
            }
        }
    });

    let prepare = move |source: ImportTextSource| {
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            let result =
                call::<_, ImportText>("import_text_prepare", &SourceArg { source: &source }).await;
            if busy.try_set(false).is_some() {
                return;
            }
            match result {
                Ok(text) => prepared.set(Some(text)),
                Err(e) if e.code == ErrorCode::Cancelled => {}
                Err(e) => error.set(Some(error_message(&e))),
            }
        });
    };
    let text_id = move || prepared.with_untracked(|p| p.as_ref().map(|p| p.text_id));
    let with_ai = move |_| {
        let Some(text_id) = text_id() else {
            return;
        };
        running.set(true);
        error.set(None);
        progress.set(Some((AiStage::Starting, 0)));
        early.set_value(None);
        spawn_local(async move {
            let req = AiImport { text_id };
            let result = call::<_, JobId>("ai_import", &ReqArg { req: &req }).await;
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
    let without_ai = move |_| {
        let Some(text_id) = text_id() else {
            return;
        };
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            let result =
                call::<_, ImportPreview>("import_text_basic", &TextIdArg { text_id }).await;
            if busy.try_set(false).is_some() {
                return;
            }
            match result {
                Ok(p) => done.run(p),
                Err(e) => error.set(Some(error_message(&e))),
            }
        });
    };
    let cancel_job = move |_| {
        let Some(id) = job.get_untracked() else {
            return;
        };
        spawn_local(async move {
            if let Err(e) = call::<_, ()>("ai_cancel", &JobArg { job_id: &id }).await {
                let _ = error.try_set(Some(error_message(&e)));
            }
        });
    };

    let ready =
        move || status.with(|st| st.as_ref().is_some_and(|st| st.configured && st.consent_ok));
    let needs_consent = move || {
        status.with(|st| {
            st.as_ref()
                .filter(|st| st.configured && !st.consent_ok)
                .cloned()
        })
    };
    let locked = move || busy.get() || running.get();

    view! {
        <header class="bar"><h1>{s.text_import_title}</h1></header>
        <main class="list" class:import=true>
            <div class="form">
            <Show when=move || prepared.with(Option::is_none)>
                <p class="status">{s.text_import_hint}</p>
                <label>{s.text_import_paste}
                    <textarea rows="14" prop:value=move || pasted.get()
                        on:input=move |ev| pasted.set(event_target_value(&ev))></textarea>
                </label>
                <p class="new-cv">
                    <button prop:disabled=move || locked() || pasted.with(|p| p.trim().is_empty())
                        on:click=move |_| prepare(ImportTextSource::Pasted { text: pasted.get_untracked() })>{s.text_import_continue}</button>
                    <button prop:disabled=locked on:click=move |_| prepare(ImportTextSource::File)>{s.text_import_file}</button>
                    <button prop:disabled=locked on:click=move |_| close.run(())>{s.cancel}</button>
                </p>
            </Show>
            {move || prepared.get().map(|p| {
                let host = status.with(|st| st.as_ref().and_then(|st| st.host.clone())).unwrap_or_default();
                view! {
                    <p class="status">{fill(s.text_import_sent_to, &[("host", &host)])}</p>
                    <textarea class="sent-text" rows="14" readonly=true aria-label=s.text_import_sent_label prop:value=p.text.clone()></textarea>
                    <p class="status">{redacted_note(p.redacted)}</p>
                }
            })}
            <Show when=move || prepared.with(Option::is_some)>
                {move || match (status.get(), needs_consent()) {
                    (Some(st), _) if !st.configured => Some(view! { <p class="hint">{t().text_import_needs_ai}</p> }.into_any()),
                    (_, Some(st)) => Some(view! {
                        <ConsentPanel status=st accepted=Callback::new(move |st: AiStatus| status.set(Some(st))) />
                    }.into_any()),
                    _ => None,
                }}
                {move || progress.get().map(|(stage, pct)| view! {
                    <div class="tailor-progress">
                        <progress max="100" prop:value=pct aria-label=t().text_import_progress></progress>
                        <span>{stage_label(stage)}</span>
                        <button prop:disabled=move || job.with(Option::is_none) on:click=cancel_job>{t().cancel}</button>
                    </div>
                })}
                <p class="new-cv">
                    <Show when=ready>
                        <button prop:disabled=locked on:click=with_ai>{t().text_import_with_ai}</button>
                    </Show>
                    <button prop:disabled=locked on:click=without_ai>{t().text_import_without_ai}</button>
                    <button prop:disabled=locked on:click=move |_| prepared.set(None)>{t().back}</button>
                </p>
            </Show>
            <p class="error">{move || error.get()}</p>
            </div>
        </main>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_note_says_what_was_taken_out() {
        assert_eq!(redacted_note(0), t().text_import_nothing_redacted);
        assert!(redacted_note(3).contains('3'));
    }
}
