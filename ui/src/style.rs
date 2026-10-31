//! The Style tab (spec section 12.8): template, paper size, locale, date
//! style and the template's layout options. Sliders use the ranges from the
//! template manifest; the backend still clamps whatever is saved. Styles can
//! be saved to and loaded from `.pinaxstyle.xml` files (ADR-084).

use cv_api::{ErrorCode, OptionRange, StyleOpened, StyleSave, StyleSaved, TemplateManifest};
use cv_core::{Cv, DateStyle, Locale, Paper, Style, StyleApplied, TemplateId, TemplateOptions};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::entries::Edit;
use crate::i18n::{error_message, fill, t};
use crate::ipc::{NoArgs, ReqArg, call};
use crate::layout::LayoutPanel;

/// The value shown next to a slider: at most two decimals, no trailing zeros.
pub fn show_number(value: f32) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// Parses a slider's value, clamped to `range`. `None` for text that is not
/// a finite number.
pub fn parse_in_range(text: &str, range: OptionRange) -> Option<f32> {
    let v: f32 = text.trim().parse().ok()?;
    v.is_finite().then(|| v.clamp(range.min, range.max))
}

/// `#RRGGBB` for the colour input, which cannot show "no colour": black
/// stands in for `None`.
pub fn colour_value(accent: Option<&str>) -> String {
    accent
        .filter(|a| a.len() == 7 && a.starts_with('#'))
        .unwrap_or("#000000")
        .to_ascii_lowercase()
}

/// Applies an opened style file to `cv`, keeping the CV's template when
/// the style's is not available.
pub fn apply_opened(cv: &mut Cv, opened: &StyleOpened) -> StyleApplied {
    let template = cv.template.clone();
    let applied = cv.apply_style(&opened.style);
    if opened.keep_template {
        cv.template = template;
    }
    applied
}

/// The lines shown after a style was loaded.
pub fn load_summary(opened: &StyleOpened, applied: &StyleApplied) -> Vec<String> {
    let s = t();
    let mut lines = vec![s.style_loaded.to_owned()];
    lines.extend(opened.warnings.iter().cloned());
    let unmatched = applied.unmatched_blocks.len();
    if unmatched > 0 {
        lines.push(fill(
            s.style_unmatched,
            &[("count", &unmatched.to_string())],
        ));
    }
    if applied.unplaced_sections > 0 {
        lines.push(fill(
            s.style_unplaced,
            &[("count", &applied.unplaced_sections.to_string())],
        ));
    }
    lines
}

/// Save and load buttons for style files.
#[component]
fn StyleFile(
    cv: RwSignal<Option<Cv>>,
    edit: Edit,
    error: RwSignal<Option<String>>,
) -> impl IntoView {
    let s = t();
    let busy = RwSignal::new(false);
    let message: RwSignal<Vec<String>> = RwSignal::new(Vec::new());
    let save = move |_| {
        let Some(req) = cv.with_untracked(|c| {
            c.as_ref().map(|c| StyleSave {
                style: Style::from_cv(c),
                name: c.title.clone(),
            })
        }) else {
            return;
        };
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        message.set(Vec::new());
        spawn_local(async move {
            let result = call::<_, StyleSaved>("style_save", &ReqArg { req: &req }).await;
            // `Some` hands the value back: the tab was closed meanwhile.
            if busy.try_set(false).is_some() {
                return;
            }
            match result {
                Ok(r) => message.set(vec![fill(t().style_saved, &[("path", &r.path)])]),
                Err(e) if e.code == ErrorCode::Cancelled => {}
                Err(e) => error.set(Some(error_message(&e))),
            }
        });
    };
    let load = move |_| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        message.set(Vec::new());
        spawn_local(async move {
            let result = call::<_, StyleOpened>("style_open", &NoArgs {}).await;
            // `Some` hands the value back: the tab was closed meanwhile.
            if busy.try_set(false).is_some() {
                return;
            }
            match result {
                Ok(opened) => edit.run(Box::new(move |c| {
                    let applied = apply_opened(c, &opened);
                    message.set(load_summary(&opened, &applied));
                })),
                Err(e) if e.code == ErrorCode::Cancelled => {}
                Err(e) => error.set(Some(error_message(&e))),
            }
        });
    };
    view! {
        <fieldset class="style-file">
            <legend>{s.style_file}</legend>
            <p class="hint">{s.style_file_hint}</p>
            <div class="actions">
                <button prop:disabled=move || busy.get() on:click=save>{s.style_save}</button>
                <button prop:disabled=move || busy.get() on:click=load>{s.style_load}</button>
            </div>
            <ul class="status" aria-live="polite">
                {move || message.get().into_iter().map(|line| view! { <li>{line}</li> }).collect_view()}
            </ul>
        </fieldset>
    }
}

/// The ranges for the CV's template, or `TemplateOptions`' own ranges while
/// the manifests load (they are the same for `classic`).
fn ranges_for(templates: &[TemplateManifest], id: &TemplateId) -> [OptionRange; 3] {
    let d = TemplateOptions::default();
    let fallback = |(min, max): (f32, f32), default| OptionRange { min, max, default };
    match templates.iter().find(|m| &m.id == id) {
        Some(m) => [
            m.options.font_size_pt,
            m.options.line_spacing,
            m.options.margin_mm,
        ],
        None => [
            fallback(TemplateOptions::FONT_SIZE_PT, d.font_size_pt),
            fallback(TemplateOptions::LINE_SPACING, d.line_spacing),
            fallback(TemplateOptions::MARGIN_MM, d.margin_mm),
        ],
    }
}

#[component]
pub fn StyleTab(
    cv: RwSignal<Option<Cv>>,
    edit: Edit,
    error: RwSignal<Option<String>>,
) -> impl IntoView {
    let templates: RwSignal<Vec<TemplateManifest>> = RwSignal::new(Vec::new());
    spawn_local(async move {
        match call::<_, Vec<TemplateManifest>>("list_templates", &NoArgs {}).await {
            Ok(list) => {
                let _ = templates.try_set(list);
            }
            Err(e) => {
                let _ = error.try_set(Some(error_message(&e)));
            }
        }
    });
    let template_id =
        move || cv.with(|c| c.as_ref().map(|c| c.template.clone()).unwrap_or_default());
    let ranges = move || templates.with(|list| ranges_for(list, &template_id()));

    // One slider per numeric option; `index` picks its range.
    let slider = move |label: &'static str,
                       unit: &'static str,
                       step: &'static str,
                       index: usize,
                       get: fn(&TemplateOptions) -> f32,
                       set: fn(&mut TemplateOptions, f32)| {
        let value = move || cv.with(|c| c.as_ref().map(|c| get(&c.options)).unwrap_or_default());
        view! {
            <label class="slider">
                <span>{label}" "<output>{move || show_number(value())}{unit}</output></span>
                <input type="range" step=step
                    prop:min=move || ranges()[index].min.to_string()
                    prop:max=move || ranges()[index].max.to_string()
                    prop:value=move || value().to_string()
                    on:input=move |ev| {
                        let Some(v) = parse_in_range(&event_target_value(&ev), ranges()[index]) else {
                            return;
                        };
                        edit.run(Box::new(move |c| set(&mut c.options, v)));
                    } />
            </label>
        }
    };

    // A row of radio buttons for a small enum.
    fn segmented<T: Copy + PartialEq + Send + Sync + 'static>(
        legend: &'static str,
        name: &'static str,
        choices: Vec<(T, &'static str)>,
        cv: RwSignal<Option<Cv>>,
        edit: Edit,
        get: fn(&Cv) -> T,
        set: fn(&mut Cv, T),
    ) -> impl IntoView {
        let buttons = choices
            .into_iter()
            .map(|(value, label)| {
                view! {
                    <label>
                        <input type="radio" name=name
                            prop:checked=move || cv.with(|c| c.as_ref().is_some_and(|c| get(c) == value))
                            on:change=move |_| edit.run(Box::new(move |c| set(c, value))) />
                        {label}
                    </label>
                }
            })
            .collect_view();
        view! {
            <fieldset class="segmented">
                <legend>{legend}</legend>
                {buttons}
            </fieldset>
        }
    }

    let s = t();
    view! {
        <LayoutPanel cv=cv edit=edit />
        <label>{s.template}
            <select on:change=move |ev| {
                let id = TemplateId(event_target_value(&ev));
                edit.run(Box::new(move |c| c.template = id));
            }>
                <For each=move || templates.get() key=|m| m.id.0.clone() let:m>
                    {
                        let id = m.id.clone();
                        view! {
                            <option value=m.id.0.clone() selected=move || template_id() == id>{m.name.clone()}</option>
                        }
                    }
                </For>
            </select>
        </label>
        {segmented(s.paper, "paper", vec![(Paper::A4, s.paper_a4), (Paper::Letter, s.paper_letter)],
            cv, edit, |c| c.paper, |c, v| c.paper = v)}
        <label>{s.cv_language}
            <select on:change=move |_| edit.run(Box::new(|c| c.locale = Locale::En))>
                <option value="en" selected=move || cv.with(|c| c.as_ref().is_some_and(|c| c.locale == Locale::En))>{s.english}</option>
            </select>
        </label>
        {segmented(s.date_style, "date-style", vec![
            (DateStyle::MonthNameYear, s.date_month_name),
            (DateStyle::NumericMonthYear, s.date_numeric),
            (DateStyle::YearOnly, s.date_year_only),
        ], cv, edit, |c| c.options.date_style, |c, v| c.options.date_style = v)}
        {slider(s.font_size, s.unit_pt, "0.5", 0, |o| o.font_size_pt, |o, v| o.font_size_pt = v)}
        {slider(s.line_spacing, "", "0.05", 1, |o| o.line_spacing, |o, v| o.line_spacing = v)}
        {slider(s.margins, s.unit_mm, "1", 2, |o| o.margin_mm, |o, v| o.margin_mm = v)}
        <div class="accent">
            <label class="check">
                <input type="color"
                    prop:value=move || cv.with(|c| colour_value(c.as_ref().and_then(|c| c.options.accent.as_deref())))
                    on:input=move |ev| {
                        let v = event_target_value(&ev);
                        edit.run(Box::new(move |c| c.options.accent = Some(v)));
                    } />
                {s.accent}
            </label>
            <span class="status">
                {move || cv.with(|c| c.as_ref().map(|c| c.options.accent.is_none())).unwrap_or(true).then_some(s.accent_none)}
            </span>
            <button prop:disabled=move || cv.with(|c| c.as_ref().is_none_or(|c| c.options.accent.is_none()))
                on:click=move |_| edit.run(Box::new(|c| c.options.accent = None))>{s.accent_reset}</button>
        </div>
        <StyleFile cv=cv edit=edit error=error />
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RANGE: OptionRange = OptionRange {
        min: 9.0,
        max: 12.0,
        default: 10.0,
    };

    #[test]
    fn numbers_show_without_trailing_zeros() {
        assert_eq!(show_number(10.0), "10");
        assert_eq!(show_number(10.5), "10.5");
        assert_eq!(show_number(1.1), "1.1");
        assert_eq!(show_number(0.95), "0.95");
    }

    #[test]
    fn slider_values_parse_and_clamp() {
        assert_eq!(parse_in_range("10.5", RANGE), Some(10.5));
        assert_eq!(parse_in_range("4", RANGE), Some(9.0));
        assert_eq!(parse_in_range("99", RANGE), Some(12.0));
        assert_eq!(parse_in_range("NaN", RANGE), None);
        assert_eq!(parse_in_range("inf", RANGE), None);
        assert_eq!(parse_in_range("", RANGE), None);
    }

    #[test]
    fn colour_input_shows_black_for_no_accent() {
        assert_eq!(colour_value(None), "#000000");
        assert_eq!(colour_value(Some("#2B5CAB")), "#2b5cab");
        assert_eq!(colour_value(Some("blue")), "#000000");
    }

    fn opened(template: &str, keep_template: bool) -> StyleOpened {
        let mut style = Style::from_cv(&crate::layout::tests::cv());
        style.template = TemplateId(template.into());
        style.paper = Paper::Letter;
        StyleOpened {
            style,
            warnings: vec!["The margin was out of range and was adjusted.".into()],
            keep_template,
        }
    }

    #[test]
    fn an_unknown_template_is_not_applied() {
        let mut cv = crate::layout::tests::cv();
        apply_opened(&mut cv, &opened("fancy", true));
        assert_eq!(cv.template, TemplateId::default());
        assert_eq!(cv.paper, Paper::Letter);
        apply_opened(&mut cv, &opened("fancy", false));
        assert_eq!(cv.template.0, "fancy");
    }

    #[test]
    fn the_load_summary_lists_warnings_and_counts() {
        let applied = StyleApplied {
            unmatched_blocks: vec![cv_core::StyleBlock::Section {
                section: cv_core::SectionKind::Languages,
                title: None,
            }],
            unplaced_sections: 2,
        };
        let lines = load_summary(&opened("classic", false), &applied);
        assert_eq!(lines.len(), 4, "{lines:?}");
        assert_eq!(lines[0], t().style_loaded);
        assert!(lines[2].contains('1') && lines[3].contains('2'));
    }

    #[test]
    fn ranges_fall_back_to_the_clamping_ranges() {
        let [font, spacing, margin] = ranges_for(&[], &TemplateId::default());
        assert_eq!((font.min, font.max), TemplateOptions::FONT_SIZE_PT);
        assert_eq!((spacing.min, spacing.max), TemplateOptions::LINE_SPACING);
        assert_eq!((margin.min, margin.max), TemplateOptions::MARGIN_MM);
    }
}
