//! Optional European personal details (ADR-088): the "Personal details"
//! group in the editor, and the per-variant switches. Every field has its own
//! show switch, off by default, like the photo.

use cv_core::{CalendarDate, Cv, PersonalDetails, PersonalField, StoredVariant};
use leptos::prelude::*;

use crate::entries::Edit;
use crate::fit::EditFn;
use crate::i18n::t;

/// The UI label of `field`.
pub fn label(field: PersonalField) -> &'static str {
    let s = t();
    match field {
        PersonalField::Nationality => s.personal_nationality,
        PersonalField::BirthDate => s.personal_birth_date,
        PersonalField::BirthPlace => s.personal_birth_place,
        PersonalField::Gender => s.personal_gender,
        PersonalField::MaritalStatus => s.personal_marital_status,
        PersonalField::Address => s.personal_address,
        PersonalField::DrivingLicence => s.personal_driving_licence,
        PersonalField::WorkPermit => s.personal_work_permit,
    }
}

/// The field's text as the editor shows it: lists joined with ", ", the
/// birth date as `YYYY-MM-DD`.
fn text_of(p: &PersonalDetails, field: PersonalField) -> String {
    let opt = |s: &Option<String>| s.clone().unwrap_or_default();
    match field {
        PersonalField::Nationality => p.nationality.join(", "),
        PersonalField::BirthDate => p.birth_date.map(|d| d.iso()).unwrap_or_default(),
        PersonalField::BirthPlace => opt(&p.birth_place),
        PersonalField::Gender => opt(&p.gender),
        PersonalField::MaritalStatus => opt(&p.marital_status),
        PersonalField::Address => opt(&p.address),
        PersonalField::DrivingLicence => p.driving_licence.join(", "),
        PersonalField::WorkPermit => opt(&p.work_permit),
    }
}

/// Splits a comma-separated list, dropping blank items.
fn split_list(v: &str) -> Vec<String> {
    v.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Stores `v` in a text field: blank becomes `None`.
fn set_text(p: &mut PersonalDetails, field: PersonalField, v: String) {
    let v = (!v.trim().is_empty()).then_some(v);
    match field {
        PersonalField::BirthPlace => p.birth_place = v,
        PersonalField::Gender => p.gender = v,
        PersonalField::MaritalStatus => p.marital_status = v,
        PersonalField::Address => p.address = v,
        PersonalField::WorkPermit => p.work_permit = v,
        PersonalField::Nationality => p.nationality = split_list(v.as_deref().unwrap_or("")),
        PersonalField::DrivingLicence => {
            p.driving_licence = split_list(v.as_deref().unwrap_or(""));
        }
        PersonalField::BirthDate => {}
    }
}

fn toggle(set: &mut std::collections::BTreeSet<PersonalField>, field: PersonalField, on: bool) {
    if on {
        set.insert(field);
    } else {
        set.remove(&field);
    }
}

/// The "Personal details" group in the Content tab, collapsed by default.
#[component]
pub fn PersonalSection(cv: RwSignal<Option<Cv>>, edit: Edit) -> impl IntoView {
    // The birth date text the user typed that is not a date yet.
    let bad_date: RwSignal<Option<String>> = RwSignal::new(None);
    let filled = move || {
        cv.with(|c| {
            c.as_ref().map_or(0, |c| {
                PersonalField::ALL
                    .iter()
                    .filter(|f| c.basics.personal.is_filled(**f))
                    .count()
            })
        })
    };
    let row = move |field: PersonalField| {
        let value = move || {
            cv.with(|c| {
                c.as_ref()
                    .map(|c| text_of(&c.basics.personal, field))
                    .unwrap_or_default()
            })
        };
        let shown = move || {
            cv.with(|c| {
                c.as_ref()
                    .is_some_and(|c| c.basics.show_personal.contains(&field))
            })
        };
        let input = match field {
            PersonalField::Address => view! {
                <textarea rows="2" prop:value=value
                    on:input=move |ev| {
                        let v = event_target_value(&ev);
                        edit.run(Box::new(move |c| set_text(&mut c.basics.personal, field, v)));
                    }></textarea>
            }
            .into_any(),
            PersonalField::BirthDate => view! {
                <input placeholder=t().personal_birth_date_placeholder inputmode="numeric"
                    aria-invalid=move || bad_date.with(Option::is_some).to_string()
                    aria-describedby="birth-date-hint"
                    prop:value=move || bad_date.get().unwrap_or_else(value)
                    on:change=move |ev| {
                        let v = event_target_value(&ev);
                        if v.trim().is_empty() {
                            bad_date.set(None);
                            edit.run(Box::new(|c| c.basics.personal.birth_date = None));
                        } else if let Some(d) = CalendarDate::parse(&v) {
                            bad_date.set(None);
                            edit.run(Box::new(move |c| c.basics.personal.birth_date = Some(d)));
                        } else {
                            bad_date.set(Some(v));
                        }
                    } />
            }
            .into_any(),
            // Lists are split when the field loses focus, so typing a comma
            // does not move the cursor.
            PersonalField::Nationality | PersonalField::DrivingLicence => view! {
                <input placeholder=if field == PersonalField::Nationality {
                        t().personal_nationality_placeholder
                    } else {
                        t().personal_driving_licence_placeholder
                    }
                    prop:value=value
                    on:change=move |ev| {
                        let v = event_target_value(&ev);
                        edit.run(Box::new(move |c| set_text(&mut c.basics.personal, field, v)));
                    } />
            }
            .into_any(),
            _ => view! {
                <input prop:value=value
                    on:input=move |ev| {
                        let v = event_target_value(&ev);
                        edit.run(Box::new(move |c| set_text(&mut c.basics.personal, field, v)));
                    }
                    // Trim when the field loses focus, not while typing.
                    on:change=move |ev| {
                        let v = event_target_value(&ev);
                        if v.trim().len() != v.len() {
                            let v = v.trim().to_owned();
                            edit.run(Box::new(move |c| set_text(&mut c.basics.personal, field, v)));
                        }
                    } />
            }
            .into_any(),
        };
        view! {
            <div class="personal-row">
                <label>{label(field)}{input}</label>
                <label class="check">
                    <input type="checkbox" prop:checked=shown
                        on:change=move |ev| {
                            let on = event_target_checked(&ev);
                            edit.run(Box::new(move |c| toggle(&mut c.basics.show_personal, field, on)));
                        } />
                    {t().personal_show}
                </label>
            </div>
        }
    };
    view! {
        <details class="personal">
            <summary>
                {t().personal_details}
                {move || match filled() {
                    0 => String::new(),
                    n => format!(" ({n})"),
                }}
            </summary>
            <p class="hint">{t().personal_hint}</p>
            {PersonalField::ALL.into_iter().map(row).collect::<Vec<_>>()}
            <p id="birth-date-hint" class="hint" class:error=move || bad_date.with(Option::is_some)>
                {move || if bad_date.with(Option::is_some) {
                    t().personal_birth_date_invalid
                } else {
                    t().personal_birth_date_format
                }}
            </p>
        </details>
    }
}

/// The variant's own switches: one per personal detail the base CV fills.
#[component]
pub fn VariantPersonal(
    cv: RwSignal<Option<Cv>>,
    variant: RwSignal<Option<StoredVariant>>,
    edit: Callback<EditFn>,
    disabled: Signal<bool>,
) -> impl IntoView {
    let fields = Memo::new(move |_| {
        cv.with(|c| {
            c.as_ref().map_or_else(Vec::new, |c| {
                PersonalField::ALL
                    .into_iter()
                    .filter(|f| c.basics.personal.is_filled(*f))
                    .collect::<Vec<_>>()
            })
        })
    });
    view! {
        <Show when=move || fields.with(|f| !f.is_empty())>
            <details class="personal">
                <summary>{t().personal_variant}</summary>
                <For each=move || fields.get() key=|f| *f let:field>
                    <label class="check">
                        <input type="checkbox" prop:disabled=disabled
                            prop:checked=move || variant.with(|v| {
                                v.as_ref().is_some_and(|v| v.variant.show_personal.contains(&field))
                            })
                            on:change=move |ev| {
                                let on = event_target_checked(&ev);
                                edit.run(Box::new(move |v| toggle(&mut v.show_personal, field, on)));
                            } />
                        {label(field)}
                    </label>
                </For>
            </details>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_split_on_commas_and_drop_blanks() {
        let mut p = PersonalDetails::default();
        set_text(
            &mut p,
            PersonalField::Nationality,
            "Italian, , German ".into(),
        );
        assert_eq!(p.nationality, ["Italian", "German"]);
        assert_eq!(text_of(&p, PersonalField::Nationality), "Italian, German");
        set_text(&mut p, PersonalField::DrivingLicence, " ".into());
        assert!(p.driving_licence.is_empty());
        set_text(&mut p, PersonalField::Gender, "  ".into());
        assert_eq!(p.gender, None);
        set_text(
            &mut p,
            PersonalField::Address,
            "Via Esempio 1\n10100 Torino".into(),
        );
        assert_eq!(
            text_of(&p, PersonalField::Address),
            "Via Esempio 1\n10100 Torino"
        );
    }
}
