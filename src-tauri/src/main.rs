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
mod log;
mod preview;

/// File name of the database inside the app data directory.
const DB_FILE: &str = "pinaxcv.sqlite3";

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .register_uri_scheme_protocol("preview", |ctx, request| {
            let app = ctx.app_handle().try_state::<Arc<App>>();
            preview::respond(app.as_deref().map(Arc::as_ref), request.uri().path())
        })
        .setup(|tauri_app| {
            if let Ok(logs) = tauri_app.path().app_log_dir() {
                log::init(&logs);
            }
            let dir = tauri_app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            // The error text is not logged: it can quote stored content.
            let store = SqliteStore::open(&dir.join(DB_FILE))
                .inspect_err(|_| tracing::error!("could not open the database"))?;
            let app = App::new(
                Box::new(store),
                Box::new(dialog::TauriSaveDialog::new(tauri_app.handle().clone())),
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
            commands::preview,
            commands::export_pdf,
            commands::export_html,
            commands::ats_check,
            commands::lint,
            commands::list_templates,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| panic!("PinaxCV could not start: {e}"));
}
