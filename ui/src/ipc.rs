//! Calls the desktop shell's commands (spec section 10.2) through Tauri's
//! global `invoke`. Argument names are camelCase, as Tauri expects.

use cv_api::{ApiError, ErrorCode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;

use crate::i18n::{fill, t};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI__", "core"], js_name = invoke)]
    async fn tauri_invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI__", "event"], js_name = listen)]
    async fn tauri_listen(event: &str, handler: &js_sys::Function) -> Result<JsValue, JsValue>;
}

/// A subscription to a backend event (`ai://progress` and the like).
/// Dropping it unsubscribes.
pub struct Listener {
    unlisten: Option<js_sys::Function>,
    _handler: Closure<dyn FnMut(JsValue)>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        if let Some(f) = &self.unlisten {
            let _ = f.call0(&JsValue::NULL);
        }
    }
}

/// Calls `f` with the payload of every `event` until the returned
/// [`Listener`] is dropped. Payloads that do not decode as `T` are skipped.
pub async fn listen<T: DeserializeOwned + 'static>(
    event: &str,
    mut f: impl FnMut(T) + 'static,
) -> Result<Listener, ApiError> {
    let handler = Closure::<dyn FnMut(JsValue)>::new(move |ev: JsValue| {
        let payload = js_sys::Reflect::get(&ev, &JsValue::from_str("payload")).unwrap_or_default();
        if let Ok(p) = serde_wasm_bindgen::from_value(payload) {
            f(p);
        }
    });
    let unlisten = tauri_listen(event, handler.as_ref().unchecked_ref())
        .await
        .map_err(|_| ApiError::new(ErrorCode::Internal, t().app_not_answering))?;
    Ok(Listener {
        unlisten: unlisten.dyn_into().ok(),
        _handler: handler,
    })
}

/// Invokes `cmd` with `args` (a struct whose fields are the command's
/// parameters) and decodes the reply.
pub async fn call<A: Serialize, R: DeserializeOwned>(cmd: &str, args: &A) -> Result<R, ApiError> {
    // Maps (a variant's order and text overrides) must arrive as plain
    // objects: the IPC sends JSON, and a JavaScript Map turns into `{}`.
    let args = args
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(internal)?;
    match tauri_invoke(cmd, args).await {
        Ok(value) => serde_wasm_bindgen::from_value(value).map_err(internal),
        Err(err) => Err(serde_wasm_bindgen::from_value(err)
            .unwrap_or_else(|_| ApiError::new(ErrorCode::Internal, t().app_not_answering))),
    }
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(
        ErrorCode::Internal,
        fill(t().unexpected_data, &[("error", &e.to_string())]),
    )
}

/// `{}` for commands without parameters.
#[derive(Serialize)]
pub struct NoArgs {}

#[derive(Serialize)]
pub struct IdArg<'a> {
    pub id: &'a cv_core::Id,
}

#[derive(Serialize)]
pub struct ReqArg<'a, T> {
    pub req: &'a T,
}
