//! Settings apart from AI: defaults for new CVs, the data folder and the
//! about box.

use std::path::PathBuf;

use cv_api::{CvDefaults, NewCv};
use cv_app::{App, ExportKind, FileDialog, UlidIds};
use cv_core::{DateStyle, Locale, Paper};
use cv_store::{SqliteStore, Store, SystemClock};

struct NoDialog;

impl FileDialog for NoDialog {
    fn choose_save_path(&self, _: &str, _: ExportKind) -> Option<PathBuf> {
        None
    }

    fn choose_open_path(&self, _: &[&str]) -> Option<PathBuf> {
        None
    }
}

fn app(store: SqliteStore) -> App {
    App::new(
        Box::new(store),
        Box::new(NoDialog),
        Box::new(UlidIds),
        "preview://localhost",
    )
}

fn store() -> SqliteStore {
    SqliteStore::open_in_memory(Box::new(SystemClock)).expect("store")
}

#[test]
fn new_cvs_start_with_the_saved_defaults() {
    let app = app(store());
    assert_eq!(app.cv_defaults().expect("defaults"), CvDefaults::default());
    let before = app
        .create_cv(NewCv {
            title: "Before".into(),
        })
        .expect("cv");

    let defaults = CvDefaults {
        paper: Paper::Letter,
        locale: Locale::En,
        date_style: DateStyle::YearOnly,
    };
    let settings = app.set_cv_defaults(defaults).expect("saved");
    assert_eq!(settings.defaults, defaults);

    let cv = app
        .create_cv(NewCv {
            title: "After".into(),
        })
        .expect("cv")
        .cv;
    assert_eq!(
        (cv.paper, cv.locale, cv.options.date_style),
        (Paper::Letter, Locale::En, DateStyle::YearOnly)
    );
    // Existing CVs keep their own settings.
    let old = app.get_cv(&before.cv.id).expect("cv").cv;
    assert_eq!(
        (old.paper, old.options.date_style),
        (Paper::A4, DateStyle::MonthNameYear)
    );
}

#[test]
fn unknown_saved_values_fall_back_to_the_default() {
    let store = store();
    store
        .set_setting("defaults.paper", "tabloid")
        .expect("setting");
    store
        .set_setting("defaults.date_style", "numeric_month_year")
        .expect("setting");
    let app = app(store);
    let d = app.cv_defaults().expect("defaults");
    assert_eq!(
        (d.paper, d.date_style),
        (Paper::A4, DateStyle::NumericMonthYear)
    );
}

#[test]
fn settings_name_the_data_and_log_folders_version_and_licence() {
    let s = app(store()).get_settings().expect("settings");
    assert_eq!(s.data_dir, None);
    assert_eq!(s.log_dir, None);
    assert!(!s.version.is_empty());
    assert_eq!(s.license, "Apache-2.0");

    let dir = std::env::temp_dir().join("pinaxcv-settings-test");
    // On macOS and Windows the log is not inside the data folder.
    let logs = std::env::temp_dir().join("pinaxcv-settings-test-logs");
    let s = app(store())
        .with_data_dir(dir.clone())
        .with_log_dir(logs.clone())
        .get_settings()
        .expect("settings");
    assert_eq!(s.data_dir, Some(dir.display().to_string()));
    assert_eq!(s.log_dir, Some(logs.display().to_string()));
}
