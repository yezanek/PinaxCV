//! LLM provider abstraction, prompts, redaction and verification.
//! Receives explicit payloads; never reads the database.
//!
//! The provider seam and the providers (spec section 11.2), the payload and
//! its redaction (11.3), and the prompts and response cache (11.14) are
//! here, with the tailoring stages and rewrite verification (11.4 to 11.8).

#![forbid(unsafe_code)]

pub mod cache;
pub mod fake;
pub mod http;
pub mod payload;
pub mod prompts;
pub mod providers;
pub mod schema;
pub mod stages;
pub mod verify;

pub use cache::{MemoryCache, ResponseCache, cache_key, generate_cached};
pub use payload::{CvPayload, PayloadOptions, build_cv_payload};
pub use prompts::{Kind, Prompt, PromptError};
pub use stages::{
    ChangeType, CheckedRewrite, Relevance, Rewrites, StageContext, parse_job, rewrite_bullets,
    score_relevance,
};

pub use fake::FakeLlm;
pub use providers::{AnthropicLlm, GeminiLlm, OpenAiCompatibleLlm, OpenAiLlm};

use serde_json::Value;
use tokio_util::sync::CancellationToken;

/// Lowest and highest sampling temperature the pipeline uses.
pub const TEMPERATURE_RANGE: (f32, f32) = (0.0, 0.4);

/// Temperature used unless a stage asks for another one.
pub const DEFAULT_TEMPERATURE: f32 = 0.2;

/// One structured-output request to a provider.
#[derive(Clone, Debug)]
pub struct LlmRequest {
    pub system: String,
    /// Already redacted; contact details never appear here except for `IMPORT_EXTRACT`.
    pub user: String,
    /// JSON Schema of the expected output.
    pub schema: Value,
    pub schema_name: &'static str,
    pub max_output_tokens: u32,
    /// Always within [`TEMPERATURE_RANGE`]; set through [`LlmRequest::with_temperature`].
    temperature: f32,
    pub cancel: CancellationToken,
}

impl LlmRequest {
    /// A request with the pipeline defaults: temperature 0.2, 4096 output tokens.
    pub fn new(system: String, user: String, schema: Value, schema_name: &'static str) -> Self {
        Self {
            system,
            user,
            schema,
            schema_name,
            max_output_tokens: 4096,
            temperature: DEFAULT_TEMPERATURE,
            cancel: CancellationToken::new(),
        }
    }

    /// Sets the temperature, clamped to [`TEMPERATURE_RANGE`]; NaN falls back to
    /// [`DEFAULT_TEMPERATURE`].
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = if temperature.is_nan() {
            DEFAULT_TEMPERATURE
        } else {
            temperature.clamp(TEMPERATURE_RANGE.0, TEMPERATURE_RANGE.1)
        };
        self
    }

    /// The sampling temperature, always within [`TEMPERATURE_RANGE`].
    pub fn temperature(&self) -> f32 {
        self.temperature
    }
}

/// Provider errors. Messages never include request or response bodies.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("AI is not configured")]
    NotConfigured,
    #[error("the request was cancelled")]
    Cancelled,
    #[error("the provider did not respond in time")]
    Timeout,
    #[error("the provider returned HTTP {status}")]
    Http { status: u16 },
    #[error("provider error: {0}")]
    Provider(String),
    #[error("the provider's output did not match the expected format: {0}")]
    InvalidOutput(String),
}

/// A language-model provider returning JSON that matches `request.schema`.
#[async_trait::async_trait]
pub trait Llm: Send + Sync {
    /// Sends one request and returns the provider's JSON output, which callers
    /// still validate against `request.schema`.
    async fn generate_json(&self, request: LlmRequest) -> Result<Value, LlmError>;
    /// Provider and host, for consent text and logs.
    fn name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct Echo;

    #[async_trait::async_trait]
    impl Llm for Echo {
        async fn generate_json(&self, request: LlmRequest) -> Result<Value, LlmError> {
            if request.cancel.is_cancelled() {
                return Err(LlmError::Cancelled);
            }
            Ok(serde_json::json!({ "schema": request.schema_name }))
        }

        fn name(&self) -> &str {
            "echo"
        }
    }

    fn request() -> LlmRequest {
        LlmRequest::new("sys".into(), "user".into(), serde_json::json!({}), "TEST")
    }

    #[test]
    fn new_uses_pipeline_defaults() {
        let r = request();
        assert_eq!((r.temperature(), r.max_output_tokens), (0.2, 4096));
        assert!(!r.cancel.is_cancelled());
    }

    #[test]
    fn with_temperature_clamps_to_range() {
        assert_eq!(request().with_temperature(0.9).temperature(), 0.4);
        assert_eq!(request().with_temperature(-1.0).temperature(), 0.0);
        assert_eq!(request().with_temperature(0.3).temperature(), 0.3);
    }

    #[test]
    fn with_temperature_maps_nan_to_default() {
        let r = request().with_temperature(f32::NAN);
        assert_eq!(r.temperature(), DEFAULT_TEMPERATURE);
    }

    #[tokio::test]
    async fn llm_is_usable_as_a_trait_object() {
        let llm: Arc<dyn Llm> = Arc::new(Echo);
        assert_eq!(llm.name(), "echo");
        let out = llm.generate_json(request()).await.expect("echo");
        assert_eq!(out["schema"], "TEST");

        let cancelled = request();
        cancelled.cancel.cancel();
        assert!(matches!(
            llm.generate_json(cancelled).await,
            Err(LlmError::Cancelled)
        ));
    }
}
