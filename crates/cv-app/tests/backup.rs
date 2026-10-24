//! Backup, restore and deletion against a SQLite database on disk.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cv_api::{AiConfig, AiProvider, ErrorCode, KeyStorage, NewCv};
use cv_app::{App, ExportKind, FileDialog, SecretError, SecretStore, UlidIds};
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

/// A keychain in a shared map, so the test can look inside.
#[derive(Clone, Default)]
struct Keychain(Arc<Mutex<HashMap<String, String>>>);

impl SecretStore for Keychain {
    fn get(&self, account: &str) -> Result<Option<String>, SecretError> {
        Ok(self.0.lock().expect("lock").get(account).cloned())
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), SecretError> {
        self.0
            .lock()
            .expect("lock")
            .insert(account.into(), secret.into());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        self.0.lock().expect("lock").remove(account);
        Ok(())
    }
}

#[test]
fn delete_all_data_removes_cvs_settings_cache_and_keys() {
    let dir = tempfile::tempdir().expect("tempdir");
    let keychain = Keychain::default();
    let app =
        app(&dir.path().join("pinaxcv.db"), None, None).with_secrets(Box::new(keychain.clone()));
    let status = app
        .ai_configure(AiConfig {
            provider: AiProvider::Anthropic,
            model: "some-model".into(),
            base_url: None,
            api_key: Some("sk-test".into()),
            send_name: false,
            send_location: true,
        })
        .expect("configure");
    assert_eq!(status.key, KeyStorage::Keychain);
    new_cv(&app, "Mine");
    app.clear_ai_cache().expect("clear cache");
    assert_eq!(titles(&app), ["Mine"], "clearing the cache keeps CVs");

    app.delete_all_data().expect("delete");
    assert!(titles(&app).is_empty());
    let status = app.ai_status().expect("status");
    assert_eq!(
        (status.provider, status.key, status.configured),
        (AiProvider::None, KeyStorage::None, false)
    );
    assert!(keychain.0.lock().expect("lock").is_empty(), "keys deleted");
    // The app keeps working.
    new_cv(&app, "Fresh start");
    assert_eq!(titles(&app), ["Fresh start"]);
}
