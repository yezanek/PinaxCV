//! LLM provider abstraction, prompts, redaction and verification.
//! Receives explicit payloads; never reads the database.
//!
//! This commit defines the provider seam (spec section 11.2); providers, the
//! pipeline and verification arrive in M7a and M7b.

#![forbid(unsafe_code)]

use serde_json::Value;
use tokio_util::sync::CancellationToken;

/// Lowest and highest sampling temperature the pipeline uses.
pub const TEMPERATURE_RANGE: (f32, f32) = (0.0, 0.4);

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
    pub temperature: f32,
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
            temperature: 0.2,
            cancel: CancellationToken::new(),
        }
    }

    /// Sets the temperature, clamped to [`TEMPERATURE_RANGE`].
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature.clamp(TEMPERATURE_RANGE.0, TEMPERATURE_RANGE.1);
        self
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
        assert_eq!((r.temperature, r.max_output_tokens), (0.2, 4096));
        assert!(!r.cancel.is_cancelled());
    }

    #[test]
    fn with_temperature_clamps_to_range() {
        assert_eq!(request().with_temperature(0.9).temperature, 0.4);
        assert_eq!(request().with_temperature(-1.0).temperature, 0.0);
        assert_eq!(request().with_temperature(0.3).temperature, 0.3);
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
