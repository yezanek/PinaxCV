//! Domain model, variant resolution, validation, lint and fit-to-length.
//! Pure: no I/O, no platform, database or network dependencies, and WASM-compatible.
//!
//! See `pinaxcv-spec.md` section 4 for the model and `docs/DECISIONS.md` (ADR-015)
//! for the changes made to it during planning.

#![forbid(unsafe_code)]

mod cv;
mod id;
mod job;
mod pages;
mod records;
mod report;
mod resolved;
mod suggestion;
mod variant;

pub use cv::*;
pub use id::{Id, IdSource, InvalidId};
pub use job::{JobRequirements, Requirement, RequirementKind};
pub use pages::{PageCountError, PageCounter};
pub use records::{
    CvSummary, SnapshotReason, SnapshotSummary, StoredCv, StoredVariant, Timestamp, VariantSummary,
};
pub use report::{AtsCheck, AtsReport, Diagnostic, LintIssue, ValidationIssue};
pub use resolved::{
    ResolvedCv, ResolvedEntry, ResolvedEntrySection, ResolvedLanguagesSection, ResolvedSection,
    ResolvedSkillsSection,
};
pub use suggestion::{
    Severity, Suggestion, SuggestionAction, SuggestionKind, SuggestionSource, SuggestionStatus,
    SuggestionTarget, VerificationKind, VerificationWarning,
};
pub use variant::{JobTarget, VARIANT_SCHEMA_VERSION, Variant};

/// Deterministic IDs for unit tests.
#[cfg(test)]
pub(crate) mod test_ids {
    use crate::Id;

    /// Returns a valid ULID whose last characters encode `n`.
    pub fn id(n: u32) -> Id {
        Id::parse(&format!("01HZX3M8Q4V7K2N9P5R{n:07}")).expect("test ULID")
    }
}

#[cfg(test)]
mod schema_tests {
    use super::*;

    #[test]
    fn json_schemas_generate_for_top_level_documents() {
        for schema in [
            schemars::schema_for!(Cv),
            schemars::schema_for!(Variant),
            schemars::schema_for!(ResolvedCv),
        ] {
            let value = serde_json::to_value(&schema).expect("schema serializes");
            assert!(
                value.get("properties").is_some(),
                "schema has properties: {value}"
            );
        }
    }
}
