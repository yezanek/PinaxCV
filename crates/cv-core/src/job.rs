//! Parsed job requirements (spec section 11.4).
//!
//! Plain data defined here because [`crate::Variant`] caches it; `cv-ai` produces it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct JobRequirements {
    pub title: Option<String>,
    /// "junior", "mid", "senior", "lead" or "unspecified".
    pub seniority: Option<String>,
    /// ISO language code of the job description.
    pub language: String,
    pub must_have: Vec<Requirement>,
    pub nice_to_have: Vec<Requirement>,
    pub responsibilities: Vec<String>,
    /// Normalized terms likely used by ATS keyword matching.
    pub keywords: Vec<String>,
    pub domain: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct Requirement {
    /// "r1", "r2", … unique across `must_have` and `nice_to_have`.
    pub id: String,
    pub text: String,
    pub kind: RequirementKind,
    pub keywords: Vec<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequirementKind {
    Skill,
    Experience,
    Education,
    Language,
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_requirements_round_trip_with_snake_case_kinds() {
        let req = JobRequirements {
            title: Some("Backend Engineer".into()),
            seniority: Some("senior".into()),
            language: "en".into(),
            must_have: vec![Requirement {
                id: "r1".into(),
                text: "5 years of Rust".into(),
                kind: RequirementKind::Experience,
                keywords: vec!["Rust".into()],
            }],
            nice_to_have: vec![],
            responsibilities: vec![],
            keywords: vec!["Rust".into()],
            domain: None,
        };
        let value = serde_json::to_value(&req).expect("serialize");
        assert_eq!(value["must_have"][0]["kind"], "experience");
        let back: JobRequirements = serde_json::from_value(value).expect("deserialize");
        assert_eq!(back, req);
    }
}
