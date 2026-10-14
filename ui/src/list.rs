//! The home screen: the user's CVs, and creating a new one.

use cv_api::{ApiError, NewCv};
use cv_core::{CvSummary, Id, StoredCv};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::ipc::{IdArg, NoArgs, ReqArg, call};

#[component]
pub fn CvList(open: RwSignal<Option<Id>>) -> impl IntoView {
    let cvs: RwSignal<Vec<CvSummary>> = RwSignal::new(Vec::new());
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let title = RwSignal::new(String::new());

    let reload = move || {
        spawn_local(async move {
            match call::<_, Vec<CvSummary>>("list_cvs", &NoArgs {}).await {
                Ok(list) => cvs.set(list),
                Err(e) => error.set(Some(e.message)),
            }
        });
    };
    reload();

    let create = move |_| {
        let req = NewCv {
            title: title.get_untracked(),
        };
        spawn_local(async move {
            match call::<_, StoredCv>("create_cv", &ReqArg { req: &req }).await {
                Ok(stored) => open.set(Some(stored.cv.id)),
                Err(e) => error.set(Some(e.message)),
            }
        });
    };

    let delete = move |id: Id| {
        spawn_local(async move {
            let result: Result<(), ApiError> = call("delete_cv", &IdArg { id: &id }).await;
            match result {
                Ok(()) => reload(),
                Err(e) => error.set(Some(e.message)),
            }
        });
    };

    view! {
        <header class="bar"><h1>"PinaxCV"</h1></header>
        <main class="list">
            <h2>"Your CVs"</h2>
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
                                <button on:click=move |_| delete(del.clone())>"Delete"</button>
                            </li>
                        }
                    }
                </For>
            </ul>
            <p>
                <input placeholder="Title, for example Main CV" prop:value=move || title.get()
                    on:input=move |ev| title.set(event_target_value(&ev)) />
                " "
                <button on:click=create>"New CV"</button>
            </p>
        </main>
    }
}
