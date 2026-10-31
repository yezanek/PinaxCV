//! The optional closing block (ADR-089): a consent clause with presets and
//! the "place, date" line, each with its own show switch on the base CV and
//! on each variant.

use cv_core::{CONSENT_PRESETS, ClosingPart, Cv, StoredVariant};
use leptos::prelude::*;

use crate::entries::Edit;
use crate::fit::EditFn;
use crate::i18n::t;

/// The UI label of `part`.
fn label(part: ClosingPart) -> &'static str {
    match part {
        ClosingPart::Consent => t().closing_consent,
        ClosingPart::PlaceDate => t().closing_place_date,
    }
}

/// The button text for the preset in language `lang`.
fn preset_label(lang: &str) -> &'static str {
    match lang {
        "it" => t().closing_preset_it,
        "pl" => t().closing_preset_pl,
        _ => t().closing_preset_en,
    }
}

fn toggle(set: &mut std::collections::BTreeSet<ClosingPart>, part: ClosingPart, on: bool) {
    if on {
        set.insert(part);
    } else {
        set.remove(&part);
    }
}

/// The "Closing" group at the end of the Content tab, collapsed by default.
#[component]
pub fn ClosingSection(cv: RwSignal<Option<Cv>>, edit: Edit) -> impl IntoView {
    let shown = move |part: ClosingPart| {
        move || cv.with(|c| c.as_ref().is_some_and(|c| c.closing.show.contains(&part)))
    };
    let show_switch = move |part: ClosingPart| {
        view! {
            <label class="check">
                <input type="checkbox" prop:checked=shown(part)
                    on:change=move |ev| {
                        let on = event_target_checked(&ev);
                        edit.run(Box::new(move |c| toggle(&mut c.closing.show, part, on)));
                    } />
                {t().personal_show}
            </label>
        }
    };
    let presets = CONSENT_PRESETS
        .iter()
        .map(|p| {
            let text = p.text;
            view! {
                <button type="button" on:click=move |_| {
                    edit.run(Box::new(move |c| c.closing.consent = Some(text.to_owned())));
                }>{preset_label(p.lang)}</button>
            }
        })
        .collect::<Vec<_>>();
    view! {
        <details class="personal" class:closing=true>
            <summary>{t().closing}</summary>
            <p class="hint">{t().closing_hint}</p>
            <div class="personal-row">
                <label>{t().closing_consent}
                    <textarea rows="3"
                        prop:value=move || cv.with(|c| c.as_ref().and_then(|c| c.closing.consent.clone()).unwrap_or_default())
                        on:input=move |ev| {
                            let v = event_target_value(&ev);
                            edit.run(Box::new(move |c| c.closing.consent = (!v.trim().is_empty()).then_some(v)));
                        }></textarea>
                </label>
                {show_switch(ClosingPart::Consent)}
            </div>
            <div class="presets">{t().closing_insert}" "{presets}</div>
            <div class="personal-row">
                <label>{t().closing_place}
                    <input prop:value=move || cv.with(|c| c.as_ref().and_then(|c| c.closing.place.clone()).unwrap_or_default())
                        on:input=move |ev| {
                            let v = event_target_value(&ev);
                            edit.run(Box::new(move |c| c.closing.place = (!v.trim().is_empty()).then_some(v)));
                        }
                        // Trim when the field loses focus, not while typing.
                        on:change=move |ev| {
                            let v = event_target_value(&ev);
                            if v.trim().len() != v.len() {
                                let v = v.trim().to_owned();
                                edit.run(Box::new(move |c| c.closing.place = (!v.is_empty()).then_some(v)));
                            }
                        } />
                </label>
                {show_switch(ClosingPart::PlaceDate)}
            </div>
            <p class="hint">{t().closing_date_hint}</p>
        </details>
    }
}

/// The variant's own switches for the closing block.
#[component]
pub fn VariantClosing(
    variant: RwSignal<Option<StoredVariant>>,
    edit: Callback<EditFn>,
    disabled: Signal<bool>,
) -> impl IntoView {
    let switch = move |part: ClosingPart| {
        view! {
            <label class="check">
                <input type="checkbox" prop:disabled=disabled
                    prop:checked=move || variant.with(|v| {
                        v.as_ref().is_some_and(|v| v.variant.show_closing.contains(&part))
                    })
                    on:change=move |ev| {
                        let on = event_target_checked(&ev);
                        edit.run(Box::new(move |v| toggle(&mut v.show_closing, part, on)));
                    } />
                {label(part)}
            </label>
        }
    };
    view! {
        <details class="personal">
            <summary>{t().closing_variant}</summary>
            {switch(ClosingPart::Consent)}
            {switch(ClosingPart::PlaceDate)}
        </details>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_has_a_button_label() {
        let labels: Vec<&str> = CONSENT_PRESETS
            .iter()
            .map(|p| preset_label(p.lang))
            .collect();
        assert_eq!(
            labels,
            [
                t().closing_preset_en,
                t().closing_preset_it,
                t().closing_preset_pl
            ]
        );
        let mut set = std::collections::BTreeSet::new();
        toggle(&mut set, ClosingPart::Consent, true);
        toggle(&mut set, ClosingPart::PlaceDate, false);
        assert_eq!(set, [ClosingPart::Consent].into());
    }
}
