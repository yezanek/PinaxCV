//! Token usage per job (spec section 11.13): providers record the counts
//! they report, a job adds them up, and the user sees the total. There is no
//! price, since pricing changes. Cache hits send nothing, so they add zero.

use std::sync::{Arc, Mutex, PoisonError};

use serde_json::Value;

/// Token counts added up over the requests of one job.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Answers received from the provider.
    pub requests: u32,
    /// Answers whose server gave no token counts (some compatible servers).
    pub unreported: u32,
}

/// A shared counter a job hands to each of its requests. Clones count into
/// the same total.
#[derive(Clone, Debug, Default)]
pub struct TokenMeter(Arc<Mutex<TokenUsage>>);

impl TokenMeter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one answer: its input and output counts, or `None` when the
    /// server reported none.
    pub fn record(&self, counts: Option<(u64, u64)>) {
        let mut u = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        u.requests = u.requests.saturating_add(1);
        match counts {
            Some((input, output)) => {
                u.input_tokens = u.input_tokens.saturating_add(input);
                u.output_tokens = u.output_tokens.saturating_add(output);
            }
            None => u.unreported = u.unreported.saturating_add(1),
        }
    }

    /// The total so far.
    pub fn total(&self) -> TokenUsage {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A count at `path` in a response body, if it is a non-negative integer.
fn count(body: &Value, path: &[&str]) -> Option<u64> {
    path.iter().try_fold(body, |v, k| v.get(*k))?.as_u64()
}

/// Anthropic Messages: `usage.input_tokens` and `usage.output_tokens`.
pub(crate) fn anthropic_counts(body: &Value) -> Option<(u64, u64)> {
    Some((
        count(body, &["usage", "input_tokens"])?,
        count(body, &["usage", "output_tokens"])?,
    ))
}

/// OpenAI chat completions and compatible servers:
/// `usage.prompt_tokens` and `usage.completion_tokens`.
pub(crate) fn openai_counts(body: &Value) -> Option<(u64, u64)> {
    Some((
        count(body, &["usage", "prompt_tokens"])?,
        count(body, &["usage", "completion_tokens"])?,
    ))
}

/// Gemini: `usageMetadata.promptTokenCount`, and the output as
/// `candidatesTokenCount` plus any `thoughtsTokenCount` (billed as output).
pub(crate) fn gemini_counts(body: &Value) -> Option<(u64, u64)> {
    let input = count(body, &["usageMetadata", "promptTokenCount"])?;
    let candidates = count(body, &["usageMetadata", "candidatesTokenCount"]);
    let thoughts = count(body, &["usageMetadata", "thoughtsTokenCount"]);
    if candidates.is_none() && thoughts.is_none() {
        return None;
    }
    Some((
        input,
        candidates
            .unwrap_or(0)
            .saturating_add(thoughts.unwrap_or(0)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_meter_adds_up_across_clones() {
        let meter = TokenMeter::new();
        let other = meter.clone();
        meter.record(Some((10, 2)));
        other.record(Some((5, 1)));
        other.record(None);
        assert_eq!(
            meter.total(),
            TokenUsage {
                input_tokens: 15,
                output_tokens: 3,
                requests: 3,
                unreported: 1,
            }
        );
        assert_eq!(TokenMeter::new().total(), TokenUsage::default());
    }

    #[test]
    fn each_provider_shape_is_read() {
        assert_eq!(
            anthropic_counts(&json!({ "usage": { "input_tokens": 12, "output_tokens": 3 } })),
            Some((12, 3))
        );
        assert_eq!(
            openai_counts(
                &json!({ "usage": { "prompt_tokens": 7, "completion_tokens": 4, "total_tokens": 11 } })
            ),
            Some((7, 4))
        );
        assert_eq!(
            gemini_counts(&json!({ "usageMetadata": {
                "promptTokenCount": 9, "candidatesTokenCount": 5, "thoughtsTokenCount": 20
            } })),
            Some((9, 25))
        );
        assert_eq!(
            gemini_counts(
                &json!({ "usageMetadata": { "promptTokenCount": 9, "candidatesTokenCount": 5 } })
            ),
            Some((9, 5))
        );
    }

    #[test]
    fn missing_or_odd_counts_are_not_reported() {
        assert_eq!(anthropic_counts(&json!({})), None);
        assert_eq!(openai_counts(&json!({ "usage": null })), None);
        assert_eq!(
            openai_counts(&json!({ "usage": { "prompt_tokens": -1, "completion_tokens": 4 } })),
            None
        );
        assert_eq!(
            gemini_counts(&json!({ "usageMetadata": { "promptTokenCount": 9 } })),
            None
        );
    }
}
