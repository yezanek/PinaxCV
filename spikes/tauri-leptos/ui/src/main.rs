use leptos::prelude::*;

/// Linux and macOS load custom schemes as `preview://localhost/…`;
/// Windows (WebView2) maps them to `http://preview.localhost/…`.
fn preview_base() -> &'static str {
    let protocol = web_sys::window().and_then(|w| w.location().protocol().ok()).unwrap_or_default();
    if protocol == "tauri:" { "preview://localhost" } else { "http://preview.localhost" }
}

#[component]
fn App() -> impl IntoView {
    let (n, set_n) = signal(0u32);
    view! {
        <h1>"PinaxCV spike"</h1>
        <button on:click=move |_| set_n.update(|n| *n += 1)>"Reload preview"</button>
        <p>{move || format!("reloads: {}", n.get())}</p>
        <img style="width:100%;border:1px solid #ccc"
             src=move || format!("{}/page/1.png?v={}", preview_base(), n.get()) />
    }
}

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}
