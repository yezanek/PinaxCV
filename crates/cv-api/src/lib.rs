//! Application-service request/response types and error contracts shared by the
//! desktop shell and the WASM UI.
//!
//! This crate holds data only: the services that implement the operations live
//! in `cv-app` (ADR-021), so `cv-api` stays free of storage, rendering and
//! network dependencies.

#![forbid(unsafe_code)]

mod dto;
mod error;

pub use dto::*;
pub use error::{ApiError, ErrorCode};
