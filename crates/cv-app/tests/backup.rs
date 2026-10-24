//! Backup and restore against a SQLite database on disk.

use std::path::{Path, PathBuf};

use cv_api::{ErrorCode, NewCv};
use cv_app::{App, ExportKind, FileDialog, UlidIds};
use cv_store::{SqliteStore, SystemClock};

/// Saves to `save` and opens `open`; `None` cancels.
struct Dialog {
    save: Option<PathBuf>,
    open: Option<PathBuf>,
}

impl FileDialog for Dialog {
    fn choose_save_path(&self, suggested: &str, kind: ExportKind) -> Option<PathBuf> {
        assert_eq!(kind, ExportKind::Backup);
        assert!(suggested.starts_with("PinaxCV backup ") && suggested.ends_with(".pinaxcv"));
        self.save.clone()
    }

    fn choose_open_path(&self, extensions: &[&str]) -> Option<PathBuf> {
        assert_eq!(extensions, ["pinaxcv"]);
        self.open.clone()
    }
}

fn app(db: &Path, save: Option<PathBuf>, open: Option<PathBuf>) -> App {
    let store = SqliteStore::open_with_clock(db, Box::new(SystemClock)).expect("store");
    App::new(
        Box::new(store),
        Box::new(Dialog { save, open }),
        Box::new(UlidIds),
        "preview://localhost",
    )
}

fn titles(app: &App) -> Vec<String> {
    let mut t: Vec<String> = app
        .list_cvs()
        .expect("list")
        .into_iter()
        .map(|c| c.title)
        .collect();
    t.sort();
    t
}

fn new_cv(app: &App, title: &str) {
    app.create_cv(NewCv {
        title: title.into(),
    })
    .expect("cv");
}

#[test]
fn a_backup_restores_the_data_it_was_made_from() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("pinaxcv.db");
    let backup = dir.path().join("PinaxCV backup.pinaxcv");
    let app = app(&db, Some(backup.clone()), Some(backup.clone()));
    new_cv(&app, "Kept");
    let written = app.backup_db().expect("backup");
    assert_eq!(written.path, backup.display().to_string());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&backup)
            .expect("meta")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "a backup holds personal data");
    }
    // A second backup over the first replaces it.
    app.backup_db().expect("backup again");

    new_cv(&app, "After the backup");
    assert_eq!(titles(&app), ["After the backup", "Kept"]);
    let restored = app.restore_db().expect("restore");
    assert_eq!(titles(&app), ["Kept"]);
    // The data from before the restore is kept next to the database.
    assert!(Path::new(&restored.previous_data).exists());
    // The app keeps working on the restored data.
    new_cv(&app, "New");
    assert_eq!(titles(&app), ["Kept", "New"]);
}

#[test]
fn a_file_that_is_not_a_backup_changes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("pinaxcv.db");
    let junk = dir.path().join("notes.pinaxcv");
    std::fs::write(&junk, "not a database").expect("junk");
    let app = app(&db, None, Some(junk));
    new_cv(&app, "Mine");
    let e = app.restore_db().expect_err("refused");
    assert_eq!(e.code, ErrorCode::Validation);
    assert!(e.message.contains("Nothing was changed"), "{}", e.message);
    assert_eq!(titles(&app), ["Mine"]);
}

#[test]
fn cancelled_dialogs_do_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let app = app(&dir.path().join("pinaxcv.db"), None, None);
    assert_eq!(
        app.backup_db().expect_err("cancel").code,
        ErrorCode::Cancelled
    );
    assert_eq!(
        app.restore_db().expect_err("cancel").code,
        ErrorCode::Cancelled
    );
}
