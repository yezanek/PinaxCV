//! PinaxCV editor UI (M3): the CV list and the editor with live preview.
#![forbid(unsafe_code)]

use cv_core::Id;
use leptos::prelude::*;

mod editor;
mod entries;
mod ids;
mod ipc;
mod list;

#[component]
fn App() -> impl IntoView {
    let open: RwSignal<Option<Id>> = RwSignal::new(None);
    view! {
        {move || match open.get() {
            None => view! { <list::CvList open=open /> }.into_any(),
            Some(id) => view! { <editor::Editor id=id open=open /> }.into_any(),
        }}
    }
}

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}
