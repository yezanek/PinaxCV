//! `FakeLlm` for pipeline tests: canned answers by schema name, and a record
//! of every request, so tests can check what would have been sent.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use serde_json::Value;

use crate::{Llm, LlmError, LlmRequest};

/// What a test saw of one request.
#[derive(Clone, Debug, PartialEq)]
pub struct SeenRequest {
    pub schema_name: &'static str,
    pub system: String,
    pub user: String,
}

/// Answers each schema name with its queued replies, in order. A request
/// with nothing queued fails with `InvalidOutput`.
#[derive(Default)]
pub struct FakeLlm {
    replies: Mutex<HashMap<&'static str, VecDeque<Result<Value, LlmError>>>>,
    seen: Mutex<Vec<SeenRequest>>,
}

impl FakeLlm {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues `output` for the next request named `schema_name`.
    pub fn reply(self, schema_name: &'static str, output: Value) -> Self {
        self.queue(schema_name, Ok(output))
    }

    /// Queues `error` for the next request named `schema_name`.
    pub fn fail(self, schema_name: &'static str, error: LlmError) -> Self {
        self.queue(schema_name, Err(error))
    }

    fn queue(self, schema_name: &'static str, reply: Result<Value, LlmError>) -> Self {
        self.replies
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(schema_name)
            .or_default()
            .push_back(reply);
        self
    }

    /// Every request so far, in order.
    pub fn seen(&self) -> Vec<SeenRequest> {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

#[async_trait::async_trait]
impl Llm for FakeLlm {
    async fn generate_json(&self, request: LlmRequest) -> Result<Value, LlmError> {
        if request.cancel.is_cancelled() {
            return Err(LlmError::Cancelled);
        }
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(SeenRequest {
                schema_name: request.schema_name,
                system: request.system,
                user: request.user,
            });
        self.replies
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_mut(request.schema_name)
            .and_then(VecDeque::pop_front)
            .unwrap_or_else(|| Err(LlmError::InvalidOutput("no fake reply queued".into())))
    }

    fn name(&self) -> &str {
        "Fake (no network)"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(name: &'static str) -> LlmRequest {
        LlmRequest::new("sys".into(), "user".into(), serde_json::json!({}), name)
    }

    #[test]
    fn name_identifies_the_fake_provider() {
        assert_eq!(FakeLlm::new().name(), "Fake (no network)");
    }

    #[tokio::test]
    async fn replies_come_back_in_order_per_schema() {
        let llm = FakeLlm::new()
            .reply("A", serde_json::json!({ "n": 1 }))
            .reply("A", serde_json::json!({ "n": 2 }))
            .fail("B", LlmError::Timeout);
        assert_eq!(llm.generate_json(request("A")).await.expect("1")["n"], 1);
        assert!(matches!(
            llm.generate_json(request("B")).await,
            Err(LlmError::Timeout)
        ));
        assert_eq!(llm.generate_json(request("A")).await.expect("2")["n"], 2);
        assert!(matches!(
            llm.generate_json(request("A")).await,
            Err(LlmError::InvalidOutput(_))
        ));
        let names: Vec<_> = llm.seen().iter().map(|s| s.schema_name).collect();
        assert_eq!(names, ["A", "B", "A", "A"]);
    }
}
