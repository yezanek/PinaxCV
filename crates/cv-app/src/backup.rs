//! Backup and restore (spec sections 5.3 and 13): a backup is a copy of the
//! whole database made with `VACUUM INTO`. It holds every CV, variant,
//! setting and cached AI answer, and never an API key, which lives in the
//! OS keychain.

use cv_api::{ApiError, BackupResult, ErrorCode, RestoreResult};
use cv_store::StoreError;

use crate::files::{BACKUP_EXTENSION, ExportKind, create_atomically};
use crate::{App, Result};

/// Today's date in UTC as `YYYY-MM-DD`.
fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    date_of_day(i64::try_from(secs / 86_400).unwrap_or(0))
}

/// The civil date of a day count since 1970-01-01 (H. Hinnant's algorithm).
fn date_of_day(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// The file name the save dialog suggests, with today's date.
fn suggested_name(today: &str) -> String {
    format!("PinaxCV backup {today}.{BACKUP_EXTENSION}")
}

/// Restore errors say what to do and that nothing changed; the store's own
/// text speaks of stored content.
fn restore_error(e: StoreError) -> ApiError {
    match e {
        StoreError::NewerSchema { .. } => ApiError::new(
            ErrorCode::Validation,
            "This backup is from a newer PinaxCV. Update PinaxCV, then restore it. Nothing was changed.",
        ),
        StoreError::Invalid(_) => ApiError::new(
            ErrorCode::Validation,
            "This file is not a PinaxCV backup, or it is damaged. Nothing was changed.",
        ),
        other => crate::store_error(other),
    }
}

impl App {
    /// Asks where to save and writes a backup there.
    pub fn backup_db(&self) -> Result<BackupResult> {
        let suggested = suggested_name(&today());
        let path = self
            .dialog
            .choose_save_path(&suggested, ExportKind::Backup)
            .ok_or_else(|| ApiError::new(ErrorCode::Cancelled, "Backup cancelled."))?;
        // The error text can carry the path, which holds the user's name.
        create_atomically(&path, |tmp| {
            self.store
                .backup_to(tmp)
                .map_err(|_| std::io::Error::other("backup failed"))
        })
        .map_err(|_| {
            ApiError::new(
                ErrorCode::Io,
                "Could not write the backup. Check the folder's permissions and free space.",
            )
        })?;
        Ok(BackupResult {
            path: path.display().to_string(),
        })
    }

    /// Asks for a backup and replaces all data with it, keeping a copy of
    /// the data from before next to the database.
    pub fn restore_db(&self) -> Result<RestoreResult> {
        let path = self
            .dialog
            .choose_open_path(&[BACKUP_EXTENSION])
            .ok_or_else(|| ApiError::new(ErrorCode::Cancelled, "Restore cancelled."))?;
        let previous = self.store.restore_from(&path).map_err(restore_error)?;
        // The settings came from the backup too.
        self.reset_llm();
        Ok(RestoreResult {
            previous_data: previous.display().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_from_day_counts() {
        assert_eq!(date_of_day(0), "1970-01-01");
        assert_eq!(date_of_day(11_016), "2000-02-29");
        assert_eq!(date_of_day(20_750), "2026-10-24");
    }

    #[test]
    fn names_and_errors() {
        assert_eq!(
            suggested_name("2026-10-24"),
            "PinaxCV backup 2026-10-24.pinaxcv"
        );
        let e = restore_error(StoreError::Invalid("the backup file is damaged".into()));
        assert_eq!(e.code, ErrorCode::Validation);
        assert!(e.message.contains("Nothing was changed"));
        let e = restore_error(StoreError::NewerSchema {
            found: 9,
            supported: 1,
        });
        assert!(e.message.contains("newer PinaxCV"));
    }
}
