//! `ai_review` (spec section 11.11): stage G on the base CV, without a job
//! or a variant. The latest review is kept per CV in `review_sessions`;
//! accepting one of its suggestions changes the base CV.

use std::sync::{Arc, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use cv_ai::{
    CheckedSummary, NoteCategory, PayloadOptions, StageContext, build_cv_payload_redacting,
};
use cv_api::{
    AiReview, AiStage, ApiError, DiscardedRewrite, ErrorCode, JobId, ReviewDecide, ReviewDecided,
    ReviewNote, ReviewNoteCategory, ReviewSession, SaveCv, Target,
};
use cv_core::{
    Id, IdSource, SnapshotReason, Suggestion, SuggestionKind, SuggestionSource, SuggestionStatus,
    SuggestionTarget, validate,
};

use crate::tailor::StoreCache;
use crate::{App, JobContext, JobEvents, Result, llm_error, store_error};

/// The calendar year now, for "present" in experience spans.
pub(crate) fn year_now() -> i32 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // Average Gregorian year; off by at most a day around New Year.
    1970 + i32::try_from(secs / 31_556_952).unwrap_or(0)
}

/// A checked summary as a pending suggestion, or its discarded details.
pub(crate) fn summary_suggestion(
    summary: CheckedSummary,
    before: Option<&str>,
    source: SuggestionSource,
    ids: &mut dyn IdSource,
) -> std::result::Result<Suggestion, DiscardedRewrite> {
    if summary.dropped {
        return Err(DiscardedRewrite {
            bullet_id: None,
            before: before.unwrap_or_default().to_owned(),
            after: summary.after,
            warnings: summary.warnings,
        });
    }
    Ok(Suggestion {
        id: ids.next_id(),
        kind: SuggestionKind::Summary,
        target: SuggestionTarget::Summary,
        before: before.map(str::to_owned),
        after: Some(summary.after),
        action: None,
        reason: "A summary written from your CV.".into(),
        confidence: 0.6,
        warnings: summary.warnings,
        status: SuggestionStatus::Pending,
        source,
    })
}

fn category(c: NoteCategory) -> ReviewNoteCategory {
    match c {
        NoteCategory::Impact => ReviewNoteCategory::Impact,
        NoteCategory::Clarity => ReviewNoteCategory::Clarity,
        NoteCategory::Structure => ReviewNoteCategory::Structure,
        NoteCategory::Consistency => ReviewNoteCategory::Consistency,
    }
}

fn internal(message: &str) -> ApiError {
    ApiError::new(ErrorCode::Internal, message)
}

impl App {
    /// Starts reviewing the saved base CV and returns the job's ID and the
    /// future to spawn. Fails at once when AI is not set up or there is no
    /// such CV.
    pub fn ai_review(
        self: &Arc<Self>,
        events: Arc<dyn JobEvents>,
        req: AiReview,
    ) -> Result<(JobId, impl Future<Output = ()> + Send + 'static)> {
        let generation = self.ai_generation();
        self.llm()?;
        self.get_cv(&req.cv_id)?;
        Ok(self.start_job(events, move |app, ctx| async move {
            let result = app.review(&ctx, req, generation).await?;
            serde_json::to_value(result)
                .map_err(|_| internal("The review result could not be sent."))
        }))
    }

    async fn review(
        self: &Arc<Self>,
        ctx: &JobContext,
        req: AiReview,
        generation: u64,
    ) -> Result<ReviewSession> {
        let (llm, status) = self
            .blocking(|app| Ok((app.llm()?, app.ai_status()?)))
            .await?;
        let model = status.model.clone();
        let cache = StoreCache::new(Arc::clone(self), generation);
        let stage = StageContext {
            llm: llm.as_ref(),
            cache: &cache,
            model: &model,
            cancel: ctx.cancel.clone(),
        };

        ctx.progress(AiStage::Review, 0);
        let app = Arc::clone(self);
        let target = Target {
            cv_id: req.cv_id.clone(),
            variant_id: None,
        };
        let lint = tokio::task::spawn_blocking(move || app.lint(&target))
            .await
            .map_err(|_| internal("The checks stopped unexpectedly."))??;
        let cv_id = req.cv_id.clone();
        let mut cv = self.blocking(move |app| app.get_cv(&cv_id)).await?.cv;
        let _ = validate::for_save(&mut cv);
        let base = cv.resolve(None);
        let payload = build_cv_payload_redacting(
            &base,
            &cv.basics.personal,
            PayloadOptions {
                send_name: status.send_name,
                send_location: status.send_location,
            },
        );
        ctx.progress(AiStage::Review, 20);
        let review = cv_ai::review_cv(&stage, &lint, &payload, year_now())
            .await
            .map_err(llm_error)?;
        ctx.check()?;

        let (kept, mut discarded) = crate::tailor::rewrite_suggestions(&base, review.rewrites);
        let mut suggestions: Vec<Suggestion> = self.with_ids(|ids| {
            kept.into_iter()
                .map(|r| r.into_suggestion(SuggestionSource::Review, ids))
                .collect()
        });
        if let Some(summary) = review.summary {
            let before = base.summary.as_deref();
            match self
                .with_ids(|ids| summary_suggestion(summary, before, SuggestionSource::Review, ids))
            {
                Ok(s) => suggestions.push(s),
                Err(d) => discarded.push(d),
            }
        }
        let notes = review
            .notes
            .into_iter()
            .map(|n| ReviewNote {
                target: n.target,
                category: category(n.category),
                message: n.message,
            })
            .collect();
        let mut session = ReviewSession {
            cv_id: req.cv_id.clone(),
            created_at: cv_core::Timestamp(String::new()),
            suggestions,
            notes,
            discarded,
        };
        let content = serde_json::to_string(&session)
            .map_err(|_| internal("The review could not be saved."))?;
        let cv_id = req.cv_id.clone();
        session.created_at = self
            .blocking(move |app| {
                app.store
                    .review_session_put(&cv_id, &content)
                    .map_err(store_error)
            })
            .await?;
        ctx.progress(AiStage::Review, 100);
        Ok(session)
    }

    /// The latest review of a CV, if it has one.
    pub fn get_review(&self, cv_id: &Id) -> Result<Option<ReviewSession>> {
        let Some((content, created_at)) =
            self.store.review_session_get(cv_id).map_err(store_error)?
        else {
            return Ok(None);
        };
        let mut session: ReviewSession = serde_json::from_str(&content)
            .map_err(|_| internal("The saved review could not be read."))?;
        session.created_at = created_at;
        Ok(Some(session))
    }

    /// Accepts or rejects a pending review suggestion. Accepting applies
    /// its text (or the user's edit of it) to the base CV, and fails with
    /// `Conflict`, changing nothing, when that text changed since the
    /// review.
    pub fn review_decide(&self, req: ReviewDecide) -> Result<ReviewDecided> {
        // One decision at a time: each one rewrites the whole session.
        let _decision = self
            .review_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut session = self
            .get_review(&req.cv_id)?
            .ok_or_else(|| ApiError::new(ErrorCode::NotFound, "This CV has no review yet."))?;
        let Some(s) = session
            .suggestions
            .iter_mut()
            .find(|s| s.id == req.suggestion_id && s.status == SuggestionStatus::Pending)
        else {
            return Err(ApiError::new(
                ErrorCode::NotFound,
                "This suggestion was already decided.",
            ));
        };
        let mut saved = None;
        if req.accept {
            let text = req
                .text
                .as_deref()
                .or(s.after.as_deref())
                .unwrap_or_default()
                .trim()
                .to_owned();
            if text.is_empty() {
                return Err(ApiError::new(ErrorCode::Validation, "The text is empty."));
            }
            let stored = self.get_cv(&req.cv_id)?;
            let mut cv = stored.cv;
            let before = s.before.as_deref().unwrap_or_default();
            if !cv.apply_text(&s.target, before, &text) {
                return Err(ApiError::new(
                    ErrorCode::Conflict,
                    "This part of the CV changed since the review. Run the review again.",
                ));
            }
            self.with_ids(|ids| {
                self.store
                    .add_snapshot(&req.cv_id, SnapshotReason::AiApply, ids)
            })
            .map_err(store_error)?;
            saved = Some(self.save_cv(SaveCv {
                cv,
                expected_revision: stored.revision,
            })?);
            s.after = Some(text);
            s.status = SuggestionStatus::Accepted;
        } else {
            s.status = SuggestionStatus::Rejected;
        }
        let content = serde_json::to_string(&session)
            .map_err(|_| internal("The review could not be saved."))?;
        self.store
            .review_session_update(&req.cv_id, &content)
            .map_err(store_error)?;
        Ok(ReviewDecided { session, cv: saved })
    }
}
