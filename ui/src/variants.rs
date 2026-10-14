//! Variant switcher and per-variant visibility (M5, spec section 8). Edits in
//! the form always change the base CV; this panel only changes the selected
//! variant's overlay: what is hidden and what is collapsed.

use cv_api::{NewVariant, SaveVariant};
use cv_core::{Cv, Entry, Id, JobTarget, Section, StoredVariant, VariantSummary};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Serialize;

use crate::ipc::{IdArg, ReqArg, call};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CvIdArg<'a> {
    cv_id: &'a Id,
}

/// The heading shown for a section in the variant panel.
pub fn section_label(s: &Section) -> String {
    match s {
        Section::Experience(_) => "Experience".into(),
        Section::Education(_) => "Education".into(),
        Section::Projects(_) => "Projects".into(),
        Section::Skills(_) => "Skills".into(),
        Section::Languages(_) => "Languages".into(),
        Section::Certifications(_) => "Certifications".into(),
        Section::Custom(c) => c.title.clone(),
    }
}

/// A one-line label for an entry.
pub fn entry_label(e: &Entry) -> String {
    match (e.title.trim(), e.org.trim()) {
        ("", "") => "(untitled entry)".into(),
        (t, "") => t.into(),
        ("", o) => o.into(),
        (t, o) => format!("{t}, {o}"),
    }
}

fn entries(s: &Section) -> &[Entry] {
    match s {
        Section::Experience(x)
        | Section::Education(x)
        | Section::Projects(x)
        | Section::Certifications(x) => &x.entries,
        Section::Custom(c) => &c.entries,
        Section::Skills(_) | Section::Languages(_) => &[],
    }
}

#[component]
pub fn VariantBar(
    cv_id: Id,
    cv: RwSignal<Option<Cv>>,
    variant: RwSignal<Option<StoredVariant>>,
    error: RwSignal<Option<String>>,
    /// Called after the selected variant or its overlay changes.
    changed: Callback<()>,
) -> impl IntoView {
    let list: RwSignal<Vec<VariantSummary>> = RwSignal::new(Vec::new());
    let reload = {
        let cv_id = cv_id.clone();
        move || {
            let cv_id = cv_id.clone();
            spawn_local(async move {
                match call::<_, Vec<VariantSummary>>("list_variants", &CvIdArg { cv_id: &cv_id })
                    .await
                {
                    Ok(v) => list.set(v),
                    Err(e) => error.set(Some(e.message)),
                }
            });
        }
    };
    reload();

    let select = move |id: String| {
        if id.is_empty() {
            variant.set(None);
            changed.run(());
            return;
        }
        let Ok(id) = Id::parse(&id) else { return };
        spawn_local(async move {
            match call::<_, StoredVariant>("get_variant", &IdArg { id: &id }).await {
                Ok(v) => {
                    variant.set(Some(v));
                    changed.run(());
                }
                Err(e) => error.set(Some(e.message)),
            }
        });
    };

    let create = {
        let reload = reload.clone();
        move |_| {
            let req = NewVariant {
                cv_id: cv_id.clone(),
                name: format!("Variant {}", list.with_untracked(Vec::len) + 1),
                job: JobTarget::default(),
            };
            let reload = reload.clone();
            spawn_local(async move {
                match call::<_, StoredVariant>("create_variant", &ReqArg { req: &req }).await {
                    Ok(v) => {
                        variant.set(Some(v));
                        reload();
                        changed.run(());
                    }
                    Err(e) => error.set(Some(e.message)),
                }
            });
        }
    };

    // Applies `f` to the overlay and saves it right away.
    let edit = move |f: Box<dyn FnOnce(&mut cv_core::Variant)>| {
        let Some(mut stored) = variant.get_untracked() else {
            return;
        };
        f(&mut stored.variant);
        let req = SaveVariant {
            variant: stored.variant.clone(),
            expected_revision: stored.revision,
        };
        variant.set(Some(stored));
        changed.run(());
        spawn_local(async move {
            match call::<_, StoredVariant>("save_variant", &ReqArg { req: &req }).await {
                Ok(saved) => variant.update(|v| {
                    if let Some(v) = v {
                        v.revision = saved.revision;
                    }
                }),
                Err(e) => error.set(Some(e.message)),
            }
        });
    };

    let rename = Callback::new({
        let reload = reload.clone();
        move |name: String| {
            let name = name.trim().to_owned();
            if name.is_empty() {
                return;
            }
            edit(Box::new(move |v| v.name = name));
            reload();
        }
    });
    let confirming = RwSignal::new(false);
    let delete = Callback::new({
        let reload = reload.clone();
        move |()| {
            let Some(id) = variant.with_untracked(|v| v.as_ref().map(|v| v.variant.id.clone()))
            else {
                return;
            };
            let reload = reload.clone();
            spawn_local(async move {
                match call::<_, ()>("delete_variant", &IdArg { id: &id }).await {
                    Ok(()) => {
                        confirming.set(false);
                        variant.set(None);
                        reload();
                        changed.run(());
                    }
                    Err(e) => error.set(Some(e.message)),
                }
            });
        }
    });

    let shown =
        move |id: &Id| variant.with(|v| v.as_ref().is_none_or(|v| !v.variant.hidden.contains(id)));
    let collapsed = move |id: &Id| {
        variant.with(|v| v.as_ref().is_some_and(|v| v.variant.collapsed.contains(id)))
    };
    let toggle = move |id: Id, collapse: bool| {
        edit(Box::new(move |v| {
            if collapse {
                v.toggle_collapsed(&id);
            } else {
                v.toggle_hidden(&id);
            }
        }))
    };

    view! {
        <div class="variants">
            <label>"Version "
                <select on:change=move |ev| select(event_target_value(&ev))>
                    <option value="" selected=move || variant.with(Option::is_none)>"Base CV"</option>
                    <For each=move || list.get() key=|s| s.id.clone() let:s>
                        {
                            let id = s.id.clone();
                            view! {
                                <option value=s.id.to_string()
                                    selected=move || variant.with(|v| v.as_ref().is_some_and(|v| v.variant.id == id))>
                                    {s.name.clone()}
                                </option>
                            }
                        }
                    </For>
                </select>
            </label>
            <button on:click=create>"New variant"</button>
        </div>
        <Show when=move || variant.with(Option::is_some)>
            <div class="banner" role="status">
                {move || variant.with(|v| v.as_ref().map(|v| format!(
                    "Showing variant \u{201c}{}\u{201d}. Text edits below change the base CV; choose here what this variant shows.",
                    v.variant.name
                )))}
            </div>
            <div class="variant-actions">
                <label>"Variant name "
                    <input prop:value=move || variant.with(|v| v.as_ref().map(|v| v.variant.name.clone()).unwrap_or_default())
                        on:change=move |ev| rename.run(event_target_value(&ev)) />
                </label>
                <Show when=move || !confirming.get()
                    fallback=move || view! {
                        <span>"Delete this variant? The base CV is not changed."</span>
                        <button on:click=move |_| delete.run(())>"Delete"</button>
                        <button on:click=move |_| confirming.set(false)>"Cancel"</button>
                    }>
                    <button on:click=move |_| confirming.set(true)>"Delete variant"</button>
                </Show>
            </div>
            <fieldset class="variant-visibility">
                <legend>"Shown in this variant"</legend>
                {move || cv.with(|c| c.as_ref().map(|c| c.sections.iter().map(|s| {
                    let sid = s.id().clone();
                    let sid2 = sid.clone();
                    let rows = entries(s).iter().map(|e| {
                        let (a, b, c2, d) = (e.id.clone(), e.id.clone(), e.id.clone(), e.id.clone());
                        view! {
                            <li>
                                <label><input type="checkbox" prop:checked=move || shown(&a)
                                    on:change=move |_| toggle(b.clone(), false) />{entry_label(e)}</label>
                                <label class="collapse"><input type="checkbox" prop:checked=move || collapsed(&c2)
                                    on:change=move |_| toggle(d.clone(), true) />"one line"</label>
                            </li>
                        }
                    }).collect_view();
                    view! {
                        <div class="variant-section">
                            <label><input type="checkbox" prop:checked=move || shown(&sid)
                                on:change=move |_| toggle(sid2.clone(), false) /><strong>{section_label(s)}</strong></label>
                            <ul>{rows}</ul>
                        </div>
                    }
                }).collect_view()))}
            </fieldset>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_for_entries_and_sections() {
        let cv: Cv =
            serde_json::from_str(include_str!("../../testdata/cvs/typical.json")).expect("cv");
        assert_eq!(section_label(&cv.sections[0]), "Experience");
        let e = &entries(&cv.sections[0])[0];
        assert!(entry_label(e).contains(", "));
        let mut blank = e.clone();
        blank.title.clear();
        blank.org.clear();
        assert_eq!(entry_label(&blank), "(untitled entry)");
    }
}
