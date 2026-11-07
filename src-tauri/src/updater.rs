//! The opt-in update check and the signed updater (spec sections 13 and 14,
//! issue #70, ADR-095). Only the Linux AppImage updates itself for now. The
//! check runs only after the user switched it on; nothing is sent otherwise.
//!
//! The public key that verifies update signatures is built in from
//! `PINAXCV_UPDATER_PUBKEY`, which the release workflow sets. A build
//! without it, such as one made from source, never checks.

use std::sync::{Arc, Mutex, PoisonError};

use cv_api::{ApiError, ErrorCode, UpdateStatus};
use cv_app::App;
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_updater::{Update, UpdaterExt};

/// The minisign public key for update signatures, from the release build.
const PUBKEY: Option<&str> = option_env!("PINAXCV_UPDATER_PUBKEY");

/// The update found by the last check, kept for `update_install`.
#[derive(Default)]
pub struct Pending(Mutex<Option<Update>>);

/// The update plugin, with the built-in key when there is one.
pub fn plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R, tauri_plugin_updater::Config> {
    let builder = tauri_plugin_updater::Builder::new();
    match pubkey() {
        Some(key) => builder.pubkey(key).build(),
        None => builder.build(),
    }
}

fn pubkey() -> Option<&'static str> {
    PUBKEY.map(str::trim).filter(|k| !k.is_empty())
}

/// Why this copy cannot update itself, or `None` when it can. `appimage` is
/// the `APPIMAGE` variable the AppImage runtime sets.
fn unsupported(has_key: bool, linux: bool, appimage: Option<&str>) -> Option<&'static str> {
    if !linux {
        Some("Automatic updates are available for the Linux AppImage only for now.")
    } else if appimage.is_none_or(str::is_empty) {
        Some("This copy is not running as the AppImage, so it cannot update itself.")
    } else if !has_key {
        Some("This build has no update key, so it cannot verify updates.")
    } else {
        None
    }
}

fn io_error(message: &str) -> ApiError {
    ApiError::new(ErrorCode::Io, message)
}

/// Whether the user switched the update check on.
async fn enabled(app: &State<'_, Arc<App>>) -> Result<bool, ApiError> {
    let app = Arc::clone(app.inner());
    let setting = tauri::async_runtime::spawn_blocking(move || app.update_check())
        .await
        .map_err(|_| {
            ApiError::new(
                ErrorCode::Internal,
                "The update check stopped unexpectedly.",
            )
        })??;
    Ok(setting == Some(true))
}

/// Checks GitHub Releases for a newer signed AppImage, if the user switched
/// the check on and this copy can update itself.
#[tauri::command]
pub async fn update_check<R: Runtime>(
    handle: AppHandle<R>,
    app: State<'_, Arc<App>>,
    pending: State<'_, Pending>,
) -> Result<UpdateStatus, ApiError> {
    if !enabled(&app).await? {
        return Ok(UpdateStatus::Off);
    }
    let appimage = std::env::var("APPIMAGE").ok();
    if let Some(reason) = unsupported(
        pubkey().is_some(),
        cfg!(target_os = "linux"),
        appimage.as_deref(),
    ) {
        return Ok(UpdateStatus::Unsupported {
            reason: reason.to_owned(),
        });
    }
    let found = handle
        .updater()
        .map_err(|_| io_error("The updater could not start."))?
        .check()
        .await;
    match found {
        Ok(Some(update)) => {
            tracing::info!(version = %update.version, "update available");
            let status = UpdateStatus::Available {
                version: update.version.clone(),
                notes: update.body.clone(),
            };
            *pending.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(update);
            Ok(status)
        }
        Ok(None) => {
            tracing::info!("no update");
            Ok(UpdateStatus::UpToDate)
        }
        Err(_) => {
            tracing::warn!("update check failed");
            Err(io_error(
                "Could not check for updates. Check the internet connection and try again.",
            ))
        }
    }
}

/// Downloads the update found by the last check, verifies its signature,
/// replaces the AppImage and restarts the app.
#[tauri::command]
pub async fn update_install<R: Runtime>(
    handle: AppHandle<R>,
    app: State<'_, Arc<App>>,
    pending: State<'_, Pending>,
) -> Result<(), ApiError> {
    // The update is taken in every case, so a failed or refused install asks
    // for a new check.
    let update = pending
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
        .ok_or_else(|| ApiError::new(ErrorCode::Validation, "Check for updates first."))?;
    // Switched off after the check: nothing is downloaded (ADR-095).
    if !enabled(&app).await? {
        return Err(ApiError::new(
            ErrorCode::Validation,
            "Update checks are off. Turn them on in Settings > Updates.",
        ));
    }
    let version = update.version.clone();
    if update.download_and_install(|_, _| {}, || {}).await.is_err() {
        tracing::warn!(%version, "update install failed");
        return Err(io_error(
            "The update could not be downloaded or verified. Nothing was changed.",
        ));
    }
    tracing::info!(%version, "update installed, restarting");
    handle.restart()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_keyed_appimage_on_linux_updates_itself() {
        assert_eq!(
            unsupported(true, true, Some("/home/a/PinaxCV.AppImage")),
            None
        );
        assert!(unsupported(true, false, Some("/x")).is_some_and(|r| r.contains("Linux")));
        assert!(unsupported(true, true, None).is_some_and(|r| r.contains("AppImage")));
        assert!(unsupported(true, true, Some("")).is_some());
        assert!(unsupported(false, true, Some("/x")).is_some_and(|r| r.contains("key")));
    }
}
