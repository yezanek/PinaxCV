//! `ai_tailor` (spec section 11.13, M7a): stages A (job parse), B
//! (relevance) and C (selection), then fit-to-length when the variant has a
//! page target. Runs as a job; the suggestions are saved on the variant as
//! `Pending` and nothing else in the variant changes.

use std::sync::Arc;

use cv_ai::cache::{ResponseCache, sha256_hex};
use cv_ai::{PayloadOptions, StageContext, build_cv_payload, parse_job, score_relevance};
use cv_api::{AiStage, AiTailor, ApiError, ErrorCode, JobId, TailorResult};
use cv_core::fit::{FitResult, fit};
use cv_core::selection::plan_selection;
use cv_core::{
    SuggestionAction, SuggestionSource, SuggestionStatus, SuggestionTarget, Variant, validate,
};
use cv_store::Store;

use crate::{App, JobContext, JobEvents, Result, llm_error, store_error};

/// The `ai_cache` table as the stages' cache. A failing read is a miss and
/// a failing write is skipped: the provider is only asked again.
struct StoreCache<'a>(&'a dyn Store);

impl ResponseCache for StoreCache<'_> {
    fn get(&self, key: &str) -> Option<String> {
        self.0.ai_cache_get(key).ok().flatten()
    }

    fn put(&self, key: &str, kind: &str, response: &str) {
        let _ = self.0.ai_cache_put(key, kind, response);
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
            let result = app.tailor(&ctx, req).await?;
            serde_json::to_value(result).map_err(|_| {
                ApiError::new(
                    ErrorCode::Internal,
                    "The tailoring result could not be sent.",
                )
            })
        }))
    }

    async fn tailor(self: &Arc<Self>, ctx: &JobContext, req: AiTailor) -> Result<TailorResult> {
        let llm = self.llm()?;
        let status = self.ai_status()?;
        let model = self.ai_model()?;
        let stored = self.get_variant(&req.variant_id)?;
        let job = stored.variant.job.clone();
        let stage = StageContext {
            llm: llm.as_ref(),
            cache: &StoreCache(self.store.as_ref()),
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
        let mut cv = self.get_cv(&stored.variant.cv_id)?.cv;
        // Score and count pages as a save would render them.
        let _ = validate::for_save(&mut cv);
        let base = cv.resolve(None);
        let opts = PayloadOptions {
            send_name: status.send_name,
            send_location: status.send_location,
        };
        let relevance = score_relevance(&stage, &requirements, &build_cv_payload(&base, opts))
            .await
            .map_err(llm_error)?;

        // C: hide, collapse and order suggestions.
        ctx.progress(AiStage::Selection, 40);
        let selection = self.with_ids(|ids| {
            plan_selection(
                &base,
                &relevance.scores,
                &relevance.reasons,
                &req.options,
                ids,
            )
        });
        ctx.check()?;

        // Fit to length on top of the selection, as if it were accepted.
        let fitted = match req.options.page_target {
            Some(target) => {
                ctx.progress(AiStage::Fit, 50);
                let mut trial = stored.variant.clone();
                apply(&mut trial, &selection);
                let app = Arc::clone(self);
                let scores = relevance.scores.clone();
                let result = tokio::task::spawn_blocking(move || {
                    app.with_ids(|ids| fit(&cv, &trial, Some(&scores), target, &app.renderer, ids))
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
        let mut count = selection.len();
        let (pages, fits) = match &fitted {
            Some(FitResult::Fits { pages, cuts }) => {
                count += cuts.len();
                (Some(*pages), Some(true))
            }
            Some(FitResult::CannotFit { pages }) => (Some(*pages), Some(false)),
            None => (None, None),
        };
        let mut attempts = 0;
        let saved = loop {
            attempts += 1;
            let latest = self.get_variant(&req.variant_id)?;
            let mut variant = latest.variant;
            if variant.job.description_text == job.description_text {
                variant.job.requirements = Some(requirements.clone());
                variant.job.description_hash = Some(hash.clone());
            }
            replace_pending(&mut variant, SuggestionSource::Tailor, selection.clone());
            if let Some(FitResult::Fits { cuts, .. }) = &fitted {
                variant.page_target = req.options.page_target;
                variant.replace_fit_cuts(cuts.clone());
            }
            match self.store.save_variant(&variant, latest.revision) {
                Ok(saved) => break saved,
                // Someone saved in between: merge onto their version.
                Err(cv_store::StoreError::Conflict { .. }) if attempts < 3 => ctx.check()?,
                Err(e) => return Err(store_error(e)),
            }
        };
        ctx.progress(AiStage::Selection, 100);
        Ok(TailorResult {
            variant: saved,
            suggestions: count,
            pages,
            fits,
        })
    }
}

/// Replaces the pending suggestions from `source` with `new`; accepted and
/// rejected ones stay, so a decision is never undone by a rerun.
fn replace_pending(variant: &mut Variant, source: SuggestionSource, new: Vec<cv_core::Suggestion>) {
    variant
        .suggestions
        .retain(|s| !(s.source == source && s.status == SuggestionStatus::Pending));
    variant.suggestions.extend(new);
}

/// Applies the hide and collapse suggestions to a trial copy.
fn apply(trial: &mut Variant, suggestions: &[cv_core::Suggestion]) {
    for s in suggestions {
        let id = match &s.target {
            SuggestionTarget::Bullet { id }
            | SuggestionTarget::Entry { id }
            | SuggestionTarget::Section { id } => id.clone(),
            SuggestionTarget::Summary | SuggestionTarget::Skills => continue,
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
