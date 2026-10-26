//! AI job events as Tauri events (spec section 10.2): `ai://progress`,
//! `ai://done` and `ai://error`, sent to every window.

use std::sync::Arc;

use cv_api::JobId;
use cv_app::{App, JobContext, JobEvent, JobEvents};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Runtime};

pub const PROGRESS: &str = "ai://progress";
pub const DONE: &str = "ai://done";
pub const ERROR: &str = "ai://error";

/// Emits job events on `handle`. An event that cannot be sent (the window
/// is gone) is dropped: there is nobody to tell.
pub struct TauriEvents<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> JobEvents for TauriEvents<R> {
    fn emit(&self, event: JobEvent) {
        let _ = match event {
            JobEvent::Progress(p) => self.0.emit(PROGRESS, p),
            JobEvent::Done(d) => self.0.emit(DONE, d),
            JobEvent::Failed(f) => self.0.emit(ERROR, f),
        };
    }
}

/// Starts `work` as a job on the async runtime and returns its ID at once;
/// the result arrives as an `ai://done` or `ai://error` event.
// The AI commands that start jobs (ai_tailor, ai_review) arrive with their
// stages; until then only the tests start one.
#[cfg_attr(not(test), allow(dead_code))]
pub fn spawn<R, W, F>(handle: &AppHandle<R>, app: &Arc<App>, work: W) -> JobId
where
    R: Runtime,
    W: FnOnce(Arc<App>, JobContext) -> F + Send + 'static,
    F: Future<Output = cv_app::Result<Value>> + Send + 'static,
{
    let (id, run) = app.start_job(Arc::new(TauriEvents(handle.clone())), work);
    tauri::async_runtime::spawn(run);
    id
}
