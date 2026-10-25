//! The optional profile photo (ADR-083): add, crop, replace and remove it,
//! and choose whether the CV shows it. The photo is stored with the CV on
//! the backend; only the "show" switch is part of the CV document.

use cv_api::{ApiError, ErrorCode, PhotoApply, PhotoCrop, PhotoInfo, PhotoShape, PhotoSource};
use cv_core::{Cv, Id};
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;

use crate::entries::Edit;
use crate::i18n::{error_message, t};
use crate::ipc::{IdArg, NoArgs, ReqArg, call};

/// Width of the crop frame on screen, in CSS pixels.
const FRAME_WIDTH: f64 = 240.0;
/// Smallest crop side the backend accepts, in photo pixels.
const MIN_SIDE: f64 = 32.0;
/// Most the slider zooms in.
const MAX_ZOOM: f64 = 8.0;

/// The crop being edited: zoom 1 is the largest frame that fits the photo,
/// and the centre is in the photo's pixels.
#[derive(Clone, Debug, PartialEq)]
struct Cropping {
    source: PhotoSource,
    shape: PhotoShape,
    zoom: f64,
    cx: f64,
    cy: f64,
}

/// The frame's size in photo pixels at zoom 1.
fn base(width: u32, height: u32, shape: PhotoShape) -> (f64, f64) {
    let (rw, rh) = shape.ratio();
    let (w, h, rw, rh) = (
        f64::from(width),
        f64::from(height),
        f64::from(rw),
        f64::from(rh),
    );
    if w * rh >= h * rw {
        (h * rw / rh, h)
    } else {
        (w, w * rh / rw)
    }
}

/// How far in the slider may zoom before the crop gets too small.
fn max_zoom(width: u32, height: u32, shape: PhotoShape) -> f64 {
    let (bw, bh) = base(width, height, shape);
    (bw.min(bh) / MIN_SIDE).clamp(1.0, MAX_ZOOM)
}

impl Cropping {
    /// Starts centred on the whole photo.
    fn new(source: PhotoSource, shape: PhotoShape) -> Self {
        let (cx, cy) = (
            f64::from(source.width) / 2.0,
            f64::from(source.height) / 2.0,
        );
        Self {
            source,
            shape,
            zoom: 1.0,
            cx,
            cy,
        }
    }

    /// Starts from a saved crop of `source`.
    fn from_crop(source: PhotoSource, crop: PhotoCrop) -> Self {
        let (bw, _) = base(source.width, source.height, crop.shape);
        let mut c = Self {
            zoom: bw / f64::from(crop.width.max(1)),
            cx: f64::from(crop.x) + f64::from(crop.width) / 2.0,
            cy: f64::from(crop.y) + f64::from(crop.height) / 2.0,
            shape: crop.shape,
            source,
        };
        c.clamp();
        c
    }

    /// The frame's size in photo pixels.
    fn size(&self) -> (f64, f64) {
        let (bw, bh) = base(self.source.width, self.source.height, self.shape);
        (bw / self.zoom, bh / self.zoom)
    }

    /// Keeps the zoom in range and the frame inside the photo.
    fn clamp(&mut self) {
        self.zoom = self.zoom.clamp(
            1.0,
            max_zoom(self.source.width, self.source.height, self.shape),
        );
        let (w, h) = self.size();
        let (pw, ph) = (f64::from(self.source.width), f64::from(self.source.height));
        self.cx = self.cx.clamp(w / 2.0, (pw - w / 2.0).max(w / 2.0));
        self.cy = self.cy.clamp(h / 2.0, (ph - h / 2.0).max(h / 2.0));
    }

    /// Screen pixels per photo pixel.
    fn scale(&self) -> f64 {
        FRAME_WIDTH / self.size().0
    }

    /// Moves the photo by a drag of `dx`, `dy` screen pixels.
    fn drag(&mut self, dx: f64, dy: f64) {
        let s = self.scale();
        self.cx -= dx / s;
        self.cy -= dy / s;
        self.clamp();
    }

    fn set_zoom(&mut self, zoom: f64) {
        self.zoom = zoom;
        self.clamp();
    }

    fn set_shape(&mut self, shape: PhotoShape) {
        self.shape = shape;
        self.clamp();
    }

    /// The crop to send, in whole photo pixels.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn crop(&self) -> PhotoCrop {
        let (w, h) = self.size();
        let x = (self.cx - w / 2.0).max(0.0);
        let y = (self.cy - h / 2.0).max(0.0);
        PhotoCrop {
            x: x.round() as u32,
            y: y.round() as u32,
            width: w.round() as u32,
            height: h.round() as u32,
            shape: self.shape,
        }
    }

    /// The frame's height on screen.
    fn frame_height(&self) -> f64 {
        let (rw, rh) = self.shape.ratio();
        FRAME_WIDTH * f64::from(rh) / f64::from(rw)
    }

    /// Where the photo sits inside the frame: its width and offset in
    /// screen pixels.
    fn placement(&self) -> (f64, f64, f64) {
        let s = self.scale();
        let (w, h) = self.size();
        (
            f64::from(self.source.width) * s,
            -(self.cx - w / 2.0) * s,
            -(self.cy - h / 2.0) * s,
        )
    }
}

/// The photo controls in the editor's Content tab.
#[component]
pub fn PhotoSection(
    cv_id: Id,
    cv: RwSignal<Option<Cv>>,
    edit: Edit,
    error: RwSignal<Option<String>>,
    /// Called after the stored photo changed, so the preview re-renders.
    changed: Callback<()>,
) -> impl IntoView {
    let photo: RwSignal<Option<PhotoInfo>> = RwSignal::new(None);
    let cropping: RwSignal<Option<Cropping>> = RwSignal::new(None);
    let busy = RwSignal::new(false);
    let cv_id = StoredValue::new(cv_id);

    spawn_local(async move {
        let id = cv_id.get_value();
        match call::<_, Option<PhotoInfo>>("photo_get", &IdArg { id: &id }).await {
            Ok(p) => {
                let _ = photo.try_set(p);
            }
            Err(e) => {
                let _ = error.try_set(Some(error_message(&e)));
            }
        }
    });

    let choose = move |_| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        spawn_local(async move {
            let result = call::<_, PhotoSource>("photo_choose", &NoArgs {}).await;
            let _ = busy.try_set(false);
            match result {
                Ok(source) => {
                    let shape = photo.with_untracked(|p| p.as_ref().map(|p| p.crop.shape));
                    cropping.set(Some(Cropping::new(
                        source,
                        shape.unwrap_or(PhotoShape::Portrait),
                    )));
                }
                Err(e) if e.code == ErrorCode::Cancelled => {}
                Err(e) => error.set(Some(error_message(&e))),
            }
        });
    };

    let recrop = move |_| {
        if let Some(p) = photo.get_untracked() {
            cropping.set(Some(Cropping::from_crop(p.original, p.crop)));
        }
    };

    let apply = Callback::new(move |()| {
        let Some(c) = cropping.get_untracked() else {
            return;
        };
        if busy.get_untracked() {
            return;
        }
        let first = photo.with_untracked(Option::is_none);
        let req = PhotoApply {
            cv_id: cv_id.get_value(),
            source_id: c.source.source_id,
            crop: c.crop(),
        };
        busy.set(true);
        spawn_local(async move {
            let result: Result<PhotoInfo, ApiError> =
                call("photo_apply", &ReqArg { req: &req }).await;
            let _ = busy.try_set(false);
            match result {
                Ok(info) => {
                    photo.set(Some(info));
                    cropping.set(None);
                    // A first photo is meant to be seen; the switch stays
                    // the user's after that.
                    if first
                        && cv.with_untracked(|c| c.as_ref().is_some_and(|c| !c.basics.show_photo))
                    {
                        edit.run(Box::new(|c| c.basics.show_photo = true));
                    }
                    changed.run(());
                }
                Err(e) => error.set(Some(error_message(&e))),
            }
        });
    });

    let remove = move |_| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        spawn_local(async move {
            let id = cv_id.get_value();
            let result = call::<_, ()>("photo_remove", &IdArg { id: &id }).await;
            let _ = busy.try_set(false);
            match result {
                Ok(()) => {
                    photo.set(None);
                    changed.run(());
                }
                Err(e) => error.set(Some(error_message(&e))),
            }
        });
    };

    let shown = move || cv.with(|c| c.as_ref().is_some_and(|c| c.basics.show_photo));

    view! {
        <fieldset class="photo">
            <legend>{t().photo}</legend>
            <p class="hint">{t().photo_hint}</p>
            <Show when=move || photo.with(Option::is_some)
                fallback=move || view! {
                    <button prop:disabled=move || busy.get() on:click=choose>{t().photo_add}</button>
                }>
                <div class="photo-row">
                    <img class="photo-thumb" alt=t().photo_alt
                        src=move || photo.with(|p| p.as_ref().map(|p| p.data_url.clone()).unwrap_or_default()) />
                    <div class="photo-actions">
                        <label class="check">
                            <input type="checkbox" prop:checked=shown
                                on:change=move |ev| {
                                    let on = event_target_checked(&ev);
                                    edit.run(Box::new(move |c| c.basics.show_photo = on));
                                } />
                            {t().photo_show}
                        </label>
                        <div>
                            <button prop:disabled=move || busy.get() on:click=recrop>{t().photo_recrop}</button>
                            <button prop:disabled=move || busy.get() on:click=choose>{t().photo_replace}</button>
                            <button prop:disabled=move || busy.get() on:click=remove>{t().photo_remove}</button>
                        </div>
                    </div>
                </div>
            </Show>
        </fieldset>
        <Show when=move || cropping.with(Option::is_some)>
            <CropDialog cropping=cropping busy=busy apply=apply />
        </Show>
    }
}

/// The crop dialog: drag the photo, zoom with the slider or the wheel, and
/// pick the frame's shape.
#[component]
fn CropDialog(
    cropping: RwSignal<Option<Cropping>>,
    busy: RwSignal<bool>,
    apply: Callback<()>,
) -> impl IntoView {
    // The pointer dragging the photo and where it last was.
    let drag: StoredValue<Option<(i32, f64, f64)>> = StoredValue::new(None);
    let update = move |f: &dyn Fn(&mut Cropping)| {
        cropping.update(|c| {
            if let Some(c) = c {
                f(c);
            }
        });
    };
    let get = move |f: fn(&Cropping) -> f64| cropping.with(|c| c.as_ref().map_or(0.0, f));
    let shape_button = move |shape: PhotoShape, label: &'static str| {
        view! {
            <label class="check">
                <input type="radio" name="photo-shape"
                    prop:checked=move || cropping.with(|c| c.as_ref().is_some_and(|c| c.shape == shape))
                    on:change=move |_| update(&|c| c.set_shape(shape)) />
                {label}
            </label>
        }
    };
    let cancel = move || cropping.set(None);
    // Keyboard users land in the dialog, so Escape and Tab work at once.
    let first = NodeRef::<leptos::html::Button>::new();
    Effect::new(move || {
        if let Some(b) = first.get() {
            let _ = b.focus();
        }
    });

    view! {
        <div class="modal-backdrop">
            <div class="modal" role="dialog" aria-modal="true" aria-labelledby="photo-crop-title"
                on:keydown=move |ev| {
                    if ev.key() == "Escape" {
                        cancel();
                    }
                }>
                <h2 id="photo-crop-title">{t().photo_crop_title}</h2>
                <p class="hint">{t().photo_crop_hint}</p>
                <div class="crop-frame"
                    style:width=format!("{FRAME_WIDTH}px")
                    style:height=move || format!("{}px", get(Cropping::frame_height))
                    on:pointerdown=move |ev: web_sys::PointerEvent| {
                        if ev.button() != 0 || !ev.is_primary() {
                            return;
                        }
                        ev.prevent_default();
                        if let Some(el) = ev.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
                            let _ = el.set_pointer_capture(ev.pointer_id());
                        }
                        drag.set_value(Some((ev.pointer_id(), f64::from(ev.client_x()), f64::from(ev.client_y()))));
                    }
                    on:pointermove=move |ev: web_sys::PointerEvent| {
                        let Some((id, x, y)) = drag.get_value() else { return };
                        if id != ev.pointer_id() {
                            return;
                        }
                        let (nx, ny) = (f64::from(ev.client_x()), f64::from(ev.client_y()));
                        drag.set_value(Some((id, nx, ny)));
                        update(&|c| c.drag(nx - x, ny - y));
                    }
                    on:pointerup=move |_| drag.set_value(None)
                    on:pointercancel=move |_| drag.set_value(None)
                    on:wheel=move |ev: web_sys::WheelEvent| {
                        ev.prevent_default();
                        let factor = if ev.delta_y() < 0.0 { 1.1 } else { 1.0 / 1.1 };
                        update(&|c| c.set_zoom(c.zoom * factor));
                    }>
                    <img alt="" draggable="false"
                        src=move || cropping.with(|c| c.as_ref().map(|c| c.source.data_url.clone()).unwrap_or_default())
                        style:width=move || format!("{}px", get(|c| c.placement().0))
                        style:left=move || format!("{}px", get(|c| c.placement().1))
                        style:top=move || format!("{}px", get(|c| c.placement().2)) />
                </div>
                <label>{t().photo_zoom}
                    <input type="range" min="1" step="0.01"
                        prop:max=move || cropping.with(|c| c.as_ref().map_or(1.0, |c| max_zoom(c.source.width, c.source.height, c.shape))).to_string()
                        prop:value=move || get(|c| c.zoom).to_string()
                        on:input=move |ev| {
                            if let Ok(z) = event_target_value(&ev).parse::<f64>() {
                                update(&|c| c.set_zoom(z));
                            }
                        } />
                </label>
                <div class="shapes" role="radiogroup">
                    {shape_button(PhotoShape::Portrait, t().photo_portrait)}
                    {shape_button(PhotoShape::Square, t().photo_square)}
                </div>
                <div class="modal-actions">
                    <button node_ref=first on:click=move |_| cancel()>{t().cancel}</button>
                    <button class="primary" prop:disabled=move || busy.get()
                        on:click=move |_| apply.run(())>{t().photo_apply}</button>
                </div>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(width: u32, height: u32) -> PhotoSource {
        PhotoSource {
            source_id: Some(1),
            data_url: String::new(),
            width,
            height,
        }
    }

    #[test]
    fn starts_with_the_largest_centred_frame() {
        let c = Cropping::new(source(800, 600), PhotoShape::Portrait);
        assert_eq!(
            c.crop(),
            PhotoCrop {
                x: 175,
                y: 0,
                width: 450,
                height: 600,
                shape: PhotoShape::Portrait
            }
        );
        let c = Cropping::new(source(300, 900), PhotoShape::Square);
        assert_eq!(
            (c.crop().y, c.crop().width, c.crop().height),
            (300, 300, 300)
        );
    }

    #[test]
    fn dragging_and_zooming_stay_inside_the_photo() {
        let mut c = Cropping::new(source(800, 600), PhotoShape::Square);
        c.drag(10_000.0, 10_000.0);
        assert_eq!((c.crop().x, c.crop().y), (0, 0));
        c.set_zoom(2.0);
        assert_eq!((c.crop().width, c.crop().height), (300, 300));
        c.drag(-10_000.0, -10_000.0);
        assert_eq!((c.crop().x, c.crop().y), (500, 300));
        c.set_zoom(1_000.0);
        assert_eq!(c.crop().width, 75, "zoom stops at its maximum");
        c.set_zoom(0.1);
        assert_eq!(c.zoom, 1.0);
        // A small photo stops zooming before the crop gets too small to use.
        let mut small = Cropping::new(source(80, 120), PhotoShape::Square);
        small.set_zoom(1_000.0);
        assert_eq!(small.crop().width, 32);
    }

    #[test]
    fn a_saved_crop_opens_where_it_was() {
        let saved = PhotoCrop {
            x: 20,
            y: 40,
            width: 60,
            height: 80,
            shape: PhotoShape::Portrait,
        };
        let c = Cropping::from_crop(source(80, 120), saved);
        assert_eq!(c.crop(), saved);
        let (w, x, y) = c.placement();
        assert_eq!((w.round(), x.round(), y.round()), (320.0, -80.0, -160.0));
    }
}
