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
use crate::ipc::{IdArg, ReqArg, call};
use crate::lists::ListSections;
use crate::variants::VariantBar;

/// Preview debounce from spec section 16.
const PREVIEW_DELAY: Duration = Duration::from_millis(150);
/// Autosave debounce.
const SAVE_DELAY: Duration = Duration::from_millis(800);

/// Trims; an empty field becomes `None`.
fn opt(s: String) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_owned())
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
    // The variant being viewed; `None` is the base CV.
    let variant: RwSignal<Option<StoredVariant>> = RwSignal::new(None);

    let preview_now = move |edit: u64| {
        let Some(current) = cv.get_untracked() else {
            return;
        };
        let shown = variant.get_untracked().map(|v| v.variant);
        let req = PreviewRequest {
            target: Target {
                cv_id: current.id.clone(),
                variant_id: shown.as_ref().map(|v| v.id.clone()),
            },
            revision: edit,
            unsaved_cv: Some(current),
            unsaved_variant: shown,
        };
        spawn_local(async move {
            match call::<_, PreviewResponse>("preview", &ReqArg { req: &req }).await {
                // Ignore answers to older edits.
                Ok(resp) if resp.revision == edits.get_untracked() => pages.set(resp.page_urls),
                Ok(_) => {}
                Err(e) => error.set(Some(e.message)),
            }
        });
    };

    let save_now = move || {
        let Some(current) = cv.get_untracked() else {
            return;
        };
        let req = SaveCv {
            cv: current,
            expected_revision: revision.get_untracked(),
        };
        status.set("Saving…".into());
        spawn_local(async move {
            match call::<_, StoredCv>("save_cv", &ReqArg { req: &req }).await {
                Ok(stored) => {
                    revision.set(stored.revision);
                    status.set("Saved".into());
                    error.set(None);
                }
                // A conflict keeps the local edits and shows the message;
                // nothing is overwritten.
                Err(e) => {
                    status.set("Not saved".into());
                    error.set(Some(e.message));
                }
            }
        });
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
    let edit = Callback::new(move |f: Box<dyn FnOnce(&mut Cv)>| {
        cv.update(|c| {
            if let Some(c) = c {
                f(c);
            }
        });
        let n = edits.get_untracked() + 1;
        edits.set(n);
        status.set("Editing…".into());
        set_timeout(
            move || {
                if edits.get_untracked() == n {
                    preview_now(n)
                }
            },
            PREVIEW_DELAY,
        );
        set_timeout(
            move || {
                if edits.get_untracked() == n {
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
                    } />
            </label>
        }
    };

    // Re-renders the preview after the variant or its overlay changed.
    let variant_changed = Callback::new(move |()| {
        let n = edits.get_untracked() + 1;
        edits.set(n);
        preview_now(n);
    });

    let export_id = id.clone();
    let export = move |pdf: bool| {
        let target = Target {
            cv_id: export_id.clone(),
            variant_id: variant.with_untracked(|v| v.as_ref().map(|v| v.variant.id.clone())),
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
    let export_pdf = {
        let export = export.clone();
        move |_| export(true)
    };
    let export_html = move |_| export(false);

    view! {
        <header class="bar">
            <button on:click=move |_| open.set(None)>"← CVs"</button>
            <h1>{move || cv.with(|c| c.as_ref().map(|c| c.title.clone()).unwrap_or_default())}</h1>
            <span class="status">{move || status.get()}</span>
            <button on:click=export_pdf>"Export PDF"</button>
            <button on:click=export_html>"Export HTML"</button>
        </header>
        <div class="editor">
            <section class="form" aria-label="Content">
                <p class="error">{move || error.get()}</p>
                <VariantBar cv_id=id.clone() cv=cv variant=variant error=error changed=variant_changed />
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
                            edit.run(Box::new(move |c| c.summary = opt(v)));
                        }></textarea>
                </label>
                <EntrySections cv=cv edit=edit />
                <ListSections cv=cv edit=edit />
            </section>
            <section class="preview" aria-label="Preview">
                <For each=move || pages.get() key=|u| u.clone() let:url>
                    <img src=url alt="CV page preview" />
                </For>
            </section>
        </div>
    }
}
