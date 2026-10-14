//! Importers from JSON Resume, Europass, LinkedIn exports and text into CvPartial. Never writes to storage.
//!
//! See `pinaxcv-spec.md` and `docs/ARCHITECTURE.md` for this crate's responsibilities.

#![forbid(unsafe_code)]

pub mod europass;

pub use europass::{AttachmentError, embedded_xml};
pub use europass_xml::{EuropassCv, XmlError, parse_candidate};
pub mod europass_map;
pub mod europass_xml;
pub mod partial;

pub use europass_map::to_partial;
pub use partial::{CvPartial, ImportResult, ImportWarning};
pub mod merge;
pub use merge::{
    EntryDecision, Field, FieldConflict, MergePlan, field_conflicts, merge, suggest_matches,
};
pub mod import;
pub use import::{ImportError, import_europass};
pub mod pdf_text;
pub use pdf_text::{Heading, NotReadable, page_text, split_sections};
