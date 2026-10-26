//! The providers (spec section 11.2): Anthropic, OpenAI, Gemini, and any
//! OpenAI-compatible server by base URL, which covers Ollama and other local
//! servers. The user enters the model name; there is no default.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::http::{HttpRequest, Transport, UreqTransport, host, send};
use crate::{Llm, LlmError, LlmRequest};

/// Anthropic Messages API version sent with every request.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
pub const ANTHROPIC_URL: &str = "https://api.anthropic.com";
pub const OPENAI_URL: &str = "https://api.openai.com/v1";
pub const GEMINI_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

/// What every provider needs: the model the user typed, the key if the
/// provider takes one, where to send requests, and how.
#[derive(Clone)]
struct Endpoint {
    model: String,
    key: Option<String>,
    base_url: String,
    transport: Arc<dyn Transport>,
}

impl Endpoint {
    fn new(
        model: &str,
        key: Option<&str>,
        base_url: &str,
        transport: Arc<dyn Transport>,
    ) -> Result<Self, LlmError> {
        let model = model.trim();
        let base_url = base_url.trim().trim_end_matches('/');
        if model.is_empty()
            || !(base_url.starts_with("https://") || base_url.starts_with("http://"))
        {
            return Err(LlmError::NotConfigured);
        }
        Ok(Self {
            model: model.to_owned(),
            key: key
                .map(str::trim)
                .filter(|k| !k.is_empty())
                .map(str::to_owned),
            base_url: base_url.to_owned(),
            transport,
        })
    }

    fn key(&self) -> Result<&str, LlmError> {
        self.key.as_deref().ok_or(LlmError::NotConfigured)
    }
}

fn default_transport() -> Arc<dyn Transport> {
    Arc::new(UreqTransport::default())
}

/// Parses text a model returned as the JSON object it was asked for.
fn json_text(text: &str) -> Result<Value, LlmError> {
    let text = text.trim();
    // Some models wrap JSON in a Markdown fence despite the schema.
    let text = text
        .strip_prefix("```json")
        .or_else(|| text.strip_prefix("```"))
        .and_then(|t| t.strip_suffix("```"))
        .map_or(text, str::trim);
    match serde_json::from_str::<Value>(text) {
        Ok(v @ Value::Object(_)) => Ok(v),
        Ok(_) => Err(LlmError::InvalidOutput("not a JSON object".into())),
        Err(_) => Err(LlmError::InvalidOutput("not JSON".into())),
    }
}

/// Claude through the Messages API. Structured output uses one forced tool
/// whose input schema is the request's schema.
pub struct AnthropicLlm {
    endpoint: Endpoint,
    name: String,
}

impl AnthropicLlm {
    pub fn new(key: &str, model: &str) -> Result<Self, LlmError> {
        Self::with_transport(key, model, ANTHROPIC_URL, default_transport())
    }

    pub fn with_transport(
        key: &str,
        model: &str,
        base_url: &str,
        transport: Arc<dyn Transport>,
    ) -> Result<Self, LlmError> {
        let endpoint = Endpoint::new(model, Some(key), base_url, transport)?;
        endpoint.key()?;
        let name = format!("Anthropic ({})", host(&endpoint.base_url));
        Ok(Self { endpoint, name })
    }

    fn request(&self, r: &LlmRequest) -> Result<HttpRequest, LlmError> {
        Ok(HttpRequest {
            url: format!("{}/v1/messages", self.endpoint.base_url),
            headers: vec![
                ("x-api-key", self.endpoint.key()?.to_owned()),
                ("anthropic-version", ANTHROPIC_VERSION.to_owned()),
            ],
            body: json!({
                "model": self.endpoint.model,
                "max_tokens": r.max_output_tokens,
                "temperature": r.temperature(),
                "system": r.system,
                "messages": [{ "role": "user", "content": r.user }],
                "tools": [{
                    "name": r.schema_name,
                    "description": "Return the result in this shape.",
                    "input_schema": r.schema,
                }],
                "tool_choice": { "type": "tool", "name": r.schema_name },
            }),
        })
    }

    fn output(body: &Value) -> Result<Value, LlmError> {
        if body["stop_reason"] == "max_tokens" {
            return Err(LlmError::InvalidOutput("the output was cut off".into()));
        }
        if body["stop_reason"] == "refusal" {
            return Err(LlmError::Provider("the model declined the request".into()));
        }
        body["content"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|block| block["type"] == "tool_use")
            .map(|block| block["input"].clone())
            .filter(Value::is_object)
            .ok_or_else(|| LlmError::InvalidOutput("no structured output".into()))
    }
}

#[async_trait::async_trait]
impl Llm for AnthropicLlm {
    async fn generate_json(&self, request: LlmRequest) -> Result<Value, LlmError> {
        let http = self.request(&request)?;
        let body = send(self.endpoint.transport.clone(), http, request.cancel).await?;
        Self::output(&body)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Which OpenAI-style server an [`OpenAiLlm`] talks to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Flavor {
    OpenAi,
    Compatible,
}

/// OpenAI's Chat Completions API with a JSON Schema response format.
pub struct OpenAiLlm {
    endpoint: Endpoint,
    flavor: Flavor,
    name: String,
}

/// Any server with an OpenAI-compatible Chat Completions endpoint, such as
/// Ollama (`http://localhost:11434/v1`). The key is optional.
pub type OpenAiCompatibleLlm = OpenAiLlm;

impl OpenAiLlm {
    pub fn new(key: &str, model: &str) -> Result<Self, LlmError> {
        Self::build(
            Some(key),
            model,
            OPENAI_URL,
            Flavor::OpenAi,
            default_transport(),
        )
    }

    /// An OpenAI-compatible server at `base_url`, the part before
    /// `/chat/completions`.
    pub fn compatible(base_url: &str, key: Option<&str>, model: &str) -> Result<Self, LlmError> {
        Self::build(
            key,
            model,
            base_url,
            Flavor::Compatible,
            default_transport(),
        )
    }

    pub fn with_transport(
        key: Option<&str>,
        model: &str,
        base_url: &str,
        compatible: bool,
        transport: Arc<dyn Transport>,
    ) -> Result<Self, LlmError> {
        let flavor = if compatible {
            Flavor::Compatible
        } else {
            Flavor::OpenAi
        };
        Self::build(key, model, base_url, flavor, transport)
    }

    fn build(
        key: Option<&str>,
        model: &str,
        base_url: &str,
        flavor: Flavor,
        transport: Arc<dyn Transport>,
    ) -> Result<Self, LlmError> {
        let endpoint = Endpoint::new(model, key, base_url, transport)?;
        if flavor == Flavor::OpenAi {
            endpoint.key()?;
        }
        let label = match flavor {
            Flavor::OpenAi => "OpenAI",
            Flavor::Compatible => "OpenAI-compatible",
        };
        let name = format!("{label} ({})", host(&endpoint.base_url));
        Ok(Self {
            endpoint,
            flavor,
            name,
        })
    }

    fn request(&self, r: &LlmRequest) -> HttpRequest {
        let mut body = json!({
            "model": self.endpoint.model,
            "temperature": r.temperature(),
            "messages": [
                { "role": "system", "content": r.system },
                { "role": "user", "content": r.user },
            ],
            "response_format": {
                "type": "json_schema",
                "json_schema": { "name": r.schema_name, "schema": r.schema },
            },
        });
        // OpenAI enforces the schema; other servers vary, so their output is
        // checked here and retried once instead.
        if self.flavor == Flavor::OpenAi {
            body["response_format"]["json_schema"]["strict"] = json!(true);
        }
        // OpenAI's newer models take only `max_completion_tokens`; local
        // servers mostly know only `max_tokens`.
        let tokens = match self.flavor {
            Flavor::OpenAi => "max_completion_tokens",
            Flavor::Compatible => "max_tokens",
        };
        body[tokens] = json!(r.max_output_tokens);
        let headers = self
            .endpoint
            .key
            .as_ref()
            .map(|k| vec![("authorization", format!("Bearer {k}"))])
            .unwrap_or_default();
        HttpRequest {
            url: format!("{}/chat/completions", self.endpoint.base_url),
            headers,
            body,
        }
    }

    /// The model's text, or why there is none.
    fn content(body: &Value) -> Result<String, LlmError> {
        let choice = &body["choices"][0];
        if choice["finish_reason"] == "length" {
            return Err(LlmError::InvalidOutput("the output was cut off".into()));
        }
        if choice["message"]["refusal"].is_string() {
            return Err(LlmError::Provider("the model declined the request".into()));
        }
        choice["message"]["content"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| LlmError::InvalidOutput("no output".into()))
    }

    #[cfg(test)]
    fn output(body: &Value) -> Result<Value, LlmError> {
        json_text(&Self::content(body)?)
    }
}

#[async_trait::async_trait]
impl Llm for OpenAiLlm {
    async fn generate_json(&self, request: LlmRequest) -> Result<Value, LlmError> {
        let mut http = self.request(&request);
        let transport = self.endpoint.transport.clone();
        let body = match send(transport.clone(), http.clone(), request.cancel.clone()).await {
            // OpenAI's reasoning models refuse any temperature but their
            // default, and which models those are depends on the model and its
            // reasoning effort. A 400 gets one more try without it.
            Err(LlmError::Http { status: 400 }) if self.flavor == Flavor::OpenAi => {
                if let Some(fields) = http.body.as_object_mut() {
                    fields.remove("temperature");
                }
                send(transport.clone(), http.clone(), request.cancel.clone()).await?
            }
            other => other?,
        };
        let text = Self::content(&body)?;
        match json_text(&text) {
            // Spec section 11.2: a compatible server gets one more try, told
            // what was wrong with its last answer.
            Err(LlmError::InvalidOutput(why)) if self.flavor == Flavor::Compatible => {
                if let Some(messages) = http.body["messages"].as_array_mut() {
                    messages.push(json!({ "role": "assistant", "content": text }));
                    messages.push(json!({
                        "role": "user",
                        "content": format!(
                            "Your last output was invalid: {why}. Reply with only a JSON object that matches the schema."
                        ),
                    }));
                }
                let body = send(transport, http, request.cancel).await?;
                json_text(&Self::content(&body)?)
            }
            other => other,
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Gemini's `generateContent` with a JSON response schema.
pub struct GeminiLlm {
    endpoint: Endpoint,
    name: String,
}

impl GeminiLlm {
    pub fn new(key: &str, model: &str) -> Result<Self, LlmError> {
        Self::with_transport(key, model, GEMINI_URL, default_transport())
    }

    pub fn with_transport(
        key: &str,
        model: &str,
        base_url: &str,
        transport: Arc<dyn Transport>,
    ) -> Result<Self, LlmError> {
        let endpoint = Endpoint::new(model, Some(key), base_url, transport)?;
        endpoint.key()?;
        let name = format!("Gemini ({})", host(&endpoint.base_url));
        Ok(Self { endpoint, name })
    }

    fn request(&self, r: &LlmRequest) -> Result<HttpRequest, LlmError> {
        // The model name goes into the path; keep it to one segment.
        let model = self.endpoint.model.trim_start_matches("models/");
        if model.contains(['/', '?', '#']) {
            return Err(LlmError::NotConfigured);
        }
        Ok(HttpRequest {
            url: format!("{}/models/{model}:generateContent", self.endpoint.base_url),
            headers: vec![("x-goog-api-key", self.endpoint.key()?.to_owned())],
            body: json!({
                "systemInstruction": { "parts": [{ "text": r.system }] },
                "contents": [{ "role": "user", "parts": [{ "text": r.user }] }],
                "generationConfig": {
                    "temperature": r.temperature(),
                    "maxOutputTokens": r.max_output_tokens,
                    "responseMimeType": "application/json",
                    "responseJsonSchema": r.schema,
                },
            }),
        })
    }

    fn output(body: &Value) -> Result<Value, LlmError> {
        if body["promptFeedback"]["blockReason"].is_string() {
            return Err(LlmError::Provider("the request was blocked".into()));
        }
        let candidate = &body["candidates"][0];
        match candidate["finishReason"].as_str() {
            Some("MAX_TOKENS") => {
                return Err(LlmError::InvalidOutput("the output was cut off".into()));
            }
            Some("SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT") => {
                return Err(LlmError::Provider("the output was blocked".into()));
            }
            _ => {}
        }
        let text: String = candidate["content"]["parts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p["text"].as_str())
            .collect();
        if text.is_empty() {
            return Err(LlmError::InvalidOutput("no output".into()));
        }
        json_text(&text)
    }
}

#[async_trait::async_trait]
impl Llm for GeminiLlm {
    async fn generate_json(&self, request: LlmRequest) -> Result<Value, LlmError> {
        let http = self.request(&request)?;
        let body = send(self.endpoint.transport.clone(), http, request.cancel).await?;
        Self::output(&body)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::testing::Recorder;

    fn request() -> LlmRequest {
        LlmRequest::new(
            "You parse jobs.".into(),
            "<job>Rust</job>".into(),
            json!({ "type": "object", "properties": { "skills": { "type": "array" } } }),
            "JOB_PARSE",
        )
    }

    fn header<'a>(r: &'a HttpRequest, name: &str) -> Option<&'a str> {
        r.headers
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
    }

    #[test]
    fn default_constructors_check_their_settings() {
        assert_eq!(
            AnthropicLlm::new("k", "claude-x").expect("ok").name(),
            "Anthropic (api.anthropic.com)"
        );
        assert!(AnthropicLlm::new("", "claude-x").is_err());
        assert_eq!(
            OpenAiLlm::new("k", "gpt-x").expect("ok").name(),
            "OpenAI (api.openai.com)"
        );
        assert!(OpenAiLlm::new("k", "").is_err());
        assert!(OpenAiLlm::compatible("http://localhost:11434/v1", None, "llama3").is_ok());
        assert!(OpenAiLlm::compatible("ftp://x", None, "llama3").is_err());
        assert_eq!(
            GeminiLlm::new("k", "gemini-x").expect("ok").name(),
            "Gemini (generativelanguage.googleapis.com)"
        );
    }

    #[test]
    fn models_and_keys_are_required() {
        let t: Arc<dyn Transport> = Recorder::replying(Vec::new());
        assert!(matches!(
            AnthropicLlm::with_transport("k", " ", ANTHROPIC_URL, t.clone()),
            Err(LlmError::NotConfigured)
        ));
        assert!(matches!(
            GeminiLlm::with_transport("", "gemini-x", GEMINI_URL, t.clone()),
            Err(LlmError::NotConfigured)
        ));
        assert!(matches!(
            OpenAiLlm::with_transport(None, "gpt-x", OPENAI_URL, false, t.clone()),
            Err(LlmError::NotConfigured)
        ));
        assert!(matches!(
            OpenAiLlm::with_transport(None, "llama", "localhost:11434", true, t.clone()),
            Err(LlmError::NotConfigured)
        ));
        let local = OpenAiLlm::with_transport(None, "llama", "http://localhost:11434/v1/", true, t)
            .expect("no key needed");
        assert_eq!(local.name(), "OpenAI-compatible (localhost:11434)");
    }

    #[tokio::test]
    async fn anthropic_forces_one_tool_and_returns_its_input() {
        let t = Recorder::replying(vec![Recorder::ok(json!({
            "stop_reason": "tool_use",
            "content": [
                { "type": "text", "text": "Sure." },
                { "type": "tool_use", "name": "JOB_PARSE", "input": { "skills": ["Rust"] } },
            ],
        }))]);
        let llm = AnthropicLlm::with_transport("sk-test", "claude-x", ANTHROPIC_URL, t.clone())
            .expect("llm");
        assert_eq!(llm.name(), "Anthropic (api.anthropic.com)");
        let out = llm.generate_json(request()).await.expect("output");
        assert_eq!(out, json!({ "skills": ["Rust"] }));

        let sent = &t.sent()[0];
        assert_eq!(sent.url, "https://api.anthropic.com/v1/messages");
        assert_eq!(header(sent, "x-api-key"), Some("sk-test"));
        assert_eq!(header(sent, "anthropic-version"), Some(ANTHROPIC_VERSION));
        assert_eq!(sent.body["model"], "claude-x");
        assert_eq!(sent.body["tool_choice"]["name"], "JOB_PARSE");
        assert_eq!(sent.body["tools"][0]["input_schema"], request().schema);
        assert_eq!(sent.body["system"], "You parse jobs.");
    }

    #[test]
    fn anthropic_cut_off_and_missing_output_are_errors() {
        assert!(matches!(
            AnthropicLlm::output(&json!({ "stop_reason": "max_tokens", "content": [] })),
            Err(LlmError::InvalidOutput(_))
        ));
        assert!(matches!(
            AnthropicLlm::output(&json!({ "content": [{ "type": "text", "text": "{}" }] })),
            Err(LlmError::InvalidOutput(_))
        ));
    }

    #[tokio::test]
    async fn openai_sends_a_json_schema_and_parses_the_content() {
        let t = Recorder::replying(vec![Recorder::ok(json!({
            "choices": [{ "finish_reason": "stop", "message": { "content": "{\"skills\":[\"Rust\"]}" } }],
        }))]);
        let llm = OpenAiLlm::with_transport(Some("sk-o"), "gpt-x", OPENAI_URL, false, t.clone())
            .expect("llm");
        let out = llm.generate_json(request()).await.expect("output");
        assert_eq!(out["skills"][0], "Rust");
        let sent = &t.sent()[0];
        assert_eq!(sent.url, "https://api.openai.com/v1/chat/completions");
        assert_eq!(header(sent, "authorization"), Some("Bearer sk-o"));
        assert_eq!(
            sent.body["response_format"]["json_schema"]["name"],
            "JOB_PARSE"
        );
        assert_eq!(sent.body["response_format"]["json_schema"]["strict"], true);
        assert_eq!(sent.body["max_completion_tokens"], 4096);
        assert!(sent.body.get("max_tokens").is_none());
        assert_eq!(sent.body["messages"][0]["role"], "system");
    }

    #[tokio::test]
    async fn openai_retries_once_without_temperature_after_a_400() {
        let bad = || {
            Ok(crate::http::HttpResponse {
                status: 400,
                body: Value::Null,
            })
        };
        let t = Recorder::replying(vec![
            bad(),
            Recorder::ok(json!({
                "choices": [{ "finish_reason": "stop", "message": { "content": "{}" } }],
            })),
        ]);
        let llm = OpenAiLlm::with_transport(Some("sk-o"), "o-x", OPENAI_URL, false, t.clone())
            .expect("llm");
        assert_eq!(
            llm.generate_json(request()).await.expect("retried"),
            json!({})
        );
        let sent = t.sent();
        assert_eq!(sent.len(), 2);
        assert!(sent[0].body.get("temperature").is_some());
        assert!(sent[1].body.get("temperature").is_none());

        // A second 400 is the answer, and compatible servers get no retry.
        let t = Recorder::replying(vec![bad(), bad()]);
        let llm = OpenAiLlm::with_transport(Some("sk-o"), "o-x", OPENAI_URL, false, t.clone())
            .expect("llm");
        let out = llm.generate_json(request()).await;
        assert!(matches!(out, Err(LlmError::Http { status: 400 })));
        assert_eq!(t.sent().len(), 2);
        let t = Recorder::replying(vec![bad(), bad()]);
        let llm =
            OpenAiLlm::with_transport(None, "llama3", "http://localhost:11434/v1", true, t.clone())
                .expect("llm");
        let out = llm.generate_json(request()).await;
        assert!(matches!(out, Err(LlmError::Http { status: 400 })));
        assert_eq!(t.sent().len(), 1);
    }

    #[tokio::test]
    async fn compatible_servers_need_no_key_and_get_max_tokens() {
        let t = Recorder::replying(vec![Recorder::ok(json!({
            "choices": [{ "message": { "content": "```json\n{\"skills\":[]}\n```" } }],
        }))]);
        let llm =
            OpenAiLlm::with_transport(None, "llama3", "http://localhost:11434/v1", true, t.clone())
                .expect("llm");
        let out = llm.generate_json(request()).await.expect("fenced JSON");
        assert_eq!(out, json!({ "skills": [] }));
        let sent = &t.sent()[0];
        assert_eq!(sent.url, "http://localhost:11434/v1/chat/completions");
        assert!(sent.headers.is_empty());
        assert_eq!(sent.body["max_tokens"], 4096);
    }

    #[tokio::test]
    async fn compatible_servers_get_one_retry_after_invalid_output() {
        let answer =
            |text: &str| Recorder::ok(json!({ "choices": [{ "message": { "content": text } }] }));
        let t = Recorder::replying(vec![answer("Sure! Here it is."), answer("{\"skills\":[]}")]);
        let llm =
            OpenAiLlm::with_transport(None, "llama3", "http://localhost:11434/v1", true, t.clone())
                .expect("llm");
        assert_eq!(
            llm.generate_json(request()).await.expect("retried"),
            json!({ "skills": [] })
        );
        let sent = t.sent();
        assert_eq!(sent.len(), 2);
        assert!(
            sent[0].body["response_format"]["json_schema"]
                .get("strict")
                .is_none()
        );
        let messages = sent[1].body["messages"].as_array().expect("messages");
        assert_eq!(messages.len(), 4);
        assert!(
            messages[3]["content"]
                .as_str()
                .expect("text")
                .contains("not JSON")
        );

        let t = Recorder::replying(vec![answer("no"), answer("still no")]);
        let llm = OpenAiLlm::with_transport(None, "llama3", "http://localhost:11434/v1", true, t)
            .expect("llm");
        assert!(matches!(
            llm.generate_json(request()).await,
            Err(LlmError::InvalidOutput(_))
        ));

        // OpenAI itself enforces the schema, so there is no second try.
        let t = Recorder::replying(vec![answer("no"), answer("{}")]);
        let llm = OpenAiLlm::with_transport(Some("k"), "gpt-x", OPENAI_URL, false, t.clone())
            .expect("llm");
        assert!(matches!(
            llm.generate_json(request()).await,
            Err(LlmError::InvalidOutput(_))
        ));
        assert_eq!(t.sent().len(), 1);
    }

    #[test]
    fn openai_errors_name_no_content() {
        let cut = OpenAiLlm::output(
            &json!({ "choices": [{ "finish_reason": "length", "message": { "content": "{" } }] }),
        );
        assert!(matches!(cut, Err(LlmError::InvalidOutput(_))));
        let refused = OpenAiLlm::output(
            &json!({ "choices": [{ "message": { "refusal": "no", "content": null } }] }),
        );
        assert!(matches!(refused, Err(LlmError::Provider(_))));
        let Err(LlmError::InvalidOutput(m)) = OpenAiLlm::output(
            &json!({ "choices": [{ "message": { "content": "secret CV text" } }] }),
        ) else {
            panic!("not JSON")
        };
        assert!(!m.contains("secret"));
    }

    #[tokio::test]
    async fn gemini_puts_the_model_in_the_path_and_joins_parts() {
        let t = Recorder::replying(vec![Recorder::ok(json!({
            "candidates": [{
                "finishReason": "STOP",
                "content": { "parts": [{ "text": "{\"skills\":" }, { "text": "[\"Go\"]}" }] },
            }],
        }))]);
        let llm = GeminiLlm::with_transport("g-key", "models/gemini-x", GEMINI_URL, t.clone())
            .expect("llm");
        let out = llm.generate_json(request()).await.expect("output");
        assert_eq!(out["skills"][0], "Go");
        let sent = &t.sent()[0];
        assert_eq!(
            sent.url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-x:generateContent"
        );
        assert_eq!(header(sent, "x-goog-api-key"), Some("g-key"));
        assert_eq!(
            sent.body["generationConfig"]["responseMimeType"],
            "application/json"
        );
        assert_eq!(
            sent.body["generationConfig"]["responseJsonSchema"],
            request().schema
        );
    }

    #[tokio::test]
    async fn gemini_refuses_model_names_that_change_the_path() {
        let t = Recorder::replying(Vec::new());
        let llm = GeminiLlm::with_transport("g", "../x", GEMINI_URL, t.clone()).expect("llm");
        assert!(matches!(
            llm.generate_json(request()).await,
            Err(LlmError::NotConfigured)
        ));
        assert!(t.sent().is_empty());
        assert!(matches!(
            GeminiLlm::output(&json!({ "promptFeedback": { "blockReason": "SAFETY" } })),
            Err(LlmError::Provider(_))
        ));
    }
}
