//! Tailoring stages A and B (spec sections 11.4 and 11.5). Stage C is
//! deterministic and lives in `cv_core::selection`.
//!
//! Every answer is checked against its Rust type. An answer that does not
//! fit gets one retry that says what was wrong (spec section 11.2), and only
//! answers that fit are cached.

use std::collections::{BTreeMap, BTreeSet};

use cv_core::fit::RelevanceScores;
use cv_core::selection::Reasons;
use cv_core::{Id, JobRequirements};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use tokio_util::sync::CancellationToken;

use crate::cache::{ResponseCache, cache_key};
use crate::payload::CvPayload;
use crate::prompts::{self, Prompt};
use crate::schema::output_schema;
use crate::{Llm, LlmError};

/// Input-token budget per relevance request; larger CVs are scored in
/// batches of entries (spec section 11.5).
pub const RELEVANCE_TOKEN_BUDGET: usize = 12_000;

/// What every stage needs: the provider, the cache, the model name (part of
/// the cache key) and the job's cancellation token.
pub struct StageContext<'a> {
    pub llm: &'a dyn Llm,
    pub cache: &'a dyn ResponseCache,
    pub model: &'a str,
    pub cancel: CancellationToken,
}

impl StageContext<'_> {
    /// Sends `prompt` for a `T`, or takes it from the cache.
    pub async fn generate<T>(
        &self,
        prompt: Prompt,
        schema_name: &'static str,
    ) -> Result<T, LlmError>
    where
        T: DeserializeOwned + JsonSchema,
    {
        if self.cancel.is_cancelled() {
            return Err(LlmError::Cancelled);
        }
        let key = cache_key(prompt.kind, self.model, &prompt.user);
        if let Some(hit) = self
            .cache
            .get(&key)
            .and_then(|s| serde_json::from_str(&s).ok())
        {
            return Ok(hit);
        }
        let schema = output_schema::<T>();
        let mut user = prompt.user.clone();
        for attempt in 0..2 {
            let mut request = Prompt {
                user: user.clone(),
                ..prompt.clone()
            }
            .request(schema.clone(), schema_name);
            request.cancel = self.cancel.clone();
            let value = self.llm.generate_json(request).await?;
            match T::deserialize(&value) {
                Ok(out) => {
                    self.cache
                        .put(&key, prompt.kind.as_str(), &value.to_string());
                    return Ok(out);
                }
                Err(e) if attempt == 0 => {
                    // The model's own output may be quoted back to it; it
                    // never goes into an error or a log.
                    user = format!(
                        "{}\n\nYour last output was invalid: {e}. Respond again with JSON matching the schema.",
                        prompt.user
                    );
                }
                Err(_) => break,
            }
        }
        Err(LlmError::InvalidOutput(format!(
            "the answer did not match {schema_name}"
        )))
    }
}

/// Stage A: the requirements of a job description.
pub async fn parse_job(
    ctx: &StageContext<'_>,
    job_description: &str,
) -> Result<JobRequirements, LlmError> {
    let prompt = prompts::job_parse(job_description).map_err(prompt_error)?;
    let mut req: JobRequirements = ctx.generate(prompt, "JobRequirements").await?;
    // Requirement IDs must be unique: later duplicates are dropped.
    let mut seen = BTreeSet::new();
    req.must_have.retain(|r| seen.insert(r.id.clone()));
    req.nice_to_have.retain(|r| seen.insert(r.id.clone()));
    req.keywords.truncate(40);
    Ok(req)
}

/// Stage B's answer (spec section 11.5).
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct RelevanceResult {
    pub entries: Vec<EntryScore>,
    pub bullets: Vec<BulletScore>,
    pub skills: Vec<SkillScore>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct EntryScore {
    pub entry_id: String,
    #[serde(deserialize_with = "score")]
    pub score: u8,
    pub matched_requirements: Vec<String>,
    pub reason: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct BulletScore {
    pub bullet_id: String,
    #[serde(deserialize_with = "score")]
    pub score: u8,
    pub matched_requirements: Vec<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct SkillScore {
    pub skill_id: String,
    #[serde(deserialize_with = "score")]
    pub score: u8,
}

/// Scores as integers 0 to 100; a fractional or out-of-range number from
/// the model is rounded and clamped rather than refused.
fn score<'de, D: Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    let n = f64::deserialize(d)?;
    if n.is_nan() {
        return Err(serde::de::Error::custom("score is not a number"));
    }
    // In range after clamping, so the cast cannot truncate.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(n.round().clamp(0.0, 100.0) as u8)
}

/// Stage B merged over its batches, by the CV's own IDs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Relevance {
    pub scores: RelevanceScores,
    /// The model's reason per entry.
    pub reasons: Reasons,
    /// Requirement IDs each entry, bullet or skill supports.
    pub matched: BTreeMap<Id, Vec<String>>,
}

/// Stage B: scores every entry, bullet and skill of `cv` against
/// `requirements`, in batches when the CV is large. IDs the CV does not
/// have are dropped.
pub async fn score_relevance(
    ctx: &StageContext<'_>,
    requirements: &JobRequirements,
    cv: &CvPayload,
) -> Result<Relevance, LlmError> {
    let mut out = Relevance::default();
    for batch in relevance_batches(cv) {
        let prompt = prompts::relevance(requirements, &batch).map_err(prompt_error)?;
        let result: RelevanceResult = ctx.generate(prompt, "RelevanceResult").await?;
        merge(&mut out, &batch, result);
    }
    Ok(out)
}

fn merge(out: &mut Relevance, batch: &CvPayload, result: RelevanceResult) {
    let entries: BTreeSet<&str> = batch.entries.iter().map(|e| e.id.as_str()).collect();
    let bullets: BTreeSet<&str> = batch
        .entries
        .iter()
        .flat_map(|e| e.bullets.iter().map(|b| b.id.as_str()))
        .collect();
    let skills: BTreeSet<&str> = batch.skills.iter().map(|s| s.id.as_str()).collect();
    let known =
        |set: &BTreeSet<&str>, id: &str| set.contains(id).then(|| Id::parse(id).ok()).flatten();
    for e in result.entries {
        if let Some(id) = known(&entries, &e.entry_id) {
            out.scores.entries.insert(id.clone(), e.score);
            out.reasons.insert(id.clone(), e.reason);
            out.matched.insert(id, e.matched_requirements);
        }
    }
    for b in result.bullets {
        if let Some(id) = known(&bullets, &b.bullet_id) {
            out.scores.bullets.insert(id.clone(), b.score);
            out.matched.insert(id, b.matched_requirements);
        }
    }
    for s in result.skills {
        if let Some(id) = known(&skills, &s.skill_id) {
            out.scores.skills.insert(id, s.score);
        }
    }
}

/// Rough token count: JSON characters / 4.
fn tokens(cv: &CvPayload) -> usize {
    serde_json::to_string(cv).map_or(0, |s| s.chars().count() / 4)
}

/// `cv` split into payloads within [`RELEVANCE_TOKEN_BUDGET`], by whole
/// entries in order. Skills, languages and the summary go with the first.
pub fn relevance_batches(cv: &CvPayload) -> Vec<CvPayload> {
    if tokens(cv) <= RELEVANCE_TOKEN_BUDGET {
        return vec![cv.clone()];
    }
    let mut batches = Vec::new();
    let mut current = CvPayload {
        entries: Vec::new(),
        ..cv.clone()
    };
    for entry in &cv.entries {
        current.entries.push(entry.clone());
        if current.entries.len() > 1 && tokens(&current) > RELEVANCE_TOKEN_BUDGET {
            let last = current.entries.pop();
            let next = CvPayload {
                entries: last.into_iter().collect(),
                summary: None,
                skills: Vec::new(),
                languages: Vec::new(),
                ..cv.clone()
            };
            batches.push(std::mem::replace(&mut current, next));
        }
    }
    batches.push(current);
    batches
}

fn prompt_error(e: prompts::PromptError) -> LlmError {
    LlmError::Provider(format!("the prompt could not be built ({e})"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FakeLlm;
    use crate::cache::MemoryCache;
    use crate::payload::{PayloadBullet, PayloadEntry, PayloadSkill};
    use serde_json::json;

    const E1: &str = "01J02000000000000000000011";
    const B1: &str = "01J02000000000000000000012";
    const S1: &str = "01J02000000000000000000013";

    fn payload(entries: usize, bullet_len: usize) -> CvPayload {
        CvPayload {
            language: "en".into(),
            name: None,
            location: None,
            headline: None,
            summary_id: crate::payload::SUMMARY_ID,
            summary: Some("Summary".into()),
            entries: (0..entries)
                .map(|i| PayloadEntry {
                    id: if i == 0 { E1.into() } else { format!("E{i}") },
                    section: "experience",
                    heading: None,
                    title: "Engineer".into(),
                    org: "Org".into(),
                    location: None,
                    period: None,
                    locked: false,
                    description: None,
                    bullets: vec![PayloadBullet {
                        id: if i == 0 { B1.into() } else { format!("B{i}") },
                        text: "x".repeat(bullet_len),
                    }],
                })
                .collect(),
            skills: vec![PayloadSkill {
                id: S1.into(),
                name: "Rust".into(),
            }],
            languages: vec![],
        }
    }

    fn requirements() -> serde_json::Value {
        json!({
            "title": "Engineer", "seniority": null, "language": "en",
            "must_have": [{ "id": "r1", "text": "Rust", "kind": "skill", "keywords": ["Rust"] }],
            "nice_to_have": [{ "id": "r1", "text": "Dup", "kind": "other", "keywords": [] }],
            "responsibilities": [], "keywords": ["Rust"], "domain": null
        })
    }

    fn ctx<'a>(llm: &'a FakeLlm, cache: &'a MemoryCache) -> StageContext<'a> {
        StageContext {
            llm,
            cache,
            model: "m",
            cancel: CancellationToken::new(),
        }
    }

    #[tokio::test]
    async fn job_parse_is_cached_and_dedupes_ids() {
        let llm = FakeLlm::new().reply("JobRequirements", requirements());
        let cache = MemoryCache::default();
        let req = parse_job(&ctx(&llm, &cache), "We need Rust")
            .await
            .expect("parse");
        assert_eq!(req.must_have.len(), 1);
        assert!(req.nice_to_have.is_empty(), "duplicate id r1 dropped");
        // Nothing is queued any more, so this answer comes from the cache.
        let again = parse_job(&ctx(&llm, &cache), "We need Rust")
            .await
            .expect("cached");
        assert_eq!(again, req);
        assert_eq!(llm.seen().len(), 1);
        assert_eq!(llm.seen()[0].schema_name, "JobRequirements");
    }

    #[tokio::test]
    async fn invalid_output_gets_one_retry_that_names_the_problem() {
        let llm = FakeLlm::new()
            .reply("JobRequirements", json!({ "title": 5 }))
            .reply("JobRequirements", requirements());
        let cache = MemoryCache::default();
        parse_job(&ctx(&llm, &cache), "jd")
            .await
            .expect("second try");
        let seen = llm.seen();
        assert_eq!(seen.len(), 2);
        assert!(seen[1].user.contains("Your last output was invalid"));

        let llm = FakeLlm::new()
            .reply("JobRequirements", json!({}))
            .reply("JobRequirements", json!({}));
        let err = parse_job(&ctx(&llm, &MemoryCache::default()), "jd").await;
        assert!(matches!(err, Err(LlmError::InvalidOutput(m)) if !m.contains('{')));
    }

    #[tokio::test]
    async fn relevance_keeps_known_ids_and_clamps_scores() {
        let llm = FakeLlm::new().reply(
            "RelevanceResult",
            json!({
                "entries": [
                    { "entry_id": E1, "score": 87.6, "matched_requirements": ["r1"], "reason": "Rust work." },
                    { "entry_id": "01J0200000000000000000ZZZZ", "score": 50, "matched_requirements": [], "reason": "?" }
                ],
                "bullets": [{ "bullet_id": B1, "score": 140, "matched_requirements": [] }],
                "skills": [{ "skill_id": S1, "score": -3 }]
            }),
        );
        let cache = MemoryCache::default();
        let req: JobRequirements = serde_json::from_value(requirements()).expect("req");
        let r = score_relevance(&ctx(&llm, &cache), &req, &payload(1, 10))
            .await
            .expect("scores");
        let id = |s: &str| Id::parse(s).expect("id");
        assert_eq!(r.scores.entries, BTreeMap::from([(id(E1), 88)]));
        assert_eq!(r.scores.bullets, BTreeMap::from([(id(B1), 100)]));
        assert_eq!(r.scores.skills, BTreeMap::from([(id(S1), 0)]));
        assert_eq!(r.reasons[&id(E1)], "Rust work.");
        assert_eq!(r.matched[&id(E1)], ["r1"]);
    }

    #[test]
    fn large_cvs_are_split_by_whole_entries() {
        let small = payload(3, 10);
        assert_eq!(relevance_batches(&small).len(), 1);
        // About 4,000 tokens per entry: three per batch.
        let big = payload(10, 16_000);
        let batches = relevance_batches(&big);
        assert!(batches.len() >= 3, "{}", batches.len());
        assert!(batches.iter().all(|b| tokens(b) <= RELEVANCE_TOKEN_BUDGET));
        let ids: Vec<_> = batches
            .iter()
            .flat_map(|b| b.entries.iter().map(|e| e.id.clone()))
            .collect();
        let all: Vec<_> = big.entries.iter().map(|e| e.id.clone()).collect();
        assert_eq!(ids, all);
        assert_eq!(batches[0].skills.len(), 1);
        assert!(
            batches[1..]
                .iter()
                .all(|b| b.skills.is_empty() && b.summary.is_none())
        );
    }

    #[tokio::test]
    async fn cancelled_stages_send_nothing() {
        let llm = FakeLlm::new().reply("JobRequirements", requirements());
        let cache = MemoryCache::default();
        let c = ctx(&llm, &cache);
        c.cancel.cancel();
        assert!(matches!(
            parse_job(&c, "jd").await,
            Err(LlmError::Cancelled)
        ));
        assert!(llm.seen().is_empty());
    }
}
