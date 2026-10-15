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

impl ErrorCode {
    /// The stable snake_case name, identical to the serialized form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::Validation => "validation",
            Self::Cancelled => "cancelled",
            Self::Io => "io",
            Self::Db => "db",
            Self::Render => "render",
            Self::Import => "import",
            Self::UnsupportedFormat => "unsupported_format",
            Self::NoTextLayer => "no_text_layer",
            Self::AiNotConfigured => "ai_not_configured",
            Self::AiConsentRequired => "ai_consent_required",
            Self::AiProvider => "ai_provider",
            Self::AiInvalidOutput => "ai_invalid_output",
            Self::AiCancelled => "ai_cancelled",
            Self::Keychain => "keychain",
            Self::Internal => "internal",
        }
    }
}

impl ApiError {
    /// An error with a code and an actionable English message, without details.
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
        write!(f, "{}: {}", self.code.as_str(), self.message)
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
        assert_eq!(err.to_string(), "not_found: No such CV.");
    }

    #[test]
    fn as_str_matches_serialized_name() {
        use ErrorCode::*;
        let all = [
            NotFound,
            Conflict,
            Validation,
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
        ];
        for code in all {
            let json = serde_json::to_value(code).expect("serialize");
            assert_eq!(json, code.as_str(), "{code:?}");
        }
    }
}
