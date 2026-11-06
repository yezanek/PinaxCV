//! The opt-in update check (spec section 13, issue #70, ADR-095): the
//! first-run question with "off" preselected, the switch and "Check now" in
//! Settings, and the notice on the CV list when an update is ready.

use std::sync::atomic::{AtomicBool, Ordering};

use cv_api::{AppSettings, UpdateStatus};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Serialize;

use crate::i18n::{error_message, fill, t};
use crate::ipc::{NoArgs, call};

#[derive(Serialize)]
struct EnabledArg {
    enabled: bool,
}

/// The startup check runs once per app session, not on every return to the
/// CV list.
static CHECKED_AT_STARTUP: AtomicBool = AtomicBool::new(false);

/// The line shown for a check's result.
pub fn status_message(status: &UpdateStatus) -> String {
    match status {
        UpdateStatus::Off => t().updates_off.to_owned(),
        UpdateStatus::Unsupported { reason } => reason.clone(),
        UpdateStatus::UpToDate => t().updates_up_to_date.to_owned(),
        UpdateStatus::Available { version, .. } => {
            fill(t().updates_available, &[("version", version)])
        }
    }
}

/// "Install and restart", or the error when it failed.
fn install(busy: RwSignal<bool>, error: RwSignal<Option<String>>) {
    busy.set(true);
    error.set(None);
    spawn_local(async move {
        // On success the app restarts and nothing after this runs.
        let result = call::<_, ()>("update_install", &NoArgs {}).await;
        let _ = busy.try_set(false);
        if let Err(e) = result {
            let _ = error.try_set(Some(error_message(&e)));
        }
    });
}

/// On the CV list: asks once whether to check for updates ("off"
/// preselected), then, when the check is on, checks once per session and
/// offers a found update.
#[component]
pub fn UpdateNotice() -> impl IntoView {
    let unasked = RwSignal::new(false);
    let choice = RwSignal::new(false);
    let found: RwSignal<Option<UpdateStatus>> = RwSignal::new(None);
    let busy = RwSignal::new(false);
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let check = move || {
        if CHECKED_AT_STARTUP.swap(true, Ordering::Relaxed) {
            return;
        }
        spawn_local(async move {
            // A failed startup check stays quiet: Settings has "Check now".
            if let Ok(s @ UpdateStatus::Available { .. }) =
                call::<_, UpdateStatus>("update_check", &NoArgs {}).await
            {
                let _ = found.try_set(Some(s));
            }
        });
    };
    spawn_local(async move {
        if let Ok(s) = call::<_, AppSettings>("get_settings", &NoArgs {}).await {
            match s.update_check {
                None => {
                    let _ = unasked.try_set(true);
                }
                Some(true) => check(),
                Some(false) => {}
            }
        }
    });
    let save = move |_| {
        let enabled = choice.get_untracked();
        busy.set(true);
        spawn_local(async move {
            let result = call::<_, AppSettings>("set_update_check", &EnabledArg { enabled }).await;
            if busy.try_set(false).is_some() {
                return;
            }
            match result {
                Ok(_) => {
                    unasked.set(false);
                    if enabled {
                        check();
                    }
                }
                Err(e) => error.set(Some(error_message(&e))),
            }
        });
    };
    let s = t();
    view! {
        <Show when=move || unasked.get()>
            <section class="notice" role="region" aria-label=s.updates_heading>
                <p>{s.updates_question}</p>
                <label><input type="radio" name="update-check" prop:checked=move || !choice.get()
                    on:change=move |_| choice.set(false) />{s.updates_choice_off}</label>
                <label><input type="radio" name="update-check" prop:checked=move || choice.get()
                    on:change=move |_| choice.set(true) />{s.updates_choice_on}</label>
                <p class="hint">{s.updates_privacy}</p>
                <p class="actions"><button prop:disabled=move || busy.get() on:click=save>{s.save}</button></p>
            </section>
        </Show>
        {move || found.get().map(|status| view! {
            <section class="notice" role="status">
                <p>{status_message(&status)}</p>
                <p class="actions">
                    <button prop:disabled=move || busy.get() on:click=move |_| install(busy, error)>{t().updates_install}</button>
                </p>
            </section>
        })}
        <p class="error" role="alert">{move || error.get()}</p>
    }
}

/// The Updates part of Settings: the startup switch, "Check now", and
/// "Install and restart" once an update was found.
#[component]
pub fn UpdatesSection(settings: RwSignal<Option<AppSettings>>) -> impl IntoView {
    let busy = RwSignal::new(false);
    let message: RwSignal<Option<String>> = RwSignal::new(None);
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let available = RwSignal::new(false);
    let toggle = move |ev| {
        let enabled = event_target_checked(&ev);
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            let result = call::<_, AppSettings>("set_update_check", &EnabledArg { enabled }).await;
            if busy.try_set(false).is_some() {
                return;
            }
            match result {
                Ok(s) => settings.set(Some(s)),
                Err(e) => error.set(Some(error_message(&e))),
            }
        });
    };
    let check_now = move |_| {
        busy.set(true);
        error.set(None);
        message.set(None);
        spawn_local(async move {
            let result = call::<_, UpdateStatus>("update_check", &NoArgs {}).await;
            if busy.try_set(false).is_some() {
                return;
            }
            match result {
                Ok(s) => {
                    available.set(matches!(s, UpdateStatus::Available { .. }));
                    message.set(Some(status_message(&s)));
                }
                Err(e) => error.set(Some(error_message(&e))),
            }
        });
    };
    let on = move || settings.with(|s| s.as_ref().and_then(|s| s.update_check) == Some(true));
    let s = t();
    view! {
        <h2>{s.updates_heading}</h2>
        <p class="hint">{s.updates_privacy}</p>
        <label class="check">
            <input type="checkbox" prop:checked=on
                prop:disabled=move || busy.get() || settings.with(Option::is_none)
                on:change=toggle />
            {s.updates_switch}
        </label>
        <p class="actions">
            <button prop:disabled=move || busy.get() || !on() on:click=check_now>{s.updates_check_now}</button>
            <Show when=move || available.get()>
                <button prop:disabled=move || busy.get() on:click=move |_| install(busy, error)>{t().updates_install}</button>
            </Show>
        </p>
        <p role="status">{move || message.get()}</p>
        <p class="error" role="alert">{move || error.get()}</p>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_lines_name_the_version_or_the_reason() {
        assert_eq!(
            status_message(&UpdateStatus::Available {
                version: "1.2.0".into(),
                notes: None
            }),
            "PinaxCV 1.2.0 is available."
        );
        assert_eq!(
            status_message(&UpdateStatus::Unsupported {
                reason: "Not the AppImage.".into()
            }),
            "Not the AppImage."
        );
        assert_eq!(
            status_message(&UpdateStatus::UpToDate),
            t().updates_up_to_date
        );
        assert_eq!(status_message(&UpdateStatus::Off), t().updates_off);
    }
}
