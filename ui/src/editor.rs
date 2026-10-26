//! The editor: contact details and summary on the left, live preview on the
//! right. Edits autosave and re-render after a short pause (spec section 12).

use std::time::Duration;

use cv_api::{
    ApiError, ErrorCode, ExportHtmlRequest, ExportPdfRequest, ExportResult, PreviewRequest,
    PreviewResponse, SaveCv, Target,
};
use cv_core::{Cv, Id, StoredCv, StoredVariant};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::entries::EntrySections;
use crate::i18n::{error_message, fill, t};
use crate::ipc::{IdArg, ReqArg, call};
use crate::lists::ListSections;
use crate::style::StyleTab;
use crate::variants::VariantBar;

/// Preview debounce from spec section 16.
const PREVIEW_DELAY: Duration = Duration::from_millis(150);
/// Autosave debounce from spec section 12.
const SAVE_DELAY: Duration = Duration::from_millis(600);

/// A blank field becomes `None`; other text is kept as typed, so the input
/// is not rewritten mid-word (trailing spaces survive until the next key).
fn opt(s: String) -> Option<String> {
    (!s.trim().is_empty()).then_some(s)
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
    let status = RwSignal::new(String::from(t().loading));
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    // The variant being viewed; `None` is the base CV.
    let variant: RwSignal<Option<StoredVariant>> = RwSignal::new(None);
    // The last edit number that is on disk, whether a save is in flight,
    // and what to do once everything is saved.
    let saved_edit = RwSignal::new(0u64);
    let saving = RwSignal::new(false);
    let after_save: RwSignal<Option<AfterSave>> = RwSignal::new(None);
    // True while a newer preview is rendering; the old pages show dimmed.
    let rendering = RwSignal::new(false);
    // Bumped on every preview request, separately from `edits`, so a variant
    // change re-renders without touching the CV's autosave.
    let previews = RwSignal::new(0u64);
    // True while variant overlay edits are being saved, and what to run once
    // they are (export and leave read the variant from storage).
    let variant_busy = RwSignal::new(false);
    let after_variant: RwSignal<Option<AfterSave>> = RwSignal::new(None);

    let preview_now = move || {
        let Some(current) = cv.get_untracked() else {
            return;
        };
        let shown = variant.get_untracked().map(|v| v.variant);
        let n = previews.get_untracked() + 1;
        previews.set(n);
        let req = PreviewRequest {
            target: Target {
                cv_id: current.id.clone(),
                variant_id: shown.as_ref().map(|v| v.id.clone()),
            },
            revision: n,
            unsaved_cv: Some(current),
            unsaved_variant: shown,
        };
        rendering.set(true);
        spawn_local(async move {
            let result = call::<_, PreviewResponse>("preview", &ReqArg { req: &req }).await;
            // Ignore answers to older requests; the newest clears the spinner.
            if previews.try_get_untracked() != Some(n) {
                return;
            }
            rendering.set(false);
            match result {
                Ok(resp) => pages.set(resp.page_urls),
                Err(e) => error.set(Some(error_message(&e))),
            }
        });
    };

    // Re-renders the preview after the variant or its overlay changed.
    // Variants save themselves, so this does not make the CV unsaved.
    let variant_changed = Callback::new(move |()| preview_now());

    // Kept in a stored value so the closures below stay `Copy`.
    let cv_id = StoredValue::new(id.clone());
    let export = move |pdf: bool| {
        let target = Target {
            cv_id: cv_id.get_value(),
            variant_id: variant.with_untracked(|v| v.as_ref().map(|v| v.variant.id.clone())),
        };
        status.set(t().exporting.into());
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
                        .map(|a| fill(t().ats_score, &[("score", &a.score.to_string())]))
                        .unwrap_or_default();
                    status.set(fill(t().exported_to, &[("path", &r.path), ("ats", &ats)]));
                    error.set(None);
                }
                Err(e) if e.code == ErrorCode::Cancelled => status.set(t().export_cancelled.into()),
                Err(e) => {
                    status.set(t().export_failed.into());
                    error.set(Some(error_message(&e)));
                }
            }
        });
    };

    // Runs `then` once variant edits are on disk too.
    let finish = move |then: AfterSave| {
        if variant_busy.get_untracked() {
            after_variant.set(Some(then));
            status.set(t().saving.into());
            return;
        }
        match then {
            AfterSave::Export(pdf) => export(pdf),
            AfterSave::Leave => open.set(None),
        }
    };
    // The variant save queue emptied; `ok` is false if a save failed, and
    // then the waiting export or leave is dropped.
    let variant_settled = Callback::new(move |ok: bool| {
        let Some(then) = after_variant.get_untracked() else {
            return;
        };
        after_variant.set(None);
        if ok {
            finish(then);
        } else {
            status.set(t().not_saved.into());
        }
    });

    // Saves until the newest edit is on disk. Only one save runs at a time:
    // an edit made while a save is in flight is sent next, with the revision
    // that save returned. When everything is saved, the queued action runs.
    let save_now = move || {
        if saving.get_untracked() || saved_edit.get_untracked() == edits.get_untracked() {
            return;
        }
        saving.set(true);
        status.set(t().saving.into());
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
                        status.set(t().saved.into());
                        error.set(None);
                        if let Some(then) = after_save.get_untracked() {
                            after_save.set(None);
                            finish(then);
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
                        status.set(t().not_saved.into());
                        error.set(Some(error_message(&e)));
                        return;
                    }
                }
            }
        });
    };

    // Runs `then` once the newest edit is saved, saving first if needed.
    let flush_then = move |then: AfterSave| {
        if !saving.get_untracked() && saved_edit.get_untracked() == edits.get_untracked() {
            finish(then);
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
                    status.set(t().saved.into());
                    preview_now();
                }
                Err(e) => error.set(Some(error_message(&e))),
            }
        }
    });

    // Applies `f` to the CV and schedules preview and autosave.
    let edit = Callback::new(move |f: Box<dyn FnOnce(&mut Cv)>| {
        cv.update(|c| {
            if let Some(c) = c {
                f(c);
            }
        });
        let n = edits.get_untracked() + 1;
        edits.set(n);
        status.set(t().editing.into());
        // Timers check the editor is still open (`try_get_untracked`).
        set_timeout(
            move || {
                if edits.try_get_untracked() == Some(n) {
                    preview_now()
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
    });

    let field = move |label: &'static str, get: fn(&Cv) -> String, set: fn(&mut Cv, String)| {
        view! {
            <label>{label}
                <input prop:value=move || cv.with(|c| c.as_ref().map(get).unwrap_or_default())
                    on:input=move |ev| {
                        let v = event_target_value(&ev);
                        edit.run(Box::new(move |c| set(c, v)));
                    }
                    // Trim when the field loses focus, not while typing.
                    on:change=move |ev| {
                        let v = event_target_value(&ev);
                        if v.trim().len() != v.len() {
                            let t = v.trim().to_owned();
                            edit.run(Box::new(move |c| set(c, t)));
                        }
                    } />
            </label>
        }
    };

    // The Style tab only hides the Content tab, so inputs keep their state.
    let style_tab = RwSignal::new(false);
    let export_pdf = move |_| flush_then(AfterSave::Export(true));
    let export_html = move |_| flush_then(AfterSave::Export(false));
    let leave = move |_| flush_then(AfterSave::Leave);

    view! {
        <header class="bar">
            <button on:click=leave>{t().back_to_cvs}</button>
            <h1>{move || cv.with(|c| c.as_ref().map(|c| c.title.clone()).unwrap_or_default())}</h1>
            <span class="status">{move || status.get()}</span>
            <button on:click=export_pdf>{t().export_pdf}</button>
            <button on:click=export_html>{t().export_html}</button>
        </header>
        <div class="editor">
            <section class="form">
                <div class="tabs" role="tablist">
                    <button id="tab-content" role="tab" aria-controls="panel-content" aria-selected=move || (!style_tab.get()).to_string()
                        on:click=move |_| style_tab.set(false)>{t().content}</button>
                    <button id="tab-style" role="tab" aria-controls="panel-style" aria-selected=move || style_tab.get().to_string()
                        on:click=move |_| style_tab.set(true)>{t().style}</button>
                </div>
                <p class="error">{move || error.get()}</p>
                <div id="panel-style" role="tabpanel" aria-labelledby="tab-style" hidden=move || !style_tab.get()>
                    <StyleTab cv=cv edit=edit error=error />
                </div>
                <div id="panel-content" role="tabpanel" aria-labelledby="tab-content" hidden=move || style_tab.get()>
                <VariantBar cv_id=id.clone() cv=cv variant=variant error=error changed=variant_changed
                    busy=variant_busy settled=variant_settled />
                {field(t().cv_title, |c| c.title.clone(), |c, v| c.title = v)}
                {field(t().full_name, |c| c.basics.full_name.clone(), |c, v| c.basics.full_name = v)}
                {field(t().headline, |c| c.basics.headline.clone().unwrap_or_default(), |c, v| c.basics.headline = opt(v))}
                {field(t().email, |c| c.basics.email.clone().unwrap_or_default(), |c, v| c.basics.email = opt(v))}
                {field(t().phone, |c| c.basics.phone.clone().unwrap_or_default(), |c, v| c.basics.phone = opt(v))}
                {field(t().location, |c| c.basics.location.clone().unwrap_or_default(), |c, v| c.basics.location = opt(v))}
                <label>{t().summary}
                    <textarea prop:value=move || cv.with(|c| c.as_ref().and_then(|c| c.summary.clone()).unwrap_or_default())
                        on:input=move |ev| {
                            let v = event_target_value(&ev);
                            edit.run(Box::new(move |c| c.summary = opt(v)));
                        }></textarea>
                </label>
                <EntrySections cv=cv edit=edit />
                <ListSections cv=cv edit=edit />
                </div>
            </section>
            <section class="preview" class:stale=move || rendering.get() aria-label=t().preview
                aria-busy=move || rendering.get().to_string()>
                <Show when=move || rendering.get()>
                    <span class="spinner" role="status" aria-label=t().updating_preview></span>
                </Show>
                <For each=move || pages.get() key=|u| u.clone() let:url>
                    <img src=url alt=t().page_preview />
                </For>
            </section>
        </div>
    }
}
