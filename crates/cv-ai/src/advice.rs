//! Stages E (summary), F (gaps) and G (review without a job), spec
//! sections 11.9 to 11.11. Every proposed text goes through `verify`
//! before anyone sees it; gaps are questions, never CV content.

use std::collections::BTreeSet;

use cv_core::fit::RelevanceScores;
use cv_core::{
    GapQuestion, GapSeverity, Id, JobRequirements, LintIssue, Requirement, VerificationWarning,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::LlmError;
use crate::payload::{CvPayload, REDACTED};
use crate::prompts;
use crate::stages::{Relevance, Rewrite, Rewrites, StageContext, check_rewrites, prompt_error};
use crate::verify::{self, Verdict};

/// Stage E's answer (spec section 11.9).
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct SummaryResult {
    pub summary: String,
    pub used_entry_ids: Vec<String>,
}

/// A proposed summary after verification.
#[derive(Clone, Debug, PartialEq)]
pub struct CheckedSummary {
    pub after: String,
    pub warnings: Vec<VerificationWarning>,
    /// An error check failed: show it only as discarded.
    pub dropped: bool,
}

/// Every text of `cv` the summary may draw facts from.
fn cv_text(cv: &CvPayload) -> String {
    let mut parts: Vec<&str> = Vec::new();
    parts.extend(cv.headline.as_deref());
    parts.extend(cv.summary.as_deref());
    for e in &cv.entries {
        parts.extend([e.title.as_str(), e.org.as_str()]);
        parts.extend(e.heading.as_deref());
        parts.extend(e.location.as_deref());
        parts.extend(e.period.as_deref());
        parts.extend(e.description.as_deref());
        parts.extend(e.bullets.iter().map(|b| b.text.as_str()));
    }
    parts.extend(cv.skills.iter().map(|s| s.name.as_str()));
    parts.extend(cv.languages.iter().map(|l| l.name.as_str()));
    parts.join("\n")
}

/// The longest experience span the periods of `cv` allow, in whole years
/// rounded up, with `year_now` for "present".
pub fn experience_years(cv: &CvPayload, year_now: i32) -> Option<u32> {
    let year = |s: &str| -> Option<i32> {
        if s == "present" {
            Some(year_now)
        } else {
            s.get(..4)?.parse().ok()
        }
    };
    let mut first: Option<i32> = None;
    let mut last: Option<i32> = None;
    for p in cv
        .entries
        .iter()
        .filter(|e| e.section == "experience")
        .filter_map(|e| e.period.as_deref())
    {
        let mut ends = p.split('/');
        let start = ends.next().and_then(year);
        let end = ends.next().and_then(year).or(start);
        first = match (first, start) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        last = match (last, end) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
    }
    let span = last? - first?;
    u32::try_from(span + 1).ok()
}

fn check_summary(cv: &CvPayload, text: &str, year_now: i32) -> Option<CheckedSummary> {
    let after = text.trim().to_owned();
    let years = experience_years(cv, year_now);
    match verify::verify_summary(&after, cv.summary.as_deref(), &cv_text(cv), years) {
        Verdict::Keep(warnings) => Some(CheckedSummary {
            after,
            warnings,
            dropped: false,
        }),
        Verdict::Drop(warnings) => Some(CheckedSummary {
            after,
            warnings,
            dropped: true,
        }),
        Verdict::Same => None,
    }
}

/// Stage E: a summary tailored to `requirements`, from `cv` only. `None`
/// when the model proposed the current summary again. A CV whose summary
/// was redacted gets none: the check could not see what it replaces.
pub async fn summarize(
    ctx: &StageContext<'_>,
    requirements: &JobRequirements,
    cv: &CvPayload,
    year_now: i32,
) -> Result<Option<CheckedSummary>, LlmError> {
    if cv.summary.as_deref().is_some_and(|s| s.contains(REDACTED)) {
        return Ok(None);
    }
    let prompt = prompts::summary(requirements, cv).map_err(prompt_error)?;
    let result: SummaryResult = ctx.generate(prompt, "SummaryResult").await?;
    Ok(check_summary(cv, &result.summary, year_now))
}

/// Items scoring below this only weakly show a requirement.
pub const WEAK_MATCH_SCORE: u8 = 50;

/// Stage F's answer (spec section 11.10).
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct GapResult {
    pub gaps: Vec<Gap>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct Gap {
    pub requirement_id: String,
    pub severity: GapSeverity,
    pub question: String,
    pub where_to_add_hint: String,
}

/// The requirements nothing in the CV matches, or only items scoring
/// below [`WEAK_MATCH_SCORE`]; must-haves first.
pub fn unmatched(
    requirements: &JobRequirements,
    relevance: &Relevance,
) -> Vec<(Requirement, bool, GapSeverity)> {
    let score = |id: &Id, s: &RelevanceScores| {
        s.entries
            .get(id)
            .or_else(|| s.bullets.get(id))
            .or_else(|| s.skills.get(id))
            .copied()
            .unwrap_or(0)
    };
    let best = |rid: &str| {
        relevance
            .matched
            .iter()
            .filter(|(_, reqs)| reqs.iter().any(|r| r == rid))
            .map(|(id, _)| score(id, &relevance.scores))
            .max()
    };
    let mut out = Vec::new();
    for (list, must) in [
        (&requirements.must_have, true),
        (&requirements.nice_to_have, false),
    ] {
        for r in list {
            match best(&r.id) {
                None => out.push((r.clone(), must, GapSeverity::Missing)),
                Some(s) if s < WEAK_MATCH_SCORE => out.push((r.clone(), must, GapSeverity::Weak)),
                Some(_) => {}
            }
        }
    }
    out
}

/// Stage F: one question per requirement the CV does not clearly show.
/// Nothing is sent when every requirement is matched. Answers for other
/// requirements, repeated answers and empty questions are dropped; the
/// order and severity are ours, not the model's.
pub async fn find_gaps(
    ctx: &StageContext<'_>,
    requirements: &JobRequirements,
    relevance: &Relevance,
    cv: &CvPayload,
) -> Result<Vec<GapQuestion>, LlmError> {
    let open = unmatched(requirements, relevance);
    if open.is_empty() {
        return Ok(Vec::new());
    }
    let list: Vec<Requirement> = open.iter().map(|(r, ..)| r.clone()).collect();
    let prompt = prompts::gaps(requirements, &list, cv).map_err(prompt_error)?;
    let result: GapResult = ctx.generate(prompt, "GapResult").await?;
    let mut out = Vec::new();
    for (r, must, severity) in open {
        let Some(g) = result
            .gaps
            .iter()
            .find(|g| g.requirement_id == r.id && !g.question.trim().is_empty())
        else {
            continue;
        };
        out.push(GapQuestion {
            requirement_id: r.id,
            requirement: r.text,
            must_have: must,
            severity,
            question: g.question.trim().to_owned(),
            where_to_add_hint: g.where_to_add_hint.trim().to_owned(),
            dismissed: false,
        });
    }
    Ok(out)
}

/// Stage G's answer (spec section 11.11).
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ReviewResult {
    pub rewrites: Vec<Rewrite>,
    pub notes: Vec<ReviewNote>,
    pub summary_suggestion: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ReviewNote {
    /// An entry or bullet ID, or none for the CV as a whole.
    pub target_id: Option<String>,
    pub category: NoteCategory,
    pub message: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteCategory {
    Impact,
    Clarity,
    Structure,
    Consistency,
}

/// Most rewrites and notes kept from a review (spec section 11.11).
pub const REVIEW_MAX_REWRITES: usize = 12;
pub const REVIEW_MAX_NOTES: usize = 8;

/// A note with a target the CV has.
#[derive(Clone, Debug, PartialEq)]
pub struct CheckedNote {
    pub target: Option<Id>,
    pub category: NoteCategory,
    pub message: String,
}

/// Stage G after verification.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Review {
    pub rewrites: Rewrites,
    pub notes: Vec<CheckedNote>,
    pub summary: Option<CheckedSummary>,
}

/// Stage G: a review of `cv` without a job. `lint` goes along so the model
/// does not repeat it. Rewrites get the stage D checks (with no job, so
/// every new term is a warning), the summary the stage E checks; redacted
/// bullets are not rewritten.
pub async fn review_cv(
    ctx: &StageContext<'_>,
    lint: &[LintIssue],
    cv: &CvPayload,
    year_now: i32,
) -> Result<Review, LlmError> {
    let prompt = prompts::review(lint, cv).map_err(prompt_error)?;
    let result: ReviewResult = ctx.generate(prompt, "ReviewResult").await?;
    let skills: Vec<&str> = cv.skills.iter().map(|s| s.name.as_str()).collect();
    let mut out = Review::default();
    let mut answered: Vec<Rewrite> = result.rewrites;
    answered.truncate(REVIEW_MAX_REWRITES);
    for entry in &cv.entries {
        let batch: Vec<(Id, &str)> = entry
            .bullets
            .iter()
            .filter(|b| !b.text.contains(REDACTED))
            .filter_map(|b| Some((Id::parse(&b.id).ok()?, b.text.as_str())))
            .collect();
        let mine: Vec<Rewrite> = answered
            .iter()
            .filter(|r| batch.iter().any(|(id, _)| id.as_str() == r.bullet_id))
            .cloned()
            .collect();
        check_rewrites(entry, &batch, mine, &skills, &[], &mut out.rewrites);
    }
    let known: BTreeSet<&str> = cv
        .entries
        .iter()
        .flat_map(|e| std::iter::once(e.id.as_str()).chain(e.bullets.iter().map(|b| b.id.as_str())))
        .collect();
    out.notes = result
        .notes
        .into_iter()
        .filter(|n| !n.message.trim().is_empty())
        .take(REVIEW_MAX_NOTES)
        .map(|n| CheckedNote {
            target: n
                .target_id
                .as_deref()
                .filter(|t| known.contains(t))
                .and_then(|t| Id::parse(t).ok()),
            category: n.category,
            message: n.message.trim().to_owned(),
        })
        .collect();
    let summary_redacted = cv.summary.as_deref().is_some_and(|s| s.contains(REDACTED));
    if !summary_redacted && let Some(text) = result.summary_suggestion {
        out.summary = check_summary(cv, &text, year_now);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FakeLlm;
    use crate::cache::MemoryCache;
    use crate::payload::{PayloadBullet, PayloadEntry, PayloadSkill, SUMMARY_ID};
    use serde_json::json;
    use tokio_util::sync::CancellationToken;

    const E1: &str = "01J02000000000000000000021";
    const B1: &str = "01J02000000000000000000022";
    const B2: &str = "01J02000000000000000000023";
    const S1: &str = "01J02000000000000000000024";

    fn id(s: &str) -> Id {
        Id::parse(s).expect("id")
    }

    fn cv() -> CvPayload {
        CvPayload {
            language: "en".into(),
            name: None,
            location: None,
            headline: Some("Backend Engineer".into()),
            summary_id: SUMMARY_ID,
            summary: Some("Backend engineer.".into()),
            entries: vec![PayloadEntry {
                id: E1.into(),
                section: "experience",
                heading: None,
                title: "Backend Engineer".into(),
                org: "Acme".into(),
                location: None,
                period: Some("2019-03/present".into()),
                locked: false,
                description: None,
                bullets: vec![
                    PayloadBullet {
                        id: B1.into(),
                        text: "Led the migration of the billing platform to Rust for four teams"
                            .into(),
                    },
                    PayloadBullet {
                        id: B2.into(),
                        text: "Wrote the on-call runbooks for the payments team".into(),
                    },
                ],
            }],
            skills: vec![PayloadSkill {
                id: S1.into(),
                name: "Rust".into(),
            }],
            languages: vec![],
        }
    }

    fn requirements() -> JobRequirements {
        serde_json::from_value(json!({
            "title": "Engineer", "seniority": null, "language": "en",
            "must_have": [
                { "id": "r1", "text": "Rust", "kind": "skill", "keywords": ["Rust"] },
                { "id": "r2", "text": "Kubernetes", "kind": "skill", "keywords": ["Kubernetes"] }
            ],
            "nice_to_have": [{ "id": "r3", "text": "On-call", "kind": "other", "keywords": [] }],
            "responsibilities": [], "keywords": ["Rust"], "domain": null
        }))
        .expect("req")
    }

    fn ctx<'a>(llm: &'a FakeLlm, cache: &'a MemoryCache) -> StageContext<'a> {
        StageContext {
            llm,
            cache,
            model: "m",
            cancel: CancellationToken::new(),
        }
    }

    #[test]
    fn experience_years_span_the_periods() {
        assert_eq!(experience_years(&cv(), 2026), Some(8));
        let mut none = cv();
        none.entries[0].period = None;
        assert_eq!(experience_years(&none, 2026), None);
    }

    #[tokio::test]
    async fn summaries_are_verified_against_the_cv() {
        let llm = FakeLlm::new()
            .reply(
                "SummaryResult",
                json!({ "summary": "Backend engineer with 7 years of Rust, who led a billing migration for four teams.", "used_entry_ids": [E1] }),
            );
        let cache = MemoryCache::default();
        let s = summarize(&ctx(&llm, &cache), &requirements(), &cv(), 2026)
            .await
            .expect("summary")
            .expect("new");
        assert!(!s.dropped && s.warnings.is_empty(), "{s:?}");

        let llm = FakeLlm::new().reply(
            "SummaryResult",
            json!({ "summary": "Backend engineer who cut costs by 30%.", "used_entry_ids": [] }),
        );
        let s = summarize(
            &ctx(&llm, &MemoryCache::default()),
            &requirements(),
            &cv(),
            2026,
        )
        .await
        .expect("summary")
        .expect("new");
        assert!(s.dropped);

        // A redacted summary is not replaced.
        let mut redacted = cv();
        redacted.summary = Some(format!("Call {REDACTED}"));
        let llm = FakeLlm::new();
        let s = summarize(&ctx(&llm, &cache), &requirements(), &redacted, 2026)
            .await
            .expect("none");
        assert_eq!(s, None);
        assert!(llm.seen().is_empty());
    }

    fn relevance() -> Relevance {
        let mut r = Relevance::default();
        r.scores.entries.insert(id(E1), 80);
        r.scores.bullets.insert(id(B2), 30);
        r.matched.insert(id(E1), vec!["r1".into()]);
        r.matched.insert(id(B2), vec!["r3".into()]);
        r
    }

    #[test]
    fn unmatched_requirements_are_missing_or_weak() {
        let open = unmatched(&requirements(), &relevance());
        let got: Vec<_> = open
            .iter()
            .map(|(r, must, sev)| (r.id.as_str(), *must, *sev))
            .collect();
        assert_eq!(
            got,
            [
                ("r2", true, GapSeverity::Missing),
                ("r3", false, GapSeverity::Weak)
            ]
        );
    }

    #[tokio::test]
    async fn gaps_are_questions_for_unmatched_requirements_only() {
        let llm = FakeLlm::new().reply(
            "GapResult",
            json!({ "gaps": [
                { "requirement_id": "r3", "severity": "missing", "question": "Have you been on call?", "where_to_add_hint": "your most recent role" },
                { "requirement_id": "r1", "severity": "missing", "question": "Rust?", "where_to_add_hint": "" },
                { "requirement_id": "r2", "severity": "weak", "question": " The job asks for Kubernetes. Have you used it? ", "where_to_add_hint": "the skills section" }
            ]}),
        );
        let cache = MemoryCache::default();
        let gaps = find_gaps(&ctx(&llm, &cache), &requirements(), &relevance(), &cv())
            .await
            .expect("gaps");
        let got: Vec<_> = gaps
            .iter()
            .map(|g| (g.requirement_id.as_str(), g.severity, g.question.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                (
                    "r2",
                    GapSeverity::Missing,
                    "The job asks for Kubernetes. Have you used it?"
                ),
                ("r3", GapSeverity::Weak, "Have you been on call?"),
            ]
        );
        assert_eq!(gaps[0].requirement, "Kubernetes");
        assert!(gaps[0].must_have && !gaps[1].must_have);

        // Everything matched: nothing is sent.
        let mut all = relevance();
        all.matched.insert(id(B1), vec!["r2".into(), "r3".into()]);
        all.scores.bullets.insert(id(B1), 90);
        let llm = FakeLlm::new();
        let gaps = find_gaps(&ctx(&llm, &cache), &requirements(), &all, &cv())
            .await
            .expect("none");
        assert!(gaps.is_empty() && llm.seen().is_empty());
    }

    #[tokio::test]
    async fn reviews_verify_rewrites_and_keep_known_note_targets() {
        let llm = FakeLlm::new().reply(
            "ReviewResult",
            json!({
                "rewrites": [
                    { "bullet_id": B1, "new_text": "Migrated the billing platform to Rust for four teams", "change_type": "Tighten", "reason": "Shorter." },
                    { "bullet_id": B2, "new_text": "Wrote on-call runbooks for 12 payments engineers", "change_type": "Impact", "reason": "Scope." },
                    { "bullet_id": "01J0200000000000000000ZZZZ", "new_text": "Unknown", "change_type": "Clarity", "reason": "" }
                ],
                "notes": [
                    { "target_id": B2, "category": "Impact", "message": " If you know how many incidents the runbooks covered, add it. " },
                    { "target_id": "nope", "category": "Consistency", "message": "Use one tense." },
                    { "target_id": null, "category": "Structure", "message": "  " }
                ],
                "summary_suggestion": "Backend engineer who led a billing migration to Rust for four teams."
            }),
        );
        let cache = MemoryCache::default();
        let r = review_cv(&ctx(&llm, &cache), &[], &cv(), 2026)
            .await
            .expect("review");
        assert_eq!(r.rewrites.kept.len(), 1);
        assert_eq!(r.rewrites.kept[0].bullet_id, id(B1));
        assert_eq!(r.rewrites.discarded.len(), 1);
        let notes: Vec<_> = r
            .notes
            .iter()
            .map(|n| (n.target.clone(), n.category, n.message.as_str()))
            .collect();
        assert_eq!(
            notes,
            [
                (
                    Some(id(B2)),
                    NoteCategory::Impact,
                    "If you know how many incidents the runbooks covered, add it."
                ),
                (None, NoteCategory::Consistency, "Use one tense."),
            ]
        );
        let s = r.summary.expect("summary");
        assert!(!s.dropped);
    }
}
