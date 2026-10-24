//! The Checks tab (spec section 12.7, M6): lint findings for the shown CV or
//! variant, grouped by severity. A finding about a field or an item has a
//! "Show" button that jumps to it in the Content tab.

use std::time::Duration;

use cv_api::Target;
use cv_core::{Id, LintIssue, Severity};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Serialize;
use wasm_bindgen::JsCast;

use crate::i18n::{error_message, t};
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

/// Where `issue` points, if anywhere.
pub fn spot(issue: &LintIssue) -> Option<Spot> {
    use cv_core::lint::codes;
    if let Some(id) = &issue.target {
        return Some(Spot::Item(id.clone()));
    }
    match issue.code.as_str() {
        codes::MISSING_NAME => Some(Spot::Field(FULL_NAME_FIELD)),
        codes::MISSING_CONTACT | codes::INVALID_EMAIL => Some(Spot::Field(EMAIL_FIELD)),
        codes::SUMMARY_TOO_LONG => Some(Spot::Field(SUMMARY_FIELD)),
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
    /// Changes whenever the checks should run again: the tab was opened or
    /// the CV or variant was saved.
    refresh: Signal<u64>,
    /// Jumps to a finding's field or item.
    jump: Callback<Spot>,
) -> impl IntoView {
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
                Err(e) => error.set(Some(error_message(&e))),
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
                                    let to = spot(&issue);
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
        </div>
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
            spot(&issue("bullet_too_long", Some(id.clone()))),
            Some(Spot::Item(id))
        );
        assert_eq!(
            spot(&issue("missing_name", None)),
            Some(Spot::Field(FULL_NAME_FIELD))
        );
        assert_eq!(
            spot(&issue("invalid_email", None)),
            Some(Spot::Field(EMAIL_FIELD))
        );
        assert_eq!(
            spot(&issue("summary_too_long", None)),
            Some(Spot::Field(SUMMARY_FIELD))
        );
        assert_eq!(spot(&issue("page_count", None)), None);
    }
}
