//! AI jobs (spec sections 10.2 and 11.13): long AI work runs as a job with an
//! ID, reports progress as it goes, and can be cancelled with `ai_cancel`.
//!
//! The registry knows nothing about Tauri. Starting a job returns its ID at
//! once and a future that does the work; the shell spawns that future and
//! turns [`JobEvent`]s into `ai://progress`, `ai://done` and `ai://error`.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use cv_api::{AiDone, AiFailed, AiProgress, AiStage, ApiError, ErrorCode, JobId};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::{App, Result};

/// What a job tells the UI.
#[derive(Clone, Debug, PartialEq)]
pub enum JobEvent {
    Progress(AiProgress),
    Done(AiDone),
    Failed(AiFailed),
}

/// Delivers job events; the shell emits them as Tauri events.
pub trait JobEvents: Send + Sync {
    fn emit(&self, event: JobEvent);
}

/// The running jobs, by ID, with the token that cancels each.
#[derive(Default)]
pub struct JobRegistry {
    running: Mutex<HashMap<JobId, CancellationToken>>,
}

impl JobRegistry {
    fn insert(&self, id: &JobId, token: CancellationToken) {
        self.lock().insert(id.clone(), token);
    }

    fn remove(&self, id: &JobId) {
        self.lock().remove(id);
    }

    /// Cancels `id`; false when no such job is running (it may just have
    /// finished, which is not an error).
    pub fn cancel(&self, id: &str) -> bool {
        match self.lock().get(id) {
            Some(token) => {
                token.cancel();
                true
            }
            None => false,
        }
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.lock().contains_key(id)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<JobId, CancellationToken>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Handed to a job's work: its ID, its cancellation token (pass it to each
/// `LlmRequest`) and a way to report progress.
#[derive(Clone)]
pub struct JobContext {
    pub id: JobId,
    pub cancel: CancellationToken,
    events: Arc<dyn JobEvents>,
    last_pct: Arc<AtomicU8>,
}

impl JobContext {
    /// Reports `stage` at `pct` percent, clamped to 100 and never below an
    /// earlier report.
    pub fn progress(&self, stage: AiStage, pct: u8) {
        let pct = pct.min(100);
        let pct = self.last_pct.fetch_max(pct, Ordering::Relaxed).max(pct);
        self.events.emit(JobEvent::Progress(AiProgress {
            job_id: self.id.clone(),
            stage,
            pct,
        }));
    }

    /// `Err(ai_cancelled)` once the job was cancelled, for checks between stages.
    pub fn check(&self) -> Result<()> {
        if self.cancel.is_cancelled() {
            Err(cancelled())
        } else {
            Ok(())
        }
    }
}

fn cancelled() -> ApiError {
    ApiError::new(ErrorCode::AiCancelled, "The AI job was cancelled.")
}

/// Owned by a job's future: if the future is dropped before it finishes, or
/// `work` panics, the job still leaves the registry and reports `internal`.
struct JobGuard {
    app: Arc<App>,
    id: JobId,
    events: Arc<dyn JobEvents>,
    finished: bool,
}

impl Drop for JobGuard {
    fn drop(&mut self) {
        self.app.jobs.remove(&self.id);
        if !self.finished {
            self.events.emit(JobEvent::Failed(AiFailed {
                job_id: self.id.clone(),
                error: ApiError::new(ErrorCode::Internal, "The AI job stopped unexpectedly."),
            }));
        }
    }
}

impl App {
    /// Registers a job running `work` and returns its ID and the future to
    /// spawn. The future ends with exactly one `Done` or `Failed` event;
    /// cancelling ends it at once with `ai_cancelled`, whatever `work` is
    /// waiting for.
    pub fn start_job<W, F>(
        self: &Arc<Self>,
        events: Arc<dyn JobEvents>,
        work: W,
    ) -> (JobId, impl Future<Output = ()> + Send + 'static)
    where
        W: FnOnce(Arc<App>, JobContext) -> F + Send + 'static,
        F: Future<Output = Result<Value>> + Send + 'static,
    {
        let id = self.next_id().to_string();
        let cancel = CancellationToken::new();
        self.jobs.insert(&id, cancel.clone());
        let ctx = JobContext {
            id: id.clone(),
            cancel: cancel.clone(),
            events: Arc::clone(&events),
            last_pct: Arc::default(),
        };
        let mut guard = JobGuard {
            app: Arc::clone(self),
            id: id.clone(),
            events,
            finished: false,
        };
        let app = Arc::clone(self);
        let run = async move {
            ctx.progress(AiStage::Starting, 0);
            let result = tokio::select! {
                biased;
                () = cancel.cancelled() => Err(cancelled()),
                r = work(app, ctx) => r,
            };
            // A job cancelled while finishing still reports the cancellation.
            let result = if cancel.is_cancelled() {
                Err(cancelled())
            } else {
                result
            };
            let job_id = guard.id.clone();
            let event = match result {
                Ok(result) => JobEvent::Done(AiDone { job_id, result }),
                Err(error) => JobEvent::Failed(AiFailed { job_id, error }),
            };
            guard.app.jobs.remove(&guard.id);
            guard.finished = true;
            guard.events.emit(event);
        };
        (id, run)
    }

    /// `ai_cancel(job_id)`: cancels the job if it is still running.
    pub fn ai_cancel(&self, job_id: &str) -> Result<()> {
        self.jobs.cancel(job_id);
        Ok(())
    }

    /// Whether job `job_id` is still running.
    pub fn job_running(&self, job_id: &str) -> bool {
        self.jobs.is_running(job_id)
    }
}
