//! `ai_tailor` (spec section 11.13): stages A (job parse), B (relevance),
//! C (selection), D (verified rewrites), E (summary) and F (gap
//! questions), then fit-to-length when the variant has a page target. Runs as a job; the suggestions are saved on
//! the variant as `Pending` and nothing else in the variant changes.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use cv_ai::cache::{ResponseCache, sha256_hex};
use cv_ai::{
    LlmError, PayloadOptions, Rewrites, StageContext, build_cv_payload, find_gaps, parse_job,
    rewrite_bullets, score_relevance, summarize,
};
use cv_api::{AiStage, AiTailor, ApiError, DiscardedRewrite, ErrorCode, JobId, TailorResult};
use cv_core::fit::{FitResult, fit};
use cv_core::selection::plan_selection;
use cv_core::{
    Id, IdSource, ResolvedCv, ResolvedSection, Suggestion, SuggestionAction, SuggestionKind,
    SuggestionSource, SuggestionStatus, SuggestionTarget, Variant, validate,
};

use crate::review::{summary_suggestion, year_now};
use crate::{App, JobContext, JobEvents, Result, llm_error, store_error};

/// The `ai_cache` table as the stages' cache. A failing read is a miss and
/// a failing write is skipped: the provider is only asked again. Both run on
/// the blocking pool, like the job's other store work. Answers that arrive
/// after the cache was cleared or the data deleted are not written.
pub(crate) struct StoreCache {
    app: Arc<App>,
    generation: u64,
}

impl StoreCache {
    /// `generation` is [`App::ai_generation`] taken when the job was started,
    /// before it read anything it could send or cache.
    pub(crate) fn new(app: Arc<App>, generation: u64) -> Self {
        Self { app, generation }
    }
}

#[async_trait::async_trait]
impl ResponseCache for StoreCache {
    async fn get(&self, key: &str) -> Option<String> {
        let key = key.to_owned();
        self.app
            .blocking(move |app| Ok(app.store.ai_cache_get(&key).ok().flatten()))
            .await
            .ok()
            .flatten()
    }

    async fn put(&self, key: &str, kind: &str, response: &str) {
        let (key, kind, response) = (key.to_owned(), kind.to_owned(), response.to_owned());
        let generation = self.generation;
        let _ = self
            .app
            .blocking(move |app| {
                let _writes = app.ai_writes();
                if app.ai_generation() == generation {
                    let _ = app.store.ai_cache_put(&key, &kind, &response);
                }
                Ok(())
            })
            .await;
    }
}

impl App {
    /// Starts tailoring the saved variant to its saved job description and
    /// returns the job's ID and the future to spawn. Fails at once, without
    /// a job, when AI is not set up or the variant has no job description.
    pub fn ai_tailor(
        self: &Arc<Self>,
        events: Arc<dyn JobEvents>,
        req: AiTailor,
    ) -> Result<(JobId, impl Future<Output = ()> + Send + 'static)> {
        let generation = self.ai_generation();
        self.llm()?;
        let stored = self.get_variant(&req.variant_id)?;
        if stored.variant.job.description_text.trim().is_empty() {
            return Err(ApiError::new(
                ErrorCode::Validation,
                "Paste the job description first.",
            ));
        }
        if req
            .options
            .page_target
            .is_some_and(|p| !(1..=2).contains(&p))
        {
            return Err(ApiError::new(
                ErrorCode::Validation,
                "Choose 1 or 2 pages to fit to.",
            ));
        }
        Ok(self.start_job(events, move |app, ctx| async move {
            let result = app.tailor(&ctx, req, generation).await?;
            serde_json::to_value(result).map_err(|_| {
                ApiError::new(
                    ErrorCode::Internal,
                    "The tailoring result could not be sent.",
                )
            })
        }))
    }

    async fn tailor(
        self: &Arc<Self>,
        ctx: &JobContext,
        req: AiTailor,
        generation: u64,
    ) -> Result<TailorResult> {
        // Settings, keychain and database reads run on the blocking pool;
        // only the provider calls run on the async runtime (as in ADR-069).
        let variant_id = req.variant_id.clone();
        let (llm, status, stored) = self
            .blocking(move |app| Ok((app.llm()?, app.ai_status()?, app.get_variant(&variant_id)?)))
            .await?;
        let model = status.model.clone();
        let job = stored.variant.job.clone();
        let cache = StoreCache::new(Arc::clone(self), generation);
        let stage = StageContext {
            llm: llm.as_ref(),
            cache: &cache,
            model: &model,
            cancel: ctx.cancel.clone(),
        };

        // A: the parse saved on the variant is reused while its text is unchanged.
        ctx.progress(AiStage::JobParse, 0);
        let hash = sha256_hex(&job.description_text);
        let requirements = match &job.requirements {
            Some(r) if job.description_hash.as_deref() == Some(hash.as_str()) => r.clone(),
            _ => parse_job(&stage, &job.description_text)
                .await
                .map_err(llm_error)?,
        };

        // B: scores for every item of the base CV, hidden ones included.
        ctx.progress(AiStage::Relevance, 10);
        let cv_id = stored.variant.cv_id.clone();
        let mut cv = self.blocking(move |app| app.get_cv(&cv_id)).await?.cv;
        // Score and count pages as a save would render them.
        let _ = validate::for_save(&mut cv);
        let base = cv.resolve(None);
        let opts = PayloadOptions {
            send_name: status.send_name,
            send_location: status.send_location,
        };
        let payload = build_cv_payload(&base, opts);
        let relevance = score_relevance(&stage, &requirements, &payload)
            .await
            .map_err(llm_error)?;

        // C: hide, collapse and order suggestions.
        ctx.progress(AiStage::Selection, 40);
        let mut selection = self.with_ids(|ids| {
            plan_selection(
                &base,
                &relevance.scores,
                &relevance.reasons,
                &req.options,
                ids,
            )
        });
        ctx.check()?;

        // D: rewrites of the kept, relevant bullets the user has not edited
        // in this variant. Optional: a failure keeps the other suggestions.
        ctx.progress(AiStage::Rewrite, 50);
        let mut skip: BTreeSet<Id> = stored.variant.text_overrides.keys().cloned().collect();
        skip.extend(stored.variant.hidden.iter().cloned());
        skip.extend(stored.variant.collapsed.iter().cloned());
        skip.extend(hidden_by(&selection));
        let mut failed_stages = Vec::new();
        let mut discarded = Vec::new();
        match rewrite_bullets(&stage, &requirements, &payload, &relevance.scores, &skip).await {
            Ok(rewrites) => {
                let (kept, dropped) = rewrite_suggestions(&base, rewrites);
                selection.extend(self.with_ids(|ids| {
                    kept.into_iter()
                        .map(|r| r.into_suggestion(SuggestionSource::Tailor, ids))
                        .collect::<Vec<_>>()
                }));
                discarded = dropped;
            }
            Err(LlmError::Cancelled) => return Err(llm_error(LlmError::Cancelled)),
            Err(_) => failed_stages.push(AiStage::Rewrite),
        }
        ctx.check()?;

        // E: a tailored summary, unless this variant already has its own.
        // Optional, like D.
        if stored.variant.summary_override.is_none() {
            ctx.progress(AiStage::Summary, 60);
            match summarize(&stage, &requirements, &payload, year_now()).await {
                Ok(Some(summary)) => {
                    let before = base.summary.as_deref();
                    match self.with_ids(|ids| {
                        summary_suggestion(summary, before, SuggestionSource::Tailor, ids)
                    }) {
                        Ok(s) => selection.push(s),
                        Err(d) => discarded.push(d),
                    }
                }
                Ok(None) => {}
                Err(LlmError::Cancelled) => return Err(llm_error(LlmError::Cancelled)),
                Err(_) => failed_stages.push(AiStage::Summary),
            }
            ctx.check()?;
        }

        // F: questions for the requirements the CV does not clearly show.
        // A failure keeps the questions of the last run.
        ctx.progress(AiStage::Gaps, 70);
        let gaps = match find_gaps(&stage, &requirements, &relevance, &payload).await {
            Ok(gaps) => Some(gaps),
            Err(LlmError::Cancelled) => return Err(llm_error(LlmError::Cancelled)),
            Err(_) => {
                failed_stages.push(AiStage::Gaps);
                None
            }
        };
        ctx.check()?;

        // Fit to length on top of the selection, as if it were accepted;
        // suggestions the user already decided are left out, as the save
        // below leaves them out.
        let fitted = match req.options.page_target {
            Some(target) => {
                ctx.progress(AiStage::Fit, 80);
                let mut trial = stored.variant.clone();
                apply(&mut trial, &stored.variant.undecided(&selection));
                // Count pages with the photo when the variant shows it, as
                // the PDF export draws them.
                let resolved = cv.resolve(Some(&trial));
                let (photo, signature) = self
                    .blocking(move |app| {
                        Ok((app.photo_for(&resolved)?, app.signature_for(&resolved)?))
                    })
                    .await?;
                let app = Arc::clone(self);
                let scores = relevance.scores.clone();
                let result = tokio::task::spawn_blocking(move || {
                    let pages = crate::PhotoPages {
                        renderer: &app.renderer,
                        photo,
                        signature,
                    };
                    app.with_ids(|ids| fit(&cv, &trial, Some(&scores), target, &pages, ids))
                })
                .await
                .map_err(|_| ApiError::new(ErrorCode::Internal, "Fitting stopped unexpectedly."))?
                .map_err(|e| {
                    ApiError::new(
                        ErrorCode::Render,
                        "The CV could not be rendered to count its pages.",
                    )
                    .with_details(serde_json::json!({ "error": e.0 }))
                })?;
                Some(result)
            }
            None => None,
        };
        ctx.check()?;
        ctx.progress(AiStage::Selection, 95);

        // Save on the latest revision: the user may have edited meanwhile.
        let (pages, fits) = match &fitted {
            Some(FitResult::Fits { pages, .. }) => (Some(*pages), Some(true)),
            Some(FitResult::CannotFit { pages }) => (Some(*pages), Some(false)),
            None => (None, None),
        };
        let cancel = ctx.clone();
        let variant_id = req.variant_id.clone();
        let target = req.options.page_target;
        let (saved, count) = self
            .blocking(move |app| {
                let mut attempts = 0;
                loop {
                    attempts += 1;
                    let latest = app.get_variant(&variant_id)?;
                    let mut variant = latest.variant;
                    if variant.job.description_text == job.description_text {
                        variant.job.requirements = Some(requirements.clone());
                        variant.job.description_hash = Some(hash.clone());
                    }
                    let mut count =
                        variant.replace_pending(SuggestionSource::Tailor, selection.clone());
                    if let Some(gaps) = &gaps {
                        variant.replace_gaps(gaps.clone());
                    }
                    if let Some(FitResult::Fits { cuts, .. }) = &fitted {
                        variant.page_target = target;
                        count += cuts.len();
                        variant.replace_fit_cuts(cuts.clone());
                    }
                    match app.store.save_variant(&variant, latest.revision) {
                        Ok(saved) => return Ok((saved, count)),
                        // Someone saved in between: merge onto their version.
                        Err(cv_store::StoreError::Conflict { .. }) if attempts < 3 => {
                            cancel.check()?
                        }
                        Err(e) => return Err(store_error(e)),
                    }
                }
            })
            .await?;
        ctx.progress(AiStage::Selection, 100);
        Ok(TailorResult {
            variant: saved,
            suggestions: count,
            pages,
            fits,
            discarded,
            failed_stages,
        })
    }
}

/// The entries and bullets `selection` hides or collapses.
fn hidden_by(selection: &[Suggestion]) -> impl Iterator<Item = Id> + '_ {
    selection
        .iter()
        .filter_map(|s| match (&s.target, &s.action) {
            (
                SuggestionTarget::Entry { id } | SuggestionTarget::Bullet { id },
                Some(SuggestionAction::Hide | SuggestionAction::Collapse),
            ) => Some(id.clone()),
            _ => None,
        })
}

/// A kept rewrite whose original is still the bullet's text.
pub(crate) struct KeptRewrite(cv_ai::CheckedRewrite);

impl KeptRewrite {
    pub(crate) fn into_suggestion(
        self,
        source: SuggestionSource,
        ids: &mut dyn IdSource,
    ) -> Suggestion {
        let r = self.0;
        Suggestion {
            id: ids.next_id(),
            kind: SuggestionKind::Rewrite,
            target: SuggestionTarget::Bullet { id: r.bullet_id },
            before: Some(r.before),
            after: Some(r.after),
            action: None,
            reason: r.reason,
            confidence: 0.6,
            warnings: r.warnings,
            status: SuggestionStatus::Pending,
            source,
        }
    }
}

/// Stage D's rewrites as suggestions and discarded details. A rewrite whose
/// original is not the bullet's current text (it was redacted or changed)
/// is dropped without a trace: it could not be applied as reviewed.
pub(crate) fn rewrite_suggestions(
    base: &ResolvedCv,
    rewrites: Rewrites,
) -> (Vec<KeptRewrite>, Vec<DiscardedRewrite>) {
    let mut texts: BTreeMap<&Id, &str> = BTreeMap::new();
    for section in &base.sections {
        let entries = match section {
            ResolvedSection::Experience(s)
            | ResolvedSection::Education(s)
            | ResolvedSection::Projects(s)
            | ResolvedSection::Certifications(s)
            | ResolvedSection::Custom(s) => &s.entries,
            ResolvedSection::Skills(_) | ResolvedSection::Languages(_) => continue,
        };
        for b in entries.iter().flat_map(|e| &e.bullets) {
            texts.insert(&b.id, b.text.as_str());
        }
    }
    let current = |r: &cv_ai::CheckedRewrite| texts.get(&r.bullet_id) == Some(&r.before.as_str());
    let kept = rewrites
        .kept
        .into_iter()
        .filter(|r| current(r))
        .map(KeptRewrite)
        .collect();
    let discarded = rewrites
        .discarded
        .into_iter()
        .filter(|r| current(r))
        .map(|r| DiscardedRewrite {
            bullet_id: Some(r.bullet_id),
            before: r.before,
            after: r.after,
            warnings: r.warnings,
        })
        .collect();
    (kept, discarded)
}

impl App {
    /// Runs `f` on the blocking pool, for store and keychain work inside a job.
    pub(crate) async fn blocking<T, F>(self: &Arc<Self>, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&App) -> Result<T> + Send + 'static,
    {
        let app = Arc::clone(self);
        tokio::task::spawn_blocking(move || f(&app))
            .await
            .map_err(|_| ApiError::new(ErrorCode::Internal, "The AI job stopped unexpectedly."))?
    }
}

/// Applies the hide and collapse suggestions to a trial copy.
fn apply(trial: &mut Variant, suggestions: &[cv_core::Suggestion]) {
    for s in suggestions {
        let id = match &s.target {
            SuggestionTarget::Bullet { id }
            | SuggestionTarget::Entry { id }
            | SuggestionTarget::Section { id } => id.clone(),
            SuggestionTarget::SkillGroup { .. }
            | SuggestionTarget::Summary
            | SuggestionTarget::Skills => continue,
        };
        match s.action {
            Some(SuggestionAction::Hide) => {
                trial.hidden.insert(id);
            }
            Some(SuggestionAction::Collapse) => {
                trial.collapsed.insert(id);
            }
            _ => {}
        }
    }
}
