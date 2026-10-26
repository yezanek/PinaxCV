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
}

/// Invokes `cmd` with `args` (a struct whose fields are the command's
/// parameters) and decodes the reply.
pub async fn call<A: Serialize, R: DeserializeOwned>(cmd: &str, args: &A) -> Result<R, ApiError> {
    let args = serde_wasm_bindgen::to_value(args).map_err(internal)?;
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
