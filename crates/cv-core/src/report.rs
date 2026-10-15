//! Findings produced by validation, lint, rendering and the ATS check.
//!
//! Plain data shared by the producing crates and `cv-api` (ADR-021).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Id, Severity};

/// A save-time or export-time validation finding (spec section 4.3).
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ValidationIssue {
    /// Stable machine-readable code, for example `missing_name`.
    pub code: String,
    pub severity: Severity,
    /// JSON-pointer-like path to the offending field, for example `/basics/email`.
    pub path: String,
    pub message: String,
}

/// A deterministic lint finding (spec section 7.1).
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct LintIssue {
    /// Stable code from the lint table, for example `bullet_too_long`.
    pub code: String,
    pub severity: Severity,
    /// Entity the finding is about, used for jump-to-field.
    pub target: Option<Id>,
    pub message: String,
    pub fix_hint: Option<String>,
}

/// A message from compiling the template, including glyph-coverage warnings.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Stable code, for example `missing_glyphs`.
    pub code: String,
    pub message: String,
}

/// Result of checking the text extracted from an exported PDF (spec section 7.2).
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct AtsReport {
    pub extracted_text: String,
    /// 0..=100
    pub score: u8,
    pub checks: Vec<AtsCheck>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct AtsCheck {
    pub name: String,
    pub passed: bool,
    pub detail: Option<String>,
}

impl AtsReport {
    /// Returns the checks that failed, in report order.
    pub fn failed_checks(&self) -> impl Iterator<Item = &AtsCheck> {
        self.checks.iter().filter(|c| !c.passed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_checks_returns_only_failures_in_order() {
        let check = |name: &str, passed| AtsCheck {
            name: name.into(),
            passed,
            detail: None,
        };
        let report = AtsReport {
            extracted_text: String::new(),
            score: 50,
            checks: vec![
                check("name", true),
                check("email", false),
                check("order", false),
            ],
        };
        let names: Vec<_> = report.failed_checks().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["email", "order"]);
    }

    #[test]
    fn lint_issue_round_trips() {
        let issue = LintIssue {
            code: "bullet_too_long".into(),
            severity: Severity::Warning,
            target: None,
            message: "Bullet is longer than 220 characters.".into(),
            fix_hint: Some("Split it or cut filler words.".into()),
        };
        let value = serde_json::to_value(&issue).expect("serialize");
        assert_eq!(value["severity"], "warning");
        let back: LintIssue = serde_json::from_value(value).expect("deserialize");
        assert_eq!(back, issue);
    }
}
