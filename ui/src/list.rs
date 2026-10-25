//! The home screen (spec section 12.2): the user's CVs as cards, creating,
//! duplicating, deleting and importing them. A delete can be undone from a
//! toast.

use std::time::Duration;

use cv_api::{ApiError, DuplicateCv, ImportPreview, NewCv};
use cv_core::{CvSummary, Id, StoredCv};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::i18n::{error_message, fill, t};
use crate::import::{ImportWizard, choose_file};
use crate::ipc::{IdArg, NoArgs, ReqArg, call};
use crate::settings::Settings;
use crate::text_import::TextImport;

/// How long the "Deleted" toast offers Undo.
const UNDO_TIME: Duration = Duration::from_secs(8);

/// "24 Oct 2026, 09:05" from local date parts; `month` is 0-based, as
/// JavaScript's `Date` gives it.
pub fn date_label(year: u32, month: u32, day: u32, hour: u32, minute: u32) -> String {
    let s = t();
    let name = s.months.get(month as usize).copied().unwrap_or("?");
    fill(
        s.date_time,
        &[
            ("day", &day.to_string()),
            ("month", name),
            ("year", &year.to_string()),
            ("time", &format!("{hour:02}:{minute:02}")),
        ],
    )
}

/// "Edited …" in local time, or nothing if the timestamp does not parse.
fn edited(updated_at: &str) -> Option<String> {
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_str(updated_at));
    if d.get_time().is_nan() {
        return None;
    }
    let date = date_label(
        d.get_full_year(),
        d.get_month(),
        d.get_date(),
        d.get_hours(),
        d.get_minutes(),
    );
    Some(fill(t().edited, &[("date", &date)]))
}

/// "1 page" or "3 pages"; the same for variants.
pub fn count_label(n: u32, one: &str, many: &str) -> String {
    if n == 1 {
        one.to_owned()
    } else {
        fill(many, &[("n", &n.to_string())])
    }
}

/// The card's details line: last edited, page count (if known) and
/// variant count.
fn details(cv: &CvSummary) -> String {
    let s = t();
    let mut parts = Vec::new();
    parts.extend(edited(&cv.updated_at.0));
    if let Some(pages) = cv.page_count {
        parts.push(count_label(pages, s.one_page, s.n_pages));
    }
    parts.push(count_label(cv.variant_count, s.one_variant, s.n_variants));
    parts.join(" · ")
}

/// A deleted CV that can still be restored from its toast.
#[derive(Clone)]
struct Deleted {
    key: u64,
    id: Id,
    title: String,
    restoring: bool,
}

#[component]
pub fn CvList(open: RwSignal<Option<Id>>) -> impl IntoView {
    let cvs: RwSignal<Vec<CvSummary>> = RwSignal::new(Vec::new());
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let title = RwSignal::new(String::new());
    // Guards against a double click creating two CVs.
    let creating = RwSignal::new(false);
    // Deleted CVs whose Undo toast still shows, each with its own key.
    let deleted: RwSignal<Vec<Deleted>> = RwSignal::new(Vec::new());
    let toasts = StoredValue::new(0u64);
    // Only the latest list request may set the list.
    let loads = StoredValue::new(0u64);
    // The open import, while the wizard shows; `choosing` while the file
    // dialog is open.
    let importing: RwSignal<Option<ImportPreview>> = RwSignal::new(None);
    let settings_open = RwSignal::new(false);
    let choosing = RwSignal::new(false);
    // True while the text import screen shows.
    let text_import = RwSignal::new(false);

    let reload = move || {
        let Some(n) = loads.try_update_value(|n| {
            *n += 1;
            *n
        }) else {
            return;
        };
        spawn_local(async move {
            let result = call::<_, Vec<CvSummary>>("list_cvs", &NoArgs {}).await;
            if loads.try_get_value() != Some(n) {
                return;
            }
            match result {
                Ok(list) => {
                    let _ = cvs.try_set(list);
                }
                Err(e) => {
                    let _ = error.try_set(Some(error_message(&e)));
                }
            }
        });
    };
    // Hides a toast after the undo time, unless its restore is running.
    let expire = move |key: u64| {
        set_timeout(
            move || {
                let _ = deleted.try_update(|d| d.retain(|x| x.key != key || x.restoring));
            },
            UNDO_TIME,
        );
    };
    reload();

    let create = move |_| {
        if creating.get_untracked() {
            return;
        }
        creating.set(true);
        let req = NewCv {
            title: title.get_untracked(),
        };
        spawn_local(async move {
            let result = call::<_, StoredCv>("create_cv", &ReqArg { req: &req }).await;
            if creating.try_set(false).is_some() {
                return; // The list was closed.
            }
            match result {
                Ok(stored) => open.set(Some(stored.cv.id)),
                Err(e) => error.set(Some(error_message(&e))),
            }
        });
    };

    let duplicate = move |id: Id, title: String| {
        let req = DuplicateCv {
            id,
            new_title: fill(t().copy_title, &[("title", &title)]),
        };
        spawn_local(async move {
            match call::<_, StoredCv>("duplicate_cv", &ReqArg { req: &req }).await {
                Ok(_) => reload(),
                Err(e) => {
                    let _ = error.try_set(Some(error_message(&e)));
                }
            }
        });
    };

    let delete = move |id: Id, title: String| {
        spawn_local(async move {
            let result: Result<(), ApiError> = call("delete_cv", &IdArg { id: &id }).await;
            match result {
                Ok(()) => {
                    let Some(key) = toasts.try_update_value(|n| {
                        *n += 1;
                        *n
                    }) else {
                        return;
                    };
                    let _ = deleted.try_update(|d| {
                        d.push(Deleted {
                            key,
                            id,
                            title,
                            restoring: false,
                        })
                    });
                    reload();
                    expire(key);
                }
                Err(e) => {
                    let _ = error.try_set(Some(error_message(&e)));
                }
            }
        });
    };

    // Restores one deleted CV. Its toast stays until the restore succeeds,
    // so a failed one can be tried again.
    let undo_delete = move |key: u64| {
        let mut id = None;
        deleted.update(|d| {
            if let Some(x) = d.iter_mut().find(|x| x.key == key && !x.restoring) {
                x.restoring = true;
                id = Some(x.id.clone());
            }
        });
        let Some(id) = id else {
            return;
        };
        error.set(None);
        spawn_local(async move {
            let result: Result<(), ApiError> = call("restore_cv", &IdArg { id: &id }).await;
            match result {
                Ok(()) => {
                    let _ = deleted.try_update(|d| d.retain(|x| x.key != key));
                    reload();
                }
                Err(e) => {
                    let _ = deleted.try_update(|d| {
                        if let Some(x) = d.iter_mut().find(|x| x.key == key) {
                            x.restoring = false;
                        }
                    });
                    let _ = error.try_set(Some(error_message(&e)));
                    expire(key);
                }
            }
        });
    };

    let start_import = move |_| {
        if choosing.get_untracked() {
            return;
        }
        choosing.set(true);
        error.set(None);
        spawn_local(async move {
            let result = choose_file().await;
            if choosing.try_set(false).is_some() {
                return;
            }
            match result {
                Ok(preview) => importing.set(preview),
                Err(message) => error.set(Some(message)),
            }
        });
    };

    let s = t();
    let home = move || {
        if settings_open.get() {
            return view! { <Settings close=Callback::new(move |()| settings_open.set(false)) /> }
                .into_any();
        }
        if text_import.get() {
            return view! {
                <TextImport close=Callback::new(move |()| text_import.set(false))
                    done=Callback::new(move |p: ImportPreview| { text_import.set(false); importing.set(Some(p)); }) />
            }
            .into_any();
        }
        if let Some(preview) = importing.get() {
            return view! { <ImportWizard preview=preview cvs=cvs.get_untracked() importing=importing open=open /> }
                .into_any();
        }
        view! {
            <header class="bar">
                <h1>{s.app_name}</h1>
                <button class="settings-button" on:click=move |_| settings_open.set(true)>{s.settings}</button>
            </header>
            <main class="list">
                <h2>{s.your_cvs}</h2>
                <p class="error">{move || error.get()}</p>
                <p class="new-cv">
                    <input aria-label=s.new_cv_title placeholder=s.new_cv_title_placeholder prop:value=move || title.get()
                        on:input=move |ev| title.set(event_target_value(&ev)) />
                    <button on:click=create prop:disabled=move || creating.get()>{s.new_cv}</button>
                    <button title=s.import_cv_hint on:click=start_import prop:disabled=move || choosing.get()>{s.import_cv}</button>
                    <button title=s.import_text_hint on:click=move |_| { error.set(None); text_import.set(true) }>{s.import_text}</button>
                </p>
                <Show when=move || cvs.with(Vec::is_empty)>
                    <p class="status">{s.no_cvs_yet}</p>
                </Show>
                <ul class="cards">
                    <For each=move || cvs.get() key=|c| (c.id.clone(), c.updated_at.0.clone(), c.title.clone()) let:cv>
                        {
                            let (id, dup, del) = (cv.id.clone(), cv.id.clone(), cv.id.clone());
                            let (dup_title, del_title) = (cv.title.clone(), cv.title.clone());
                            view! {
                                <li class="card">
                                    <button class="open" on:click=move |_| open.set(Some(id.clone()))>
                                        <strong>{cv.title.clone()}</strong>
                                        <span class="status">{details(&cv)}</span>
                                    </button>
                                    <div class="card-actions">
                                        <button on:click=move |_| duplicate(dup.clone(), dup_title.clone())>{s.duplicate}</button>
                                        <button on:click=move |_| delete(del.clone(), del_title.clone())>{s.delete}</button>
                                    </div>
                                </li>
                            }
                        }
                    </For>
                </ul>
            </main>
            <div class="toasts" role="status">
                <For each=move || deleted.get() key=|d| (d.key, d.restoring) let:d>
                    <div class="toast">
                        <span>{fill(s.deleted_cv, &[("title", &d.title)])}</span>
                        <button prop:disabled=d.restoring on:click=move |_| undo_delete(d.key)>{s.undo}</button>
                    </div>
                </For>
            </div>
        }
        .into_any()
    };
    view! { {home} }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_read_day_month_year_and_time() {
        assert_eq!(date_label(2026, 9, 24, 9, 5), "24 Oct 2026, 09:05");
        assert_eq!(date_label(2026, 0, 1, 23, 59), "1 Jan 2026, 23:59");
        assert_eq!(date_label(2026, 12, 1, 0, 0), "1 ? 2026, 00:00");
    }

    #[test]
    fn counts_use_singular_for_one() {
        let s = t();
        assert_eq!(count_label(1, s.one_page, s.n_pages), "1 page");
        assert_eq!(count_label(3, s.one_page, s.n_pages), "3 pages");
        assert_eq!(count_label(0, s.one_variant, s.n_variants), "0 variants");
    }
}
