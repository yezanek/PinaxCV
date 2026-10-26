//! The home screen: the user's CVs, and creating a new one.

use cv_api::{ApiError, NewCv};
use cv_core::{CvSummary, Id, StoredCv};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::i18n::{error_message, t};
use crate::ipc::{IdArg, NoArgs, ReqArg, call};

#[component]
pub fn CvList(open: RwSignal<Option<Id>>) -> impl IntoView {
    let cvs: RwSignal<Vec<CvSummary>> = RwSignal::new(Vec::new());
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let title = RwSignal::new(String::new());
    // Guards against a double click creating two CVs.
    let creating = RwSignal::new(false);

    let reload = move || {
        spawn_local(async move {
            match call::<_, Vec<CvSummary>>("list_cvs", &NoArgs {}).await {
                Ok(list) => cvs.set(list),
                Err(e) => error.set(Some(error_message(&e))),
            }
        });
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

    let delete = move |id: Id| {
        spawn_local(async move {
            let result: Result<(), ApiError> = call("delete_cv", &IdArg { id: &id }).await;
            match result {
                Ok(()) => reload(),
                Err(e) => error.set(Some(error_message(&e))),
            }
        });
    };

    view! {
        <header class="bar"><h1>{t().app_name}</h1></header>
        <main class="list">
            <h2>{t().your_cvs}</h2>
            <p class="error">{move || error.get()}</p>
            <ul>
                <For each=move || cvs.get() key=|c| c.id.clone() let:cv>
                    {
                        let id = cv.id.clone();
                        let del = cv.id.clone();
                        view! {
                            <li>
                                <button class="open" on:click=move |_| open.set(Some(id.clone()))>
                                    {cv.title.clone()}
                                </button>
                                <button on:click=move |_| delete(del.clone())>{t().delete}</button>
                            </li>
                        }
                    }
                </For>
            </ul>
            <p>
                <input placeholder=t().new_cv_title_placeholder prop:value=move || title.get()
                    on:input=move |ev| title.set(event_target_value(&ev)) />
                " "
                <button on:click=create prop:disabled=move || creating.get()>{t().new_cv}</button>
            </p>
        </main>
    }
}
