//! Drag-and-drop reordering with pointer events (spec section 12.10). Each
//! item carries `data-drag-list` and `data-drag-id`; dragging its handle
//! over another item of the same list and letting go moves it there. The
//! Move up/Move down buttons stay as the keyboard alternative. WebView
//! HTML5 drag and drop is not used because Tauri's file-drop handling turns
//! it off on Windows.
//!
//! [`ZoneDragHandle`] moves items between several lists (the columns of the
//! layout canvas, ADR-084): each list is an element with `data-drop-zone`
//! and `data-drop-group`, each item in it has `data-drop-key`, and the drop
//! reports the zone and the item it lands before.

use cv_core::Id;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::i18n::t;

/// Moves the item matching `from` to where the item matching `to` is, the
/// others shifting to make room. Returns false, changing nothing, when
/// either is missing or they are the same item.
pub fn move_to<T>(items: &mut Vec<T>, from: impl Fn(&T) -> bool, to: impl Fn(&T) -> bool) -> bool {
    let (Some(i), Some(j)) = (items.iter().position(from), items.iter().position(to)) else {
        return false;
    };
    if i == j {
        return false;
    }
    let item = items.remove(i);
    items.insert(j, item);
    true
}

/// Moves the item at `from` so it ends up just before the item now at
/// `before`, or last when `before` is `None`. Returns false, changing
/// nothing, when `from` is out of range or the item would not move.
pub fn move_before<T>(items: &mut Vec<T>, from: usize, before: Option<usize>) -> bool {
    if from >= items.len() {
        return false;
    }
    let before = before.unwrap_or(items.len()).min(items.len());
    // Landing just before itself or just before its successor is no move.
    if before == from || before == from + 1 {
        return false;
    }
    let item = items.remove(from);
    let at = if before > from { before - 1 } else { before };
    items.insert(at, item);
    true
}

/// Where a [`ZoneDragHandle`] item was let go: the zone, and the key of the
/// item it lands before (`None`: at the end of the zone).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DropSpot {
    pub zone: String,
    pub before: Option<String>,
}

/// Class on the item the dragged one would land on.
const TARGET_CLASS: &str = "drop-target";
/// Classes for the insertion line of a [`ZoneDragHandle`] drag: above an
/// item, below the last item, or on an empty zone.
const LINE_BEFORE: &str = "drop-before";
const LINE_AFTER: &str = "drop-after";
const LINE_INTO: &str = "drop-into";

/// Distance from the scrolling form's top or bottom edge, in pixels, at
/// which a drag scrolls it, and how far each pointer move scrolls.
const EDGE: f64 = 40.0;
const SCROLL_STEP: f64 = 24.0;

/// Scrolls the form the handle is in when the pointer nears its top or
/// bottom edge, so an item can be dragged past what is on screen.
fn scroll_near_edge(handle: &web_sys::Element, y: f64) {
    let Some(form) = handle.closest(".form").ok().flatten() else {
        return;
    };
    let rect = form.get_bounding_client_rect();
    if y < rect.top() + EDGE {
        form.scroll_by_with_x_and_y(0.0, -SCROLL_STEP);
    } else if y > rect.bottom() - EDGE {
        form.scroll_by_with_x_and_y(0.0, SCROLL_STEP);
    }
}

/// The item of `list` under the pointer, and its ID.
fn item_at(x: i32, y: i32, list: &str) -> Option<(web_sys::Element, Id)> {
    let el = document().element_from_point(x as f32, y as f32)?;
    let item = el
        .closest(&format!("[data-drag-list=\"{list}\"]"))
        .ok()
        .flatten()?;
    let id = Id::parse(&item.get_attribute("data-drag-id")?).ok()?;
    Some((item, id))
}

/// The drag handle for item `id` in `list`. `dropped(dragged, target)` runs
/// when the item is let go over another item of the same list.
#[component]
pub fn DragHandle(list: String, id: Id, dropped: Callback<(Id, Id)>) -> impl IntoView {
    // The item currently highlighted as the drop target.
    let target: StoredValue<Option<(web_sys::Element, Id)>, LocalStorage> =
        StoredValue::new_local(None);
    // The pointer that started the drag; other pointers are ignored.
    let dragging: StoredValue<Option<i32>> = StoredValue::new(None);
    let clear = move || {
        if let Some((el, _)) = target.get_value() {
            let _ = el.class_list().remove_1(TARGET_CLASS);
        }
        target.set_value(None);
    };
    view! {
        <span class="drag-handle" aria-hidden="true" title=t().drag_to_reorder
            on:pointerdown=move |ev: web_sys::PointerEvent| {
                if ev.button() != 0 || !ev.is_primary() || dragging.get_value().is_some() {
                    return;
                }
                ev.prevent_default();
                if let Some(el) = ev.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
                    let _ = el.set_pointer_capture(ev.pointer_id());
                }
                dragging.set_value(Some(ev.pointer_id()));
            }
            on:pointermove={
                let list = list.clone();
                move |ev: web_sys::PointerEvent| {
                    if dragging.get_value() != Some(ev.pointer_id()) {
                        return;
                    }
                    if let Some(handle) = ev.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
                        scroll_near_edge(&handle, f64::from(ev.client_y()));
                    }
                    let over = item_at(ev.client_x(), ev.client_y(), &list);
                    let same = matches!((&over, target.get_value()), (Some((_, a)), Some((_, b))) if *a == b);
                    if same {
                        return;
                    }
                    clear();
                    if let Some((el, over_id)) = over {
                        let _ = el.class_list().add_1(TARGET_CLASS);
                        target.set_value(Some((el, over_id)));
                    }
                }
            }
            on:pointerup={
                let id = id.clone();
                let list = list.clone();
                move |ev: web_sys::PointerEvent| {
                    if dragging.get_value() != Some(ev.pointer_id()) {
                        return;
                    }
                    dragging.set_value(None);
                    clear();
                    // Hit-test again: the form may have scrolled under a
                    // pointer that did not move.
                    let over = item_at(ev.client_x(), ev.client_y(), &list).map(|(_, over)| over);
                    if let Some(over) = over
                        && over != id
                    {
                        dropped.run((id.clone(), over));
                    }
                }
            }
            on:pointercancel=move |ev: web_sys::PointerEvent| {
                if dragging.get_value() == Some(ev.pointer_id()) {
                    dragging.set_value(None);
                    clear();
                }
            }>
            "⠿"
        </span>
    }
}

/// The drop spot under the pointer for a drag in `group`, and the element
/// that shows the insertion line with its class. Over the lower half of an
/// item the spot is before the next item; past the last item it is the end.
fn spot_at(x: i32, y: i32, group: &str) -> Option<(DropSpot, web_sys::Element, &'static str)> {
    let el = document().element_from_point(x as f32, y as f32)?;
    let zone = el
        .closest(&format!("[data-drop-group=\"{group}\"]"))
        .ok()
        .flatten()?;
    let zone_name = zone.get_attribute("data-drop-zone")?;
    let items = zone.query_selector_all("[data-drop-key]").ok()?;
    let keyed: Vec<web_sys::Element> = (0..items.length())
        .filter_map(|i| items.item(i)?.dyn_into::<web_sys::Element>().ok())
        .collect();
    let Some(item) = el.closest("[data-drop-key]").ok().flatten() else {
        // Over the zone but not over an item: the end of the zone.
        let line = match keyed.last() {
            Some(last) => (last.clone(), LINE_AFTER),
            None => (zone, LINE_INTO),
        };
        let spot = DropSpot {
            zone: zone_name,
            before: None,
        };
        return Some((spot, line.0, line.1));
    };
    let rect = item.get_bounding_client_rect();
    let lower = f64::from(y) > rect.top() + rect.height() / 2.0;
    if !lower {
        let spot = DropSpot {
            zone: zone_name,
            before: item.get_attribute("data-drop-key"),
        };
        return Some((spot, item, LINE_BEFORE));
    }
    let next = keyed
        .iter()
        .position(|k| *k == item)
        .and_then(|i| keyed.get(i + 1));
    match next {
        Some(next) => {
            let spot = DropSpot {
                zone: zone_name,
                before: next.get_attribute("data-drop-key"),
            };
            Some((spot, next.clone(), LINE_BEFORE))
        }
        None => Some((
            DropSpot {
                zone: zone_name,
                before: None,
            },
            item,
            LINE_AFTER,
        )),
    }
}

/// The drag handle for item `key` of a drop `group`. `dropped(key, spot)`
/// runs when the item is let go over a zone of the group, unless it would
/// land where it already is.
#[component]
pub fn ZoneDragHandle(
    group: String,
    key: String,
    dropped: Callback<(String, DropSpot)>,
) -> impl IntoView {
    // The element showing the insertion line, and its class.
    let line: StoredValue<Option<(web_sys::Element, &'static str)>, LocalStorage> =
        StoredValue::new_local(None);
    let dragging: StoredValue<Option<i32>> = StoredValue::new(None);
    let clear = move || {
        if let Some((el, class)) = line.get_value() {
            let _ = el.class_list().remove_1(class);
        }
        line.set_value(None);
    };
    view! {
        <span class="drag-handle" aria-hidden="true" title=t().drag_to_move
            on:pointerdown=move |ev: web_sys::PointerEvent| {
                if ev.button() != 0 || !ev.is_primary() || dragging.get_value().is_some() {
                    return;
                }
                ev.prevent_default();
                if let Some(el) = ev.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
                    let _ = el.set_pointer_capture(ev.pointer_id());
                }
                dragging.set_value(Some(ev.pointer_id()));
            }
            on:pointermove={
                let group = group.clone();
                move |ev: web_sys::PointerEvent| {
                    if dragging.get_value() != Some(ev.pointer_id()) {
                        return;
                    }
                    if let Some(handle) = ev.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
                        scroll_near_edge(&handle, f64::from(ev.client_y()));
                    }
                    let over = spot_at(ev.client_x(), ev.client_y(), &group);
                    let same = matches!((&over, line.get_value()), (Some((_, a, ac)), Some((b, bc))) if *a == b && *ac == bc);
                    if same {
                        return;
                    }
                    clear();
                    if let Some((_, el, class)) = over {
                        let _ = el.class_list().add_1(class);
                        line.set_value(Some((el, class)));
                    }
                }
            }
            on:pointerup={
                let group = group.clone();
                let key = key.clone();
                move |ev: web_sys::PointerEvent| {
                    if dragging.get_value() != Some(ev.pointer_id()) {
                        return;
                    }
                    dragging.set_value(None);
                    clear();
                    if let Some((spot, _, _)) = spot_at(ev.client_x(), ev.client_y(), &group)
                        && spot.before.as_deref() != Some(key.as_str())
                    {
                        dropped.run((key.clone(), spot));
                    }
                }
            }
            on:pointercancel=move |ev: web_sys::PointerEvent| {
                if dragging.get_value() == Some(ev.pointer_id()) {
                    dragging.set_value(None);
                    clear();
                }
            }>
            "⠿"
        </span>
    }
}

#[cfg(test)]
mod tests {
    use super::{move_before, move_to};

    #[test]
    fn moves_before_a_position_or_to_the_end() {
        let mut v = vec![1, 2, 3, 4];
        assert!(move_before(&mut v, 0, Some(2)));
        assert_eq!(v, [2, 1, 3, 4]);
        assert!(move_before(&mut v, 3, Some(0)));
        assert_eq!(v, [4, 2, 1, 3]);
        assert!(move_before(&mut v, 1, None));
        assert_eq!(v, [4, 1, 3, 2]);
        assert!(!move_before(&mut v, 1, Some(1)), "before itself");
        assert!(!move_before(&mut v, 1, Some(2)), "before its successor");
        assert!(!move_before(&mut v, 3, None), "already last");
        assert!(!move_before(&mut v, 9, Some(0)), "out of range");
        assert!(move_before(&mut v, 0, Some(99)), "past the end is the end");
        assert_eq!(v, [1, 3, 2, 4]);
    }

    #[test]
    fn moves_onto_the_target_position() {
        let mut v = vec![1, 2, 3, 4];
        assert!(move_to(&mut v, |x| *x == 1, |x| *x == 3));
        assert_eq!(v, [2, 3, 1, 4]);
        assert!(move_to(&mut v, |x| *x == 4, |x| *x == 2));
        assert_eq!(v, [4, 2, 3, 1]);
        assert!(!move_to(&mut v, |x| *x == 2, |x| *x == 2));
        assert!(!move_to(&mut v, |x| *x == 9, |x| *x == 2));
        assert!(!move_to(&mut v, |x| *x == 2, |x| *x == 9));
        assert_eq!(v, [4, 2, 3, 1]);
    }
}
