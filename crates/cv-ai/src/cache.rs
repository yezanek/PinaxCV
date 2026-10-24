//! Response caching (spec section 11.14). Keys are
//! `sha256(kind + prompt_version + model + input)`; the store behind
//! [`ResponseCache`] is the app's `ai_cache` table, so this crate never
//! touches the database itself.

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::prompts::Kind;
use crate::{Llm, LlmError, LlmRequest};

/// Where cached responses live. Failures are the implementation's to log:
/// a cache that cannot read or write only means the provider is asked again.
pub trait ResponseCache: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
    fn put(&self, key: &str, kind: &str, response: &str);
}

/// SHA-256 of `text`, as lowercase hex.
pub fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The cache key for `input` (the system prompt is fixed by the version).
/// Parts are separated by NUL so no two inputs share a key by concatenation.
pub fn cache_key(kind: Kind, model: &str, input: &str) -> String {
    let mut hash = Sha256::new();
    for part in [kind.as_str(), kind.prompt_version(), model, input] {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Sends `request` unless a response for the same kind, prompt version,
/// model and user prompt is cached. Only successful responses are stored.
pub async fn generate_cached(
    llm: &dyn Llm,
    cache: &dyn ResponseCache,
    kind: Kind,
    model: &str,
    request: LlmRequest,
) -> Result<Value, LlmError> {
    if request.cancel.is_cancelled() {
        return Err(LlmError::Cancelled);
    }
    let key = cache_key(kind, model, &request.user);
    if let Some(hit) = cache.get(&key).and_then(|s| serde_json::from_str(&s).ok()) {
        return Ok(hit);
    }
    let value = llm.generate_json(request).await?;
    cache.put(&key, kind.as_str(), &value.to_string());
    Ok(value)
}

/// An in-memory cache, for tests and for running without a store.
#[derive(Default)]
pub struct MemoryCache {
    entries: Mutex<HashMap<String, String>>,
}

impl MemoryCache {
    pub fn len(&self) -> usize {
        self.entries.lock().map_or(0, |e| e.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ResponseCache for MemoryCache {
    fn get(&self, key: &str) -> Option<String> {
        self.entries.lock().ok()?.get(key).cloned()
    }

    fn put(&self, key: &str, _kind: &str, response: &str) {
        if let Ok(mut e) = self.entries.lock() {
            e.insert(key.to_owned(), response.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FakeLlm;

    fn request(user: &str) -> LlmRequest {
        LlmRequest::new(
            "s".into(),
            user.into(),
            serde_json::json!({}),
            "JobRequirements",
        )
    }

    #[test]
    fn keys_depend_on_every_part() {
        let k = cache_key(Kind::JobParse, "m", "jd");
        assert_eq!(k.len(), 64);
        assert_eq!(k, cache_key(Kind::JobParse, "m", "jd"));
        assert_ne!(k, cache_key(Kind::Relevance, "m", "jd"));
        assert_ne!(k, cache_key(Kind::JobParse, "m2", "jd"));
        assert_ne!(k, cache_key(Kind::JobParse, "m", "jd2"));
        assert_ne!(
            cache_key(Kind::JobParse, "ab", "c"),
            cache_key(Kind::JobParse, "a", "bc")
        );
    }

    #[tokio::test]
    async fn a_cached_response_is_not_requested_again() {
        let llm = FakeLlm::new().reply("JobRequirements", serde_json::json!({ "n": 1 }));
        let cache = MemoryCache::default();
        let first = generate_cached(&llm, &cache, Kind::JobParse, "m", request("jd"))
            .await
            .expect("first");
        // The queue is empty now: a second request would fail.
        let again = generate_cached(&llm, &cache, Kind::JobParse, "m", request("jd"))
            .await
            .expect("cached");
        assert_eq!(first, again);
        assert_eq!(cache.len(), 1);
        assert!(
            generate_cached(&llm, &cache, Kind::JobParse, "other", request("jd"))
                .await
                .is_err()
        );
        assert_eq!(cache.len(), 1, "failures are not cached");
    }
}
