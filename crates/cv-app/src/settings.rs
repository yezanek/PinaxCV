//! App settings (spec section 12.2, screen 5) apart from AI, which lives in
//! `ai.rs`: what new CVs start with, the data and log folders and the about
//! box.

use std::path::PathBuf;

use cv_api::{AppSettings, CvDefaults};
use cv_core::Cv;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{App, Result, store_error};

const PAPER: &str = "defaults.paper";
const LOCALE: &str = "defaults.locale";
const DATE_STYLE: &str = "defaults.date_style";

/// A setting value as its serde name ("a4", "month_name_year").
fn name<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

impl App {
    /// The folder holding the database, shown in Settings.
    pub fn with_data_dir(mut self, dir: PathBuf) -> Self {
        self.data_dir = Some(dir);
        self
    }

    /// The data folder, if the app has one.
    pub fn data_dir(&self) -> Option<&std::path::Path> {
        self.data_dir.as_deref()
    }

    /// The folder holding the log file, shown in Settings.
    pub fn with_log_dir(mut self, dir: PathBuf) -> Self {
        self.log_dir = Some(dir);
        self
    }

    /// The log folder, if the app has a log.
    pub fn log_dir(&self) -> Option<&std::path::Path> {
        self.log_dir.as_deref()
    }

    /// A saved default, or the type's default when unset or unknown (a
    /// value from a newer version, say).
    fn default_of<T: DeserializeOwned + Default>(&self, key: &str) -> Result<T> {
        Ok(self
            .store
            .get_setting(key)
            .map_err(store_error)?
            .and_then(|v| serde_json::from_value(serde_json::Value::String(v)).ok())
            .unwrap_or_default())
    }

    pub fn cv_defaults(&self) -> Result<CvDefaults> {
        Ok(CvDefaults {
            paper: self.default_of(PAPER)?,
            locale: self.default_of(LOCALE)?,
            date_style: self.default_of(DATE_STYLE)?,
        })
    }

    /// Applies the defaults to a CV that is being created.
    pub(crate) fn with_defaults(&self, mut cv: Cv) -> Result<Cv> {
        let d = self.cv_defaults()?;
        cv.paper = d.paper;
        cv.locale = d.locale;
        cv.options.date_style = d.date_style;
        Ok(cv)
    }

    pub fn get_settings(&self) -> Result<AppSettings> {
        Ok(AppSettings {
            defaults: self.cv_defaults()?,
            data_dir: self.data_dir.as_ref().map(|d| d.display().to_string()),
            log_dir: self.log_dir.as_ref().map(|d| d.display().to_string()),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            license: env!("CARGO_PKG_LICENSE").to_owned(),
        })
    }

    /// Saves what new CVs start with. Existing CVs keep their own settings.
    pub fn set_cv_defaults(&self, defaults: CvDefaults) -> Result<AppSettings> {
        let set = |k, v: String| self.store.set_setting(k, &v).map_err(store_error);
        set(PAPER, name(defaults.paper))?;
        set(LOCALE, name(defaults.locale))?;
        set(DATE_STYLE, name(defaults.date_style))?;
        self.get_settings()
    }
}
