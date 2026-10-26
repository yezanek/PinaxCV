//! SQLite persistence behind the storage interface. No rusqlite types leave this crate.
//!
//! [`Store`] is the storage interface; [`SqliteStore`] is the only
//! implementation in v1. The trait uses `cv-core` and `std` types exclusively,
//! so a future PostgreSQL-backed implementation can provide the same interface.

#![forbid(unsafe_code)]

mod clock;
mod migrate;
mod sqlite;

pub use clock::{Clock, SystemClock, timestamp};
pub use migrate::{migrate_cv_json, migrate_variant_json};
pub use sqlite::{DB_SCHEMA_VERSION, EXPORT_SNAPSHOT_INTERVAL, SNAPSHOTS_PER_CV, SqliteStore};

use std::path::Path;
use std::time::Duration;

use cv_core::{
    Cv, CvSummary, Id, IdSource, SnapshotReason, SnapshotSummary, StoredCv, StoredVariant,
    Timestamp, Variant, VariantSummary,
};

/// Errors the store reports. Callers map them to `cv_api::ApiError`.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("not found: {0}")]
    NotFound(Id),
    /// The caller's `expected_revision` is stale.
    #[error("revision conflict; current revision is {current_revision}")]
    Conflict { current_revision: u64 },
    /// Stored content was written by a newer PinaxCV.
    #[error(
        "content has schema version {found}, newer than supported version {supported}; update PinaxCV to open it"
    )]
    NewerSchema { found: u32, supported: u32 },
    /// Stored content could not be read or migrated.
    #[error("invalid stored content ({0}); restore a snapshot or a backup")]
    Invalid(String),
    /// The database itself failed. The source error is never shown to the user
    /// and must not carry CV content; implementations pass the driver error.
    #[error("storage failure; check free disk space and file permissions")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// Persistence for CVs, variants, snapshots, settings and the AI cache (spec section 5.3).
///
/// Synchronous by design (ADR-019): async callers go through `spawn_blocking`.
/// Every save is transactional; `save_*` increments the revision or fails with
/// [`StoreError::Conflict`].
pub trait Store: Send + Sync {
    fn list_cvs(&self) -> Result<Vec<CvSummary>>;
    fn get_cv(&self, id: &Id) -> Result<StoredCv>;
    fn create_cv(&self, cv: &Cv) -> Result<StoredCv>;
    fn save_cv(&self, cv: &Cv, expected_revision: u64) -> Result<StoredCv>;
    /// Copies a CV with fresh IDs for every entity.
    fn duplicate_cv(&self, id: &Id, new_title: &str, ids: &mut dyn IdSource) -> Result<StoredCv>;
    /// Soft delete; recoverable until purged.
    fn delete_cv(&self, id: &Id) -> Result<()>;
    fn restore_cv(&self, id: &Id) -> Result<()>;
    /// Permanently removes CVs soft-deleted longer than `older_than` ago.
    fn purge_deleted(&self, older_than: Duration) -> Result<u64>;

    fn list_variants(&self, cv_id: &Id) -> Result<Vec<VariantSummary>>;
    fn get_variant(&self, id: &Id) -> Result<StoredVariant>;
    fn create_variant(&self, variant: &Variant) -> Result<StoredVariant>;
    fn save_variant(&self, variant: &Variant, expected_revision: u64) -> Result<StoredVariant>;
    fn delete_variant(&self, id: &Id) -> Result<()>;
    fn duplicate_variant(
        &self,
        id: &Id,
        new_name: &str,
        ids: &mut dyn IdSource,
    ) -> Result<StoredVariant>;

    /// Records the current base CV; keeps the latest 50 per CV.
    fn add_snapshot(
        &self,
        cv_id: &Id,
        reason: SnapshotReason,
        ids: &mut dyn IdSource,
    ) -> Result<()>;
    fn list_snapshots(&self, cv_id: &Id) -> Result<Vec<SnapshotSummary>>;
    /// Replaces the base CV with a snapshot, as a normal revisioned save.
    fn restore_snapshot(&self, snapshot_id: &Id, expected_revision: u64) -> Result<StoredCv>;

    fn get_setting(&self, key: &str) -> Result<Option<String>>;
    fn set_setting(&self, key: &str, value: &str) -> Result<()>;

    fn ai_cache_get(&self, key: &str) -> Result<Option<String>>;
    fn ai_cache_put(&self, key: &str, kind: &str, response: &str) -> Result<()>;
    fn ai_cache_clear(&self) -> Result<()>;

    /// The latest AI review of a CV made without a variant (spec section
    /// 11.11): opaque JSON and when it was saved.
    fn review_session_get(&self, cv_id: &Id) -> Result<Option<(String, Timestamp)>>;
    /// Replaces the review session of a live CV; returns its timestamp.
    fn review_session_put(&self, cv_id: &Id, content: &str) -> Result<Timestamp>;
    /// Changes the content of an existing review session, keeping its
    /// timestamp (decisions on its suggestions).
    fn review_session_update(&self, cv_id: &Id, content: &str) -> Result<()>;

    /// Writes a consistent copy of the database (`VACUUM INTO`) to `path`,
    /// which must be absent or an empty file. Contains no secrets.
    fn backup_to(&self, path: &Path) -> Result<()>;
    /// Replaces the database with a backup (spec section 5.3) after copying
    /// the current one aside; returns that copy's path.
    fn restore_from(&self, source: &Path) -> Result<std::path::PathBuf>;
    /// Deletes every CV, variant, snapshot, review, setting and cached AI
    /// answer, compacts the database so no deleted content stays in it or
    /// its WAL, and removes the copies earlier restores kept. The schema
    /// stays, so the store keeps working.
    fn delete_all_data(&self) -> Result<()>;
}
