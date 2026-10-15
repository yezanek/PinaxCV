#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// A pre-rendered Typst page; the real app renders on demand and caches by revision.
const PAGE: &[u8] = include_bytes!("../assets/page1.png");

fn main() {
    tauri::Builder::default()
        .register_uri_scheme_protocol("preview", |_ctx, request| {
            let ok = request.uri().path().starts_with("/page/");
            tauri::http::Response::builder()
                .status(if ok { 200 } else { 404 })
                .header("Content-Type", "image/png")
                .header("Cache-Control", "no-store")
                .body(if ok { PAGE.to_vec() } else { Vec::new() })
                .expect("response")
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
