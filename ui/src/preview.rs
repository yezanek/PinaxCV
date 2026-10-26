//! The live preview's zoom and its Typst diagnostics strip (spec sections
//! 12.2 and 12.4).

use cv_api::{ApiError, ErrorCode};
use cv_core::{Diagnostic, Paper, Severity};
use leptos::prelude::*;

use crate::i18n::{fill, t};

/// How preview pages are sized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zoom {
    /// As wide as the preview pane.
    FitWidth,
    /// The page's real size at 96 CSS pixels per inch.
    Actual,
}

/// Page width in CSS pixels at 100% (1 pt = 4/3 px).
pub fn actual_width_px(paper: Paper) -> f64 {
    let points = match paper {
        Paper::A4 => 595.28,
        Paper::Letter => 612.0,
    };
    points * 4.0 / 3.0
}

/// The CSS width of page images at `zoom`; `None` leaves the stylesheet's
/// fit-width sizing.
pub fn page_width(zoom: Zoom, paper: Paper) -> Option<String> {
    match zoom {
        Zoom::FitWidth => None,
        Zoom::Actual => Some(format!("{:.0}px", actual_width_px(paper))),
    }
}

/// The diagnostics a failed render carries in its details (a list of
/// `Diagnostic`); empty for other errors.
pub fn diagnostics_from(e: &ApiError) -> Vec<Diagnostic> {
    if e.code != ErrorCode::Render {
        return Vec::new();
    }
    e.details
        .clone()
        .and_then(|d| serde_json::from_value(d).ok())
        .unwrap_or_default()
}

fn severity_label(s: Severity) -> &'static str {
    match s {
        Severity::Info => t().severity_info,
        Severity::Warning => t().severity_warning,
        Severity::Error => t().severity_error,
    }
}

/// Fit width and 100% buttons.
#[component]
pub fn ZoomControls(zoom: RwSignal<Zoom>) -> impl IntoView {
    let button = move |value: Zoom, label: &'static str| {
        view! {
            <button aria-pressed=move || (zoom.get() == value).to_string()
                on:click=move |_| zoom.set(value)>{label}</button>
        }
    };
    view! {
        <div class="zoom" role="group" aria-label=t().zoom>
            {button(Zoom::FitWidth, t().zoom_fit)}
            {button(Zoom::Actual, t().zoom_actual)}
        </div>
    }
}

/// A collapsible strip listing the template's messages; hidden when there
/// are none.
#[component]
pub fn DiagnosticsStrip(diagnostics: RwSignal<Vec<Diagnostic>>) -> impl IntoView {
    view! {
        <Show when=move || diagnostics.with(|d| !d.is_empty())>
            <details class="diagnostics">
                <summary>{move || fill(t().template_messages, &[("n", &diagnostics.with(Vec::len).to_string())])}</summary>
                <ul>
                    {move || diagnostics.get().into_iter().map(|d| view! {
                        <li class=format!("severity-{:?}", d.severity).to_lowercase()>
                            <strong>{severity_label(d.severity)}</strong>" "{d.message}
                        </li>
                    }).collect_view()}
                </ul>
            </details>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_size_is_the_paper_width_in_css_pixels() {
        assert_eq!(actual_width_px(Paper::Letter), 816.0);
        assert!((actual_width_px(Paper::A4) - 793.7).abs() < 0.1);
        assert_eq!(page_width(Zoom::FitWidth, Paper::A4), None);
        assert_eq!(
            page_width(Zoom::Actual, Paper::A4).as_deref(),
            Some("794px")
        );
    }

    #[test]
    fn render_errors_carry_their_diagnostics() {
        let d = Diagnostic {
            severity: Severity::Error,
            code: "typst".into(),
            message: "unexpected end".into(),
        };
        let e = ApiError::new(ErrorCode::Render, "x")
            .with_details(serde_json::to_value(vec![d.clone()]).expect("json"));
        assert_eq!(diagnostics_from(&e), vec![d]);
        let other = ApiError::new(ErrorCode::Io, "x").with_details(serde_json::json!([]));
        assert!(diagnostics_from(&other).is_empty());
        let odd = ApiError::new(ErrorCode::Render, "x").with_details(serde_json::json!({"a": 1}));
        assert!(diagnostics_from(&odd).is_empty());
    }
}
