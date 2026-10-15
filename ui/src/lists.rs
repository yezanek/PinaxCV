//! Editing skills and languages sections. Skills in a group are typed as one
//! comma-separated line; languages are rows of name and level.

use cv_core::{Cv, Id, LanguageItem, LanguagesSection, Section, Skill, SkillGroup, SkillsSection};
use leptos::prelude::*;

use crate::entries::Edit;
use crate::ids::new_id;
use crate::order::shift;

/// Turns a comma-separated line into skills. Positions keep their existing
/// IDs, so editing a name keeps variant references to it; extra names get
/// fresh IDs and empty names are dropped.
pub fn merge_skills(
    existing: &[Skill],
    line: &str,
    mut fresh: impl FnMut() -> Option<Id>,
) -> Option<Vec<Skill>> {
    let mut old = existing.iter();
    line.split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(|name| {
            let id = match old.next() {
                Some(s) => s.id.clone(),
                None => fresh()?,
            };
            Some(Skill {
                id,
                name: name.to_owned(),
            })
        })
        .collect()
}

fn skills_mut<'a>(cv: &'a mut Cv, id: &Id) -> Option<&'a mut SkillsSection> {
    cv.sections.iter_mut().find_map(|s| match s {
        Section::Skills(x) if &x.id == id => Some(x),
        _ => None,
    })
}

fn languages_mut<'a>(cv: &'a mut Cv, id: &Id) -> Option<&'a mut LanguagesSection> {
    cv.sections.iter_mut().find_map(|s| match s {
        Section::Languages(x) if &x.id == id => Some(x),
        _ => None,
    })
}

fn group_mut<'a>(cv: &'a mut Cv, section: &Id, group: &Id) -> Option<&'a mut SkillGroup> {
    skills_mut(cv, section)?
        .groups
        .iter_mut()
        .find(|g| &g.id == group)
}

fn language_mut<'a>(cv: &'a mut Cv, section: &Id, item: &Id) -> Option<&'a mut LanguageItem> {
    languages_mut(cv, section)?
        .items
        .iter_mut()
        .find(|i| &i.id == item)
}

fn new_group(id: Id) -> SkillGroup {
    SkillGroup {
        id,
        label: None,
        skills: Vec::new(),
    }
}

fn new_language(id: Id) -> LanguageItem {
    LanguageItem {
        id,
        name: String::new(),
        level: None,
    }
}

/// A blank field becomes `None`; other text is kept as typed, so the input
/// is not rewritten mid-word. Fields trim on `change` instead.
fn opt(s: String) -> Option<String> {
    (!s.trim().is_empty()).then_some(s)
}

/// The trimmed text, when trimming changes it (for `on:change` handlers).
fn needs_trim(v: &str) -> Option<String> {
    (v.trim().len() != v.len()).then(|| v.trim().to_owned())
}

/// The skills and languages sections, in CV order, with buttons to add them.
#[component]
pub fn ListSections(cv: RwSignal<Option<Cv>>, edit: Edit) -> impl IntoView {
    let sections = move || {
        cv.with(|c| {
            c.as_ref()
                .map(|c| {
                    c.sections
                        .iter()
                        .filter_map(|s| match s {
                            Section::Skills(x) => Some((x.id.clone(), true)),
                            Section::Languages(x) => Some((x.id.clone(), false)),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
    };
    let add_skills = move |_| {
        let (Some(section_id), Some(group_id)) = (new_id(), new_id()) else {
            return;
        };
        edit.run(Box::new(move |c| {
            c.sections.push(Section::Skills(SkillsSection {
                id: section_id,
                title_override: None,
                groups: vec![new_group(group_id)],
            }))
        }))
    };
    let add_languages = move |_| {
        let (Some(section_id), Some(item_id)) = (new_id(), new_id()) else {
            return;
        };
        edit.run(Box::new(move |c| {
            c.sections.push(Section::Languages(LanguagesSection {
                id: section_id,
                title_override: None,
                items: vec![new_language(item_id)],
            }))
        }))
    };
    view! {
        <For each=sections key=|(id, _)| id.clone() let:section>
            {if section.1 {
                view! { <SkillsEditor cv=cv edit=edit id=section.0 /> }.into_any()
            } else {
                view! { <LanguagesEditor cv=cv edit=edit id=section.0 /> }.into_any()
            }}
        </For>
        <p>
            <button on:click=add_skills>"+ Skills"</button>
            " "
            <button on:click=add_languages>"+ Languages"</button>
        </p>
    }
}

/// Up, down and remove buttons for a whole section.
fn section_buttons(edit: Edit, id: Id) -> impl IntoView {
    let mv = {
        let id = id.clone();
        move |up: bool| {
            let id = id.clone();
            edit.run(Box::new(move |c| {
                shift(&mut c.sections, |s| s.id() == &id, up);
            }))
        }
    };
    let up = {
        let mv = mv.clone();
        move |_| mv(true)
    };
    let down = move |_| mv(false);
    let remove = move |_| {
        let id = id.clone();
        edit.run(Box::new(move |c| c.sections.retain(|s| s.id() != &id)))
    };
    view! {
        <div class="order">
            <button aria-label="Move section up" on:click=up>"↑"</button>
            <button aria-label="Move section down" on:click=down>"↓"</button>
            <button aria-label="Remove section" on:click=remove>"×"</button>
        </div>
    }
}

#[component]
fn SkillsEditor(cv: RwSignal<Option<Cv>>, edit: Edit, id: Id) -> impl IntoView {
    let sid = id.clone();
    let groups = move || {
        cv.with(|c| {
            c.as_ref()
                .and_then(|c| {
                    c.sections.iter().find_map(|s| match s {
                        Section::Skills(x) if x.id == sid => {
                            Some(x.groups.iter().map(|g| g.id.clone()).collect::<Vec<_>>())
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
            let Some(new) = new_id() else {
                return;
            };
            let id = id.clone();
            edit.run(Box::new(move |c| {
                if let Some(s) = skills_mut(c, &id) {
                    s.groups.push(new_group(new));
                }
            }))
        }
    };
    let section = id.clone();
    view! {
        <fieldset class="section">
            <legend>"Skills"</legend>
            {section_buttons(edit, id)}
            <For each=groups key=|g| g.clone() let:group>
                <GroupEditor cv=cv edit=edit section=section.clone() id=group />
            </For>
            <button on:click=add>"+ Skill group"</button>
        </fieldset>
    }
}

#[component]
fn GroupEditor(cv: RwSignal<Option<Cv>>, edit: Edit, section: Id, id: Id) -> impl IntoView {
    // Both fields start from the model and are not rewritten while typing,
    // so "Rust, " keeps its trailing comma until the next name is typed.
    let (label, line) = cv.with_untracked(|c| {
        let g = c.as_ref().and_then(|c| {
            c.sections.iter().find_map(|s| match s {
                Section::Skills(x) if x.id == section => x.groups.iter().find(|g| g.id == id),
                _ => None,
            })
        });
        (
            g.and_then(|g| g.label.clone()).unwrap_or_default(),
            g.map(|g| {
                g.skills
                    .iter()
                    .map(|k| k.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default(),
        )
    });
    let (s1, g1, s2, g2, s3, g3, s4, g4) = (
        section.clone(),
        id.clone(),
        section.clone(),
        id.clone(),
        section.clone(),
        id.clone(),
        section.clone(),
        id.clone(),
    );
    let move_group = {
        let (s, g) = (section.clone(), id.clone());
        move |up: bool| {
            let (s, g) = (s.clone(), g.clone());
            edit.run(Box::new(move |c| {
                if let Some(x) = skills_mut(c, &s) {
                    shift(&mut x.groups, |gr| gr.id == g, up);
                }
            }))
        }
    };
    let group_up = {
        let m = move_group.clone();
        move |_| m(true)
    };
    let group_down = move |_| move_group(false);
    view! {
        <div class="entry">
            <label>"Group label (optional)"
                <input value=label on:input=move |ev| {
                    let v = event_target_value(&ev);
                    let (s, g) = (s1.clone(), g1.clone());
                    edit.run(Box::new(move |c| if let Some(x) = group_mut(c, &s, &g) { x.label = opt(v) }));
                }
                // Trim when the field loses focus, not while typing.
                on:change=move |ev| {
                    let Some(t) = needs_trim(&event_target_value(&ev)) else { return };
                    let (s, g) = (s4.clone(), g4.clone());
                    edit.run(Box::new(move |c| if let Some(x) = group_mut(c, &s, &g) { x.label = opt(t) }));
                } />
            </label>
            <label>"Skills, separated by commas"
                <input value=line on:input=move |ev| {
                    let v = event_target_value(&ev);
                    let (s, g) = (s2.clone(), g2.clone());
                    edit.run(Box::new(move |c| if let Some(x) = group_mut(c, &s, &g)
                        && let Some(merged) = merge_skills(&x.skills, &v, new_id)
                    {
                        x.skills = merged;
                    }));
                } />
            </label>
            <button on:click=move |_| {
                let (s, g) = (s3.clone(), g3.clone());
                edit.run(Box::new(move |c| if let Some(x) = skills_mut(c, &s) { x.groups.retain(|gr| gr.id != g) }));
            }>"Remove group"</button>
            " "
            <button aria-label="Move skill group up" on:click=group_up>"↑"</button>
            <button aria-label="Move skill group down" on:click=group_down>"↓"</button>
        </div>
    }
}

#[component]
fn LanguagesEditor(cv: RwSignal<Option<Cv>>, edit: Edit, id: Id) -> impl IntoView {
    let sid = id.clone();
    let items = move || {
        cv.with(|c| {
            c.as_ref()
                .and_then(|c| {
                    c.sections.iter().find_map(|s| match s {
                        Section::Languages(x) if x.id == sid => {
                            Some(x.items.iter().map(|i| i.id.clone()).collect::<Vec<_>>())
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
            let Some(new) = new_id() else {
                return;
            };
            let id = id.clone();
            edit.run(Box::new(move |c| {
                if let Some(s) = languages_mut(c, &id) {
                    s.items.push(new_language(new));
                }
            }))
        }
    };
    let section = id.clone();
    view! {
        <fieldset class="section">
            <legend>"Languages"</legend>
            {section_buttons(edit, id)}
            <For each=items key=|i| i.clone() let:item>
                <LanguageEditor cv=cv edit=edit section=section.clone() id=item />
            </For>
            <button on:click=add>"+ Language"</button>
        </fieldset>
    }
}

#[component]
fn LanguageEditor(cv: RwSignal<Option<Cv>>, edit: Edit, section: Id, id: Id) -> impl IntoView {
    let read = {
        let (s, i) = (section.clone(), id.clone());
        move |get: fn(&LanguageItem) -> String| {
            let (s, i) = (s.clone(), i.clone());
            move || {
                cv.with(|c| {
                    c.as_ref()
                        .and_then(|c| {
                            c.sections.iter().find_map(|x| match x {
                                Section::Languages(x) if x.id == s => {
                                    x.items.iter().find(|it| it.id == i)
                                }
                                _ => None,
                            })
                        })
                        .map(get)
                        .unwrap_or_default()
                })
            }
        }
    };
    let write = {
        let (s, i) = (section.clone(), id.clone());
        move |set: fn(&mut LanguageItem, String)| {
            let (s, i) = (s.clone(), i.clone());
            move |ev| {
                let v = event_target_value(&ev);
                let (s, i) = (s.clone(), i.clone());
                edit.run(Box::new(move |c| {
                    if let Some(x) = language_mut(c, &s, &i) {
                        set(x, v)
                    }
                }))
            }
        }
    };
    // Trim when a field loses focus, not while typing.
    let tidy = {
        let (s, i) = (section.clone(), id.clone());
        move |set: fn(&mut LanguageItem, String)| {
            let (s, i) = (s.clone(), i.clone());
            move |ev| {
                let Some(v) = needs_trim(&event_target_value(&ev)) else {
                    return;
                };
                let (s, i) = (s.clone(), i.clone());
                edit.run(Box::new(move |c| {
                    if let Some(x) = language_mut(c, &s, &i) {
                        set(x, v)
                    }
                }))
            }
        }
    };
    let move_item = {
        let (s, i) = (section.clone(), id.clone());
        move |up: bool| {
            let (s, i) = (s.clone(), i.clone());
            edit.run(Box::new(move |c| {
                if let Some(x) = languages_mut(c, &s) {
                    shift(&mut x.items, |it| it.id == i, up);
                }
            }))
        }
    };
    let item_up = {
        let m = move_item.clone();
        move |_| m(true)
    };
    let item_down = move |_| move_item(false);
    let remove = move |_| {
        let (s, i) = (section.clone(), id.clone());
        edit.run(Box::new(move |c| {
            if let Some(x) = languages_mut(c, &s) {
                x.items.retain(|it| it.id != i)
            }
        }))
    };
    view! {
        <div class="language">
            <input aria-label="Language" placeholder="Language"
                prop:value=read(|l| l.name.clone()) on:input=write(|l, v| l.name = v)
                on:change=tidy(|l, v| l.name = v) />
            <input aria-label="Level" placeholder="Level, for example C1"
                prop:value=read(|l| l.level.clone().unwrap_or_default()) on:input=write(|l, v| l.level = opt(v))
                on:change=tidy(|l, v| l.level = opt(v)) />
            <button aria-label="Move language up" on:click=item_up>"↑"</button>
            <button aria-label="Move language down" on:click=item_down>"↓"</button>
            <button aria-label="Remove language" on:click=remove>"×"</button>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> Id {
        Id::parse(&format!("01HZX3M8Q4V7K2N9P5R6T0W1{:02}", n)).expect("id")
    }

    #[test]
    fn opt_keeps_spaces_and_blanks_become_none() {
        assert_eq!(opt("Native ".into()), Some("Native ".into()));
        assert_eq!(opt("  ".into()), None);
    }

    #[test]
    fn needs_trim_only_when_trimming_changes_text() {
        assert_eq!(needs_trim(" B2 "), Some("B2".into()));
        assert_eq!(needs_trim("Native speaker"), None);
    }

    #[test]
    fn merge_keeps_ids_by_position_and_drops_empty_names() {
        let existing = vec![
            Skill {
                id: id(1),
                name: "Rust".into(),
            },
            Skill {
                id: id(2),
                name: "Go".into(),
            },
        ];
        let mut n = 10;
        let mut fresh = || {
            n += 1;
            Some(id(n))
        };
        let merged = merge_skills(&existing, " Rust , Golang,, SQL, ", &mut fresh)
            .expect("fresh IDs available");
        let names: Vec<_> = merged.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Rust", "Golang", "SQL"]);
        assert_eq!(merged[0].id, id(1));
        assert_eq!(merged[1].id, id(2));
        assert_eq!(merged[2].id, id(11));
        assert!(
            merge_skills(&existing, " , ", &mut fresh)
                .expect("fresh IDs available")
                .is_empty()
        );
        // No new skill is made without a fresh ID.
        assert!(merge_skills(&existing, "Rust, Go, SQL", || None).is_none());
        assert_eq!(
            merge_skills(&existing, "Rust", || None)
                .expect("fresh IDs available")
                .len(),
            1
        );
    }
}
