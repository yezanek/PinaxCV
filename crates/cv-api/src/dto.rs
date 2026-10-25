//! Request and response types for the operations in spec section 10.2.
//!
//! AI job DTOs arrive with M7a.

use std::collections::BTreeMap;
use std::fmt;

use crate::ApiError;
use cv_core::{
    AtsReport, Cv, Diagnostic, Id, JobTarget, Link, Section, StoredVariant, TemplateId, Variant,
};
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

/// Runs fit-to-length on `variant` (the editor's overlay, saved by the
/// call) for `page_target` pages, 1 or 2.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct FitVariant {
    pub variant: Variant,
    pub expected_revision: u64,
    pub page_target: u8,
}

/// The saved variant, with its new pending cuts, and the page count with
/// every cut accepted: `fits` is false when even that is over the target.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct FitOutcome {
    pub variant: StoredVariant,
    pub pages: u32,
    pub fits: bool,
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

// Templates

/// An embedded template's `manifest.json`, for the Style tab (spec section 7).
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct TemplateManifest {
    pub id: TemplateId,
    /// Shown in the template picker.
    pub name: String,
    pub ats_safe: bool,
    /// Section kinds the template can render, in snake_case.
    pub sections: Vec<String>,
    pub options: TemplateOptionRanges,
}

/// The allowed range of each numeric `TemplateOptions` field. The backend
/// clamps saved values to these ranges.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct TemplateOptionRanges {
    pub font_size_pt: OptionRange,
    pub line_spacing: OptionRange,
    pub margin_mm: OptionRange,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq)]
pub struct OptionRange {
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

// Import (spec section 9)

/// A basics field (or the summary) that an import can fill or replace.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ImportField {
    FullName,
    Headline,
    Email,
    Phone,
    Location,
    Summary,
}

/// What an importer read from a file. Nothing is saved until
/// `import_commit`; every item already has a fresh ID.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ImportPreview {
    /// Names this preview in `import_match` and `import_commit`.
    pub preview_id: u64,
    pub full_name: Option<String>,
    pub headline: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub location: Option<String>,
    pub links: Vec<Link>,
    pub summary: Option<String>,
    pub sections: Vec<Section>,
    /// What the importer skipped or had to guess, in words for the user.
    pub warnings: Vec<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ImportMatchRequest {
    pub preview_id: u64,
    pub cv_id: Id,
}

/// How an import lines up with an existing CV, for the merge review.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ImportMatch {
    /// Fields both sides fill with different values; the user picks one.
    pub conflicts: Vec<ImportConflict>,
    /// Imported entries that look like an existing entry (same kind of
    /// section, organisation, title and start year).
    pub matches: Vec<EntryMatch>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ImportConflict {
    pub field: ImportField,
    pub mine: String,
    pub imported: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct EntryMatch {
    pub imported: Id,
    pub base: Id,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportTarget {
    /// A new CV with this title (blank: the imported name, then "Imported CV").
    New { title: String },
    /// Merge into an existing CV, which is snapshotted first (spec section 9.5).
    Merge { cv_id: Id, expected_revision: u64 },
}

/// What happens to one imported entry in a merge.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EntryChoice {
    Skip,
    AddNew,
    /// Overwrites the base entry's content and keeps its ID.
    Replace {
        base: Id,
    },
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct EntryDecision {
    pub imported: Id,
    pub choice: EntryChoice,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ImportCommit {
    pub preview_id: u64,
    pub target: ImportTarget,
    /// Conflicting fields to take from the import; the others keep the
    /// CV's value. Empty fields on the CV are always filled.
    pub use_imported: Vec<ImportField>,
    /// Entries not listed are added as new.
    pub entries: Vec<EntryDecision>,
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

/// Where the provider's API key is kept.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KeyStorage {
    /// No key is saved.
    None,
    /// In the OS keychain.
    Keychain,
    /// Only in memory until the app closes: no keychain is available.
    Memory,
}

/// The AI settings and whether the feature may be used.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct AiStatus {
    pub provider: AiProvider,
    pub model: String,
    pub base_url: Option<String>,
    pub key: KeyStorage,
    /// Provider, model and (where needed) key are all set.
    pub configured: bool,
    /// The user accepted the current consent text for this provider and host.
    pub consent_ok: bool,
    /// Where requests go, for the consent text, for example `api.anthropic.com`.
    pub host: Option<String>,
    pub send_name: bool,
    pub send_location: bool,
    /// False when no OS keychain could be reached; keys are then asked for
    /// each session (spec section 13.1).
    pub keychain_available: bool,
}

/// Sent once by the settings screen. The API key goes to the OS keychain (or
/// memory) and is never persisted in the database, logged, or echoed back.
///
/// `Serialize` exists only so the UI can send this request to the backend; no
/// response type contains an `AiConfig` (the backend answers with [`AiStatus`]),
/// and backend code never serializes one. `Debug` redacts the key.
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

/// Names a running AI job (spec section 10.2), for `ai_cancel` and events.
pub type JobId = String;

/// The stage an AI job is in, for its progress line.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AiStage {
    Starting,
    JobParse,
    Relevance,
    Selection,
    Rewrite,
    Summary,
    Gaps,
    Fit,
    Review,
    Import,
}

/// `ai://progress`: a job reached `stage`, `pct` percent done (0 to 100,
/// never lower than the last event of the same job).
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct AiProgress {
    pub job_id: JobId,
    pub stage: AiStage,
    pub pct: u8,
}

/// `ai://done`: a job finished; `result` is the job's own result type.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct AiDone {
    pub job_id: JobId,
    pub result: serde_json::Value,
}

/// `ai://error`: a job failed or was cancelled (`ai_cancelled`).
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct AiFailed {
    pub job_id: JobId,
    pub error: ApiError,
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
