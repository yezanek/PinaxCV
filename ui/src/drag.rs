//! Drag-and-drop reordering with pointer events (spec section 12.10). Each
//! item carries `data-drag-list` and `data-drag-id`; dragging its handle
//! over another item of the same list and letting go moves it there. The
//! Move up/Move down buttons stay as the keyboard alternative. WebView
//! HTML5 drag and drop is not used because Tauri's file-drop handling turns
//! it off on Windows.

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

/// Class on the item the dragged one would land on.
const TARGET_CLASS: &str = "drop-target";

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

#[cfg(test)]
mod tests {
    use super::move_to;

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
