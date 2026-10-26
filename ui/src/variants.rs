//! Variant switcher and per-variant visibility and order (M5, spec section
//! 8). Edits in the form always change the base CV; this panel only changes
//! the selected variant's overlay: what is hidden, what is collapsed, and the
//! order of sections, entries, bullets, skill groups, skills and languages.

use cv_api::{NewVariant, SaveVariant};
use cv_core::{Cv, Entry, Id, JobTarget, Section, StoredVariant, Variant, VariantSummary};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Serialize;

use crate::drag::DragHandle;
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

/// What a row in the visibility panel stands for, which picks its move labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    Entry,
    Bullet,
    Group,
    Skill,
    Language,
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
    pub kind: RowKind,
    /// The item the row is ordered within, and its children in base order.
    pub parent: Id,
    pub siblings: Vec<Id>,
    /// First or last in the variant's order, so it cannot move further.
    pub first: bool,
    pub last: bool,
    /// The base text a variant can replace: a bullet's text or an entry's
    /// description (empty when it has none).
    pub text: Option<String>,
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

/// `items` in the variant's order for `parent` (base order without one),
/// each with its base sibling IDs and whether it is first or last.
fn in_order<'a, T>(
    v: Option<&Variant>,
    parent: &Id,
    items: &'a [T],
    id_of: fn(&T) -> &Id,
) -> Vec<(&'a T, Vec<Id>, bool, bool)> {
    let base: Vec<Id> = items.iter().map(|i| id_of(i).clone()).collect();
    let order = match v {
        Some(v) => v.children_order(parent, &base),
        None => base.clone(),
    };
    let n = order.len();
    order
        .iter()
        .enumerate()
        .filter_map(|(i, id)| {
            let item = items.iter().find(|x| id_of(x) == id)?;
            Some((item, base.clone(), i == 0, i + 1 == n))
        })
        .collect()
}

/// The CV's sections in the variant's order, with base sibling IDs and
/// whether each is first or last.
pub fn sections_in_order<'a>(
    cv: &'a Cv,
    v: Option<&Variant>,
) -> Vec<(&'a Section, Vec<Id>, bool, bool)> {
    in_order(v, &cv.id, &cv.sections, Section::id)
}

/// An item's ID, its base sibling IDs, and whether it is first or last.
type Placed<'a> = (&'a Id, Vec<Id>, bool, bool);

/// Adds a row: the item, its parent, label, kind and replaceable text.
type Push<'b> = dyn for<'a> FnMut(Placed<'a>, &Id, String, RowKind, Option<String>) + 'b;

/// The rows under a section heading: every item a variant can hide
/// (ADR-015), in the variant's order.
pub fn rows(s: &Section, v: Option<&Variant>) -> Vec<Row> {
    let mut out = Vec::new();
    let mut push =
        |item: Placed, parent: &Id, label: String, kind: RowKind, text: Option<String>| {
            let (id, siblings, first, last) = item;
            out.push(Row {
                id: id.clone(),
                label,
                depth: u8::from(matches!(kind, RowKind::Bullet | RowKind::Skill)),
                collapsible: kind == RowKind::Entry,
                kind,
                parent: parent.clone(),
                siblings,
                first,
                last,
                text,
            });
        };
    let entries = |out: &mut Push, section: &Id, list: &[Entry]| {
        for (e, sib, first, last) in in_order(v, section, list, |e| &e.id) {
            out(
                (&e.id, sib, first, last),
                section,
                entry_label(e),
                RowKind::Entry,
                Some(e.description.clone().unwrap_or_default()),
            );
            for (b, sib, first, last) in in_order(v, &e.id, &e.bullets, |b| &b.id) {
                out(
                    (&b.id, sib, first, last),
                    &e.id,
                    short(&b.text),
                    RowKind::Bullet,
                    Some(b.text.clone()),
                );
            }
        }
    };
    match s {
        Section::Experience(x)
        | Section::Education(x)
        | Section::Projects(x)
        | Section::Certifications(x) => entries(&mut push, &x.id, &x.entries),
        Section::Custom(c) => entries(&mut push, &c.id, &c.entries),
        Section::Skills(k) => {
            for (g, sib, first, last) in in_order(v, &k.id, &k.groups, |g| &g.id) {
                let label = g
                    .label
                    .as_deref()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .unwrap_or(t().unlabelled_group);
                push(
                    (&g.id, sib, first, last),
                    &k.id,
                    label.into(),
                    RowKind::Group,
                    None,
                );
                for (k, sib, first, last) in in_order(v, &g.id, &g.skills, |k| &k.id) {
                    push(
                        (&k.id, sib, first, last),
                        &g.id,
                        k.name.clone(),
                        RowKind::Skill,
                        None,
                    );
                }
            }
        }
        Section::Languages(l) => {
            for (i, sib, first, last) in in_order(v, &l.id, &l.items, |i| &i.id) {
                push(
                    (&i.id, sib, first, last),
                    &l.id,
                    i.name.clone(),
                    RowKind::Language,
                    None,
                );
            }
        }
    }
    out
}

/// The up and down button labels for a row.
fn move_labels(kind: RowKind) -> (&'static str, &'static str) {
    let s = t();
    match kind {
        RowKind::Entry => (s.move_entry_up, s.move_entry_down),
        RowKind::Bullet => (s.move_bullet_up, s.move_bullet_down),
        RowKind::Group => (s.move_group_up, s.move_group_down),
        RowKind::Skill => (s.move_skill_up, s.move_skill_down),
        RowKind::Language => (s.move_language_up, s.move_language_down),
    }
}

/// "Edit for this variant" for one piece of text: a button that opens a
/// text area holding the variant's text (the base text until it differs),
/// and, while the variant has its own text, an "edited" mark and "Revert to
/// base".
#[component]
fn TextOverride(
    /// The edit button's accessible name and tooltip, also the text area's.
    label: &'static str,
    base: String,
    /// The variant's own text, if it has one.
    current: Signal<Option<String>>,
    set: Callback<String>,
    revert: Callback<()>,
) -> impl IntoView {
    let open = RwSignal::new(false);
    view! {
        <span class="text-override">
            <Show when=move || current.with(Option::is_some)>
                <span class="edited">{t().edited_in_variant}</span>
                <button on:click=move |_| revert.run(())>{t().revert_to_base}</button>
            </Show>
            <button class="icon" aria-label=label title=label aria-expanded=move || open.get().to_string()
                on:click=move |_| open.update(|o| *o = !*o)>"✎"</button>
        </span>
        <Show when=move || open.get()>
            {
                let base = base.clone();
                view! {
                    <textarea class="override" aria-label=label rows="3"
                        prop:value=move || current.get().unwrap_or_else(|| base.clone())
                        on:change=move |ev| set.run(event_target_value(&ev))></textarea>
                }
            }
        </Show>
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
    /// True while overlay edits are waiting to be saved.
    busy: RwSignal<bool>,
    /// Called when the save queue empties: `true` if everything was saved.
    settled: Callback<bool>,
) -> impl IntoView {
    let list: RwSignal<Vec<VariantSummary>> = RwSignal::new(Vec::new());
    let cv_id_for_order = StoredValue::new(cv_id.clone());
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
        // Drop the old variant while the new one loads, so an edit made in
        // the meantime can't land on it.
        variant.set(None);
        changed.run(());
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
    // Moves `id` among `siblings` (base order) of `parent`, one step or onto
    // a drop target, in this variant only.
    let reorder = move |parent: Id, siblings: Vec<Id>, id: Id, to: Result<bool, Id>| {
        edit(Box::new(move |v| {
            let current = v.children_order(&parent, &siblings);
            match to {
                Ok(up) => v.move_child(&parent, &current, &id, up),
                Err(target) => v.move_child_to(&parent, &current, &id, &target),
            };
        }))
    };
    let override_of = move |id: Id| {
        Signal::derive(move || {
            variant.with(|v| {
                v.as_ref()
                    .and_then(|v| v.variant.text_overrides.get(&id).cloned())
            })
        })
    };
    let summary_override = Signal::derive(move || {
        variant.with(|v| v.as_ref().and_then(|v| v.variant.summary_override.clone()))
    });
    let base_summary = Memo::new(move |_| cv.with(|c| c.as_ref().and_then(|c| c.summary.clone())));
    // The order lists alone, so hiding or collapsing does not rebuild the panel.
    let order = Memo::new(move |_| variant.with(|v| v.as_ref().map(|v| v.variant.order.clone())));
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
            <div class="variant-summary">
                <strong>{t().summary}</strong>
                {move || {
                    let base = base_summary.get();
                    view! {
                        <TextOverride label=t().edit_summary_in_variant base=base.clone().unwrap_or_default()
                            current=summary_override
                            set=Callback::new(move |text: String| {
                                let base = base.clone();
                                edit(Box::new(move |v| v.set_summary(text, base.as_deref())));
                            })
                            revert=Callback::new(move |()| edit(Box::new(|v| { v.revert_summary(); }))) />
                    }
                }}
            </div>
            <fieldset class="variant-visibility">
                <legend>{t().shown_in_variant}</legend>
                {move || {
                    // Only the order matters here; the rest of the overlay is read
                    // by each checkbox.
                    let order = order.get();
                    let overlay = order.map(|order| Variant { order, ..Variant::empty(cv_id_for_order.get_value(), cv_id_for_order.get_value(), "") });
                    cv.with(|c| c.as_ref().map(|c| sections_in_order(c, overlay.as_ref()).into_iter().map(|(s, siblings, first, last)| {
                    let sid = s.id().clone();
                    let sid2 = sid.clone();
                    let cv_list = c.id.to_string();
                    let (list_attr, id_attr, handle_id) = (cv_list.clone(), sid.to_string(), sid.clone());
                    let (up_ids, down_ids, drop_ids) = (
                        (c.id.clone(), siblings.clone(), sid.clone()),
                        (c.id.clone(), siblings.clone(), sid.clone()),
                        (c.id.clone(), siblings),
                    );
                    let section_dropped = Callback::new(move |(dragged, target): (Id, Id)| {
                        reorder(drop_ids.0.clone(), drop_ids.1.clone(), dragged, Err(target));
                    });
                    let rows = rows(s, overlay.as_ref()).into_iter().map(|r| {
                        let (a, b, c2, d) = (r.id.clone(), r.id.clone(), r.id.clone(), r.id.clone());
                        let nested = r.depth > 0;
                        let one_line = r.collapsible.then(|| view! {
                            <label class="collapse"><input type="checkbox" prop:checked=move || collapsed(&c2)
                                on:change=move |_| toggle(d.clone(), true) />{t().one_line}</label>
                        });
                        let text_edit = r.text.clone().map(|base| {
                            let label = if r.kind == RowKind::Entry { t().edit_description_in_variant } else { t().edit_bullet_in_variant };
                            let (set_id, set_base, revert_id) = (r.id.clone(), base.clone(), r.id.clone());
                            view! {
                                <TextOverride label=label base=base current=override_of(r.id.clone())
                                    set=Callback::new(move |text: String| {
                                        let (id, base) = (set_id.clone(), set_base.clone());
                                        edit(Box::new(move |v| v.set_text(&id, text, &base)));
                                    })
                                    revert=Callback::new(move |()| {
                                        let id = revert_id.clone();
                                        edit(Box::new(move |v| { v.revert_text(&id); }));
                                    }) />
                            }
                        });
                        let (up_label, down_label) = move_labels(r.kind);
                        let up = (r.parent.clone(), r.siblings.clone(), r.id.clone());
                        let down = up.clone();
                        let drop = (r.parent.clone(), r.siblings.clone());
                        let dropped = Callback::new(move |(dragged, target): (Id, Id)| {
                            reorder(drop.0.clone(), drop.1.clone(), dragged, Err(target));
                        });
                        view! {
                            <li class:nested=nested data-drag-list=r.parent.to_string() data-drag-id=r.id.to_string()>
                                <DragHandle list=r.parent.to_string() id=r.id.clone() dropped=dropped />
                                <label><input type="checkbox" prop:checked=move || shown(&a)
                                    on:change=move |_| toggle(b.clone(), false) />{r.label}</label>
                                {one_line}
                                {text_edit}
                                <span class="order">
                                    <button aria-label=up_label prop:disabled=r.first
                                        on:click=move |_| reorder(up.0.clone(), up.1.clone(), up.2.clone(), Ok(true))>"↑"</button>
                                    <button aria-label=down_label prop:disabled=r.last
                                        on:click=move |_| reorder(down.0.clone(), down.1.clone(), down.2.clone(), Ok(false))>"↓"</button>
                                </span>
                            </li>
                        }
                    }).collect_view();
                    view! {
                        <div class="variant-section" data-drag-list=list_attr data-drag-id=id_attr>
                            <div class="variant-section-head">
                                <DragHandle list=cv_list id=handle_id dropped=section_dropped />
                                <label><input type="checkbox" prop:checked=move || shown(&sid)
                                    on:change=move |_| toggle(sid2.clone(), false) /><strong>{section_label(s)}</strong></label>
                                <span class="order">
                                    <button aria-label=t().move_section_up prop:disabled=first
                                        on:click=move |_| reorder(up_ids.0.clone(), up_ids.1.clone(), up_ids.2.clone(), Ok(true))>"↑"</button>
                                    <button aria-label=t().move_section_down prop:disabled=last
                                        on:click=move |_| reorder(down_ids.0.clone(), down_ids.1.clone(), down_ids.2.clone(), Ok(false))>"↓"</button>
                                </span>
                            </div>
                            <ul>{rows}</ul>
                        </div>
                    }
                }).collect_view()))
                }}
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
            let r = rows(s, None);
            match s {
                Section::Experience(x) => {
                    let e = &x.entries[0];
                    assert_eq!(r[0].id, e.id);
                    assert!(r[0].collapsible);
                    assert_eq!(r[1].id, e.bullets[0].id);
                    assert_eq!((r[1].depth, r[1].collapsible), (1, false));
                    assert_eq!(r[1].text.as_deref(), Some(e.bullets[0].text.as_str()));
                    assert_eq!(
                        r[0].text.as_deref(),
                        Some(e.description.as_deref().unwrap_or_default())
                    );
                    let bullets: usize = x.entries.iter().map(|e| e.bullets.len()).sum();
                    assert_eq!(r.len(), x.entries.len() + bullets);
                }
                Section::Skills(k) => {
                    let skills: usize = k.groups.iter().map(|g| g.skills.len()).sum();
                    assert_eq!(r.len(), k.groups.len() + skills);
                    assert!(r.iter().all(|r| !r.collapsible && r.text.is_none()));
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
    fn rows_and_sections_follow_the_variant_order() {
        let cv: Cv =
            serde_json::from_str(include_str!("../../testdata/cvs/typical.json")).expect("cv");
        let mut v = Variant::empty(cv.id.clone(), cv.id.clone(), "v");
        let base: Vec<Id> = cv.sections.iter().map(|s| s.id().clone()).collect();
        let last = base.last().expect("sections").clone();
        assert!(v.move_child_to(&cv.id, &base, &last, &base[0]));
        let ordered = sections_in_order(&cv, Some(&v));
        assert_eq!(ordered[0].0.id(), &last);
        assert!(ordered[0].2 && !ordered[0].3);
        assert!(ordered.last().expect("last").3);
        assert_eq!(ordered[0].1, base);

        let Section::Experience(x) = &cv.sections[0] else {
            panic!("experience first")
        };
        let e = &x.entries[0];
        let bullets: Vec<Id> = e.bullets.iter().map(|b| b.id.clone()).collect();
        assert!(bullets.len() > 1);
        assert!(v.move_child(&e.id, &bullets, &bullets[1], true));
        let r = rows(&cv.sections[0], Some(&v));
        assert_eq!(r[1].id, bullets[1]);
        assert_eq!(r[1].parent, e.id);
        assert_eq!(r[1].siblings, bullets);
        assert!(r[1].first && !r[1].last);
        assert_eq!(r[1].kind, RowKind::Bullet);
        assert_eq!(r[0].kind, RowKind::Entry);
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
