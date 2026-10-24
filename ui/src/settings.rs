//! Settings (spec section 12.2, screen 5): the AI provider, model, server
//! address and API key, what may be sent, and the consent to send it
//! (spec sections 11.1 and 13.1); what new CVs start with; the data
//! folder; and the about box.

use cv_api::{AiConfig, AiProvider, AiStatus, AppSettings, CvDefaults, KeyStorage};
use cv_core::{DateStyle, Locale, Paper};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Serialize;

use crate::i18n::{error_message, fill, t};
use crate::ipc::{NoArgs, call};

#[derive(Serialize)]
struct ConfigArg<'a> {
    config: &'a AiConfig,
}

#[derive(Serialize)]
struct DefaultsArg {
    defaults: CvDefaults,
}

fn paper_value(p: Paper) -> &'static str {
    match p {
        Paper::A4 => "a4",
        Paper::Letter => "letter",
    }
}

fn parse_paper(v: &str) -> Paper {
    match v {
        "letter" => Paper::Letter,
        _ => Paper::A4,
    }
}

fn date_style_value(d: DateStyle) -> &'static str {
    match d {
        DateStyle::MonthNameYear => "month_name_year",
        DateStyle::NumericMonthYear => "numeric_month_year",
        DateStyle::YearOnly => "year_only",
    }
}

fn parse_date_style(v: &str) -> DateStyle {
    match v {
        "numeric_month_year" => DateStyle::NumericMonthYear,
        "year_only" => DateStyle::YearOnly,
        _ => DateStyle::MonthNameYear,
    }
}

/// What new CVs start with, the data folder, and the about box.
#[component]
fn AppSection() -> impl IntoView {
    let settings: RwSignal<Option<AppSettings>> = RwSignal::new(None);
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let message: RwSignal<Option<&'static str>> = RwSignal::new(None);
    spawn_local(async move {
        match call::<_, AppSettings>("get_settings", &NoArgs {}).await {
            Ok(s) => {
                let _ = settings.try_set(Some(s));
            }
            Err(e) => {
                let _ = error.try_set(Some(error_message(&e)));
            }
        }
    });
    let defaults = move || settings.with(|s| s.as_ref().map(|s| s.defaults).unwrap_or_default());
    let change = move |edit: Box<dyn FnOnce(&mut CvDefaults)>| {
        let mut d = defaults();
        edit(&mut d);
        message.set(None);
        error.set(None);
        spawn_local(async move {
            match call::<_, AppSettings>("set_cv_defaults", &DefaultsArg { defaults: d }).await {
                Ok(s) => {
                    let _ = settings.try_set(Some(s));
                    let _ = message.try_set(Some(t().defaults_saved));
                }
                Err(e) => {
                    let _ = error.try_set(Some(error_message(&e)));
                }
            }
        });
    };
    let open_folder = move |_| {
        error.set(None);
        spawn_local(async move {
            if let Err(e) = call::<_, ()>("open_data_dir", &NoArgs {}).await {
                let _ = error.try_set(Some(error_message(&e)));
            }
        });
    };
    let s = t();
    view! {
        <h2>{s.defaults_heading}</h2>
        <p class="hint">{s.defaults_intro}</p>
        <label>{s.paper}
            <select prop:disabled=move || settings.with(Option::is_none)
                on:change=move |ev| {
                    let p = parse_paper(&event_target_value(&ev));
                    change(Box::new(move |d| d.paper = p));
                }>
                {[(Paper::A4, s.paper_a4), (Paper::Letter, s.paper_letter)].into_iter().map(|(p, label)| view! {
                    <option value=paper_value(p) selected=move || defaults().paper == p>{label}</option>
                }).collect_view()}
            </select>
        </label>
        <label>{s.cv_language}
            <select prop:disabled=move || settings.with(Option::is_none)
                on:change=move |_| change(Box::new(|d| d.locale = Locale::En))>
                <option value="en" selected=move || defaults().locale == Locale::En>{s.english}</option>
            </select>
        </label>
        <label>{s.date_style}
            <select prop:disabled=move || settings.with(Option::is_none)
                on:change=move |ev| {
                    let v = parse_date_style(&event_target_value(&ev));
                    change(Box::new(move |d| d.date_style = v));
                }>
                {[
                    (DateStyle::MonthNameYear, s.date_month_name),
                    (DateStyle::NumericMonthYear, s.date_numeric),
                    (DateStyle::YearOnly, s.date_year_only),
                ].into_iter().map(|(v, label)| view! {
                    <option value=date_style_value(v) selected=move || defaults().date_style == v>{label}</option>
                }).collect_view()}
            </select>
        </label>
        <p role="status">{move || message.get()}</p>

        {move || settings.get().and_then(|st| st.data_dir).map(|dir| view! {
            <h2>{t().data_heading}</h2>
            <p class="hint">{t().data_folder_intro}</p>
            <p><code class="path">{dir}</code></p>
            <p class="actions"><button on:click=open_folder>{t().open_data_folder}</button></p>
        })}

        <h2>{s.about_heading}</h2>
        {move || settings.get().map(|st| view! {
            <p>{fill(t().about_version, &[("version", &st.version)])}</p>
            <p>{fill(t().about_license, &[("license", &st.license)])}</p>
        })}
        <p>{s.about_fonts}</p>
        <p class="hint">{s.about_privacy}</p>
        <p class="error" role="alert">{move || error.get()}</p>
    }
}

/// The value a provider has in the select, and back.
fn provider_value(p: AiProvider) -> &'static str {
    match p {
        AiProvider::None => "none",
        AiProvider::Anthropic => "anthropic",
        AiProvider::Openai => "openai",
        AiProvider::Gemini => "gemini",
        AiProvider::OpenaiCompatible => "openai_compatible",
    }
}

fn parse_provider(v: &str) -> AiProvider {
    match v {
        "anthropic" => AiProvider::Anthropic,
        "openai" => AiProvider::Openai,
        "gemini" => AiProvider::Gemini,
        "openai_compatible" => AiProvider::OpenaiCompatible,
        _ => AiProvider::None,
    }
}

/// The lines of the consent text for `status` (spec section 11.1).
pub fn consent_lines(status: &AiStatus) -> Vec<String> {
    let s = t();
    let mut lines = vec![
        fill(
            s.consent_where,
            &[("host", status.host.as_deref().unwrap_or_default())],
        ),
        s.consent_content.into(),
        s.consent_job.into(),
        s.consent_never.into(),
    ];
    lines.push(
        if status.send_name {
            s.consent_name_on
        } else {
            s.consent_name_off
        }
        .into(),
    );
    lines.push(
        if status.send_location {
            s.consent_location_on
        } else {
            s.consent_location_off
        }
        .into(),
    );
    lines.push(s.consent_import.into());
    lines
}

/// What the key line says about where the key is kept.
fn key_note(status: &AiStatus) -> &'static str {
    match status.key {
        KeyStorage::Keychain => t().key_in_keychain,
        KeyStorage::Memory => t().key_in_memory,
        KeyStorage::None => t().key_none,
    }
}

/// "What is sent", with "I agree". Calls `accepted` with the new status.
#[component]
pub fn ConsentPanel(status: AiStatus, accepted: Callback<AiStatus>) -> impl IntoView {
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let busy = RwSignal::new(false);
    let agree = move |_| {
        busy.set(true);
        spawn_local(async move {
            let result = call::<_, AiStatus>("ai_consent", &NoArgs {}).await;
            if busy.try_set(false).is_some() {
                return;
            }
            match result {
                Ok(s) => accepted.run(s),
                Err(e) => error.set(Some(error_message(&e))),
            }
        });
    };
    view! {
        <section class="consent" role="region" aria-label=t().consent_heading>
            <h3>{t().consent_heading}</h3>
            <ul>{consent_lines(&status).into_iter().map(|l| view! { <li>{l}</li> }).collect_view()}</ul>
            <p class="error">{move || error.get()}</p>
            <button prop:disabled=move || busy.get() on:click=agree>{t().consent_agree}</button>
        </section>
    }
}

#[component]
pub fn Settings(
    /// Closes the screen.
    close: Callback<()>,
) -> impl IntoView {
    let status: RwSignal<Option<AiStatus>> = RwSignal::new(None);
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let message: RwSignal<Option<&'static str>> = RwSignal::new(None);
    let busy = RwSignal::new(false);
    // The form, filled from the saved settings.
    let provider = RwSignal::new(AiProvider::None);
    let model = RwSignal::new(String::new());
    let base_url = RwSignal::new(String::new());
    let key = RwSignal::new(String::new());
    let send_name = RwSignal::new(false);
    let send_location = RwSignal::new(true);

    let show = move |s: AiStatus| {
        provider.set(s.provider);
        model.set(s.model.clone());
        base_url.set(s.base_url.clone().unwrap_or_default());
        send_name.set(s.send_name);
        send_location.set(s.send_location);
        key.set(String::new());
        status.set(Some(s));
    };
    spawn_local(async move {
        match call::<_, AiStatus>("ai_status", &NoArgs {}).await {
            Ok(s) => show(s),
            Err(e) => {
                let _ = error.try_set(Some(error_message(&e)));
            }
        }
    });

    // Runs `cmd`, which answers with the new status, then says `done`.
    let act = move |cmd: &'static str, config: Option<AiConfig>, done: &'static str| {
        busy.set(true);
        message.set(None);
        error.set(None);
        spawn_local(async move {
            let result = match &config {
                Some(c) => call::<_, AiStatus>(cmd, &ConfigArg { config: c }).await,
                None => call::<_, AiStatus>(cmd, &NoArgs {}).await,
            };
            if busy.try_set(false).is_some() {
                return;
            }
            match result {
                Ok(s) => {
                    show(s);
                    message.set(Some(done));
                }
                Err(e) => error.set(Some(error_message(&e))),
            }
        });
    };
    let save = move |_| {
        let config = AiConfig {
            provider: provider.get_untracked(),
            model: model.get_untracked(),
            base_url: Some(base_url.get_untracked()).filter(|u| !u.trim().is_empty()),
            api_key: Some(key.get_untracked()).filter(|k| !k.trim().is_empty()),
            send_name: send_name.get_untracked(),
            send_location: send_location.get_untracked(),
        };
        act("ai_configure", Some(config), t().settings_saved);
    };
    let forget = move |_| act("ai_forget_key", None, t().key_forgotten);
    let test = move |_| {
        busy.set(true);
        message.set(Some(t().testing_connection));
        error.set(None);
        spawn_local(async move {
            let result = call::<_, ()>("ai_test_connection", &NoArgs {}).await;
            if busy.try_set(false).is_some() {
                return;
            }
            match result {
                Ok(()) => message.set(Some(t().connection_ok)),
                Err(e) => {
                    message.set(None);
                    error.set(Some(error_message(&e)));
                }
            }
        });
    };
    let choosing = move || provider.get() != AiProvider::None;
    let needs_key = move || {
        matches!(
            provider.get(),
            AiProvider::Anthropic | AiProvider::Openai | AiProvider::Gemini
        )
    };
    let saved_provider = move || status.with(|s| s.as_ref().map(|s| s.provider));

    view! {
        <header class="bar">
            <button on:click=move |_| close.run(())>{t().back_to_cvs}</button>
            <h1>{t().settings}</h1>
        </header>
        <main class="settings">
            <h2>{t().ai_settings}</h2>
            <p class="hint">{t().ai_settings_intro}</p>
            <label>{t().ai_provider}
                <select on:change=move |ev| provider.set(parse_provider(&event_target_value(&ev)))>
                    {[
                        (AiProvider::None, t().provider_none),
                        (AiProvider::Anthropic, t().provider_anthropic),
                        (AiProvider::Openai, t().provider_openai),
                        (AiProvider::Gemini, t().provider_gemini),
                        (AiProvider::OpenaiCompatible, t().provider_compatible),
                    ].into_iter().map(|(p, label)| view! {
                        <option value=provider_value(p) selected=move || provider.get() == p>{label}</option>
                    }).collect_view()}
                </select>
            </label>
            <Show when=choosing>
                <label>{t().ai_model}
                    <input prop:value=move || model.get() placeholder=t().ai_model_placeholder
                        on:input=move |ev| model.set(event_target_value(&ev)) />
                </label>
                <Show when=move || provider.get() == AiProvider::OpenaiCompatible>
                    <label>{t().ai_base_url}
                        <input prop:value=move || base_url.get() placeholder="http://localhost:11434/v1"
                            on:input=move |ev| base_url.set(event_target_value(&ev)) />
                    </label>
                </Show>
                <label>{move || if needs_key() { t().ai_key } else { t().ai_key_optional }}
                    <input type="password" autocomplete="off" prop:value=move || key.get()
                        placeholder=move || {
                            let same = saved_provider() == Some(provider.get());
                            status.with(|s| match s {
                                Some(s) if same && s.key != KeyStorage::None => t().key_saved_placeholder,
                                _ => "",
                            })
                        }
                        on:input=move |ev| key.set(event_target_value(&ev)) />
                </label>
                <p class="hint">{move || status.with(|s| s.as_ref().map(key_note))}</p>
                <Show when=move || status.with(|s| s.as_ref().is_some_and(|s| !s.keychain_available))>
                    <p class="hint">{t().no_keychain}</p>
                </Show>
                <label class="check"><input type="checkbox" prop:checked=move || send_name.get()
                    on:change=move |ev| send_name.set(event_target_checked(&ev)) />{t().send_name}</label>
                <label class="check"><input type="checkbox" prop:checked=move || send_location.get()
                    on:change=move |ev| send_location.set(event_target_checked(&ev)) />{t().send_location}</label>
            </Show>
            <p class="actions">
                <button prop:disabled=move || busy.get() on:click=save>{t().save}</button>
                <Show when=move || status.with(|s| s.as_ref().is_some_and(|s| s.configured))>
                    <button prop:disabled=move || busy.get() on:click=test>{t().test_connection}</button>
                </Show>
                <Show when=move || status.with(|s| s.as_ref().is_some_and(|s| s.key != KeyStorage::None))>
                    <button prop:disabled=move || busy.get() on:click=forget>{t().forget_key}</button>
                </Show>
            </p>
            <p role="status">{move || message.get()}</p>
            <p class="error" role="alert">{move || error.get()}</p>
            {move || status.get().filter(|s| s.configured && !s.consent_ok).map(|s| view! {
                <ConsentPanel status=s accepted=Callback::new(move |s: AiStatus| {
                    show(s);
                    message.set(Some(t().consent_saved));
                }) />
            })}
            <Show when=move || status.with(|s| s.as_ref().is_some_and(|s| s.consent_ok))>
                <p class="hint">{t().consent_given}</p>
            </Show>
            <AppSection />
        </main>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(send_name: bool, send_location: bool) -> AiStatus {
        AiStatus {
            provider: AiProvider::Anthropic,
            model: "m".into(),
            base_url: None,
            key: KeyStorage::Keychain,
            configured: true,
            consent_ok: false,
            host: Some("api.anthropic.com".into()),
            send_name,
            send_location,
            keychain_available: true,
        }
    }

    #[test]
    fn consent_names_the_host_and_follows_the_switches() {
        let lines = consent_lines(&status(false, true));
        assert!(lines[0].contains("api.anthropic.com"));
        assert!(lines.contains(&t().consent_name_off.to_owned()));
        assert!(lines.contains(&t().consent_location_on.to_owned()));
        let lines = consent_lines(&status(true, false));
        assert!(lines.contains(&t().consent_name_on.to_owned()));
        assert!(lines.contains(&t().consent_location_off.to_owned()));
    }

    #[test]
    fn defaults_map_to_select_values() {
        for p in [Paper::A4, Paper::Letter] {
            assert_eq!(parse_paper(paper_value(p)), p);
        }
        for d in [
            DateStyle::MonthNameYear,
            DateStyle::NumericMonthYear,
            DateStyle::YearOnly,
        ] {
            assert_eq!(parse_date_style(date_style_value(d)), d);
        }
    }

    #[test]
    fn providers_map_to_select_values() {
        for p in [
            AiProvider::None,
            AiProvider::Anthropic,
            AiProvider::Openai,
            AiProvider::Gemini,
            AiProvider::OpenaiCompatible,
        ] {
            assert_eq!(parse_provider(provider_value(p)), p);
        }
        assert_eq!(key_note(&status(false, true)), t().key_in_keychain);
    }
}
