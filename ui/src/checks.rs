//! The Checks tab (spec section 12.7, M6): lint findings for the shown CV or
//! variant, grouped by severity, and the ATS check. A finding about a field
//! or an item has a "Show" button that jumps to it in the Content tab.

use std::time::Duration;

use cv_api::Target;
use cv_core::{AtsReport, Id, LintIssue, Severity, Variant};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Serialize;
use wasm_bindgen::JsCast;

use crate::i18n::{error_message, fill, t};
use crate::ipc::call;

/// Where a finding points in the Content tab.
#[derive(Clone, Debug, PartialEq)]
pub enum Spot {
    /// A section, entry, bullet, skill group or language row.
    Item(Id),
    /// One of the contact or summary fields, by its element ID.
    Field(&'static str),
}

/// Element IDs of the fields findings without an item point to.
pub const FULL_NAME_FIELD: &str = "field-full-name";
pub const EMAIL_FIELD: &str = "field-email";
pub const SUMMARY_FIELD: &str = "field-summary";

/// Selects the elements a jump can focus.
const FIELDS: &str = "input,textarea";

/// Where `issue` points, if anywhere. When the shown `variant` overrides
/// the text a finding is about, the finding describes the override, which
/// the Content tab does not edit, so there is nothing to jump to.
pub fn spot(issue: &LintIssue, variant: Option<&Variant>) -> Option<Spot> {
    use cv_core::lint::codes;
    if let Some(id) = &issue.target {
        let overridden = variant.is_some_and(|v| v.text_overrides.contains_key(id));
        return (!overridden).then(|| Spot::Item(id.clone()));
    }
    match issue.code.as_str() {
        codes::MISSING_NAME => Some(Spot::Field(FULL_NAME_FIELD)),
        codes::MISSING_CONTACT | codes::INVALID_EMAIL => Some(Spot::Field(EMAIL_FIELD)),
        codes::SUMMARY_TOO_LONG => variant
            .is_none_or(|v| v.summary_override.is_none())
            .then_some(Spot::Field(SUMMARY_FIELD)),
        _ => None,
    }
}

/// The severities in the order they are listed, with their headings.
fn groups() -> [(Severity, &'static str); 3] {
    [
        (Severity::Error, t().lint_errors),
        (Severity::Warning, t().lint_warnings),
        (Severity::Info, t().lint_tips),
    ]
}

/// Focuses the field or item `spot` names in the Content tab, opening its
/// section first if it is collapsed.
pub fn focus_spot(spot: &Spot) {
    let el = match spot {
        Spot::Field(id) => document().get_element_by_id(id),
        Spot::Item(id) => item_element(id),
    };
    let Some(el) = el else {
        return;
    };
    let toggle = el
        .closest("fieldset.section")
        .ok()
        .flatten()
        .and_then(|s| s.query_selector(".collapse-toggle").ok().flatten())
        .filter(|t| t.get_attribute("aria-expanded").as_deref() == Some("false"))
        .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok());
    match toggle {
        // Focus once the opened section is shown.
        Some(button) => {
            button.click();
            set_timeout(move || focus_in(&el), Duration::ZERO);
        }
        None => focus_in(&el),
    }
}

/// The Content tab's element for item `id`. The variant panel's rows carry
/// the same IDs, so they are skipped.
fn item_element(id: &Id) -> Option<web_sys::Element> {
    let found = document()
        .query_selector_all(&format!("[data-drag-id=\"{id}\"]"))
        .ok()?;
    (0..found.length())
        .filter_map(|i| found.item(i)?.dyn_into::<web_sys::Element>().ok())
        .find(|el| el.closest(".variant-visibility").ok().flatten().is_none())
}

/// Scrolls to `el` and focuses it, or the first field inside it.
fn focus_in(el: &web_sys::Element) {
    el.scroll_into_view();
    let field = if el.matches(FIELDS).unwrap_or(false) {
        Some(el.clone())
    } else {
        el.query_selector(FIELDS).ok().flatten()
    };
    if let Some(field) = field.and_then(|f| f.dyn_into::<web_sys::HtmlElement>().ok()) {
        let _ = field.focus();
    }
}

#[derive(Serialize)]
struct TargetArg<'a> {
    target: &'a Target,
}

#[component]
pub fn ChecksTab(
    cv_id: Id,
    /// The shown variant, if any.
    variant_id: Signal<Option<Id>>,
    /// The shown variant's overlay, to tell which findings are about its
    /// text overrides.
    variant: Signal<Option<Variant>>,
    /// Changes whenever the checks should run again: the tab was opened or
    /// the CV or variant was saved.
    refresh: Signal<u64>,
    /// Jumps to a finding's field or item.
    jump: Callback<Spot>,
) -> impl IntoView {
    let ats_cv_id = cv_id.clone();
    let issues: RwSignal<Option<Vec<LintIssue>>> = RwSignal::new(None);
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    // Bumped per run; an answer to an older run is dropped.
    let runs = RwSignal::new(0u64);
    Effect::new(move |_| {
        refresh.track();
        let target = Target {
            cv_id: cv_id.clone(),
            variant_id: variant_id.get(),
        };
        let n = runs.get_untracked() + 1;
        runs.set(n);
        spawn_local(async move {
            let result = call::<_, Vec<LintIssue>>("lint", &TargetArg { target: &target }).await;
            if runs.try_get_untracked() != Some(n) {
                return;
            }
            match result {
                Ok(found) => {
                    error.set(None);
                    issues.set(Some(found));
                }
                Err(e) => {
                    issues.set(None);
                    error.set(Some(error_message(&e)));
                }
            }
        });
    });

    view! {
        <div class="checks">
            <h2>{t().lint_heading}</h2>
            <p class="hint">{t().lint_saved_only}</p>
            {move || error.get().map(|e| view! { <p role="alert">{e}</p> })}
            {move || issues.get().map(|found| {
                if found.is_empty() {
                    return view! { <p>{t().lint_clean}</p> }.into_any();
                }
                groups().into_iter().filter_map(|(severity, heading)| {
                    let group: Vec<LintIssue> = found.iter().filter(|i| i.severity == severity).cloned().collect();
                    (!group.is_empty()).then(|| view! {
                        <section class="lint-group">
                            <h3>{heading}" ("{group.len()}")"</h3>
                            <ul>
                                {group.into_iter().map(|issue| {
                                    let to = variant.with(|v| spot(&issue, v.as_ref()));
                                    view! {
                                        <li class="lint-issue">
                                            <span>
                                                {issue.message.clone()}
                                                {issue.fix_hint.clone().map(|h| view! { <br /><small>{h}</small> })}
                                            </span>
                                            {to.map(|to| view! {
                                                <button on:click=move |_| jump.run(to.clone())>{t().show_field}</button>
                                            })}
                                        </li>
                                    }
                                }).collect_view()}
                            </ul>
                        </section>
                    })
                }).collect_view().into_any()
            })}
            <AtsPanel cv_id=ats_cv_id variant_id=variant_id />
        </div>
    }
}

/// "Run check": exports the shown CV or variant to a PDF in memory, reads it
/// back, and shows the score, each check, and the extracted text.
#[component]
fn AtsPanel(cv_id: Id, variant_id: Signal<Option<Id>>) -> impl IntoView {
    let report: RwSignal<Option<AtsReport>> = RwSignal::new(None);
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let running = RwSignal::new(false);
    let copied = RwSignal::new(None::<&'static str>);
    // Bumped per run and per change of the shown variant; an answer to an
    // older run, or for another variant, is dropped.
    let runs = RwSignal::new(0u64);
    let reset = move || {
        runs.update(|n| *n += 1);
        report.set(None);
        error.set(None);
        copied.set(None);
        running.set(false);
    };
    Effect::new(move |first: Option<()>| {
        variant_id.track();
        if first.is_some() {
            reset();
        }
    });
    let run = move |_| {
        let target = Target {
            cv_id: cv_id.clone(),
            variant_id: variant_id.get_untracked(),
        };
        reset();
        let n = runs.get_untracked();
        running.set(true);
        spawn_local(async move {
            let result = call::<_, AtsReport>("ats_check", &TargetArg { target: &target }).await;
            // The tab was closed, or the run is stale.
            if runs.try_get_untracked() != Some(n) {
                return;
            }
            running.set(false);
            match result {
                Ok(r) => report.set(Some(r)),
                Err(e) => error.set(Some(error_message(&e))),
            }
        });
    };
    let copy = move |_| {
        let Some(text) = report.with_untracked(|r| r.as_ref().map(|r| r.extracted_text.clone()))
        else {
            return;
        };
        let promise = window().navigator().clipboard().write_text(&text);
        spawn_local(async move {
            let done = wasm_bindgen_futures::JsFuture::from(promise).await;
            let _ = copied.try_set(Some(if done.is_ok() {
                t().copied
            } else {
                t().copy_failed
            }));
        });
    };

    view! {
        <section class="ats">
            <h2>{t().ats_heading}</h2>
            <p class="hint">{t().ats_limits}</p>
            <button prop:disabled=move || running.get() on:click=run>
                {move || if running.get() { t().ats_running } else { t().ats_run }}
            </button>
            {move || error.get().map(|e| view! { <p role="alert">{e}</p> })}
            {move || report.get().map(|r| view! {
                <p class="ats-score">{fill(t().ats_score_line, &[("score", &r.score.to_string())])}</p>
                <ul class="ats-checks">
                    {r.checks.into_iter().map(|c| view! {
                        <li class:failed=!c.passed>
                            <span aria-hidden="true">{if c.passed { "✓" } else { "✗" }}</span>
                            <span class="visually-hidden">{if c.passed { t().check_passed } else { t().check_failed }}</span>
                            <span>{c.name}{c.detail.map(|d| view! { <br /><small>{d}</small> })}</span>
                        </li>
                    }).collect_view()}
                </ul>
                <div class="ats-text-head">
                    <strong>{t().ats_extracted}</strong>
                    <button on:click=copy>{t().copy_text}</button>
                    <span role="status">{move || copied.get()}</span>
                </div>
                <pre class="ats-text">{r.extracted_text}</pre>
            })}
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(code: &str, target: Option<Id>) -> LintIssue {
        LintIssue {
            code: code.into(),
            severity: Severity::Warning,
            target,
            message: String::new(),
            fix_hint: None,
        }
    }

    #[test]
    fn findings_point_to_items_or_fields() {
        let id = Id::parse("01J02000000000000000000004").expect("id");
        assert_eq!(
            spot(&issue("bullet_too_long", Some(id.clone())), None),
            Some(Spot::Item(id))
        );
        assert_eq!(
            spot(&issue("missing_name", None), None),
            Some(Spot::Field(FULL_NAME_FIELD))
        );
        assert_eq!(
            spot(&issue("invalid_email", None), None),
            Some(Spot::Field(EMAIL_FIELD))
        );
        assert_eq!(
            spot(&issue("summary_too_long", None), None),
            Some(Spot::Field(SUMMARY_FIELD))
        );
        assert_eq!(spot(&issue("page_count", None), None), None);
    }

    #[test]
    fn findings_about_variant_overrides_have_no_jump() {
        let id = Id::parse("01J02000000000000000000004").expect("id");
        let cv_id = Id::parse("01J02000000000000000000001").expect("id");
        let mut v = Variant::empty(
            Id::parse("01J02000000000000000000002").expect("id"),
            cv_id,
            "v",
        );
        assert_eq!(
            spot(&issue("summary_too_long", None), Some(&v)),
            Some(Spot::Field(SUMMARY_FIELD))
        );
        v.summary_override = Some("Shorter".into());
        v.text_overrides.insert(id.clone(), "Other text".into());
        assert_eq!(spot(&issue("summary_too_long", None), Some(&v)), None);
        assert_eq!(spot(&issue("bullet_too_long", Some(id)), Some(&v)), None);
        assert_eq!(
            spot(&issue("missing_name", None), Some(&v)),
            Some(Spot::Field(FULL_NAME_FIELD))
        );
    }
}
