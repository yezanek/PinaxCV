//! AI settings, API keys and consent (spec sections 11.1, 13 and 13.1).
//! Keys live in the OS keychain through a [`SecretStore`], or only in memory
//! when there is none; they never reach the database or the logs.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use cv_ai::http::host;
use cv_ai::providers::OPENAI_URL;
use cv_ai::{AnthropicLlm, GeminiLlm, Llm, LlmError, LlmRequest, OpenAiLlm};
use cv_api::{AiConfig, AiProvider, AiStatus, ApiError, ErrorCode, KeyStorage};

use crate::{App, Result, store_error};

/// Version of the consent text. Raising it asks every user again.
pub const AI_CONSENT_VERSION: u32 = 1;

/// Settings keys from spec section 5.3, plus where consent was given.
mod keys {
    pub const PROVIDER: &str = "ai.provider";
    pub const MODEL: &str = "ai.model";
    pub const BASE_URL: &str = "ai.base_url";
    pub const SEND_NAME: &str = "ai.send_name";
    pub const SEND_LOCATION: &str = "ai.send_location";
    pub const CONSENT_VERSION: &str = "ai.consent_version";
    pub const CONSENT_AT: &str = "ai.consent_at";
    /// `provider|host` the consent was given for; a change asks again.
    pub const CONSENT_FOR: &str = "ai.consent_for";
}

/// Why a secret could not be read or written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SecretError {
    /// No keychain on this system, for example no Secret Service on Linux.
    Unavailable,
    /// The keychain refused or failed; the text names the kind of failure.
    Failed(String),
}

/// The OS keychain, one secret per account name. Platform code lives in the
/// shell.
pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> std::result::Result<Option<String>, SecretError>;
    fn set(&self, account: &str, secret: &str) -> std::result::Result<(), SecretError>;
    /// Deleting a missing secret is not an error.
    fn delete(&self, account: &str) -> std::result::Result<(), SecretError>;
}

/// A system without a keychain: every key is held in memory only.
pub struct NoKeychain;

impl SecretStore for NoKeychain {
    fn get(&self, _: &str) -> std::result::Result<Option<String>, SecretError> {
        Err(SecretError::Unavailable)
    }

    fn set(&self, _: &str, _: &str) -> std::result::Result<(), SecretError> {
        Err(SecretError::Unavailable)
    }

    fn delete(&self, _: &str) -> std::result::Result<(), SecretError> {
        Err(SecretError::Unavailable)
    }
}

/// The keychain, the session's in-memory keys, and the provider built from
/// the settings.
pub struct AiState {
    secrets: Box<dyn SecretStore>,
    memory: Mutex<HashMap<&'static str, String>>,
    llm: RwLock<Option<Arc<dyn Llm>>>,
    /// Used instead of the provider the settings describe (tests).
    pinned: Option<Arc<dyn Llm>>,
}

impl AiState {
    pub fn new(secrets: Box<dyn SecretStore>) -> Self {
        Self {
            secrets,
            memory: Mutex::default(),
            llm: RwLock::default(),
            pinned: None,
        }
    }
}

impl AiState {
    /// Swaps the keychain, keeping a pinned provider.
    pub(crate) fn set_secrets(&mut self, secrets: Box<dyn SecretStore>) {
        let pinned = self.pinned.take();
        *self = Self::new(secrets);
        self.pinned = pinned;
    }
}

impl Default for AiState {
    fn default() -> Self {
        Self::new(Box::new(NoKeychain))
    }
}

fn provider_name(p: AiProvider) -> &'static str {
    match p {
        AiProvider::None => "none",
        AiProvider::Anthropic => "anthropic",
        AiProvider::Openai => "openai",
        AiProvider::Gemini => "gemini",
        AiProvider::OpenaiCompatible => "openai_compatible",
    }
}

fn parse_provider(s: &str) -> AiProvider {
    match s {
        "anthropic" => AiProvider::Anthropic,
        "openai" => AiProvider::Openai,
        "gemini" => AiProvider::Gemini,
        "openai_compatible" => AiProvider::OpenaiCompatible,
        _ => AiProvider::None,
    }
}

/// Where `provider` sends requests.
fn provider_url(provider: AiProvider, base_url: Option<&str>) -> Option<String> {
    match provider {
        AiProvider::None => None,
        AiProvider::Anthropic => Some(cv_ai::providers::ANTHROPIC_URL.into()),
        AiProvider::Openai => Some(OPENAI_URL.into()),
        AiProvider::Gemini => Some(cv_ai::providers::GEMINI_URL.into()),
        AiProvider::OpenaiCompatible => base_url.map(str::to_owned),
    }
}

fn needs_key(provider: AiProvider) -> bool {
    matches!(
        provider,
        AiProvider::Anthropic | AiProvider::Openai | AiProvider::Gemini
    )
}

fn keychain_error(e: &SecretError) -> ApiError {
    let kind = match e {
        SecretError::Unavailable => "unavailable".to_owned(),
        SecretError::Failed(kind) => kind.clone(),
    };
    ApiError::new(
        ErrorCode::Keychain,
        "The system keychain could not save the API key.",
    )
    .with_details(serde_json::json!({ "kind": kind }))
}

fn invalid(message: &str) -> ApiError {
    ApiError::new(ErrorCode::Validation, message)
}

/// Maps provider errors for the UI. Messages never hold request or response
/// text.
pub fn llm_error(e: LlmError) -> ApiError {
    let code = match e {
        LlmError::NotConfigured => ErrorCode::AiNotConfigured,
        LlmError::Cancelled => ErrorCode::AiCancelled,
        LlmError::InvalidOutput(_) => ErrorCode::AiInvalidOutput,
        LlmError::Timeout | LlmError::Http { .. } | LlmError::Provider(_) => ErrorCode::AiProvider,
    };
    let message = match &e {
        LlmError::Http { status: 401 | 403 } => {
            "The provider refused the API key. Check the key in Settings.".to_owned()
        }
        LlmError::Http { status: 404 } => {
            "The provider does not know this model or address. Check them in Settings.".to_owned()
        }
        other => {
            let mut text = other.to_string();
            if let Some(first) = text.get_mut(..1) {
                first.make_ascii_uppercase();
            }
            format!("{text}.")
        }
    };
    ApiError::new(code, message)
}

impl App {
    fn setting(&self, key: &str) -> Result<Option<String>> {
        self.store.get_setting(key).map_err(store_error)
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.store.set_setting(key, value).map_err(store_error)
    }

    fn flag(&self, key: &str, default: bool) -> Result<bool> {
        Ok(self.setting(key)?.map_or(default, |v| v == "true"))
    }

    /// The saved key for `provider` and where it is kept.
    fn api_key(&self, provider: AiProvider) -> (Option<String>, KeyStorage) {
        let account = provider_name(provider);
        let memory = self
            .ai
            .memory
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(account)
            .cloned();
        if let Some(key) = memory {
            return (Some(key), KeyStorage::Memory);
        }
        match self.ai.secrets.get(account) {
            Ok(Some(key)) => (Some(key), KeyStorage::Keychain),
            _ => (None, KeyStorage::None),
        }
    }

    /// The AI settings, where the key is kept, and whether consent covers
    /// the current provider.
    pub fn ai_status(&self) -> Result<AiStatus> {
        let provider = parse_provider(self.setting(keys::PROVIDER)?.as_deref().unwrap_or(""));
        let model = self.setting(keys::MODEL)?.unwrap_or_default();
        let base_url = self.setting(keys::BASE_URL)?.filter(|u| !u.is_empty());
        let (key, storage) = self.api_key(provider);
        let url = provider_url(provider, base_url.as_deref());
        let configured = provider != AiProvider::None
            && !model.trim().is_empty()
            && url.is_some()
            && (key.is_some() || !needs_key(provider));
        let consent_for = url
            .as_deref()
            .map(|u| format!("{}|{}", provider_name(provider), host(u)));
        let consent_ok = consent_for.is_some()
            && self.setting(keys::CONSENT_VERSION)?.as_deref()
                == Some(AI_CONSENT_VERSION.to_string().as_str())
            && self.setting(keys::CONSENT_FOR)? == consent_for;
        Ok(AiStatus {
            provider,
            model,
            base_url,
            key: storage,
            configured,
            consent_ok,
            host: url.as_deref().map(|u| host(u).to_owned()),
            send_name: self.flag(keys::SEND_NAME, false)?,
            send_location: self.flag(keys::SEND_LOCATION, true)?,
            keychain_available: !matches!(
                self.ai.secrets.get(provider_name(provider)),
                Err(SecretError::Unavailable)
            ),
        })
    }

    /// Saves the AI settings. A new key goes to the keychain, or is held in
    /// memory for this session when there is no keychain (spec section 13.1).
    pub fn ai_configure(&self, config: AiConfig) -> Result<AiStatus> {
        let model = config.model.trim();
        let base_url = config
            .base_url
            .as_deref()
            .map(|u| u.trim().trim_end_matches('/'))
            .filter(|u| !u.is_empty());
        if config.provider != AiProvider::None && model.is_empty() {
            return Err(invalid("Enter the model name your provider uses."));
        }
        if config.provider == AiProvider::OpenaiCompatible
            && !base_url.is_some_and(|u| u.starts_with("http://") || u.starts_with("https://"))
        {
            return Err(invalid(
                "Enter the server address, starting with http:// or https://.",
            ));
        }
        let account = provider_name(config.provider);
        if let Some(key) = config
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty())
        {
            let mut memory = self
                .ai
                .memory
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            match self.ai.secrets.set(account, key) {
                Ok(()) => {
                    memory.remove(account);
                }
                Err(SecretError::Unavailable) => {
                    memory.insert(account, key.to_owned());
                }
                Err(e) => return Err(keychain_error(&e)),
            }
        }
        self.set(keys::PROVIDER, account)?;
        self.set(keys::MODEL, model)?;
        self.set(keys::BASE_URL, base_url.unwrap_or_default())?;
        self.set(keys::SEND_NAME, &config.send_name.to_string())?;
        self.set(keys::SEND_LOCATION, &config.send_location.to_string())?;
        self.rebuild_llm()?;
        self.ai_status()
    }

    /// Removes the saved key for the current provider, from the keychain and
    /// from memory.
    pub fn ai_forget_key(&self) -> Result<AiStatus> {
        let status = self.ai_status()?;
        let account = provider_name(status.provider);
        self.ai
            .memory
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(account);
        match self.ai.secrets.delete(account) {
            Ok(()) | Err(SecretError::Unavailable) => {}
            Err(e) => return Err(keychain_error(&e)),
        }
        self.rebuild_llm()?;
        self.ai_status()
    }

    /// Records that the user accepted the consent text for the current
    /// provider and host.
    pub fn ai_consent(&self) -> Result<AiStatus> {
        let status = self.ai_status()?;
        let Some(host) = &status.host else {
            return Err(invalid("Choose an AI provider first."));
        };
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        self.set(keys::CONSENT_VERSION, &AI_CONSENT_VERSION.to_string())?;
        self.set(keys::CONSENT_AT, &at.to_string())?;
        self.set(
            keys::CONSENT_FOR,
            &format!("{}|{host}", provider_name(status.provider)),
        )?;
        self.ai_status()
    }

    /// Uses `llm` instead of the provider the settings describe. The settings
    /// still decide whether AI is configured and consented to. For tests and
    /// for embedding with a provider of one's own.
    pub fn with_llm(mut self, llm: Arc<dyn Llm>) -> Self {
        self.ai.pinned = Some(llm);
        self
    }

    /// The model name in the settings, part of every cache key.
    pub(crate) fn ai_model(&self) -> Result<String> {
        Ok(self.ai_status()?.model)
    }

    /// Builds the provider from the settings and the saved key, or clears it
    /// when AI is not configured.
    fn rebuild_llm(&self) -> Result<()> {
        let status = self.ai_status()?;
        let llm: Option<Arc<dyn Llm>> = if !status.configured {
            None
        } else if let Some(pinned) = &self.ai.pinned {
            Some(Arc::clone(pinned))
        } else {
            let (key, _) = self.api_key(status.provider);
            let key = key.unwrap_or_default();
            let built: std::result::Result<Arc<dyn Llm>, LlmError> = match status.provider {
                AiProvider::None => Err(LlmError::NotConfigured),
                AiProvider::Anthropic => {
                    AnthropicLlm::new(&key, &status.model).map(|l| Arc::new(l) as _)
                }
                AiProvider::Openai => OpenAiLlm::new(&key, &status.model).map(|l| Arc::new(l) as _),
                AiProvider::Gemini => GeminiLlm::new(&key, &status.model).map(|l| Arc::new(l) as _),
                AiProvider::OpenaiCompatible => OpenAiLlm::compatible(
                    status.base_url.as_deref().unwrap_or_default(),
                    Some(key.as_str()).filter(|k| !k.is_empty()),
                    &status.model,
                )
                .map(|l| Arc::new(l) as _),
            };
            built.ok()
        };
        *self.ai.llm.write().unwrap_or_else(PoisonError::into_inner) = llm;
        Ok(())
    }

    /// The provider, if AI is configured and the user consented to it.
    /// Builds it on first use, so a key in the keychain works after a restart.
    pub fn llm(&self) -> Result<Arc<dyn Llm>> {
        let status = self.ai_status()?;
        if !status.configured {
            return Err(ApiError::new(
                ErrorCode::AiNotConfigured,
                "Set up an AI provider in Settings first.",
            ));
        }
        if !status.consent_ok {
            return Err(ApiError::new(
                ErrorCode::AiConsentRequired,
                "Review and accept what is sent to the AI provider first.",
            ));
        }
        if self
            .ai
            .llm
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .is_none()
        {
            self.rebuild_llm()?;
        }
        self.ai
            .llm
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(|| {
                ApiError::new(
                    ErrorCode::AiNotConfigured,
                    "Set up an AI provider in Settings first.",
                )
            })
    }

    /// Sends a request holding no CV data and checks a JSON answer comes back.
    /// Works before consent, since nothing personal is sent.
    pub async fn ai_test_connection(&self) -> Result<()> {
        let status = self.ai_status()?;
        if !status.configured {
            return Err(ApiError::new(
                ErrorCode::AiNotConfigured,
                "Set up an AI provider in Settings first.",
            ));
        }
        self.rebuild_llm()?;
        let llm = self
            .ai
            .llm
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(|| {
                ApiError::new(
                    ErrorCode::AiNotConfigured,
                    "Set up an AI provider in Settings first.",
                )
            })?;
        let mut request = LlmRequest::new(
            "You answer connection checks.".into(),
            "Reply with {\"ok\": true}.".into(),
            serde_json::json!({
                "type": "object",
                "properties": { "ok": { "type": "boolean" } },
                "required": ["ok"],
                "additionalProperties": false,
            }),
            "CONNECTION_TEST",
        );
        request.max_output_tokens = 64;
        llm.generate_json(request).await.map_err(llm_error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn providers_round_trip_through_settings_names() {
        for p in [
            AiProvider::None,
            AiProvider::Anthropic,
            AiProvider::Openai,
            AiProvider::Gemini,
            AiProvider::OpenaiCompatible,
        ] {
            assert_eq!(parse_provider(provider_name(p)), p);
            assert_eq!(
                serde_json::to_value(p).expect("json"),
                serde_json::json!(provider_name(p))
            );
        }
        assert_eq!(parse_provider("bogus"), AiProvider::None);
    }

    #[test]
    fn provider_errors_become_api_errors_without_bodies() {
        assert_eq!(
            llm_error(LlmError::Http { status: 401 }).code,
            ErrorCode::AiProvider
        );
        assert!(
            llm_error(LlmError::Http { status: 401 })
                .message
                .contains("key")
        );
        assert_eq!(llm_error(LlmError::Cancelled).code, ErrorCode::AiCancelled);
        assert_eq!(
            llm_error(LlmError::InvalidOutput("not JSON".into())).message,
            "The provider's output did not match the expected format: not JSON."
        );
    }
}
