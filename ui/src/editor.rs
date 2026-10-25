//! The editor: contact details and summary on the left, live preview on the
//! right. Edits autosave and re-render after a short pause (spec section 12).

use std::time::Duration;

use cv_api::{
    ApiError, ErrorCode, ExportHtmlRequest, ExportPdfRequest, ExportResult, PreviewRequest,
    PreviewResponse, SaveCv, Target,
};
use cv_core::lint::SUMMARY_MAX_CHARS;
use cv_core::{Cv, Diagnostic, Id, StoredCv, StoredVariant};
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;

use crate::checks::{ChecksTab, EMAIL_FIELD, FULL_NAME_FIELD, SUMMARY_FIELD, Spot, focus_spot};
use crate::entries::{AddSection, Sections};
use crate::history::{History, Shortcut, shortcut};
use crate::i18n::{error_message, fill, t};
use crate::ipc::{IdArg, ReqArg, call};
use crate::preview::{DiagnosticsStrip, Zoom, ZoomControls, diagnostics_from, page_width};
use crate::style::StyleTab;
use crate::variants::VariantBar;

/// Preview debounce from spec section 16.
const PREVIEW_DELAY: Duration = Duration::from_millis(150);
/// Autosave debounce from spec section 12.
const SAVE_DELAY: Duration = Duration::from_millis(600);

/// The editor's tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Content,
    Style,
    Checks,
}

impl Tab {
    /// The tab button's element ID and its panel's.
    fn ids(self) -> (&'static str, &'static str) {
        match self {
            Tab::Content => ("tab-content", "panel-content"),
            Tab::Style => ("tab-style", "panel-style"),
            Tab::Checks => ("tab-checks", "panel-checks"),
        }
    }
}

/// "120 / 600 characters" under the summary; past the limit the lint check
/// warns that the summary is too long.
fn summary_count(n: usize) -> String {
    fill(
        t().summary_count,
        &[
            ("n", &n.to_string()),
            ("max", &SUMMARY_MAX_CHARS.to_string()),
        ],
    )
}

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
    // Template messages from the newest preview, and the page zoom.
    let diagnostics: RwSignal<Vec<Diagnostic>> = RwSignal::new(Vec::new());
    let zoom = RwSignal::new(Zoom::FitWidth);
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
                Ok(resp) => {
                    pages.set(resp.page_urls);
                    diagnostics.set(resp.diagnostics);
                }
                Err(e) => {
                    diagnostics.set(diagnostics_from(&e));
                    error.set(Some(error_message(&e)));
                }
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

    // Undo/redo snapshots, and a counter bumped when one is restored so the
    // fields that only read the model once (dates, skill lines) start over.
    let history = StoredValue::new(History::default());
    let can_undo = RwSignal::new(false);
    let can_redo = RwSignal::new(false);
    let restored = RwSignal::new(0u64);
    let sync_history = move || {
        history.with_value(|h| {
            can_undo.set(h.can_undo());
            can_redo.set(h.can_redo());
        })
    };

    // Schedules preview and autosave after the CV changed.
    let changed = move || {
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
    };

    // Applies `f` to the CV, records the old CV for undo, and schedules
    // preview and autosave.
    let edit = Callback::new(move |f: Box<dyn FnOnce(&mut Cv)>| {
        let now = js_sys::Date::now();
        cv.update(|c| {
            if let Some(c) = c {
                history.update_value(|h| h.record(c, now));
                f(c);
            }
        });
        sync_history();
        changed();
    });

    // Puts back the CV from the undo (`true`) or redo stack.
    let step = move |back: bool| {
        let Some(current) = cv.get_untracked() else {
            return;
        };
        let target = history
            .try_update_value(|h| {
                if back {
                    h.undo(&current)
                } else {
                    h.redo(&current)
                }
            })
            .flatten();
        let Some(target) = target else {
            return;
        };
        cv.set(Some(target));
        restored.update(|n| *n += 1);
        sync_history();
        changed();
    };

    // Blurs the focused field so its `change` handler commits what was typed
    // (variant text areas only save on change) before a shortcut acts.
    let commit_focused = || {
        if let Some(el) = document().active_element()
            && let Ok(el) = el.dyn_into::<web_sys::HtmlElement>()
        {
            el.blur().ok();
        }
    };

    // Keyboard shortcuts (spec section 12.1). Undo and redo replace the
    // browser's per-field undo, which would not match the CV model.
    let keys = window_event_listener(leptos::ev::keydown, move |ev| {
        let ctrl = ev.ctrl_key() || ev.meta_key();
        let Some(action) = shortcut(&ev.key(), ctrl, ev.shift_key(), ev.alt_key()) else {
            return;
        };
        ev.prevent_default();
        match action {
            Shortcut::Undo => step(true),
            Shortcut::Redo => step(false),
            Shortcut::Save => save_now(),
            Shortcut::Export => {
                commit_focused();
                flush_then(AfterSave::Export(true));
            }
        }
    });
    on_cleanup(move || keys.remove());

    let field = move |label: &'static str,
                      id: Option<&'static str>,
                      get: fn(&Cv) -> String,
                      set: fn(&mut Cv, String)| {
        view! {
            <label>{label}
                <input id=id prop:value=move || cv.with(|c| c.as_ref().map(get).unwrap_or_default())
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

    let summary_chars = move || {
        cv.with(|c| {
            c.as_ref()
                .and_then(|c| c.summary.as_deref())
                .map_or(0, |s| s.chars().count())
        })
    };

    // The other tabs only hide the Content tab, so inputs keep their state.
    let tab = RwSignal::new(Tab::Content);
    let id_for_checks = id.clone();
    let tab_button = move |which: Tab, label: &'static str| {
        let (id, panel) = which.ids();
        view! {
            <button id=id role="tab" aria-controls=panel aria-selected=move || (tab.get() == which).to_string()
                on:click=move |_| tab.set(which)>{label}</button>
        }
    };
    // Runs the checks again after each save of the CV or the variant.
    let checks_refresh = Signal::derive(move || {
        saved_edit.get() + variant.with(|v| v.as_ref().map_or(0, |v| v.revision))
    });
    let checks_variant =
        Signal::derive(move || variant.with(|v| v.as_ref().map(|v| v.variant.id.clone())));
    let checks_overlay =
        Signal::derive(move || variant.with(|v| v.as_ref().map(|v| v.variant.clone())));
    let jump = Callback::new(move |to: Spot| {
        tab.set(Tab::Content);
        set_timeout(move || focus_spot(&to), Duration::ZERO);
    });
    let export_pdf = move |_| flush_then(AfterSave::Export(true));
    let export_html = move |_| flush_then(AfterSave::Export(false));
    let leave = move |_| flush_then(AfterSave::Leave);

    view! {
        <header class="bar">
            <button on:click=leave>{t().back_to_cvs}</button>
            <h1>{move || cv.with(|c| c.as_ref().map(|c| c.title.clone()).unwrap_or_default())}</h1>
            <span class="status">{move || status.get()}</span>
            <button aria-keyshortcuts="Control+Z" title=t().undo_hint prop:disabled=move || !can_undo.get()
                on:click=move |_| step(true)>{t().undo}</button>
            <button aria-keyshortcuts="Control+Shift+Z" title=t().redo_hint prop:disabled=move || !can_redo.get()
                on:click=move |_| step(false)>{t().redo}</button>
            <button aria-keyshortcuts="Control+E" title=t().export_pdf_hint on:click=export_pdf>{t().export_pdf}</button>
            <button on:click=export_html>{t().export_html}</button>
        </header>
        <div class="editor">
            <section class="form">
                <div class="tabs" role="tablist">
                    {tab_button(Tab::Content, t().content)}
                    {tab_button(Tab::Style, t().style)}
                    {tab_button(Tab::Checks, t().checks)}
                </div>
                <p class="error">{move || error.get()}</p>
                <div id="panel-style" role="tabpanel" aria-labelledby="tab-style" hidden=move || tab.get() != Tab::Style>
                    <StyleTab cv=cv edit=edit error=error />
                </div>
                <div id="panel-checks" role="tabpanel" aria-labelledby="tab-checks" hidden=move || tab.get() != Tab::Checks>
                    <Show when=move || tab.get() == Tab::Checks>
                        <ChecksTab cv_id=id_for_checks.clone() variant_id=checks_variant variant=checks_overlay refresh=checks_refresh jump=jump />
                    </Show>
                </div>
                <div id="panel-content" role="tabpanel" aria-labelledby="tab-content" hidden=move || tab.get() != Tab::Content>
                <VariantBar cv_id=id.clone() cv=cv variant=variant error=error changed=variant_changed
                    busy=variant_busy settled=variant_settled />
                {field(t().cv_title, None, |c| c.title.clone(), |c, v| c.title = v)}
                {field(t().full_name, Some(FULL_NAME_FIELD), |c| c.basics.full_name.clone(), |c, v| c.basics.full_name = v)}
                {field(t().headline, None, |c| c.basics.headline.clone().unwrap_or_default(), |c, v| c.basics.headline = opt(v))}
                {field(t().email, Some(EMAIL_FIELD), |c| c.basics.email.clone().unwrap_or_default(), |c, v| c.basics.email = opt(v))}
                {field(t().phone, None, |c| c.basics.phone.clone().unwrap_or_default(), |c, v| c.basics.phone = opt(v))}
                {field(t().location, None, |c| c.basics.location.clone().unwrap_or_default(), |c, v| c.basics.location = opt(v))}
                <label>{t().summary}
                    <textarea id=SUMMARY_FIELD aria-describedby="summary-count"
                        prop:value=move || cv.with(|c| c.as_ref().and_then(|c| c.summary.clone()).unwrap_or_default())
                        on:input=move |ev| {
                            let v = event_target_value(&ev);
                            edit.run(Box::new(move |c| c.summary = opt(v)));
                        }></textarea>
                </label>
                {move || {
                    let n = summary_chars();
                    let over = n > SUMMARY_MAX_CHARS;
                    view! {
                        <p id="summary-count" class="counter" class:over=over>
                            {summary_count(n)}
                        </p>
                    }
                }}
                {move || {
                    restored.track();
                    view! {
                        <Sections cv=cv edit=edit />
                    }
                }}
                <AddSection edit=edit />
                </div>
            </section>
            <section class="preview" class:stale=move || rendering.get() aria-label=t().preview
                aria-busy=move || rendering.get().to_string()>
                <div class="preview-bar">
                    <ZoomControls zoom=zoom />
                    <Show when=move || rendering.get()>
                        <span class="spinner" role="status" aria-label=t().updating_preview></span>
                    </Show>
                </div>
                <DiagnosticsStrip diagnostics=diagnostics />
                <For each=move || pages.get() key=|u| u.clone() let:url>
                    <img src=url alt=t().page_preview class:actual=move || zoom.get() == Zoom::Actual
                        style:width=move || page_width(zoom.get(), cv.with(|c| c.as_ref().map(|c| c.paper).unwrap_or_default())) />
                </For>
            </section>
        </div>
    }
}
