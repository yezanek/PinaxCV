//! The editor: contact details and summary on the left, live preview on the
//! right. Edits autosave and re-render after a short pause (spec section 12).

use std::time::Duration;

use cv_api::{
    ApiError, ErrorCode, ExportHtmlRequest, ExportPdfRequest, ExportResult, PreviewRequest,
    PreviewResponse, SaveCv, Target,
};
use cv_core::{Cv, Id, StoredCv};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::ipc::{IdArg, ReqArg, call};

/// Preview debounce from spec section 16.
const PREVIEW_DELAY: Duration = Duration::from_millis(150);
/// Autosave debounce from spec section 12.
const SAVE_DELAY: Duration = Duration::from_millis(600);

/// Trims; an empty field becomes `None`.
fn opt(s: String) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_owned())
}

/// What to do once the newest edit is saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AfterSave {
    /// Export as PDF (`true`) or HTML (`false`).
    Export(bool),
    /// Go back to the CV list.
    Leave,
}

#[component]
pub fn Editor(id: Id, open: RwSignal<Option<Id>>) -> impl IntoView {
    let cv: RwSignal<Option<Cv>> = RwSignal::new(None);
    let revision = RwSignal::new(0u64);
    // Bumped on every edit; a timer acts only if no newer edit happened.
    let edits = RwSignal::new(0u64);
    let pages: RwSignal<Vec<String>> = RwSignal::new(Vec::new());
    let status = RwSignal::new(String::from("Loading…"));
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    // The last edit number that is on disk, whether a save is in flight,
    // and what to do once everything is saved.
    let saved_edit = RwSignal::new(0u64);
    let saving = RwSignal::new(false);
    let after_save: RwSignal<Option<AfterSave>> = RwSignal::new(None);
    // True while a newer preview is rendering; the old pages show dimmed.
    let rendering = RwSignal::new(false);

    let preview_now = move |edit: u64| {
        let Some(current) = cv.get_untracked() else {
            return;
        };
        let req = PreviewRequest {
            target: Target {
                cv_id: current.id.clone(),
                variant_id: None,
            },
            revision: edit,
            unsaved_cv: Some(current),
            unsaved_variant: None,
        };
        rendering.set(true);
        spawn_local(async move {
            let result = call::<_, PreviewResponse>("preview", &ReqArg { req: &req }).await;
            // Ignore answers to older edits; the newest one clears the spinner.
            if edits.try_get_untracked() != Some(edit) {
                return;
            }
            rendering.set(false);
            match result {
                Ok(resp) => pages.set(resp.page_urls),
                Err(e) => error.set(Some(e.message)),
            }
        });
    };

    // Kept in a stored value so the closures below stay `Copy`.
    let cv_id = StoredValue::new(id.clone());
    let export = move |pdf: bool| {
        let target = Target {
            cv_id: cv_id.get_value(),
            variant_id: None,
        };
        status.set("Exporting…".into());
        spawn_local(async move {
            let result: Result<ExportResult, ApiError> = if pdf {
                call(
                    "export_pdf",
                    &ReqArg {
                        req: &ExportPdfRequest { target },
                    },
                )
                .await
            } else {
                let req = ExportHtmlRequest {
                    target,
                    include_email: true,
                    include_phone: false,
                };
                call("export_html", &ReqArg { req: &req }).await
            };
            match result {
                Ok(r) => {
                    let ats = r
                        .ats
                        .map(|a| format!(", ATS score {}", a.score))
                        .unwrap_or_default();
                    status.set(format!("Exported to {}{ats}", r.path));
                    error.set(None);
                }
                Err(e) if e.code == ErrorCode::Cancelled => status.set("Export cancelled".into()),
                Err(e) => {
                    status.set("Export failed".into());
                    error.set(Some(e.message));
                }
            }
        });
    };

    // Saves until the newest edit is on disk. Only one save runs at a time:
    // an edit made while a save is in flight is sent next, with the revision
    // that save returned. When everything is saved, the queued action runs.
    let save_now = move || {
        if saving.get_untracked() || saved_edit.get_untracked() == edits.get_untracked() {
            return;
        }
        saving.set(true);
        status.set("Saving…".into());
        spawn_local(async move {
            loop {
                let Some(current) = cv.try_get_untracked().flatten() else {
                    let _ = saving.try_set(false);
                    return;
                };
                let edit = edits.get_untracked();
                let req = SaveCv {
                    cv: current,
                    expected_revision: revision.get_untracked(),
                };
                match call::<_, StoredCv>("save_cv", &ReqArg { req: &req }).await {
                    Ok(stored) => {
                        if revision.try_set(stored.revision).is_some() {
                            return; // The editor was closed.
                        }
                        saved_edit.set(edit);
                        if edits.get_untracked() > edit {
                            continue;
                        }
                        saving.set(false);
                        status.set("Saved".into());
                        error.set(None);
                        match after_save.get_untracked() {
                            Some(AfterSave::Export(pdf)) => {
                                after_save.set(None);
                                export(pdf);
                            }
                            Some(AfterSave::Leave) => open.set(None),
                            None => {}
                        }
                        return;
                    }
                    // A conflict keeps the local edits and shows the
                    // message; nothing is overwritten, and a queued export
                    // or leave is dropped.
                    Err(e) => {
                        if saving.try_set(false).is_some() {
                            return;
                        }
                        after_save.set(None);
                        status.set("Not saved".into());
                        error.set(Some(e.message));
                        return;
                    }
                }
            }
        });
    };

    // Runs `then` once the newest edit is saved, saving first if needed.
    let flush_then = move |then: AfterSave| {
        if !saving.get_untracked() && saved_edit.get_untracked() == edits.get_untracked() {
            match then {
                AfterSave::Export(pdf) => export(pdf),
                AfterSave::Leave => open.set(None),
            }
            return;
        }
        after_save.set(Some(then));
        save_now();
    };

    // Load once.
    spawn_local({
        let id = id.clone();
        async move {
            match call::<_, StoredCv>("get_cv", &IdArg { id: &id }).await {
                Ok(stored) => {
                    revision.set(stored.revision);
                    cv.set(Some(stored.cv));
                    status.set("Saved".into());
                    preview_now(0);
                }
                Err(e) => error.set(Some(e.message)),
            }
        }
    });

    // Applies `f` to the CV and schedules preview and autosave.
    let edit = move |f: Box<dyn FnOnce(&mut Cv)>| {
        cv.update(|c| {
            if let Some(c) = c {
                f(c);
            }
        });
        let n = edits.get_untracked() + 1;
        edits.set(n);
        status.set("Editing…".into());
        // Timers check the editor is still open (`try_get_untracked`).
        set_timeout(
            move || {
                if edits.try_get_untracked() == Some(n) {
                    preview_now(n)
                }
            },
            PREVIEW_DELAY,
        );
        set_timeout(
            move || {
                if edits.try_get_untracked() == Some(n) {
                    save_now()
                }
            },
            SAVE_DELAY,
        );
    };

    let field = move |label: &'static str, get: fn(&Cv) -> String, set: fn(&mut Cv, String)| {
        view! {
            <label>{label}
                <input prop:value=move || cv.with(|c| c.as_ref().map(get).unwrap_or_default())
                    on:input=move |ev| {
                        let v = event_target_value(&ev);
                        edit(Box::new(move |c| set(c, v)));
                    } />
            </label>
        }
    };

    let export_pdf = move |_| flush_then(AfterSave::Export(true));
    let export_html = move |_| flush_then(AfterSave::Export(false));
    let leave = move |_| flush_then(AfterSave::Leave);

    view! {
        <header class="bar">
            <button on:click=leave>"← CVs"</button>
            <h1>{move || cv.with(|c| c.as_ref().map(|c| c.title.clone()).unwrap_or_default())}</h1>
            <span class="status">{move || status.get()}</span>
            <button on:click=export_pdf>"Export PDF"</button>
            <button on:click=export_html>"Export HTML"</button>
        </header>
        <div class="editor">
            <section class="form" aria-label="Content">
                <p class="error">{move || error.get()}</p>
                {field("CV title (only you see this)", |c| c.title.clone(), |c, v| c.title = v)}
                {field("Full name", |c| c.basics.full_name.clone(), |c, v| c.basics.full_name = v)}
                {field("Headline", |c| c.basics.headline.clone().unwrap_or_default(), |c, v| c.basics.headline = opt(v))}
                {field("Email", |c| c.basics.email.clone().unwrap_or_default(), |c, v| c.basics.email = opt(v))}
                {field("Phone", |c| c.basics.phone.clone().unwrap_or_default(), |c, v| c.basics.phone = opt(v))}
                {field("Location", |c| c.basics.location.clone().unwrap_or_default(), |c, v| c.basics.location = opt(v))}
                <label>"Summary"
                    <textarea prop:value=move || cv.with(|c| c.as_ref().and_then(|c| c.summary.clone()).unwrap_or_default())
                        on:input=move |ev| {
                            let v = event_target_value(&ev);
                            edit(Box::new(move |c| c.summary = opt(v)));
                        }></textarea>
                </label>
            </section>
            <section class="preview" class:stale=move || rendering.get() aria-label="Preview"
                aria-busy=move || rendering.get().to_string()>
                <Show when=move || rendering.get()>
                    <span class="spinner" role="status" aria-label="Updating preview"></span>
                </Show>
                <For each=move || pages.get() key=|u| u.clone() let:url>
                    <img src=url alt="CV page preview" />
                </For>
            </section>
        </div>
    }
}
