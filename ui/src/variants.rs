//! Variant switcher and per-variant visibility (M5, spec section 8). Edits in
//! the form always change the base CV; this panel only changes the selected
//! variant's overlay: what is hidden and what is collapsed.

use cv_api::{NewVariant, SaveVariant};
use cv_core::{Cv, Entry, Id, JobTarget, Section, StoredVariant, VariantSummary};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Serialize;

use crate::i18n::{error_message, fill, t};
use crate::ipc::{IdArg, ReqArg, call};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CvIdArg<'a> {
    cv_id: &'a Id,
}

/// The heading shown for a section in the variant panel.
pub fn section_label(s: &Section) -> String {
    match s {
        Section::Experience(_) => t().experience.into(),
        Section::Education(_) => t().education.into(),
        Section::Projects(_) => t().projects.into(),
        Section::Skills(_) => t().skills.into(),
        Section::Languages(_) => t().languages.into(),
        Section::Certifications(_) => t().certifications.into(),
        Section::Custom(c) => c.title.clone(),
    }
}

/// A one-line label for an entry.
pub fn entry_label(e: &Entry) -> String {
    match (e.title.trim(), e.org.trim()) {
        ("", "") => t().untitled_entry.into(),
        (t, "") => t.into(),
        ("", o) => o.into(),
        (t, o) => format!("{t}, {o}"),
    }
}

/// One checkbox row in the visibility panel.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: Id,
    pub label: String,
    /// 0 for entries, skill groups and language items; 1 for bullets and
    /// skills.
    pub depth: u8,
    /// Entries can also be collapsed to one line.
    pub collapsible: bool,
}

/// Shortens bullet text for a checkbox label.
fn short(text: &str) -> String {
    const MAX: usize = 60;
    let text = text.trim();
    match text.char_indices().nth(MAX) {
        Some((i, _)) => format!("{}\u{2026}", &text[..i]),
        None if text.is_empty() => t().empty_bullet.into(),
        None => text.into(),
    }
}

/// The rows under a section heading: every item a variant can hide
/// (ADR-015), in CV order.
pub fn rows(s: &Section) -> Vec<Row> {
    let row = |id: &Id, label: String, depth: u8, collapsible: bool| Row {
        id: id.clone(),
        label,
        depth,
        collapsible,
    };
    let mut out = Vec::new();
    match s {
        Section::Experience(x)
        | Section::Education(x)
        | Section::Projects(x)
        | Section::Certifications(x) => {
            for e in &x.entries {
                out.push(row(&e.id, entry_label(e), 0, true));
                out.extend(
                    e.bullets
                        .iter()
                        .map(|b| row(&b.id, short(&b.text), 1, false)),
                );
            }
        }
        Section::Custom(c) => {
            for e in &c.entries {
                out.push(row(&e.id, entry_label(e), 0, true));
                out.extend(
                    e.bullets
                        .iter()
                        .map(|b| row(&b.id, short(&b.text), 1, false)),
                );
            }
        }
        Section::Skills(k) => {
            for g in &k.groups {
                let label = g
                    .label
                    .as_deref()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .unwrap_or(t().unlabelled_group);
                out.push(row(&g.id, label.into(), 0, false));
                out.extend(
                    g.skills
                        .iter()
                        .map(|k| row(&k.id, k.name.clone(), 1, false)),
                );
            }
        }
        Section::Languages(l) => {
            out.extend(l.items.iter().map(|i| row(&i.id, i.name.clone(), 0, false)));
        }
    }
    out
}

#[component]
pub fn VariantBar(
    cv_id: Id,
    cv: RwSignal<Option<Cv>>,
    variant: RwSignal<Option<StoredVariant>>,
    error: RwSignal<Option<String>>,
    /// Called after the selected variant or its overlay changes.
    changed: Callback<()>,
    /// True while overlay edits are waiting to be saved.
    busy: RwSignal<bool>,
    /// Called when the save queue empties: `true` if everything was saved.
    settled: Callback<bool>,
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
                    Err(e) => error.set(Some(error_message(&e))),
                }
            });
        }
    };
    reload();

    // The delete confirmation holds the ID it was opened for.
    let confirming: RwSignal<Option<Id>> = RwSignal::new(None);
    // Bumped on every selection; a `get_variant` answer for an older
    // selection is dropped.
    let selection = RwSignal::new(0u64);
    let select = move |id: String| {
        let n = selection.get_untracked() + 1;
        selection.set(n);
        confirming.set(None);
        if id.is_empty() {
            variant.set(None);
            changed.run(());
            return;
        }
        let Ok(id) = Id::parse(&id) else { return };
        spawn_local(async move {
            let result = call::<_, StoredVariant>("get_variant", &IdArg { id: &id }).await;
            if selection.try_get_untracked() != Some(n) {
                return;
            }
            match result {
                Ok(v) => {
                    variant.set(Some(v));
                    changed.run(());
                }
                Err(e) => error.set(Some(error_message(&e))),
            }
        });
    };

    let create = {
        let reload = reload.clone();
        move |_| {
            let req = NewVariant {
                cv_id: cv_id.clone(),
                name: fill(
                    t().variant_default_name,
                    &[("n", &(list.with_untracked(Vec::len) + 1).to_string())],
                ),
                job: JobTarget::default(),
            };
            let reload = reload.clone();
            let n = selection.get_untracked() + 1;
            selection.set(n);
            confirming.set(None);
            spawn_local(async move {
                let result =
                    call::<_, StoredVariant>("create_variant", &ReqArg { req: &req }).await;
                match result {
                    Ok(v) => {
                        reload();
                        if selection.try_get_untracked() != Some(n) {
                            return;
                        }
                        variant.set(Some(v));
                        changed.run(());
                    }
                    Err(e) => error.set(Some(error_message(&e))),
                }
            });
        }
    };

    // The newest overlay not yet sent. Saves run one at a time: an edit made
    // while a save is in flight is sent next, with the revision that save
    // returned. Selecting another variant is disabled while `busy`, so the
    // queue only ever holds the selected variant.
    let pending: RwSignal<Option<StoredVariant>> = RwSignal::new(None);
    let flush = move || {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        spawn_local(async move {
            loop {
                let Some(next) = pending.try_get_untracked().flatten() else {
                    if busy.try_set(false).is_none() {
                        settled.run(true);
                    }
                    return;
                };
                pending.set(None);
                let req = SaveVariant {
                    variant: next.variant,
                    expected_revision: next.revision,
                };
                match call::<_, StoredVariant>("save_variant", &ReqArg { req: &req }).await {
                    Ok(saved) => {
                        let same = |v: &mut Option<StoredVariant>| {
                            if let Some(v) = v
                                && v.variant.id == saved.variant.id
                            {
                                v.revision = saved.revision;
                            }
                        };
                        if variant.try_update(same).is_none() {
                            return; // The editor was closed.
                        }
                        pending.update(same);
                    }
                    // A conflict keeps the overlay on screen and shows the
                    // message; queued edits are dropped with it.
                    Err(e) => {
                        if busy.try_set(false).is_some() {
                            return;
                        }
                        pending.set(None);
                        error.set(Some(error_message(&e)));
                        settled.run(false);
                        return;
                    }
                }
            }
        });
    };

    // Applies `f` to the overlay and queues it for saving.
    let edit = move |f: Box<dyn FnOnce(&mut cv_core::Variant)>| {
        let Some(mut stored) = variant.get_untracked() else {
            return;
        };
        f(&mut stored.variant);
        variant.set(Some(stored.clone()));
        pending.set(Some(stored));
        changed.run(());
        flush();
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
    let delete = Callback::new({
        let reload = reload.clone();
        move |()| {
            let Some(id) = confirming.get_untracked() else {
                return;
            };
            let reload = reload.clone();
            spawn_local(async move {
                match call::<_, ()>("delete_variant", &IdArg { id: &id }).await {
                    Ok(()) => {
                        confirming.set(None);
                        let n = selection.get_untracked() + 1;
                        selection.set(n);
                        variant.set(None);
                        reload();
                        changed.run(());
                    }
                    Err(e) => error.set(Some(error_message(&e))),
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
            <label>{t().version}" "
                <select prop:disabled=move || busy.get()
                    on:change=move |ev| select(event_target_value(&ev))>
                    <option value="" selected=move || variant.with(Option::is_none)>{t().base_cv}</option>
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
            <button prop:disabled=move || busy.get() on:click=create>{t().new_variant}</button>
        </div>
        <Show when=move || variant.with(Option::is_some)>
            <div class="banner" role="status">
                {move || variant.with(|v| v.as_ref().map(|v| fill(t().showing_variant, &[("name", &v.variant.name)])))}
            </div>
            <div class="variant-actions">
                <label>{t().variant_name}" "
                    <input prop:value=move || variant.with(|v| v.as_ref().map(|v| v.variant.name.clone()).unwrap_or_default())
                        on:change=move |ev| rename.run(event_target_value(&ev)) />
                </label>
                <Show when=move || confirming.with(Option::is_none)
                    fallback=move || view! {
                        <span>{t().delete_variant_confirm}</span>
                        <button prop:disabled=move || busy.get()
                            on:click=move |_| delete.run(())>{t().delete}</button>
                        <button on:click=move |_| confirming.set(None)>{t().cancel}</button>
                    }>
                    <button on:click=move |_| confirming.set(
                        variant.with_untracked(|v| v.as_ref().map(|v| v.variant.id.clone())),
                    )>{t().delete_variant}</button>
                </Show>
            </div>
            <fieldset class="variant-visibility">
                <legend>{t().shown_in_variant}</legend>
                {move || cv.with(|c| c.as_ref().map(|c| c.sections.iter().map(|s| {
                    let sid = s.id().clone();
                    let sid2 = sid.clone();
                    let rows = rows(s).into_iter().map(|r| {
                        let (a, b, c2, d) = (r.id.clone(), r.id.clone(), r.id.clone(), r.id);
                        let nested = r.depth > 0;
                        let one_line = r.collapsible.then(|| view! {
                            <label class="collapse"><input type="checkbox" prop:checked=move || collapsed(&c2)
                                on:change=move |_| toggle(d.clone(), true) />{t().one_line}</label>
                        });
                        view! {
                            <li class:nested=nested>
                                <label><input type="checkbox" prop:checked=move || shown(&a)
                                    on:change=move |_| toggle(b.clone(), false) />{r.label}</label>
                                {one_line}
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
        let Section::Experience(x) = &cv.sections[0] else {
            panic!("experience first")
        };
        let e = &x.entries[0];
        assert!(entry_label(e).contains(", "));
        let mut blank = e.clone();
        blank.title.clear();
        blank.org.clear();
        assert_eq!(entry_label(&blank), "(untitled entry)");
    }

    #[test]
    fn rows_cover_bullets_skills_and_languages() {
        let cv: Cv =
            serde_json::from_str(include_str!("../../testdata/cvs/typical.json")).expect("cv");
        for s in &cv.sections {
            let r = rows(s);
            match s {
                Section::Experience(x) => {
                    let e = &x.entries[0];
                    assert_eq!(r[0].id, e.id);
                    assert!(r[0].collapsible);
                    assert_eq!(r[1].id, e.bullets[0].id);
                    assert_eq!((r[1].depth, r[1].collapsible), (1, false));
                    let bullets: usize = x.entries.iter().map(|e| e.bullets.len()).sum();
                    assert_eq!(r.len(), x.entries.len() + bullets);
                }
                Section::Skills(k) => {
                    let skills: usize = k.groups.iter().map(|g| g.skills.len()).sum();
                    assert_eq!(r.len(), k.groups.len() + skills);
                    assert!(r.iter().all(|r| !r.collapsible));
                }
                Section::Languages(l) => {
                    assert_eq!(r.len(), l.items.len());
                    assert!(r.iter().all(|r| r.depth == 0));
                }
                _ => {}
            }
        }
    }

    #[test]
    fn long_bullets_are_shortened() {
        assert_eq!(short("  Ran CI.  "), "Ran CI.");
        assert_eq!(short(" "), "(empty bullet)");
        let long = "é".repeat(70);
        let s = short(&long);
        assert_eq!(s.chars().count(), 61);
        assert!(s.ends_with('\u{2026}'));
    }
}
