//! AI job events as Tauri events (spec section 10.2): `ai://progress`,
//! `ai://done` and `ai://error`, sent to every window.

use std::sync::Arc;

use cv_api::JobId;
use cv_app::{JobEvent, JobEvents};
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

/// Starts a job: `begin` gets the event sink and returns the job's ID and
/// its future, which runs on the async runtime. The ID is returned at once;
/// the result arrives as an `ai://done` or `ai://error` event.
pub fn start<R, F, Fut>(handle: &AppHandle<R>, begin: F) -> cv_app::Result<JobId>
where
    R: Runtime,
    F: FnOnce(Arc<dyn JobEvents>) -> cv_app::Result<(JobId, Fut)>,
    Fut: Future<Output = ()> + Send + 'static,
{
    let (id, run) = begin(Arc::new(TauriEvents(handle.clone())))?;
    tauri::async_runtime::spawn(run);
    Ok(id)
}
