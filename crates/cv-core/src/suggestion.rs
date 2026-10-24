//! Reviewable suggestions (spec section 4.5) and verification warnings (section 11.8).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Id;

/// A proposed change. Nothing is applied until the user accepts it.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub id: Id,
    pub kind: SuggestionKind,
    pub target: SuggestionTarget,
    pub before: Option<String>,
    /// Replacement text for rewrites.
    pub after: Option<String>,
    pub action: Option<SuggestionAction>,
    /// Short, user-facing explanation.
    pub reason: String,
    /// 0.0..=1.0
    pub confidence: f32,
    pub warnings: Vec<VerificationWarning>,
    pub status: SuggestionStatus,
    pub source: SuggestionSource,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionKind {
    Rewrite,
    Hide,
    Collapse,
    Reorder,
    Summary,
    SkillOrder,
    LengthCut,
}

/// What a suggestion applies to. Serialized with an explicit `"type"` tag.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SuggestionTarget {
    Bullet {
        id: Id,
    },
    Entry {
        id: Id,
    },
    Section {
        id: Id,
    },
    /// A skill group, whose skills an order suggestion reorders.
    SkillGroup {
        id: Id,
    },
    Summary,
    Skills,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SuggestionAction {
    Hide,
    Collapse,
    Show,
    /// New order for the target's children: an entry's bullets, a
    /// section's entries or a skill group's skills.
    Reorder {
        order: Vec<Id>,
    },
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionStatus {
    #[default]
    Pending,
    Accepted,
    Rejected,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionSource {
    Tailor,
    Review,
    /// Produced by the deterministic fit-to-length algorithm.
    Fit,
}

/// A job requirement the CV does not clearly show, as a question for the
/// user (spec section 11.10). Never CV content.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct GapQuestion {
    pub requirement_id: String,
    /// The requirement as the job states it.
    pub requirement: String,
    pub must_have: bool,
    pub severity: GapSeverity,
    pub question: String,
    /// The kind of place to add it, such as "the skills section".
    pub where_to_add_hint: String,
    pub dismissed: bool,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GapSeverity {
    /// Nothing in the CV matches the requirement.
    Missing,
    /// Only weakly relevant items match it.
    Weak,
}

/// A finding from deterministic rewrite verification.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct VerificationWarning {
    pub kind: VerificationKind,
    pub severity: Severity,
    /// The offending token or a short explanation, shown to the user.
    pub detail: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationKind {
    NewNumber,
    NewTerm,
    TerminologyChanged,
    LengthOutOfRange,
    LanguageChanged,
}

/// Shared severity scale for lint issues, validation issues and verification warnings.
#[derive(
    Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_ids::id;

    #[test]
    fn suggestion_round_trips_with_tagged_target_and_action() {
        let s = Suggestion {
            id: id(1),
            kind: SuggestionKind::Reorder,
            target: SuggestionTarget::Entry { id: id(2) },
            before: None,
            after: None,
            action: Some(SuggestionAction::Reorder {
                order: vec![id(3), id(4)],
            }),
            reason: "Most relevant first".into(),
            confidence: 0.8,
            warnings: vec![VerificationWarning {
                kind: VerificationKind::NewTerm,
                severity: Severity::Warning,
                detail: "Kubernetes".into(),
            }],
            status: SuggestionStatus::default(),
            source: SuggestionSource::Tailor,
        };
        let value = serde_json::to_value(&s).expect("serialize");
        assert_eq!(value["target"]["type"], "entry");
        assert_eq!(value["action"]["type"], "reorder");
        assert_eq!(value["status"], "pending");
        assert_eq!(value["warnings"][0]["kind"], "new_term");
        let back: Suggestion = serde_json::from_value(value).expect("deserialize");
        assert_eq!(back, s);
    }

    #[test]
    fn severity_orders_info_below_error() {
        assert!(Severity::Info < Severity::Warning && Severity::Warning < Severity::Error);
    }
}
