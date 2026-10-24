//! PinaxCV desktop shell (spec section 10): opens the database, builds the
//! `cv-app` services, and registers the commands and the `preview://` scheme.
#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use cv_app::{App, UlidIds};
use cv_store::SqliteStore;
use tauri::Manager;

mod commands;
mod dialog;
mod preview;

/// File name of the database inside the app data directory.
const DB_FILE: &str = "pinaxcv.sqlite3";

/// The platform file dialogs, or in an end-to-end test build the dialogs
/// that save into `PINAXCV_E2E_EXPORT_DIR`.
fn file_dialog(handle: &tauri::AppHandle) -> Box<dyn cv_app::FileDialog> {
    #[cfg(feature = "e2e")]
    if let Some(d) = dialog::E2eFileDialog::from_env() {
        return Box::new(d);
    }
    Box::new(dialog::TauriFileDialog::new(handle.clone()))
}

/// WebView2 ignores `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`, which
/// msedgedriver uses to open its debugging port, when the app passes its own
/// browser arguments, as wry always does. The end-to-end build adds the
/// variable's arguments to wry's defaults instead.
#[cfg(all(feature = "e2e", windows))]
fn with_driver_args(context: &mut tauri::Context<tauri::Wry>) {
    const WRY_DEFAULT: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";
    let Ok(extra) = std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS") else {
        return;
    };
    for window in &mut context.config_mut().app.windows {
        window.additional_browser_args = Some(format!("{WRY_DEFAULT} {extra}"));
    }
}

fn main() {
    #[allow(unused_mut)]
    let mut context = tauri::generate_context!();
    #[cfg(all(feature = "e2e", windows))]
    with_driver_args(&mut context);
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .register_uri_scheme_protocol("preview", |ctx, request| {
            let app = ctx.app_handle().try_state::<Arc<App>>();
            preview::respond(app.as_deref().map(Arc::as_ref), request.uri().path())
        })
        .setup(|tauri_app| {
            let dir = tauri_app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let store = SqliteStore::open(&dir.join(DB_FILE))?;
            let app = App::new(
                Box::new(store),
                file_dialog(tauri_app.handle()),
                Box::new(UlidIds),
                preview::BASE_URL,
            );
            tauri_app.manage(Arc::new(app));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_cvs,
            commands::get_cv,
            commands::create_cv,
            commands::save_cv,
            commands::duplicate_cv,
            commands::delete_cv,
            commands::restore_cv,
            commands::list_variants,
            commands::get_variant,
            commands::create_variant,
            commands::save_variant,
            commands::delete_variant,
            commands::duplicate_variant,
            commands::prune_variant,
            commands::fit_variant,
            commands::preview,
            commands::export_pdf,
            commands::export_html,
            commands::ats_check,
            commands::lint,
            commands::list_templates,
            commands::import_parse,
            commands::import_match,
            commands::import_commit,
        ])
        .run(context)
        .unwrap_or_else(|e| panic!("PinaxCV could not start: {e}"));
}
