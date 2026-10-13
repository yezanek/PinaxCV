//! Request and response types for the operations in spec section 10.2.
//!
//! Import DTOs arrive with the importers (M4), and AI job DTOs with M7a.

use std::collections::BTreeMap;
use std::fmt;

use cv_core::{AtsReport, Cv, Diagnostic, Id, JobTarget, Variant};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// CVs and variants

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct NewCv {
    pub title: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct SaveCv {
    pub cv: Cv,
    pub expected_revision: u64,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct DuplicateCv {
    pub id: Id,
    pub new_title: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct NewVariant {
    pub cv_id: Id,
    pub name: String,
    pub job: JobTarget,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct SaveVariant {
    pub variant: Variant,
    pub expected_revision: u64,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct DuplicateVariant {
    pub id: Id,
    pub new_name: String,
}

/// IDs removed from a variant because the base content they referred to is gone.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct PruneResult {
    pub removed: Vec<Id>,
}

/// Selects the base CV, or one of its variants.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct Target {
    pub cv_id: Id,
    pub variant_id: Option<Id>,
}

// Rendering and export

/// Renders the current, possibly unsaved, editor state.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct PreviewRequest {
    pub target: Target,
    /// Incremented by the UI on every request; stale responses are ignored.
    pub revision: u64,
    pub unsaved_cv: Option<Cv>,
    pub unsaved_variant: Option<Variant>,
}

/// Page images are fetched by the UI from `page_urls` (a `preview://` custom
/// URI scheme serving raw PNG bytes), never sent inline as JSON (ADR-016).
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct PreviewResponse {
    pub revision: u64,
    pub page_count: u32,
    pub page_urls: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
}

/// The backend opens the save dialog itself; requests never carry file paths.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ExportPdfRequest {
    pub target: Target,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ExportHtmlRequest {
    pub target: Target,
    pub include_email: bool,
    pub include_phone: bool,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ExportResult {
    /// Where the file was written, for display only.
    pub path: String,
    /// PDF only.
    pub page_count: Option<u32>,
    /// PDF only: the ATS check of the exported file.
    pub ats: Option<AtsReport>,
}

// Settings and AI configuration

/// All settings as key/value text; keys are listed in spec section 5.2.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq, Default)]
pub struct Settings {
    pub values: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AiProvider {
    None,
    Anthropic,
    Openai,
    Gemini,
    OpenaiCompatible,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct AiStatus {
    pub provider: AiProvider,
    pub model: Option<String>,
    pub configured: bool,
    pub consent_ok: bool,
}

/// Sent once by the settings screen. The API key goes to the OS keychain (or
/// memory) and is never persisted in the database, logged, or echoed back.
#[derive(Serialize, Deserialize, JsonSchema, Clone, PartialEq)]
pub struct AiConfig {
    pub provider: AiProvider,
    /// Entered by the user; there is no default model (ADR-017).
    pub model: String,
    /// Required for `openai_compatible`.
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub send_name: bool,
    pub send_location: bool,
}

impl fmt::Debug for AiConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AiConfig")
            .field("provider", &self.provider)
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("send_name", &self.send_name)
            .field("send_location", &self.send_location)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_config_debug_never_prints_the_key() {
        let config = AiConfig {
            provider: AiProvider::Anthropic,
            model: "some-model".into(),
            base_url: None,
            api_key: Some("sk-secret-value".into()),
            send_name: false,
            send_location: true,
        };
        let printed = format!("{config:?}");
        assert!(!printed.contains("sk-secret-value"), "{printed}");
        assert!(printed.contains("<redacted>"), "{printed}");
    }

    #[test]
    fn provider_names_match_settings_values() {
        let names: Vec<_> = [
            AiProvider::None,
            AiProvider::Anthropic,
            AiProvider::Openai,
            AiProvider::Gemini,
            AiProvider::OpenaiCompatible,
        ]
        .iter()
        .map(|p| serde_json::to_value(p).expect("serialize"))
        .collect();
        assert_eq!(
            names,
            ["none", "anthropic", "openai", "gemini", "openai_compatible"]
        );
    }
}
