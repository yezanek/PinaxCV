//! Editing entry sections (experience, education, projects, certifications):
//! add, remove and reorder sections, entries and bullets, and edit their text.
//! Lists are keyed by ID, so typing never rebuilds the input in focus.

use cv_core::{
    Bullet, CustomSection, Cv, Entry, EntrySection, Id, LanguagesSection, PartialDate, Period,
    Section, SkillsSection,
};
use leptos::prelude::*;

use crate::dates;
use crate::drag::{DragHandle, move_to};
use crate::i18n::t;
use crate::ids::new_id;
use crate::lists::{LanguagesEditor, SkillsEditor, new_group, new_language};
use crate::order::shift;

/// Applies an edit to the CV and schedules preview and autosave.
pub type Edit = Callback<Box<dyn FnOnce(&mut Cv)>>;

/// The entries of an entry section or a custom section.
fn entries_mut<'a>(cv: &'a mut Cv, id: &Id) -> Option<&'a mut Vec<Entry>> {
    cv.sections.iter_mut().find_map(|s| match s {
        Section::Experience(x)
        | Section::Education(x)
        | Section::Projects(x)
        | Section::Certifications(x)
            if &x.id == id =>
        {
            Some(&mut x.entries)
        }
        Section::Custom(x) if &x.id == id => Some(&mut x.entries),
        _ => None,
    })
}

fn entries_ref<'a>(cv: &'a Cv, id: &Id) -> Option<&'a [Entry]> {
    cv.sections.iter().find_map(|s| match s {
        Section::Experience(x)
        | Section::Education(x)
        | Section::Projects(x)
        | Section::Certifications(x)
            if &x.id == id =>
        {
            Some(x.entries.as_slice())
        }
        Section::Custom(x) if &x.id == id => Some(x.entries.as_slice()),
        _ => None,
    })
}

fn entry_mut<'a>(cv: &'a mut Cv, section: &Id, entry: &Id) -> Option<&'a mut Entry> {
    entries_mut(cv, section)?
        .iter_mut()
        .find(|e| &e.id == entry)
}

fn entry_ref<'a>(cv: &'a Cv, section: &Id, entry: &Id) -> Option<&'a Entry> {
    entries_ref(cv, section)?.iter().find(|e| &e.id == entry)
}

/// The editor a section needs: entries (with a kind label; custom sections
/// have none, their title is edited in place), skills or languages.
#[derive(Clone, Debug, PartialEq, Eq)]
enum SectionForm {
    Entries(Option<&'static str>),
    Skills,
    Languages,
}

/// Every section with its editor, in CV order, so the form follows
/// creation and reordering across kinds.
fn section_forms(cv: &Cv) -> Vec<(Id, SectionForm)> {
    cv.sections
        .iter()
        .map(|s| match s {
            Section::Experience(x) => (
                x.id.clone(),
                SectionForm::Entries(Some(t().work_experience)),
            ),
            Section::Education(x) => (x.id.clone(), SectionForm::Entries(Some(t().education))),
            Section::Projects(x) => (x.id.clone(), SectionForm::Entries(Some(t().projects))),
            Section::Certifications(x) => {
                (x.id.clone(), SectionForm::Entries(Some(t().certifications)))
            }
            Section::Custom(x) => (x.id.clone(), SectionForm::Entries(None)),
            Section::Skills(x) => (x.id.clone(), SectionForm::Skills),
            Section::Languages(x) => (x.id.clone(), SectionForm::Languages),
        })
        .collect()
}

fn new_entry(id: Id) -> Entry {
    Entry {
        id,
        title: String::new(),
        org: String::new(),
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

/// A blank field becomes `None`; other text is kept as typed, so the input
/// is not rewritten mid-word (trailing spaces survive until the next key).
fn opt(s: String) -> Option<String> {
    (!s.trim().is_empty()).then_some(s)
}

/// The editors of every section, in the order the CV lists them.
#[component]
pub fn Sections(cv: RwSignal<Option<Cv>>, edit: Edit) -> impl IntoView {
    view! {
        <For
            each=move || cv.with(|c| c.as_ref().map(section_forms).unwrap_or_default())
            key=|(id, _)| id.clone()
            let:section
        >
            {match section.1 {
                SectionForm::Entries(label) => {
                    view! { <SectionEditor cv=cv edit=edit id=section.0 label=label /> }.into_any()
                }
                SectionForm::Skills => view! { <SkillsEditor cv=cv edit=edit id=section.0 /> }.into_any(),
                SectionForm::Languages => {
                    view! { <LanguagesEditor cv=cv edit=edit id=section.0 /> }.into_any()
                }
            }}
        </For>
    }
}

/// The kinds of section the Add section menu offers (spec section 12.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewSection {
    Experience,
    Education,
    Projects,
    Skills,
    Languages,
    Certifications,
    Custom,
}

impl NewSection {
    pub const ALL: [NewSection; 7] = [
        Self::Experience,
        Self::Education,
        Self::Projects,
        Self::Skills,
        Self::Languages,
        Self::Certifications,
        Self::Custom,
    ];

    fn label(self) -> &'static str {
        let s = t();
        match self {
            Self::Experience => s.work_experience,
            Self::Education => s.education,
            Self::Projects => s.projects,
            Self::Skills => s.skills,
            Self::Languages => s.languages,
            Self::Certifications => s.certifications,
            Self::Custom => s.custom_section,
        }
    }

    /// A new section of this kind with one empty item, from two fresh IDs.
    pub fn build(self, section: Id, item: Id) -> Section {
        let entries = |id| EntrySection {
            id,
            title_override: None,
            entries: vec![new_entry(item.clone())],
        };
        match self {
            Self::Experience => Section::Experience(entries(section)),
            Self::Education => Section::Education(entries(section)),
            Self::Projects => Section::Projects(entries(section)),
            Self::Certifications => Section::Certifications(entries(section)),
            Self::Custom => Section::Custom(CustomSection {
                id: section,
                title: String::new(),
                entries: vec![new_entry(item)],
            }),
            Self::Skills => Section::Skills(SkillsSection {
                id: section,
                title_override: None,
                groups: vec![new_group(item)],
            }),
            Self::Languages => Section::Languages(LanguagesSection {
                id: section,
                title_override: None,
                items: vec![new_language(item)],
            }),
        }
    }
}

/// The Add section menu: picking a kind appends that section.
#[component]
pub fn AddSection(edit: Edit) -> impl IntoView {
    let options = NewSection::ALL
        .iter()
        .enumerate()
        .map(|(i, k)| view! { <option value=i.to_string()>{k.label()}</option> })
        .collect_view();
    view! {
        <label class="add-section">{t().add_section}
            <select on:change=move |ev| {
                let el = event_target::<web_sys::HtmlSelectElement>(&ev);
                let picked = el.value().parse::<usize>().ok().and_then(|i| NewSection::ALL.get(i).copied());
                el.set_value("");
                let (Some(kind), Some(section), Some(item)) = (picked, new_id(), new_id()) else {
                    return;
                };
                edit.run(Box::new(move |c| c.sections.push(kind.build(section, item))));
            }>
                <option value="" selected>{t().add_section_choose}</option>
                {options}
            </select>
        </label>
    }
}

/// The button in a section's heading that hides or shows its contents.
pub fn collapse_toggle(collapsed: RwSignal<bool>) -> impl IntoView {
    view! {
        <button class="collapse-toggle" aria-expanded=move || (!collapsed.get()).to_string()
            aria-label=move || if collapsed.get() { t().expand_section } else { t().collapse_section }
            on:click=move |_| collapsed.update(|c| *c = !*c)>
            {move || if collapsed.get() { "▸" } else { "▾" }}
        </button>
    }
}

#[component]
fn SectionEditor(
    cv: RwSignal<Option<Cv>>,
    edit: Edit,
    id: Id,
    /// `None` for a custom section, whose title is edited in the heading.
    label: Option<&'static str>,
) -> impl IntoView {
    let sid = id.clone();
    let entries = move || {
        cv.with(|c| {
            c.as_ref()
                .and_then(|c| entries_ref(c, &sid))
                .map(|x| x.iter().map(|e| e.id.clone()).collect::<Vec<_>>())
                .unwrap_or_default()
        })
    };
    let collapsed = RwSignal::new(false);
    let heading = match label {
        Some(label) => label.into_any(),
        None => {
            let (read, write) = (id.clone(), id.clone());
            view! {
                <input class="custom-title" aria-label=t().custom_title placeholder=t().custom_title
                    prop:value=move || cv.with(|c| c.as_ref().and_then(|c| c.sections.iter().find_map(|s| match s {
                        Section::Custom(x) if x.id == read => Some(x.title.clone()),
                        _ => None,
                    })).unwrap_or_default())
                    on:input=move |ev| {
                        let v = event_target_value(&ev);
                        let id = write.clone();
                        edit.run(Box::new(move |c| {
                            for s in &mut c.sections {
                                if let Section::Custom(x) = s && x.id == id {
                                    x.title = v.clone();
                                }
                            }
                        }));
                    } />
            }
            .into_any()
        }
    };
    let add = {
        let id = id.clone();
        move |_| {
            let Some(entry_id) = new_id() else {
                return;
            };
            let id = id.clone();
            edit.run(Box::new(move |c| {
                if let Some(x) = entries_mut(c, &id) {
                    x.push(new_entry(entry_id));
                }
            }));
        }
    };
    let remove_section = {
        let id = id.clone();
        move |_| {
            let id = id.clone();
            edit.run(Box::new(move |c| c.sections.retain(|s| s.id() != &id)));
        }
    };
    let move_section = {
        let id = id.clone();
        move |up: bool| {
            let id = id.clone();
            edit.run(Box::new(move |c| {
                shift(&mut c.sections, |s| s.id() == &id, up);
            }));
        }
    };
    let section_up = {
        let m = move_section.clone();
        move |_| m(true)
    };
    let section_down = move |_| move_section(false);
    let section_id = id.clone();
    let drop_section = Callback::new(move |(a, b): (Id, Id)| {
        edit.run(Box::new(move |c| {
            move_to(&mut c.sections, |s| s.id() == &a, |s| s.id() == &b);
        }));
    });
    view! {
        <fieldset class="section" data-drag-list="sections" data-drag-id=id.to_string()>
            <legend>{collapse_toggle(collapsed)}{heading}</legend>
            <div class="order">
                <DragHandle list="sections".to_owned() id=id.clone() dropped=drop_section />
                <button aria-label=t().move_section_up on:click=section_up>"↑"</button>
                <button aria-label=t().move_section_down on:click=section_down>"↓"</button>
            </div>
            <div hidden=move || collapsed.get()>
                <For each=entries key=|e| e.clone() let:entry>
                    <EntryEditor cv=cv edit=edit section=section_id.clone() id=entry />
                </For>
                <button on:click=add>{t().add_entry}</button>
                " "
                <button on:click=remove_section>{t().remove_section}</button>
            </div>
        </fieldset>
    }
}

#[component]
fn EntryEditor(cv: RwSignal<Option<Cv>>, edit: Edit, section: Id, id: Id) -> impl IntoView {
    let text = {
        let (section, id) = (section.clone(), id.clone());
        move |label: &'static str, get: fn(&Entry) -> String, set: fn(&mut Entry, String)| {
            let (s1, e1, s2, e2) = (section.clone(), id.clone(), section.clone(), id.clone());
            let (s3, e3) = (section.clone(), id.clone());
            view! {
                <label>{label}
                    <input
                        prop:value=move || cv.with(|c| c.as_ref().and_then(|c| entry_ref(c, &s1, &e1)).map(get).unwrap_or_default())
                        on:input=move |ev| {
                            let v = event_target_value(&ev);
                            let (s, e) = (s2.clone(), e2.clone());
                            edit.run(Box::new(move |c| if let Some(x) = entry_mut(c, &s, &e) { set(x, v) }));
                        }
                        // Trim when the field loses focus, not while typing.
                        on:change=move |ev| {
                            let v = event_target_value(&ev);
                            if v.trim().len() != v.len() {
                                let t = v.trim().to_owned();
                                let (s, e) = (s3.clone(), e3.clone());
                                edit.run(Box::new(move |c| if let Some(x) = entry_mut(c, &s, &e) { set(x, t) }));
                            }
                        } />
                </label>
            }
        }
    };
    let bullets = {
        let (s, e) = (section.clone(), id.clone());
        move || {
            cv.with(|c| {
                c.as_ref()
                    .and_then(|c| entry_ref(c, &s, &e))
                    .map(|x| x.bullets.iter().map(|b| b.id.clone()).collect::<Vec<_>>())
                    .unwrap_or_default()
            })
        }
    };
    let add_bullet = {
        let (s, e) = (section.clone(), id.clone());
        move |_| {
            let Some(bullet_id) = new_id() else {
                return;
            };
            let (s, e) = (s.clone(), e.clone());
            edit.run(Box::new(move |c| {
                if let Some(x) = entry_mut(c, &s, &e) {
                    x.bullets.push(Bullet {
                        id: bullet_id,
                        text: String::new(),
                    });
                }
            }));
        }
    };
    let remove_entry = {
        let (s, e) = (section.clone(), id.clone());
        move |_| {
            let (s, e) = (s.clone(), e.clone());
            edit.run(Box::new(move |c| {
                if let Some(x) = entries_mut(c, &s) {
                    x.retain(|en| en.id != e);
                }
            }));
        }
    };
    let move_entry = {
        let (s, e) = (section.clone(), id.clone());
        move |up: bool| {
            let (s, e) = (s.clone(), e.clone());
            edit.run(Box::new(move |c| {
                if let Some(x) = entries_mut(c, &s) {
                    shift(x, |en| en.id == e, up);
                }
            }));
        }
    };
    let entry_up = {
        let m = move_entry.clone();
        move |_| m(true)
    };
    let entry_down = move |_| move_entry(false);
    // Dates keep what the user typed: the model only changes once the text
    // parses (or is cleared), so a half-typed "20" is not wiped out.
    let date = {
        let (section, id) = (section.clone(), id.clone());
        move |label: &'static str,
              get: fn(&Period) -> Option<PartialDate>,
              set: fn(&mut Period, Option<PartialDate>)| {
            let initial = cv.with_untracked(|c| {
                c.as_ref()
                    .and_then(|c| entry_ref(c, &section, &id))
                    .map(|e| dates::format(get(&e.period)))
                    .unwrap_or_default()
            });
            let (s, e) = (section.clone(), id.clone());
            view! {
                <label class="date">{label}
                    <input value=initial placeholder=t().date_placeholder
                        on:input=move |ev| {
                            let v = event_target_value(&ev);
                            let parsed = dates::parse(&v);
                            if parsed.is_some() || v.trim().is_empty() {
                                let (s, e) = (s.clone(), e.clone());
                                edit.run(Box::new(move |c| if let Some(x) = entry_mut(c, &s, &e) { set(&mut x.period, parsed) }));
                            }
                        } />
                </label>
            }
        }
    };
    let current = {
        let (s1, e1, s2, e2) = (section.clone(), id.clone(), section.clone(), id.clone());
        view! {
            <label class="check">
                <input type="checkbox"
                    prop:checked=move || cv.with(|c| c.as_ref().and_then(|c| entry_ref(c, &s1, &e1)).is_some_and(|e| e.period.current))
                    on:change=move |ev| {
                        let on = event_target_checked(&ev);
                        let (s, e) = (s2.clone(), e2.clone());
                        edit.run(Box::new(move |c| if let Some(x) = entry_mut(c, &s, &e) { x.period.current = on }));
                    } />
                " "{t().current_job}
            </label>
        }
    };
    let locked = {
        let (s1, e1, s2, e2) = (section.clone(), id.clone(), section.clone(), id.clone());
        view! {
            <label class="check" title=t().locked_hint>
                <input type="checkbox"
                    prop:checked=move || cv.with(|c| c.as_ref().and_then(|c| entry_ref(c, &s1, &e1)).is_some_and(|e| e.locked))
                    on:change=move |ev| {
                        let on = event_target_checked(&ev);
                        let (s, e) = (s2.clone(), e2.clone());
                        edit.run(Box::new(move |c| if let Some(x) = entry_mut(c, &s, &e) { x.locked = on }));
                    } />
                " "{t().locked}
                <span class="hint">{t().locked_hint}</span>
            </label>
        }
    };
    let description = {
        let (s1, e1, s2, e2) = (section.clone(), id.clone(), section.clone(), id.clone());
        view! {
            <label>{t().description}
                <textarea
                    prop:value=move || cv.with(|c| c.as_ref().and_then(|c| entry_ref(c, &s1, &e1)).and_then(|e| e.description.clone()).unwrap_or_default())
                    on:input=move |ev| {
                        let v = event_target_value(&ev);
                        let (s, e) = (s2.clone(), e2.clone());
                        edit.run(Box::new(move |c| if let Some(x) = entry_mut(c, &s, &e) { x.description = opt(v) }));
                    }></textarea>
            </label>
        }
    };
    let (bs, be) = (section.clone(), id.clone());
    let drop_entry = {
        let s = section.clone();
        Callback::new(move |(a, b): (Id, Id)| {
            let s = s.clone();
            edit.run(Box::new(move |c| {
                if let Some(x) = entries_mut(c, &s) {
                    move_to(x, |e| e.id == a, |e| e.id == b);
                }
            }));
        })
    };
    let entry_list = format!("entries-{section}");
    view! {
        <div class="entry" data-drag-list=entry_list.clone() data-drag-id=id.to_string()>
            <DragHandle list=entry_list.clone() id=id.clone() dropped=drop_entry />
            {text(t().entry_title, |e| e.title.clone(), |e, v| e.title = v)}
            {text(t().organisation, |e| e.org.clone(), |e, v| e.org = v)}
            {text(t().location, |e| e.location.clone().unwrap_or_default(), |e, v| e.location = opt(v))}
            <div class="dates">
                {date(t().start, |p| p.start, |p, d| p.start = d)}
                {date(t().end, |p| p.end, |p, d| p.end = d)}
            </div>
            {current}
            {locked}
            {description}
            {text(t().link, |e| e.url.clone().unwrap_or_default(), |e, v| e.url = opt(v))}
            <For each=bullets key=|b| b.clone() let:bullet>
                <BulletEditor cv=cv edit=edit section=bs.clone() entry=be.clone() id=bullet />
            </For>
            <button on:click=add_bullet>{t().add_bullet}</button>
            " "
            <button on:click=remove_entry>{t().remove_entry}</button>
            " "
            <button aria-label=t().move_entry_up on:click=entry_up>"↑"</button>
            <button aria-label=t().move_entry_down on:click=entry_down>"↓"</button>
        </div>
    }
}

#[component]
fn BulletEditor(
    cv: RwSignal<Option<Cv>>,
    edit: Edit,
    section: Id,
    entry: Id,
    id: Id,
) -> impl IntoView {
    let (s1, e1, b1) = (section.clone(), entry.clone(), id.clone());
    let (s2, e2, b2) = (section.clone(), entry.clone(), id.clone());
    let (s3, e3, b3) = (section.clone(), entry.clone(), id.clone());
    let value = move || {
        cv.with(|c| {
            c.as_ref()
                .and_then(|c| entry_ref(c, &s1, &e1))
                .and_then(|x| x.bullets.iter().find(|b| b.id == b1))
                .map(|b| b.text.clone())
                .unwrap_or_default()
        })
    };
    let move_bullet = {
        let (s, e, b) = (section.clone(), entry.clone(), id.clone());
        move |up: bool| {
            let (s, e, b) = (s.clone(), e.clone(), b.clone());
            edit.run(Box::new(move |c| {
                if let Some(x) = entry_mut(c, &s, &e) {
                    shift(&mut x.bullets, |bl| bl.id == b, up);
                }
            }));
        }
    };
    let bullet_up = {
        let m = move_bullet.clone();
        move |_| m(true)
    };
    let bullet_down = move |_| move_bullet(false);
    let remove = move |_| {
        let (s, e, b) = (section.clone(), entry.clone(), id.clone());
        edit.run(Box::new(move |c| {
            if let Some(x) = entry_mut(c, &s, &e) {
                x.bullets.retain(|bl| bl.id != b);
            }
        }));
    };
    let drop_bullet = {
        let (s, e) = (s3.clone(), e3.clone());
        Callback::new(move |(a, b): (Id, Id)| {
            let (s, e) = (s.clone(), e.clone());
            edit.run(Box::new(move |c| {
                if let Some(x) = entry_mut(c, &s, &e) {
                    move_to(&mut x.bullets, |bl| bl.id == a, |bl| bl.id == b);
                }
            }));
        })
    };
    let bullet_list = format!("bullets-{e3}");
    view! {
        <div class="bullet" data-drag-list=bullet_list.clone() data-drag-id=b3.to_string()>
            <DragHandle list=bullet_list.clone() id=b3.clone() dropped=drop_bullet />
            <textarea aria-label=t().bullet prop:value=value
                on:input=move |ev| {
                    let v = event_target_value(&ev);
                    let (s, e, b) = (s2.clone(), e2.clone(), b2.clone());
                    edit.run(Box::new(move |c| {
                        if let Some(bl) = entry_mut(c, &s, &e).and_then(|x| x.bullets.iter_mut().find(|bl| bl.id == b)) {
                            bl.text = v;
                        }
                    }));
                }></textarea>
            <button aria-label=t().move_bullet_up on:click=bullet_up>"↑"</button>
            <button aria-label=t().move_bullet_down on:click=bullet_down>"↓"</button>
            <button aria-label=t().remove_bullet on:click=remove>"×"</button>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> Id {
        Id::parse(&format!("01HZX3M8Q4V7K2N9P5R6T0W1{n:02}")).expect("id")
    }

    #[test]
    fn every_menu_kind_builds_its_section_with_one_item() {
        for kind in NewSection::ALL {
            let s = kind.build(id(1), id(2));
            assert_eq!(s.id(), &id(1));
            let items = match (&s, kind) {
                (Section::Experience(x), NewSection::Experience)
                | (Section::Education(x), NewSection::Education)
                | (Section::Projects(x), NewSection::Projects)
                | (Section::Certifications(x), NewSection::Certifications) => {
                    x.entries.iter().map(|e| e.id.clone()).collect::<Vec<_>>()
                }
                (Section::Custom(x), NewSection::Custom) => {
                    x.entries.iter().map(|e| e.id.clone()).collect()
                }
                (Section::Skills(x), NewSection::Skills) => {
                    x.groups.iter().map(|g| g.id.clone()).collect()
                }
                (Section::Languages(x), NewSection::Languages) => {
                    x.items.iter().map(|i| i.id.clone()).collect()
                }
                _ => panic!("{kind:?} built the wrong section"),
            };
            assert_eq!(items, [id(2)]);
        }
    }

    #[test]
    fn custom_sections_are_listed_and_edited_like_entry_sections() {
        let mut cv: Cv =
            serde_json::from_str(include_str!("../../testdata/cvs/typical.json")).expect("cv");
        cv.sections.push(NewSection::Custom.build(id(1), id(2)));
        assert_eq!(
            section_forms(&cv).last(),
            Some(&(id(1), SectionForm::Entries(None)))
        );
        entry_mut(&mut cv, &id(1), &id(2)).expect("entry").title = "Talk".into();
        assert_eq!(
            entry_ref(&cv, &id(1), &id(2)).map(|e| e.title.as_str()),
            Some("Talk")
        );
    }

    #[test]
    fn the_form_follows_section_order_across_kinds() {
        let mut cv: Cv =
            serde_json::from_str(include_str!("../../testdata/cvs/typical.json")).expect("cv");
        cv.sections.push(NewSection::Custom.build(id(1), id(2)));
        cv.sections.push(NewSection::Skills.build(id(3), id(4)));
        let forms = section_forms(&cv);
        let ids: Vec<&Id> = forms.iter().map(|(id, _)| id).collect();
        let order: Vec<&Id> = cv.sections.iter().map(Section::id).collect();
        assert_eq!(ids, order);
        assert_eq!(forms[forms.len() - 1].1, SectionForm::Skills);
        cv.sections.rotate_right(1);
        assert_eq!(section_forms(&cv)[0].1, SectionForm::Skills);
    }
}
