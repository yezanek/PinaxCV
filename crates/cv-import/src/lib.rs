//! Importers from JSON Resume, Europass, LinkedIn exports and text into CvPartial. Never writes to storage.
//!
//! See `pinaxcv-spec.md` and `docs/ARCHITECTURE.md` for this crate's responsibilities.

#![forbid(unsafe_code)]

pub mod europass;

pub use europass::{AttachmentError, embedded_xml};
