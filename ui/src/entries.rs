//! Editing entry sections (experience, education, projects, certifications):
//! add, remove and reorder sections, entries and bullets, and edit their text.
//! Lists are keyed by ID, so typing never rebuilds the input in focus.

use cv_core::{Bullet, Cv, Entry, EntrySection, Id, PartialDate, Period, Section};
use leptos::prelude::*;

use crate::dates;
use crate::drag::{DragHandle, move_to};
use crate::i18n::t;
use crate::ids::new_id;
use crate::order::shift;

/// Applies an edit to the CV and schedules preview and autosave.
pub type Edit = Callback<Box<dyn FnOnce(&mut Cv)>>;

fn entry_section_mut<'a>(cv: &'a mut Cv, id: &Id) -> Option<&'a mut EntrySection> {
    cv.sections.iter_mut().find_map(|s| match s {
        Section::Experience(x)
        | Section::Education(x)
        | Section::Projects(x)
        | Section::Certifications(x)
            if &x.id == id =>
        {
            Some(x)
        }
        _ => None,
    })
}

fn entry_mut<'a>(cv: &'a mut Cv, section: &Id, entry: &Id) -> Option<&'a mut Entry> {
    entry_section_mut(cv, section)?
        .entries
        .iter_mut()
        .find(|e| &e.id == entry)
}

fn entry_ref<'a>(cv: &'a Cv, section: &Id, entry: &Id) -> Option<&'a Entry> {
    cv.sections.iter().find_map(|s| match s {
        Section::Experience(x)
        | Section::Education(x)
        | Section::Projects(x)
        | Section::Certifications(x)
            if &x.id == section =>
        {
            x.entries.iter().find(|e| &e.id == entry)
        }
        _ => None,
    })
}

/// IDs and kind labels of the entry sections, in order.
fn entry_sections(cv: &Cv) -> Vec<(Id, &'static str)> {
    cv.sections
        .iter()
        .filter_map(|s| match s {
            Section::Experience(x) => Some((x.id.clone(), t().work_experience)),
            Section::Education(x) => Some((x.id.clone(), t().education)),
            Section::Projects(x) => Some((x.id.clone(), t().projects)),
            Section::Certifications(x) => Some((x.id.clone(), t().certifications)),
            _ => None,
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

#[component]
pub fn EntrySections(cv: RwSignal<Option<Cv>>, edit: Edit) -> impl IntoView {
    let add_section = move |make: fn(EntrySection) -> Section| {
        let (Some(section_id), Some(entry_id)) = (new_id(), new_id()) else {
            return;
        };
        edit.run(Box::new(move |c| {
            c.sections.push(make(EntrySection {
                id: section_id,
                title_override: None,
                entries: vec![new_entry(entry_id)],
            }));
        }));
    };
    view! {
        <For
            each=move || cv.with(|c| c.as_ref().map(entry_sections).unwrap_or_default())
            key=|(id, _)| id.clone()
            let:section
        >
            <SectionEditor cv=cv edit=edit id=section.0 label=section.1 />
        </For>
        <p>
            <button on:click=move |_| add_section(Section::Experience)>{t().add_work_experience}</button>
            " "
            <button on:click=move |_| add_section(Section::Education)>{t().add_education}</button>
            " "
            <button on:click=move |_| add_section(Section::Projects)>{t().add_projects}</button>
        </p>
    }
}

#[component]
fn SectionEditor(
    cv: RwSignal<Option<Cv>>,
    edit: Edit,
    id: Id,
    label: &'static str,
) -> impl IntoView {
    let sid = id.clone();
    let entries = move || {
        cv.with(|c| {
            c.as_ref()
                .and_then(|c| {
                    c.sections.iter().find_map(|s| match s {
                        Section::Experience(x)
                        | Section::Education(x)
                        | Section::Projects(x)
                        | Section::Certifications(x)
                            if x.id == sid =>
                        {
                            Some(x.entries.iter().map(|e| e.id.clone()).collect::<Vec<_>>())
                        }
                        _ => None,
                    })
                })
                .unwrap_or_default()
        })
    };
    let add = {
        let id = id.clone();
        move |_| {
            let Some(entry_id) = new_id() else {
                return;
            };
            let id = id.clone();
            edit.run(Box::new(move |c| {
                if let Some(s) = entry_section_mut(c, &id) {
                    s.entries.push(new_entry(entry_id));
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
            <legend>{label}</legend>
            <div class="order">
                <DragHandle list="sections".to_owned() id=id.clone() dropped=drop_section />
                <button aria-label=t().move_section_up on:click=section_up>"↑"</button>
                <button aria-label=t().move_section_down on:click=section_down>"↓"</button>
            </div>
            <For each=entries key=|e| e.clone() let:entry>
                <EntryEditor cv=cv edit=edit section=section_id.clone() id=entry />
            </For>
            <button on:click=add>{t().add_entry}</button>
            " "
            <button on:click=remove_section>{t().remove_section}</button>
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
                if let Some(x) = entry_section_mut(c, &s) {
                    x.entries.retain(|en| en.id != e);
                }
            }));
        }
    };
    let move_entry = {
        let (s, e) = (section.clone(), id.clone());
        move |up: bool| {
            let (s, e) = (s.clone(), e.clone());
            edit.run(Box::new(move |c| {
                if let Some(x) = entry_section_mut(c, &s) {
                    shift(&mut x.entries, |en| en.id == e, up);
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
                if let Some(x) = entry_section_mut(c, &s) {
                    move_to(&mut x.entries, |e| e.id == a, |e| e.id == b);
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
