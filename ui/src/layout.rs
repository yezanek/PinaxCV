//! The layout panel of the Style tab (ADR-084): one or two columns, the
//! sidebar's width, and a page canvas whose block cards are dragged within
//! and between the columns. Every card also has Move up, Move down and
//! Move to the other column buttons (spec section 12.9).
//!
//! The canvas edits the base CV: columns and placement live on it, and a
//! section moved within a column is reordered in `cv.sections`, the same
//! order the Content tab shows.

use cv_core::{BlockRef, Column, Columns, Cv, Id, Layout, Section, TopBlock};
use leptos::prelude::*;

use crate::drag::{DropSpot, ZoneDragHandle, move_before};
use crate::entries::Edit;
use crate::i18n::{fill, t};
use crate::style::show_number;
use crate::variants::section_label;

/// A block on the canvas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    Top(TopBlock),
    Section(Id),
}

impl Key {
    /// The `data-drop-key` text.
    pub fn as_text(&self) -> String {
        match self {
            Key::Top(TopBlock::Name) => "top:name".into(),
            Key::Top(TopBlock::Contact) => "top:contact".into(),
            Key::Top(TopBlock::Summary) => "top:summary".into(),
            Key::Section(id) => format!("section:{id}"),
        }
    }

    pub fn parse(text: &str) -> Option<Key> {
        match text {
            "top:name" => Some(Key::Top(TopBlock::Name)),
            "top:contact" => Some(Key::Top(TopBlock::Contact)),
            "top:summary" => Some(Key::Top(TopBlock::Summary)),
            _ => Id::parse(text.strip_prefix("section:")?)
                .ok()
                .map(Key::Section),
        }
    }

    fn block(&self) -> BlockRef {
        match self {
            Key::Top(b) => BlockRef::Top { block: *b },
            Key::Section(id) => BlockRef::Section { id: id.clone() },
        }
    }
}

/// The `data-drop-zone` text of a column.
fn zone_name(column: Column) -> &'static str {
    match column {
        Column::Sidebar => "sidebar",
        Column::Main => "main",
    }
}

fn zone_column(zone: &str) -> Option<Column> {
    match zone {
        "sidebar" => Some(Column::Sidebar),
        "main" => Some(Column::Main),
        _ => None,
    }
}

/// The columns in the order they are drawn: the name's column on the left.
/// One column shows only Main.
pub fn visual_columns(layout: &Layout) -> Vec<Column> {
    match (layout.columns, layout.first_column()) {
        (Columns::One, _) => vec![Column::Main],
        (Columns::Two, Column::Sidebar) => vec![Column::Sidebar, Column::Main],
        (Columns::Two, Column::Main) => vec![Column::Main, Column::Sidebar],
    }
}

/// The blocks drawn in `column`, in order: its top blocks, then its
/// sections in CV order.
pub fn blocks_in(cv: &Cv, column: Column) -> Vec<Key> {
    let mut keys: Vec<Key> = cv.layout.top_in(column).into_iter().map(Key::Top).collect();
    keys.extend(
        cv.sections
            .iter()
            .filter(|s| section_column(&cv.layout, s.id()) == column)
            .map(|s| Key::Section(s.id().clone())),
    );
    keys
}

fn section_column(layout: &Layout, id: &Id) -> Column {
    layout.column_of(&BlockRef::Section { id: id.clone() })
}

/// Moves `key` into `column`, just before `before` or at the end of the
/// column. Top blocks stay above sections: a section dropped among them
/// lands first among the column's sections. Returns whether anything
/// changed. With one column only the order changes.
pub fn place(cv: &mut Cv, key: &Key, column: Column, before: Option<&Key>) -> bool {
    let mut changed = false;
    let mut crossed = false;
    if cv.layout.columns == Columns::Two {
        crossed = cv.layout.set_column(key.block(), column);
        changed |= crossed;
    }
    match key {
        Key::Top(block) => {
            let old = cv.layout.top.clone();
            // The name and the contact details change column together
            // (`Layout::set_column`), so they also move as a pair.
            let pair = [TopBlock::Name, TopBlock::Contact];
            let moving: Vec<TopBlock> = if crossed && pair.contains(block) {
                old.iter().copied().filter(|b| pair.contains(b)).collect()
            } else {
                vec![*block]
            };
            let mut top: Vec<TopBlock> = old
                .iter()
                .copied()
                .filter(|b| !moving.contains(b))
                .collect();
            let at = match before {
                Some(Key::Top(t)) if !moving.contains(t) => top.iter().position(|b| b == t),
                _ => None,
            }
            .unwrap_or_else(|| {
                // After the column's last top block, or last overall.
                top.iter()
                    .rposition(|b| cv.layout.column_of(&BlockRef::Top { block: *b }) == column)
                    .map_or(top.len(), |i| i + 1)
            });
            top.splice(at..at, moving);
            changed |= top != old;
            cv.layout.top = top;
        }
        Key::Section(id) => {
            let Some(from) = cv.sections.iter().position(|s| s.id() == id) else {
                return changed;
            };
            let in_column: Vec<usize> = cv
                .sections
                .iter()
                .enumerate()
                .filter(|(i, s)| *i != from && section_column(&cv.layout, s.id()) == column)
                .map(|(i, _)| i)
                .collect();
            let target = match before {
                Some(Key::Section(x)) => cv.sections.iter().position(|s| s.id() == x),
                Some(Key::Top(_)) => in_column.first().copied(),
                None => in_column.last().map(|i| i + 1),
            };
            if let Some(target) = target {
                changed |= move_before(&mut cv.sections, from, Some(target));
            }
        }
    }
    changed
}

/// Swaps `key` with its neighbour above (`up`) or below in its column.
/// Returns false when it is already at that end.
pub fn shift(cv: &mut Cv, key: &Key, up: bool) -> bool {
    let column = cv.layout.column_of(&key.block());
    match key {
        Key::Top(block) => {
            let mine = cv.layout.top_in(column);
            let Some(i) = mine.iter().position(|b| b == block) else {
                return false;
            };
            let j = if up { i.checked_sub(1) } else { Some(i + 1) };
            let Some(other) = j.and_then(|j| mine.get(j)) else {
                return false;
            };
            let a = cv.layout.top.iter().position(|b| b == block);
            let b = cv.layout.top.iter().position(|b| b == other);
            match (a, b) {
                (Some(a), Some(b)) => {
                    cv.layout.top.swap(a, b);
                    true
                }
                _ => false,
            }
        }
        Key::Section(id) => {
            let mine: Vec<usize> = cv
                .sections
                .iter()
                .enumerate()
                .filter(|(_, s)| section_column(&cv.layout, s.id()) == column)
                .map(|(i, _)| i)
                .collect();
            let Some(i) = mine.iter().position(|&k| cv.sections[k].id() == id) else {
                return false;
            };
            let j = if up { i.checked_sub(1) } else { Some(i + 1) };
            match j.and_then(|j| mine.get(j)) {
                Some(&other) => {
                    cv.sections.swap(mine[i], other);
                    true
                }
                None => false,
            }
        }
    }
}

/// Moves `key` to the end of the other column. Only with two columns.
pub fn switch_column(cv: &mut Cv, key: &Key) -> bool {
    if cv.layout.columns != Columns::Two {
        return false;
    }
    let other = match cv.layout.column_of(&key.block()) {
        Column::Sidebar => Column::Main,
        Column::Main => Column::Sidebar,
    };
    place(cv, key, other, None)
}

/// The card text for `key`.
fn card_label(cv: &Cv, key: &Key) -> String {
    let s = t();
    match key {
        Key::Top(TopBlock::Name) if cv.basics.show_photo => s.layout_name_photo.into(),
        Key::Top(TopBlock::Name) => s.layout_name.into(),
        Key::Top(TopBlock::Contact) => s.layout_contact.into(),
        Key::Top(TopBlock::Summary) => s.summary.into(),
        Key::Section(id) => {
            let Some(section) = cv.sections.iter().find(|x| x.id() == id) else {
                return String::new();
            };
            let label = section_label(section);
            let label = if label.trim().is_empty() {
                s.untitled_section.to_owned()
            } else {
                label
            };
            let count = match section {
                Section::Experience(x)
                | Section::Education(x)
                | Section::Projects(x)
                | Section::Certifications(x) => Some(x.entries.len()),
                Section::Custom(x) => Some(x.entries.len()),
                Section::Skills(_) | Section::Languages(_) => None,
            };
            match count {
                Some(n) => fill(
                    s.layout_entries,
                    &[("label", &label), ("count", &n.to_string())],
                ),
                None => label,
            }
        }
    }
}

#[component]
pub fn LayoutPanel(cv: RwSignal<Option<Cv>>, edit: Edit) -> impl IntoView {
    let s = t();
    let layout = move || cv.with(|c| c.as_ref().map(|c| c.layout.clone()).unwrap_or_default());
    let two = move || layout().columns == Columns::Two;
    let set_columns = move |columns: Columns| {
        edit.run(Box::new(move |c| c.layout.columns = columns));
    };
    let dropped = Callback::new(move |(dragged, spot): (String, DropSpot)| {
        let (Some(key), Some(column)) = (Key::parse(&dragged), zone_column(&spot.zone)) else {
            return;
        };
        let before = spot.before.as_deref().and_then(Key::parse);
        edit.run(Box::new(move |c| {
            place(c, &key, column, before.as_ref());
        }));
    });
    let (min, max) = Layout::SIDEBAR_WIDTH_PCT;

    let card = move |key: Key| {
        let (k1, k2, k3) = (key.clone(), key.clone(), key.clone());
        let text = key.as_text();
        let drop_key = text.clone();
        let label =
            move || cv.with(|c| c.as_ref().map(|c| card_label(c, &key)).unwrap_or_default());
        let (l1, l2, l3) = (label.clone(), label.clone(), label.clone());
        view! {
            <li class="layout-card" data-drop-key=drop_key>
                <ZoneDragHandle group="layout".to_owned() key=text dropped=dropped />
                <span class="layout-label">{label}</span>
                <span class="order">
                    <button aria-label=move || fill(s.move_block_up, &[("block", &l1())])
                        on:click=move |_| {
                            let k = k1.clone();
                            edit.run(Box::new(move |c| {
                                shift(c, &k, true);
                            }))
                        }>"↑"</button>
                    <button aria-label=move || fill(s.move_block_down, &[("block", &l2())])
                        on:click=move |_| {
                            let k = k2.clone();
                            edit.run(Box::new(move |c| {
                                shift(c, &k, false);
                            }))
                        }>"↓"</button>
                    <Show when=two>
                        {
                            let k3 = k3.clone();
                            let l3 = l3.clone();
                            view! {
                                <button aria-label=move || fill(s.move_block_across, &[("block", &l3())])
                                    title=s.move_block_across_hint
                                    on:click=move |_| {
                                        let k = k3.clone();
                                        edit.run(Box::new(move |c| {
                                            switch_column(c, &k);
                                        }))
                                    }>"⇄"</button>
                            }
                        }
                    </Show>
                </span>
            </li>
        }
    };

    let zones = move || {
        let Some(columns) = cv.with(|c| c.as_ref().map(|c| visual_columns(&c.layout))) else {
            return Vec::new();
        };
        let two = columns.len() == 2;
        let side = layout().sidebar_width_pct;
        columns
            .into_iter()
            .map(|column| {
                let keys =
                    cv.with(|c| c.as_ref().map(|c| blocks_in(c, column)).unwrap_or_default());
                let empty = keys.is_empty();
                let title = match (two, column) {
                    (false, _) => s.layout_page,
                    (true, Column::Sidebar) => s.layout_sidebar,
                    (true, Column::Main) => s.layout_main,
                };
                let grow = match (two, column) {
                    (false, _) => 100,
                    (true, Column::Sidebar) => u16::from(side),
                    (true, Column::Main) => 100 - u16::from(side),
                };
                view! {
                    <div class="layout-zone" style=format!("flex-grow: {grow}")
                        data-drop-group="layout" data-drop-zone=zone_name(column)>
                        <h4>{title}</h4>
                        <ul>{keys.into_iter().map(card).collect_view()}</ul>
                        {empty.then(|| view! { <p class="layout-empty">{s.layout_drop_here}</p> })}
                    </div>
                }
            })
            .collect_view()
    };

    view! {
        <fieldset class="layout">
            <legend>{s.layout}</legend>
            <fieldset class="segmented">
                <legend>{s.layout_columns}</legend>
                <label>
                    <input type="radio" name="layout-columns" prop:checked=move || !two()
                        on:change=move |_| set_columns(Columns::One) />
                    {s.layout_one_column}
                </label>
                <label>
                    <input type="radio" name="layout-columns" prop:checked=two
                        on:change=move |_| set_columns(Columns::Two) />
                    {s.layout_two_columns}
                </label>
            </fieldset>
            <Show when=two>
                <label class="slider">
                    <span>{s.layout_sidebar_width}" "<output>{move || show_number(f32::from(layout().sidebar_width_pct))}"%"</output></span>
                    <input type="range" step="1" min=min.to_string() max=max.to_string()
                        prop:value=move || layout().sidebar_width_pct.to_string()
                        on:input=move |ev| {
                            let Ok(v) = event_target_value(&ev).trim().parse::<u8>() else {
                                return;
                            };
                            let v = v.clamp(min, max);
                            edit.run(Box::new(move |c| c.layout.sidebar_width_pct = v));
                        } />
                </label>
                <p class="hint">{s.layout_name_left}</p>
            </Show>
            <div class="layout-page" class:paper-letter=move || {
                cv.with(|c| c.as_ref().is_some_and(|c| c.paper == cv_core::Paper::Letter))
            }>
                {zones}
            </div>
        </fieldset>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cv_core::{EntrySection, LanguagesSection, Locale, Paper, SkillsSection, TemplateId};

    fn id(n: u32) -> Id {
        Id::parse(&format!("01HZX3M8Q4V7K2N9P5R{n:07}")).expect("id")
    }

    /// Experience 1, education 2, skills 3, languages 4.
    fn cv() -> Cv {
        let entries = |n| EntrySection {
            id: id(n),
            title_override: None,
            entries: vec![],
        };
        Cv {
            schema_version: cv_core::CV_SCHEMA_VERSION,
            id: id(99),
            title: "T".into(),
            locale: Locale::En,
            paper: Paper::A4,
            template: TemplateId::default(),
            options: Default::default(),
            basics: cv_core::Basics {
                full_name: "A".into(),
                headline: None,
                email: None,
                phone: None,
                location: None,
                links: vec![],
                show_photo: false,
            },
            summary: None,
            sections: vec![
                Section::Experience(entries(1)),
                Section::Education(entries(2)),
                Section::Skills(SkillsSection {
                    id: id(3),
                    title_override: None,
                    groups: vec![],
                }),
                Section::Languages(LanguagesSection {
                    id: id(4),
                    title_override: None,
                    items: vec![],
                }),
            ],
            layout: Layout::default(),
        }
    }

    fn sec(n: u32) -> Key {
        Key::Section(id(n))
    }

    const NAME: Key = Key::Top(TopBlock::Name);
    const CONTACT: Key = Key::Top(TopBlock::Contact);
    const SUMMARY: Key = Key::Top(TopBlock::Summary);

    #[test]
    fn keys_round_trip_as_text() {
        for k in [NAME, CONTACT, SUMMARY, sec(3)] {
            assert_eq!(Key::parse(&k.as_text()), Some(k));
        }
        assert_eq!(Key::parse("section:nope"), None);
        assert_eq!(Key::parse("top:photo"), None);
    }

    #[test]
    fn one_column_lists_everything_in_main() {
        let c = cv();
        assert_eq!(visual_columns(&c.layout), [Column::Main]);
        assert_eq!(
            blocks_in(&c, Column::Main),
            [NAME, CONTACT, SUMMARY, sec(1), sec(2), sec(3), sec(4)]
        );
    }

    #[test]
    fn dan_example_builds_by_dragging() {
        // Name, contact and languages left; summary, work and education right.
        let mut c = cv();
        c.layout.columns = Columns::Two;
        assert!(place(&mut c, &NAME, Column::Sidebar, None));
        // The contact details came along with the name.
        assert!(!place(&mut c, &CONTACT, Column::Sidebar, None));
        assert!(place(&mut c, &sec(4), Column::Sidebar, None));
        assert_eq!(visual_columns(&c.layout), [Column::Sidebar, Column::Main]);
        assert_eq!(blocks_in(&c, Column::Sidebar), [NAME, CONTACT, sec(4)]);
        assert_eq!(
            blocks_in(&c, Column::Main),
            [SUMMARY, sec(1), sec(2), sec(3)]
        );
        // Skills into the sidebar above languages.
        assert!(place(&mut c, &sec(3), Column::Sidebar, Some(&sec(4))));
        assert_eq!(
            blocks_in(&c, Column::Sidebar),
            [NAME, CONTACT, sec(3), sec(4)]
        );
        // Dropped on a top block, a section goes first among the sections.
        assert!(place(&mut c, &sec(4), Column::Sidebar, Some(&NAME)));
        assert_eq!(
            blocks_in(&c, Column::Sidebar),
            [NAME, CONTACT, sec(4), sec(3)]
        );
        // Dropping where it already is changes nothing.
        assert!(!place(&mut c, &sec(3), Column::Sidebar, None));
        // The name back to main, with its contact details: main is drawn
        // on the left now.
        assert!(place(&mut c, &NAME, Column::Main, Some(&SUMMARY)));
        assert_eq!(visual_columns(&c.layout), [Column::Main, Column::Sidebar]);
        assert_eq!(
            blocks_in(&c, Column::Main),
            [NAME, CONTACT, SUMMARY, sec(1), sec(2)]
        );
    }

    #[test]
    fn one_column_drops_reorder_without_placing() {
        let mut c = cv();
        assert!(place(&mut c, &sec(4), Column::Main, Some(&sec(1))));
        assert_eq!(c.sections[0].id(), &id(4));
        assert!(c.layout.sidebar.is_empty());
        assert!(place(&mut c, &SUMMARY, Column::Main, Some(&NAME)));
        assert_eq!(
            c.layout.top,
            [TopBlock::Summary, TopBlock::Name, TopBlock::Contact]
        );
        assert!(place(&mut c, &SUMMARY, Column::Main, None));
        assert_eq!(
            c.layout.top,
            [TopBlock::Name, TopBlock::Contact, TopBlock::Summary]
        );
    }

    #[test]
    fn shift_stays_within_the_column() {
        let mut c = cv();
        c.layout.columns = Columns::Two;
        place(&mut c, &sec(2), Column::Sidebar, None);
        place(&mut c, &sec(4), Column::Sidebar, None);
        // Main holds 1 and 3; 3 moves up past 2, which is in the sidebar.
        assert!(shift(&mut c, &sec(3), true));
        assert_eq!(blocks_in(&c, Column::Main)[3..], [sec(3), sec(1)]);
        assert!(!shift(&mut c, &sec(3), true), "first section of main");
        assert!(!shift(&mut c, &sec(4), false), "last of the sidebar");
        assert!(shift(&mut c, &CONTACT, true));
        assert_eq!(
            c.layout.top_in(Column::Main),
            [TopBlock::Contact, TopBlock::Name, TopBlock::Summary]
        );
        assert!(!shift(&mut c, &SUMMARY, false));
    }

    #[test]
    fn switch_column_needs_two_columns() {
        let mut c = cv();
        assert!(!switch_column(&mut c, &sec(1)));
        c.layout.columns = Columns::Two;
        assert!(switch_column(&mut c, &sec(1)));
        assert_eq!(blocks_in(&c, Column::Sidebar), [sec(1)]);
        assert!(switch_column(&mut c, &sec(1)));
        assert!(blocks_in(&c, Column::Sidebar).is_empty());
        assert_eq!(blocks_in(&c, Column::Main).last(), Some(&sec(1)));
    }
}
