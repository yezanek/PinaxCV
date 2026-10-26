//! Response caching (spec section 11.14). Keys are
//! `sha256(kind + prompt_version + model + input)`; the store behind
//! [`ResponseCache`] is the app's `ai_cache` table, so this crate never
//! touches the database itself.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use crate::prompts::Kind;
use crate::{Llm, LlmError, LlmRequest};

/// Where cached responses live. Failures are the implementation's to log:
/// a cache that cannot read or write only means the provider is asked again.
/// The methods are async so a database-backed cache can do its reads and
/// writes off the async runtime.
#[async_trait::async_trait]
pub trait ResponseCache: Send + Sync {
    async fn get(&self, key: &str) -> Option<String>;
    async fn put(&self, key: &str, kind: &str, response: &str);
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
/// model and user prompt is cached, and reads it as a `T`. Only responses
/// that read as a `T` are stored, and a cached one that no longer does is
/// asked for again.
pub async fn generate_cached<T: DeserializeOwned>(
    llm: &dyn Llm,
    cache: &dyn ResponseCache,
    kind: Kind,
    model: &str,
    request: LlmRequest,
) -> Result<T, LlmError> {
    if request.cancel.is_cancelled() {
        return Err(LlmError::Cancelled);
    }
    let key = cache_key(kind, model, &request.user);
    if let Some(hit) = cache
        .get(&key)
        .await
        .and_then(|s| serde_json::from_str(&s).ok())
    {
        return Ok(hit);
    }
    let value = llm.generate_json(request).await?;
    let out = T::deserialize(&value)
        .map_err(|_| LlmError::InvalidOutput("the answer did not match the schema".into()))?;
    cache.put(&key, kind.as_str(), &value.to_string()).await;
    Ok(out)
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

#[async_trait::async_trait]
impl ResponseCache for MemoryCache {
    async fn get(&self, key: &str) -> Option<String> {
        self.entries.lock().ok()?.get(key).cloned()
    }

    async fn put(&self, key: &str, _kind: &str, response: &str) {
        if let Ok(mut e) = self.entries.lock() {
            e.insert(key.to_owned(), response.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FakeLlm;
    use serde_json::Value;

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
        let first: Value = generate_cached(&llm, &cache, Kind::JobParse, "m", request("jd"))
            .await
            .expect("first");
        // The queue is empty now: a second request would fail.
        let again: Value = generate_cached(&llm, &cache, Kind::JobParse, "m", request("jd"))
            .await
            .expect("cached");
        assert_eq!(first, again);
        assert_eq!(cache.len(), 1);
        assert!(
            generate_cached::<Value>(&llm, &cache, Kind::JobParse, "other", request("jd"))
                .await
                .is_err()
        );
        assert_eq!(cache.len(), 1, "failures are not cached");
    }

    #[derive(serde::Deserialize, Debug)]
    struct N {
        #[allow(dead_code)]
        n: u32,
    }

    #[tokio::test]
    async fn answers_of_the_wrong_shape_are_not_cached() {
        let llm = FakeLlm::new()
            .reply("JobRequirements", serde_json::json!({ "n": "one" }))
            .reply("JobRequirements", serde_json::json!({ "n": 1 }));
        let cache = MemoryCache::default();
        let bad = generate_cached::<N>(&llm, &cache, Kind::JobParse, "m", request("jd")).await;
        assert!(matches!(bad, Err(LlmError::InvalidOutput(_))));
        assert!(cache.is_empty());
        generate_cached::<N>(&llm, &cache, Kind::JobParse, "m", request("jd"))
            .await
            .expect("second answer");
        assert_eq!(cache.len(), 1);

        // A stored answer that does not read as the type is asked for again.
        let llm = FakeLlm::new().reply("JobRequirements", serde_json::json!({ "n": 2 }));
        let cache = MemoryCache::default();
        cache
            .put(
                &cache_key(Kind::JobParse, "m", "jd"),
                "JOB_PARSE",
                "{\"n\":\"x\"}",
            )
            .await;
        generate_cached::<N>(&llm, &cache, Kind::JobParse, "m", request("jd"))
            .await
            .expect("asked again");
    }
}
