//! PinaxCV editor UI (M3): the CV list and the editor with live preview.
#![forbid(unsafe_code)]

use cv_core::Id;
use leptos::prelude::*;

mod checks;
mod dates;
mod drag;
mod editor;
mod entries;
mod fit;
mod history;
mod i18n;
mod ids;
mod import;
mod ipc;
mod list;
mod lists;
mod order;
mod preview;
mod settings;
mod style;
mod tailor;
mod variants;

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
