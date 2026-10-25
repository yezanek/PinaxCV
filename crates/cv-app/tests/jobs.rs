//! AI jobs: progress, results, failures and cancellation.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cv_api::{AiStage, ApiError, ErrorCode};
use cv_app::{App, ExportKind, FileDialog, JobEvent, JobEvents, UlidIds};
use cv_store::{SqliteStore, SystemClock};
use serde_json::json;

struct NoDialog;

impl FileDialog for NoDialog {
    fn choose_save_path(&self, _: &str, _: ExportKind) -> Option<PathBuf> {
        None
    }

    fn choose_open_path(&self, _: &[&str]) -> Option<PathBuf> {
        None
    }
}

#[derive(Default)]
struct Recorded(Mutex<Vec<JobEvent>>);

impl JobEvents for Recorded {
    fn emit(&self, event: JobEvent) {
        self.0.lock().expect("lock").push(event);
    }
}

impl Recorded {
    fn events(&self) -> Vec<JobEvent> {
        self.0.lock().expect("lock").clone()
    }

    fn progress(&self) -> Vec<(AiStage, u8)> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                JobEvent::Progress(p) => Some((p.stage, p.pct)),
                _ => None,
            })
            .collect()
    }
}

fn app() -> Arc<App> {
    let store = SqliteStore::open_in_memory(Box::new(SystemClock)).expect("store");
    Arc::new(App::new(
        Box::new(store),
        Box::new(NoDialog),
        Box::new(UlidIds),
        "preview://localhost",
    ))
}

#[tokio::test]
async fn a_job_reports_progress_then_its_result() {
    let app = app();
    let events = Arc::new(Recorded::default());
    let (id, run) = app.start_job(events.clone(), |app, ctx| async move {
        ctx.progress(AiStage::JobParse, 10);
        // Work can use the app, as stages will to load the CV.
        let cvs = app.list_cvs()?.len();
        ctx.progress(AiStage::Relevance, 40);
        // Progress never goes back, and stops at 100.
        ctx.progress(AiStage::Selection, 30);
        ctx.progress(AiStage::Fit, 250);
        Ok(json!({ "cvs": cvs }))
    });
    assert!(app.job_running(&id));
    run.await;
    assert!(!app.job_running(&id));
    assert_eq!(
        events.progress(),
        [
            (AiStage::Starting, 0),
            (AiStage::JobParse, 10),
            (AiStage::Relevance, 40),
            (AiStage::Selection, 40),
            (AiStage::Fit, 100),
        ]
    );
    let Some(JobEvent::Done(done)) = events.events().pop() else {
        panic!("no done event");
    };
    assert_eq!((done.job_id, done.result), (id, json!({ "cvs": 0 })));
}

#[tokio::test]
async fn a_failing_job_reports_its_error() {
    let app = app();
    let events = Arc::new(Recorded::default());
    let (id, run) = app.start_job(events.clone(), |_, _| async {
        Err(ApiError::new(
            ErrorCode::AiProvider,
            "The provider returned HTTP 500.",
        ))
    });
    run.await;
    let Some(JobEvent::Failed(failed)) = events.events().pop() else {
        panic!("no error event");
    };
    assert_eq!(failed.job_id, id);
    assert_eq!(failed.error.code, ErrorCode::AiProvider);
}

#[tokio::test]
async fn a_dropped_job_leaves_the_registry_and_reports_internal() {
    let app = app();
    let events = Arc::new(Recorded::default());
    let (id, run) = app.start_job(events.clone(), |_, _| async { Ok(json!({})) });
    assert!(app.job_running(&id));
    drop(run);
    assert!(!app.job_running(&id));
    let recorded = events.events();
    let [JobEvent::Failed(failed)] = recorded.as_slice() else {
        panic!("expected one error event");
    };
    assert_eq!(failed.job_id, id);
    assert_eq!(failed.error.code, ErrorCode::Internal);
}

#[tokio::test]
async fn a_panicking_job_still_reports_once() {
    let app = app();
    let events = Arc::new(Recorded::default());
    let (id, run) = app.start_job(events.clone(), |_, _| async {
        panic!("boom");
        #[allow(unreachable_code)]
        Ok(json!({}))
    });
    assert!(tokio::spawn(run).await.is_err());
    assert!(!app.job_running(&id));
    let failed: Vec<_> = events
        .events()
        .into_iter()
        .filter_map(|e| match e {
            JobEvent::Failed(f) => Some(f),
            _ => None,
        })
        .collect();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].error.code, ErrorCode::Internal);
}

#[tokio::test]
async fn cancelling_ends_a_waiting_job_at_once() {
    let app = app();
    let events = Arc::new(Recorded::default());
    let (id, run) = app.start_job(events.clone(), |_, ctx| async move {
        // Stands for a provider call that never answers.
        tokio::time::sleep(Duration::from_secs(3600)).await;
        ctx.check()?;
        Ok(json!(null))
    });
    let task = tokio::spawn(run);
    tokio::task::yield_now().await;
    app.ai_cancel(&id).expect("cancel");
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("ended promptly")
        .expect("joined");
    assert!(!app.job_running(&id));
    let events = events.events();
    let failures: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            JobEvent::Failed(f) => Some(f.error.code),
            JobEvent::Done(_) => panic!("a cancelled job finished"),
            JobEvent::Progress(_) => None,
        })
        .collect();
    assert_eq!(failures, [ErrorCode::AiCancelled]);
    // Cancelling again, or an unknown job, is not an error.
    app.ai_cancel(&id).expect("again");
    app.ai_cancel("nope").expect("unknown");
}

#[tokio::test]
async fn jobs_get_distinct_ids_and_cancel_separately() {
    let app = app();
    let events = Arc::new(Recorded::default());
    let (a, run_a) = app.start_job(events.clone(), |_, ctx| async move {
        ctx.cancel.cancelled().await;
        ctx.check()?;
        Ok(json!("a"))
    });
    let (b, run_b) = app.start_job(events.clone(), |_, _| async { Ok(json!("b")) });
    assert_ne!(a, b);
    app.ai_cancel(&a).expect("cancel");
    run_a.await;
    run_b.await;
    let outcomes: Vec<_> = events
        .events()
        .into_iter()
        .filter_map(|e| match e {
            JobEvent::Done(d) => Some((d.job_id, Ok(d.result))),
            JobEvent::Failed(f) => Some((f.job_id, Err(f.error.code))),
            JobEvent::Progress(_) => None,
        })
        .collect();
    assert_eq!(
        outcomes,
        [(a, Err(ErrorCode::AiCancelled)), (b, Ok(json!("b")))]
    );
}
