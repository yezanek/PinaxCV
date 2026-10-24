//! The import wizard (spec section 9.5): shows what was read from a file and
//! its warnings, then saves it as a new CV or merges it into an existing one.
//! A merge asks, for each conflicting contact field, whether to keep the CV's
//! value or take the imported one, and for each imported entry whether to
//! skip it, add it or let it replace an existing entry.

use std::collections::HashMap;

use cv_api::{
    EntryChoice, EntryDecision, EntryMatch, ErrorCode, ImportCommit, ImportField, ImportMatch,
    ImportMatchRequest, ImportPreview, ImportTarget,
};
use cv_core::{CvSummary, Entry, EntrySection, Id, Section, StoredCv};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::i18n::{error_message, fill, t};
use crate::ipc::{IdArg, ReqArg, call};

/// Sections whose entries the merge review lists, tagged by kind so an
/// imported entry is only offered entries of the same kind to replace.
pub fn entry_sections(sections: &[Section]) -> Vec<(u8, &EntrySection)> {
    sections
        .iter()
        .filter_map(|s| match s {
            Section::Experience(x) => Some((0, x)),
            Section::Education(x) => Some((1, x)),
            Section::Projects(x) => Some((2, x)),
            Section::Certifications(x) => Some((3, x)),
            _ => None,
        })
        .collect()
}

/// "Title, Organisation", or as much of it as there is.
pub fn entry_label(e: &Entry) -> String {
    let parts: Vec<&str> = [e.title.trim(), e.org.trim()]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        t().untitled_entry.to_owned()
    } else {
        parts.join(", ")
    }
}

/// One line per imported section: its name and how many items it holds.
pub fn section_lines(sections: &[Section]) -> Vec<String> {
    let s = t();
    sections
        .iter()
        .map(|section| {
            let (name, n) = match section {
                Section::Experience(x) => (s.work_experience.to_owned(), x.entries.len()),
                Section::Education(x) => (s.education.to_owned(), x.entries.len()),
                Section::Projects(x) => (s.projects.to_owned(), x.entries.len()),
                Section::Certifications(x) => (s.certifications.to_owned(), x.entries.len()),
                Section::Skills(x) => (
                    s.skills.to_owned(),
                    x.groups.iter().map(|g| g.skills.len()).sum(),
                ),
                Section::Languages(x) => (s.languages.to_owned(), x.items.len()),
                Section::Custom(x) => (x.title.clone(), x.entries.len()),
            };
            fill(
                s.import_section_line,
                &[("section", &name), ("n", &n.to_string())],
            )
        })
        .collect()
}

/// Replace the suggested match, otherwise add the entry as new.
pub fn default_choices(matches: &[EntryMatch]) -> HashMap<Id, EntryChoice> {
    matches
        .iter()
        .map(|m| {
            (
                m.imported.clone(),
                EntryChoice::Replace {
                    base: m.base.clone(),
                },
            )
        })
        .collect()
}

/// The `<option>` value for a choice, and back.
pub fn choice_value(choice: &EntryChoice) -> String {
    match choice {
        EntryChoice::Skip => "skip".into(),
        EntryChoice::AddNew => "add".into(),
        EntryChoice::Replace { base } => format!("replace:{base}"),
    }
}

pub fn parse_choice(value: &str) -> EntryChoice {
    match value.strip_prefix("replace:").map(Id::parse) {
        Some(Ok(base)) => EntryChoice::Replace { base },
        _ if value == "skip" => EntryChoice::Skip,
        _ => EntryChoice::AddNew,
    }
}

fn field_label(field: ImportField) -> &'static str {
    let s = t();
    match field {
        ImportField::FullName => s.full_name,
        ImportField::Headline => s.headline,
        ImportField::Email => s.email,
        ImportField::Phone => s.phone,
        ImportField::Location => s.location,
        ImportField::Summary => s.summary,
    }
}

/// The CV being merged into and how the import lines up with it.
#[derive(Clone)]
struct Review {
    base: StoredCv,
    matched: ImportMatch,
}

#[component]
pub fn ImportWizard(
    preview: ImportPreview,
    cvs: Vec<CvSummary>,
    /// Set to `None` to close the wizard.
    importing: RwSignal<Option<ImportPreview>>,
    open: RwSignal<Option<Id>>,
) -> impl IntoView {
    let s = t();
    let preview_id = preview.preview_id;
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let busy = RwSignal::new(false);
    let as_new = RwSignal::new(true);
    let title = RwSignal::new(String::new());
    let merge_into: RwSignal<Option<Id>> = RwSignal::new(cvs.first().map(|c| c.id.clone()));
    let review: RwSignal<Option<Review>> = RwSignal::new(None);
    let use_imported: RwSignal<Vec<ImportField>> = RwSignal::new(Vec::new());
    let choices: RwSignal<HashMap<Id, EntryChoice>> = RwSignal::new(HashMap::new());
    // Counts loads, so a slow answer for a CV no longer chosen is dropped.
    let loads = RwSignal::new(0u64);
    let imported_entries: Vec<(u8, Entry)> = entry_sections(&preview.sections)
        .into_iter()
        .flat_map(|(kind, x)| x.entries.iter().map(move |e| (kind, e.clone())))
        .collect();

    let load = move |cv_id: Id| {
        let n = loads.get_untracked() + 1;
        loads.set(n);
        review.set(None);
        spawn_local(async move {
            let base = call::<_, StoredCv>("get_cv", &IdArg { id: &cv_id }).await;
            let req = ImportMatchRequest { preview_id, cv_id };
            let matched = call::<_, ImportMatch>("import_match", &ReqArg { req: &req }).await;
            if loads.try_get_untracked() != Some(n) {
                return;
            }
            match (base, matched) {
                (Ok(base), Ok(matched)) => {
                    use_imported.set(Vec::new());
                    choices.set(default_choices(&matched.matches));
                    review.set(Some(Review { base, matched }));
                }
                (Err(e), _) | (_, Err(e)) => error.set(Some(error_message(&e))),
            }
        });
    };

    let choose_merge = move || {
        as_new.set(false);
        error.set(None);
        if let Some(id) = merge_into.get_untracked() {
            load(id);
        }
    };

    let commit = move |_| {
        if busy.get_untracked() {
            return;
        }
        let target = if as_new.get_untracked() {
            ImportTarget::New {
                title: title.get_untracked(),
            }
        } else {
            let Some(r) = review.get_untracked() else {
                return;
            };
            ImportTarget::Merge {
                cv_id: r.base.cv.id,
                expected_revision: r.base.revision,
            }
        };
        let merging = matches!(target, ImportTarget::Merge { .. });
        let req = ImportCommit {
            preview_id,
            target,
            use_imported: if merging {
                use_imported.get_untracked()
            } else {
                Vec::new()
            },
            entries: if merging {
                choices
                    .get_untracked()
                    .into_iter()
                    .map(|(imported, choice)| EntryDecision { imported, choice })
                    .collect()
            } else {
                Vec::new()
            },
        };
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            let result = call::<_, StoredCv>("import_commit", &ReqArg { req: &req }).await;
            if busy.try_set(false).is_some() {
                return; // The wizard was closed.
            }
            match result {
                Ok(stored) => {
                    importing.set(None);
                    open.set(Some(stored.cv.id));
                }
                Err(e) => error.set(Some(error_message(&e))),
            }
        });
    };

    let has_cvs = !cvs.is_empty();
    let cvs = StoredValue::new(cvs);
    let name_placeholder = preview
        .full_name
        .clone()
        .unwrap_or_else(|| s.imported_cv_default.to_owned());
    let lines = section_lines(&preview.sections);
    let has_warnings = !preview.warnings.is_empty();
    let warnings = StoredValue::new(preview.warnings.clone());

    // The merge review: contact conflicts, then one row per imported entry.
    let review_view = move || {
        let Some(Review { base, matched }) = review.get() else {
            return view! { <p class="status">{s.loading}</p> }.into_any();
        };
        let hint = (!matched.conflicts.is_empty())
            .then(|| view! { <p class="status">{s.import_review_hint}</p> });
        let conflicts = matched
            .conflicts
            .into_iter()
            .map(|c| {
                let field = c.field;
                let name = format!("conflict-{field:?}");
                view! {
                    <fieldset class="segmented">
                        <legend>{field_label(field)}</legend>
                        <label>
                            <input type="radio" name=name.clone()
                                prop:checked=move || use_imported.with(|u| !u.contains(&field))
                                on:change=move |_| use_imported.update(|u| u.retain(|f| *f != field)) />
                            {fill(s.keep_mine, &[("value", &c.mine)])}
                        </label>
                        <label>
                            <input type="radio" name=name
                                prop:checked=move || use_imported.with(|u| u.contains(&field))
                                on:change=move |_| use_imported.update(|u| if !u.contains(&field) { u.push(field) }) />
                            {fill(s.use_imported, &[("value", &c.imported)])}
                        </label>
                    </fieldset>
                }
            })
            .collect_view();
        let base_sections = entry_sections(&base.cv.sections);
        let rows = imported_entries
            .iter()
            .map(|(kind, e)| {
                let id = e.id.clone();
                let current = {
                    let id = id.clone();
                    move || choices.with(|c| c.get(&id).map(choice_value).unwrap_or_else(|| choice_value(&EntryChoice::AddNew)))
                };
                let replace_options = base_sections
                    .iter()
                    .filter(|(k, _)| k == kind)
                    .flat_map(|(_, x)| &x.entries)
                    .map(|b| {
                        let value = choice_value(&EntryChoice::Replace { base: b.id.clone() });
                        let selected = {
                            let (value, current) = (value.clone(), current.clone());
                            move || current() == value
                        };
                        view! { <option value=value selected=selected>{fill(s.import_replace, &[("entry", &entry_label(b))])}</option> }
                    })
                    .collect_view();
                let (add_sel, skip_sel) = (current.clone(), current);
                view! {
                    <li>
                        <label>{entry_label(e)}
                            <select on:change=move |ev| {
                                let choice = parse_choice(&event_target_value(&ev));
                                choices.update(|c| { c.insert(id.clone(), choice); });
                            }>
                                <option value="add" selected=move || add_sel() == "add">{s.import_add_new}</option>
                                <option value="skip" selected=move || skip_sel() == "skip">{s.import_skip}</option>
                                {replace_options}
                            </select>
                        </label>
                    </li>
                }
            })
            .collect_view();
        view! {
            {hint}
            {conflicts}
            <h3>{s.import_entries}</h3>
            <ul class="import-entries">{rows}</ul>
            <p class="status">{s.import_merge_note}</p>
        }
        .into_any()
    };

    view! {
        <header class="bar"><h1>{s.import_title}</h1></header>
        <main class="list" class:import=true>
            <div class="form">
            <h2>{preview.full_name.clone().unwrap_or_else(|| s.import_no_name.to_owned())}</h2>
            <ul class="import-summary">
                {lines.into_iter().map(|l| view! { <li>{l}</li> }).collect_view()}
            </ul>
            <Show when=move || has_warnings>
                <h3>{s.import_warnings}</h3>
                <ul class="warnings">
                    {move || warnings.with_value(|list| list.iter().map(|w| view! { <li>{w.clone()}</li> }).collect_view())}
                </ul>
            </Show>
            <fieldset class="segmented">
                <legend>{s.import_into}</legend>
                <label>
                    <input type="radio" name="import-target" prop:checked=move || as_new.get()
                        on:change=move |_| { as_new.set(true); error.set(None); } />
                    {s.import_as_new}
                </label>
                <label>
                    <input type="radio" name="import-target" prop:checked=move || !as_new.get()
                        prop:disabled=!has_cvs
                        on:change=move |_| choose_merge() />
                    {s.import_merge}
                </label>
            </fieldset>
            <Show when=move || as_new.get()>
                <label>{s.cv_title}
                    <input placeholder=name_placeholder.clone() prop:value=move || title.get()
                        on:input=move |ev| title.set(event_target_value(&ev)) />
                </label>
            </Show>
            <Show when=move || !as_new.get()>
                <label>{s.import_merge_into}
                    <select on:change=move |ev| {
                        let Ok(id) = Id::parse(&event_target_value(&ev)) else { return };
                        merge_into.set(Some(id.clone()));
                        load(id);
                    }>
                        {move || cvs.with_value(|list| list.iter().map(|c| view! { <option value=c.id.to_string()>{c.title.clone()}</option> }).collect_view())}
                    </select>
                </label>
                {review_view.clone()}
            </Show>
            <p class="error">{move || error.get()}</p>
            <p class="new-cv">
                <button on:click=commit
                    prop:disabled=move || busy.get() || (!as_new.get() && review.with(Option::is_none))>
                    {s.import_button}
                </button>
                <button on:click=move |_| importing.set(None) prop:disabled=move || busy.get()>{s.cancel}</button>
            </p>
            </div>
        </main>
    }
}

/// Opens the file dialog and returns the preview, `Ok(None)` when the user
/// cancelled it.
pub async fn choose_file() -> Result<Option<ImportPreview>, String> {
    match call::<_, ImportPreview>("import_parse", &crate::ipc::NoArgs {}).await {
        Ok(p) => Ok(Some(p)),
        Err(e) if e.code == ErrorCode::Cancelled => Ok(None),
        Err(e) => Err(error_message(&e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cv_core::{LanguageItem, LanguagesSection, Period};

    fn id(n: u8) -> Id {
        Id::parse(&format!("01JAAAAAAAAAAAAAAAAAAAAA{n:02}")).expect("id")
    }

    fn entry(n: u8, title: &str, org: &str) -> Entry {
        Entry {
            id: id(n),
            title: title.into(),
            org: org.into(),
            location: None,
            period: Period {
                start: None,
                end: None,
                current: false,
            },
            bullets: Vec::new(),
            description: None,
            url: None,
            locked: false,
        }
    }

    #[test]
    fn entries_are_labelled_by_title_and_organisation() {
        assert_eq!(entry_label(&entry(1, "Engineer", "Acme")), "Engineer, Acme");
        assert_eq!(entry_label(&entry(1, " ", "Acme")), "Acme");
        assert_eq!(entry_label(&entry(1, "", "")), t().untitled_entry);
    }

    #[test]
    fn choices_round_trip_through_option_values() {
        for choice in [
            EntryChoice::Skip,
            EntryChoice::AddNew,
            EntryChoice::Replace { base: id(4) },
        ] {
            assert_eq!(parse_choice(&choice_value(&choice)), choice);
        }
        assert_eq!(parse_choice("replace:not-an-id"), EntryChoice::AddNew);
    }

    #[test]
    fn matches_default_to_replace() {
        let choices = default_choices(&[EntryMatch {
            imported: id(1),
            base: id(2),
        }]);
        assert_eq!(
            choices.get(&id(1)),
            Some(&EntryChoice::Replace { base: id(2) })
        );
        assert_eq!(choices.get(&id(3)), None);
    }

    #[test]
    fn summary_counts_items_per_section() {
        let sections = vec![
            Section::Experience(EntrySection {
                id: id(10),
                title_override: None,
                entries: vec![entry(1, "A", "B"), entry(2, "C", "D")],
            }),
            Section::Languages(LanguagesSection {
                id: id(11),
                title_override: None,
                items: vec![LanguageItem {
                    id: id(12),
                    name: "German".into(),
                    level: None,
                }],
            }),
        ];
        assert_eq!(
            section_lines(&sections),
            ["Work experience: 2", "Languages: 1"]
        );
        assert_eq!(entry_sections(&sections).len(), 1);
    }
}
