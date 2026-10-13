//! Storage-agnostic records returned by persistence (ADR-021).
//!
//! They carry the revision and timestamps the store manages, without any
//! database types, so `cv-api` and the WASM UI can use them too.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Cv, Id, Variant};

/// An RFC 3339 UTC timestamp, for example `2026-10-13T15:03:33Z`.
///
/// Kept as text so `cv-core` needs no clock or date-time dependency; the store
/// produces and orders these values.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(transparent)]
pub struct Timestamp(pub String);

/// A CV as stored, with its optimistic-concurrency revision.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct StoredCv {
    pub cv: Cv,
    pub revision: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// One row of the CV list on the home screen.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct CvSummary {
    pub id: Id,
    pub title: String,
    pub updated_at: Timestamp,
    pub variant_count: u32,
    /// Page count from the last render, if known.
    pub page_count: Option<u32>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct StoredVariant {
    pub variant: Variant,
    pub revision: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct VariantSummary {
    pub id: Id,
    pub cv_id: Id,
    pub name: String,
    pub updated_at: Timestamp,
}

/// Why an automatic or manual snapshot of a base CV was taken.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotReason {
    Export,
    Import,
    Manual,
    AiApply,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct SnapshotSummary {
    pub id: Id,
    pub cv_id: Id,
    pub reason: SnapshotReason,
    pub created_at: Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_ids::id;

    #[test]
    fn timestamps_order_chronologically_as_rfc3339_text() {
        let earlier = Timestamp("2026-10-13T09:00:00Z".into());
        let later = Timestamp("2026-10-13T15:03:33Z".into());
        assert!(earlier < later);
        assert_eq!(
            serde_json::to_value(&later).expect("serialize"),
            "2026-10-13T15:03:33Z"
        );
    }

    #[test]
    fn snapshot_summary_serializes_reason_as_snake_case() {
        let s = SnapshotSummary {
            id: id(1),
            cv_id: id(2),
            reason: SnapshotReason::AiApply,
            created_at: Timestamp("2026-10-13T15:03:33Z".into()),
        };
        let value = serde_json::to_value(&s).expect("serialize");
        assert_eq!(value["reason"], "ai_apply");
        let back: SnapshotSummary = serde_json::from_value(value).expect("deserialize");
        assert_eq!(back, s);
    }
}
