//! Deleted CVs stay restorable for 30 days and are then purged (#113).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cv_api::{ErrorCode, NewCv};
use cv_app::{App, DELETED_CV_RETENTION, ExportKind, FileDialog, UlidIds};
use cv_store::{Clock, SqliteStore};

const DAY: u64 = 24 * 60 * 60;

struct NoDialog;

impl FileDialog for NoDialog {
    fn choose_save_path(&self, _: &str, _: ExportKind) -> Option<PathBuf> {
        None
    }

    fn choose_open_path(&self, _: &[&str]) -> Option<PathBuf> {
        None
    }
}

/// A clock the test moves forward, in seconds since the epoch.
#[derive(Clone)]
struct TestClock(Arc<AtomicU64>);

impl TestClock {
    fn advance_days(&self, days: u64) {
        self.0.fetch_add(days * DAY, Ordering::SeqCst);
    }
}

impl Clock for TestClock {
    fn now(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(self.0.load(Ordering::SeqCst))
    }
}

#[test]
fn the_retention_is_thirty_days() {
    assert_eq!(DELETED_CV_RETENTION, Duration::from_secs(30 * DAY));
}

#[test]
fn cvs_deleted_over_thirty_days_ago_are_purged() {
    let clock = TestClock(Arc::new(AtomicU64::new(1_800_000_000)));
    let store = SqliteStore::open_in_memory(Box::new(clock.clone())).expect("store");
    let app = App::new(
        Box::new(store),
        Box::new(NoDialog),
        Box::new(UlidIds),
        "preview://localhost",
    );
    let new = |title: &str| {
        app.create_cv(NewCv {
            title: title.into(),
        })
        .expect("cv")
        .cv
        .id
    };
    let old = new("Deleted a month ago");
    let recent = new("Deleted yesterday");
    let kept = new("Not deleted");

    app.delete_cv(&old).expect("delete");
    clock.advance_days(30);
    app.delete_cv(&recent).expect("delete");
    clock.advance_days(1);

    assert_eq!(app.purge_deleted_cvs().expect("purge"), 1);
    assert_eq!(
        app.restore_cv(&old).map_err(|e| e.code),
        Err(ErrorCode::NotFound)
    );
    app.restore_cv(&recent).expect("still restorable");
    let ids: Vec<_> = app
        .list_cvs()
        .expect("list")
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&recent) && ids.contains(&kept));
    // A second run finds nothing more to remove.
    assert_eq!(app.purge_deleted_cvs().expect("purge"), 0);
}
