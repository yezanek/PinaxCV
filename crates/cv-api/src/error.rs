//! The error contract for every operation (spec section 15).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Error returned by every operation. The UI maps `code` to an i18n message key;
/// `message` is an actionable English fallback.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
    pub details: Option<Value>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotFound,
    /// `details` carries `{ "current_revision": u64 }`.
    Conflict,
    /// `details` carries the list of validation issues.
    Validation,
    /// The user closed a file dialog without choosing a path.
    Cancelled,
    Io,
    Db,
    Render,
    Import,
    UnsupportedFormat,
    NoTextLayer,
    AiNotConfigured,
    AiConsentRequired,
    AiProvider,
    AiInvalidOutput,
    AiCancelled,
    Keychain,
    Internal,
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    /// Attaches machine-readable details.
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    /// A save was rejected because the stored revision moved on.
    pub fn conflict(current_revision: u64) -> Self {
        Self::new(
            ErrorCode::Conflict,
            "This CV was changed elsewhere. Reload to see the latest version.",
        )
        .with_details(serde_json::json!({ "current_revision": current_revision }))
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_carries_current_revision() {
        let err = ApiError::conflict(7);
        assert_eq!(err.code, ErrorCode::Conflict);
        assert_eq!(
            err.details,
            Some(serde_json::json!({ "current_revision": 7 }))
        );
    }

    #[test]
    fn codes_serialize_as_snake_case() {
        let err = ApiError::new(ErrorCode::AiConsentRequired, "Consent needed.");
        let value = serde_json::to_value(&err).expect("serialize");
        assert_eq!(value["code"], "ai_consent_required");
        assert_eq!(value["details"], Value::Null);
    }

    #[test]
    fn display_names_code_and_message() {
        let err = ApiError::new(ErrorCode::NotFound, "No such CV.");
        assert_eq!(err.to_string(), "NotFound: No such CV.");
    }
}
