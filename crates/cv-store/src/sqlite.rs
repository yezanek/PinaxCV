//! The SQLite implementation of [`Store`] (spec sections 5.1 to 5.3).
//!
//! One connection behind a mutex. `rusqlite` types never leave this module:
//! driver errors become [`StoreError::Backend`].

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use cv_core::{
    Cv, CvSummary, Id, IdSource, SnapshotReason, SnapshotSummary, StoredCv, StoredVariant,
    Timestamp, Variant, VariantSummary, validate,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, params};

use crate::clock::{Clock, SystemClock, timestamp};
use crate::migrate::{migrate_cv_json, migrate_variant_json};
use crate::{Result, Store, StoreError};

/// Snapshots kept per CV; older ones are deleted when a new one is added.
pub const SNAPSHOTS_PER_CV: u32 = 50;
/// Export snapshots are taken at most this often per CV.
pub const EXPORT_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(3600);

/// Database schema migrations, applied in order; `PRAGMA user_version` holds
/// the number applied so far.
const MIGRATIONS: [&str; 1] = [include_str!("../migrations/0001_init.sql")];

/// The newest database schema this build understands.
pub const DB_SCHEMA_VERSION: u32 = 1;

/// SQLite-backed [`Store`].
pub struct SqliteStore {
    conn: Mutex<Connection>,
    /// `None` for in-memory databases.
    path: Option<PathBuf>,
    clock: Box<dyn Clock>,
}

impl SqliteStore {
    /// Opens (creating if needed) the database at `path` and applies pending migrations.
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_with_clock(path, Box::new(SystemClock))
    }

    /// Like [`SqliteStore::open`] with an injected clock.
    pub fn open_with_clock(path: &Path, clock: Box<dyn Clock>) -> Result<Self> {
        let conn = Connection::open(path).map_err(backend)?;
        Self::init(conn, Some(path.to_owned()), clock)
    }

    /// Opens an empty in-memory database, for tests.
    pub fn open_in_memory(clock: Box<dyn Clock>) -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(backend)?;
        Self::init(conn, None, clock)
    }

    fn init(conn: Connection, path: Option<PathBuf>, clock: Box<dyn Clock>) -> Result<Self> {
        configure(&conn)?;
        migrate_db(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            path,
            clock,
        })
    }

    /// Replaces the database with the backup at `source` (spec section 5.3).
    ///
    /// The backup must pass `PRAGMA integrity_check` and must not have a newer
    /// schema. The current database is first copied next to itself as
    /// `<name>.before-restore-<timestamp>`, whose path is returned. Fails with
    /// [`StoreError::Invalid`] for in-memory stores.
    pub fn restore_from(&self, source: &Path) -> Result<PathBuf> {
        let path = self.path.clone().ok_or_else(|| {
            StoreError::Invalid("an in-memory database cannot be restored".into())
        })?;
        check_backup(source)?;

        let stamp = timestamp(self.clock.now()).0.replace([':', '.'], "-");
        let mut safety = path.clone().into_os_string();
        safety.push(format!(".before-restore-{stamp}"));
        let safety = PathBuf::from(safety);
        self.backup_to(&safety)?;

        let mut conn = self.lock();
        // Close the live connection before touching its files.
        let old = std::mem::replace(&mut *conn, Connection::open_in_memory().map_err(backend)?);
        old.close().map_err(|(_, e)| backend(e))?;
        for suffix in ["-wal", "-shm"] {
            let mut side = path.clone().into_os_string();
            side.push(suffix);
            match std::fs::remove_file(PathBuf::from(side)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(backend(e)),
            }
        }
        std::fs::copy(source, &path).map_err(backend)?;
        let fresh = Connection::open(&path).map_err(backend)?;
        configure(&fresh)?;
        migrate_db(&fresh)?;
        *conn = fresh;
        Ok(safety)
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        // A panic while holding the lock cannot leave a half-written row:
        // every write runs in a transaction that rolls back on drop.
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn now(&self) -> Timestamp {
        timestamp(self.clock.now())
    }

    /// Timestamp of `self.now() - age`.
    fn ago(&self, age: Duration) -> Timestamp {
        let then = self
            .clock
            .now()
            .checked_sub(age)
            .unwrap_or(std::time::UNIX_EPOCH);
        timestamp(then)
    }
}

fn configure(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000; PRAGMA synchronous=NORMAL;",
    )
    .map_err(backend)
}

fn user_version(conn: &Connection) -> Result<u32> {
    let v: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(backend)?;
    u32::try_from(v).map_err(|_| StoreError::Invalid("negative database version".into()))
}

fn migrate_db(conn: &Connection) -> Result<()> {
    let current = user_version(conn)?;
    if current > DB_SCHEMA_VERSION {
        return Err(StoreError::NewerSchema {
            found: current,
            supported: DB_SCHEMA_VERSION,
        });
    }
    for (n, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let version = n + 1;
        conn.execute_batch(&format!(
            "BEGIN; {sql}\nPRAGMA user_version = {version}; COMMIT;"
        ))
        .map_err(backend)?;
    }
    Ok(())
}

/// Opens `source` read-only and checks it can be restored.
fn check_backup(source: &Path) -> Result<()> {
    let conn = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| StoreError::Invalid("the backup file cannot be opened".into()))?;
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|_| StoreError::Invalid("the backup file is not a PinaxCV database".into()))?;
    if integrity != "ok" {
        return Err(StoreError::Invalid("the backup file is damaged".into()));
    }
    let version = user_version(&conn)?;
    if version == 0 {
        return Err(StoreError::Invalid(
            "the backup file is not a PinaxCV database".into(),
        ));
    }
    if version > DB_SCHEMA_VERSION {
        return Err(StoreError::NewerSchema {
            found: version,
            supported: DB_SCHEMA_VERSION,
        });
    }
    Ok(())
}

fn backend<E: std::error::Error + Send + Sync + 'static>(e: E) -> StoreError {
    StoreError::Backend(Box::new(e))
}

fn to_i64(n: u64) -> Result<i64> {
    i64::try_from(n).map_err(|_| StoreError::Invalid("revision out of range".into()))
}

fn to_u64(n: i64) -> u64 {
    u64::try_from(n).unwrap_or(0)
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|e| StoreError::Invalid(format!("cannot serialize: {e}")))
}

fn parse_json(text: &str) -> Result<serde_json::Value> {
    serde_json::from_str(text)
        .map_err(|e| StoreError::Invalid(format!("stored JSON is broken: {e}")))
}

fn parse_id(text: &str) -> Result<Id> {
    Id::parse(text).map_err(|e| StoreError::Invalid(e.to_string()))
}

fn reason_str(reason: SnapshotReason) -> &'static str {
    match reason {
        SnapshotReason::Export => "export",
        SnapshotReason::Import => "import",
        SnapshotReason::Manual => "manual",
        SnapshotReason::AiApply => "ai_apply",
    }
}

fn parse_reason(text: &str) -> Result<SnapshotReason> {
    Ok(match text {
        "export" => SnapshotReason::Export,
        "import" => SnapshotReason::Import,
        "manual" => SnapshotReason::Manual,
        "ai_apply" => SnapshotReason::AiApply,
        other => {
            return Err(StoreError::Invalid(format!(
                "unknown snapshot reason {other}"
            )));
        }
    })
}

/// Normalizes and validates a CV before it is written; errors block the save.
fn prepare_cv(cv: &Cv) -> Result<Cv> {
    let mut cv = cv.clone();
    let issues = validate::for_save(&mut cv);
    if validate::has_errors(&issues) {
        let detail: Vec<String> = issues
            .iter()
            .filter(|i| i.severity == cv_core::Severity::Error)
            .map(|i| format!("{} at {}", i.code, i.path))
            .collect();
        return Err(StoreError::Invalid(detail.join(", ")));
    }
    Ok(cv)
}

struct CvRow {
    title: String,
    content: String,
    revision: i64,
    created_at: String,
    updated_at: String,
}

fn read_cv_row(tx: &Connection, id: &Id) -> Result<CvRow> {
    tx.query_row(
        "SELECT title, content, revision, created_at, updated_at FROM cvs WHERE id = ?1 AND deleted_at IS NULL",
        params![id.as_str()],
        |r| {
            Ok(CvRow {
                title: r.get(0)?,
                content: r.get(1)?,
                revision: r.get(2)?,
                created_at: r.get(3)?,
                updated_at: r.get(4)?,
            })
        },
    )
    .optional()
    .map_err(backend)?
    .ok_or_else(|| StoreError::NotFound(id.clone()))
}

fn stored_cv(row: CvRow) -> Result<StoredCv> {
    let mut cv = migrate_cv_json(parse_json(&row.content)?)?;
    cv.title = row.title;
    Ok(StoredCv {
        cv,
        revision: to_u64(row.revision),
        created_at: Timestamp(row.created_at),
        updated_at: Timestamp(row.updated_at),
    })
}

struct VariantRow {
    name: String,
    content: String,
    revision: i64,
    created_at: String,
    updated_at: String,
}

fn read_variant_row(conn: &Connection, id: &Id) -> Result<VariantRow> {
    conn.query_row(
        "SELECT v.name, v.content, v.revision, v.created_at, v.updated_at FROM variants v \
         JOIN cvs c ON c.id = v.cv_id WHERE v.id = ?1 AND c.deleted_at IS NULL",
        params![id.as_str()],
        |r| {
            Ok(VariantRow {
                name: r.get(0)?,
                content: r.get(1)?,
                revision: r.get(2)?,
                created_at: r.get(3)?,
                updated_at: r.get(4)?,
            })
        },
    )
    .optional()
    .map_err(backend)?
    .ok_or_else(|| StoreError::NotFound(id.clone()))
}

fn stored_variant(row: VariantRow) -> Result<StoredVariant> {
    let mut variant = migrate_variant_json(parse_json(&row.content)?)?;
    variant.name = row.name;
    variant.revision = to_u64(row.revision);
    Ok(StoredVariant {
        variant,
        revision: to_u64(row.revision),
        created_at: Timestamp(row.created_at),
        updated_at: Timestamp(row.updated_at),
    })
}

fn insert_cv(tx: &Transaction<'_>, cv: &Cv, now: &Timestamp) -> Result<StoredCv> {
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM cvs WHERE id = ?1)",
            params![cv.id.as_str()],
            |r| r.get(0),
        )
        .map_err(backend)?;
    if exists {
        return Err(StoreError::Invalid(format!("CV {} already exists", cv.id)));
    }
    tx.execute(
        "INSERT INTO cvs (id, title, content, revision, created_at, updated_at) VALUES (?1, ?2, ?3, 1, ?4, ?4)",
        params![cv.id.as_str(), cv.title, to_json(cv)?, now.0],
    )
    .map_err(backend)?;
    Ok(StoredCv {
        cv: cv.clone(),
        revision: 1,
        created_at: now.clone(),
        updated_at: now.clone(),
    })
}

fn insert_variant(
    tx: &Transaction<'_>,
    variant: &Variant,
    now: &Timestamp,
) -> Result<StoredVariant> {
    let cv_live: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM cvs WHERE id = ?1 AND deleted_at IS NULL)",
            params![variant.cv_id.as_str()],
            |r| r.get(0),
        )
        .map_err(backend)?;
    if !cv_live {
        return Err(StoreError::NotFound(variant.cv_id.clone()));
    }
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM variants WHERE id = ?1)",
            params![variant.id.as_str()],
            |r| r.get(0),
        )
        .map_err(backend)?;
    if exists {
        return Err(StoreError::Invalid(format!(
            "variant {} already exists",
            variant.id
        )));
    }
    let mut variant = variant.clone();
    variant.revision = 1;
    tx.execute(
        "INSERT INTO variants (id, cv_id, name, content, revision, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)",
        params![variant.id.as_str(), variant.cv_id.as_str(), variant.name, to_json(&variant)?, now.0],
    )
    .map_err(backend)?;
    Ok(StoredVariant {
        variant,
        revision: 1,
        created_at: now.clone(),
        updated_at: now.clone(),
    })
}

/// Writes `cv` over the stored row if its revision is `expected`.
fn update_cv(tx: &Transaction<'_>, cv: &Cv, expected: u64, now: &Timestamp) -> Result<StoredCv> {
    let row = read_cv_row(tx, &cv.id)?;
    let current = to_u64(row.revision);
    if current != expected {
        return Err(StoreError::Conflict {
            current_revision: current,
        });
    }
    let revision = current + 1;
    tx.execute(
        "UPDATE cvs SET title = ?2, content = ?3, revision = ?4, updated_at = ?5 WHERE id = ?1",
        params![
            cv.id.as_str(),
            cv.title,
            to_json(cv)?,
            to_i64(revision)?,
            now.0
        ],
    )
    .map_err(backend)?;
    Ok(StoredCv {
        cv: cv.clone(),
        revision,
        created_at: Timestamp(row.created_at),
        updated_at: now.clone(),
    })
}

impl Store for SqliteStore {
    fn list_cvs(&self) -> Result<Vec<CvSummary>> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare(
                "SELECT c.id, c.title, c.updated_at, \
                 (SELECT COUNT(*) FROM variants v WHERE v.cv_id = c.id) \
                 FROM cvs c WHERE c.deleted_at IS NULL ORDER BY c.updated_at DESC, c.id",
            )
            .map_err(backend)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .map_err(backend)?;
        let mut out = Vec::new();
        for row in rows {
            let (id, title, updated_at, count) = row.map_err(backend)?;
            out.push(CvSummary {
                id: parse_id(&id)?,
                title,
                updated_at: Timestamp(updated_at),
                variant_count: u32::try_from(count).unwrap_or(u32::MAX),
                page_count: None,
            });
        }
        Ok(out)
    }

    fn get_cv(&self, id: &Id) -> Result<StoredCv> {
        let conn = self.lock();
        stored_cv(read_cv_row(&conn, id)?)
    }

    fn create_cv(&self, cv: &Cv) -> Result<StoredCv> {
        let cv = prepare_cv(cv)?;
        let now = self.now();
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(backend)?;
        let stored = insert_cv(&tx, &cv, &now)?;
        tx.commit().map_err(backend)?;
        Ok(stored)
    }

    fn save_cv(&self, cv: &Cv, expected_revision: u64) -> Result<StoredCv> {
        let cv = prepare_cv(cv)?;
        let now = self.now();
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(backend)?;
        let stored = update_cv(&tx, &cv, expected_revision, &now)?;
        tx.commit().map_err(backend)?;
        Ok(stored)
    }

    fn duplicate_cv(&self, id: &Id, new_title: &str, ids: &mut dyn IdSource) -> Result<StoredCv> {
        let now = self.now();
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(backend)?;
        let original = stored_cv(read_cv_row(&tx, id)?)?;
        let mut copy = original.cv.with_fresh_ids(ids);
        copy.title = new_title.to_owned();
        let copy = prepare_cv(&copy)?;
        let stored = insert_cv(&tx, &copy, &now)?;
        tx.commit().map_err(backend)?;
        Ok(stored)
    }

    fn delete_cv(&self, id: &Id) -> Result<()> {
        let now = self.now();
        let conn = self.lock();
        let changed = conn
            .execute(
                "UPDATE cvs SET deleted_at = ?2 WHERE id = ?1 AND deleted_at IS NULL",
                params![id.as_str(), now.0],
            )
            .map_err(backend)?;
        if changed == 0 {
            return Err(StoreError::NotFound(id.clone()));
        }
        Ok(())
    }

    fn restore_cv(&self, id: &Id) -> Result<()> {
        let conn = self.lock();
        let changed = conn
            .execute(
                "UPDATE cvs SET deleted_at = NULL WHERE id = ?1 AND deleted_at IS NOT NULL",
                params![id.as_str()],
            )
            .map_err(backend)?;
        if changed == 0 {
            return Err(StoreError::NotFound(id.clone()));
        }
        Ok(())
    }

    fn purge_deleted(&self, older_than: Duration) -> Result<u64> {
        let cutoff = self.ago(older_than);
        let conn = self.lock();
        let n = conn
            .execute(
                "DELETE FROM cvs WHERE deleted_at IS NOT NULL AND deleted_at < ?1",
                params![cutoff.0],
            )
            .map_err(backend)?;
        Ok(n as u64)
    }

    fn list_variants(&self, cv_id: &Id) -> Result<Vec<VariantSummary>> {
        let conn = self.lock();
        read_cv_row(&conn, cv_id)?;
        let mut stmt = conn
            .prepare("SELECT id, name, updated_at FROM variants WHERE cv_id = ?1 ORDER BY updated_at DESC, id")
            .map_err(backend)?;
        let rows = stmt
            .query_map(params![cv_id.as_str()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(backend)?;
        let mut out = Vec::new();
        for row in rows {
            let (id, name, updated_at) = row.map_err(backend)?;
            out.push(VariantSummary {
                id: parse_id(&id)?,
                cv_id: cv_id.clone(),
                name,
                updated_at: Timestamp(updated_at),
            });
        }
        Ok(out)
    }

    fn get_variant(&self, id: &Id) -> Result<StoredVariant> {
        let conn = self.lock();
        stored_variant(read_variant_row(&conn, id)?)
    }

    fn create_variant(&self, variant: &Variant) -> Result<StoredVariant> {
        let now = self.now();
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(backend)?;
        let stored = insert_variant(&tx, variant, &now)?;
        tx.commit().map_err(backend)?;
        Ok(stored)
    }

    fn save_variant(&self, variant: &Variant, expected_revision: u64) -> Result<StoredVariant> {
        let now = self.now();
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(backend)?;
        let row = read_variant_row(&tx, &variant.id)?;
        let current = to_u64(row.revision);
        if current != expected_revision {
            return Err(StoreError::Conflict {
                current_revision: current,
            });
        }
        let stored_cv_id: String = tx
            .query_row(
                "SELECT cv_id FROM variants WHERE id = ?1",
                params![variant.id.as_str()],
                |r| r.get(0),
            )
            .map_err(backend)?;
        if stored_cv_id != variant.cv_id.as_str() {
            return Err(StoreError::Invalid(
                "a variant cannot move to another CV".into(),
            ));
        }
        let mut variant = variant.clone();
        variant.revision = current + 1;
        tx.execute(
            "UPDATE variants SET name = ?2, content = ?3, revision = ?4, updated_at = ?5 WHERE id = ?1",
            params![variant.id.as_str(), variant.name, to_json(&variant)?, to_i64(variant.revision)?, now.0],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)?;
        Ok(StoredVariant {
            revision: variant.revision,
            variant,
            created_at: Timestamp(row.created_at),
            updated_at: now,
        })
    }

    fn delete_variant(&self, id: &Id) -> Result<()> {
        let conn = self.lock();
        let changed = conn
            .execute("DELETE FROM variants WHERE id = ?1", params![id.as_str()])
            .map_err(backend)?;
        if changed == 0 {
            return Err(StoreError::NotFound(id.clone()));
        }
        Ok(())
    }

    fn duplicate_variant(
        &self,
        id: &Id,
        new_name: &str,
        ids: &mut dyn IdSource,
    ) -> Result<StoredVariant> {
        let now = self.now();
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(backend)?;
        let mut copy = stored_variant(read_variant_row(&tx, id)?)?.variant;
        copy.id = ids.next_id();
        copy.name = new_name.to_owned();
        for suggestion in &mut copy.suggestions {
            suggestion.id = ids.next_id();
        }
        let stored = insert_variant(&tx, &copy, &now)?;
        tx.commit().map_err(backend)?;
        Ok(stored)
    }

    fn add_snapshot(
        &self,
        cv_id: &Id,
        reason: SnapshotReason,
        ids: &mut dyn IdSource,
    ) -> Result<()> {
        let now = self.now();
        let throttle_since = self.ago(EXPORT_SNAPSHOT_INTERVAL);
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(backend)?;
        let row = read_cv_row(&tx, cv_id)?;
        if reason == SnapshotReason::Export {
            let recent: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM snapshots WHERE cv_id = ?1 AND reason = 'export' AND created_at > ?2)",
                    params![cv_id.as_str(), throttle_since.0],
                    |r| r.get(0),
                )
                .map_err(backend)?;
            if recent {
                return Ok(());
            }
        }
        tx.execute(
            "INSERT INTO snapshots (id, cv_id, content, reason, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![ids.next_id().as_str(), cv_id.as_str(), row.content, reason_str(reason), now.0],
        )
        .map_err(backend)?;
        tx.execute(
            "DELETE FROM snapshots WHERE cv_id = ?1 AND rowid NOT IN \
             (SELECT rowid FROM snapshots WHERE cv_id = ?1 ORDER BY created_at DESC, rowid DESC LIMIT ?2)",
            params![cv_id.as_str(), SNAPSHOTS_PER_CV],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)
    }

    fn list_snapshots(&self, cv_id: &Id) -> Result<Vec<SnapshotSummary>> {
        let conn = self.lock();
        read_cv_row(&conn, cv_id)?;
        let mut stmt = conn
            .prepare("SELECT id, reason, created_at FROM snapshots WHERE cv_id = ?1 ORDER BY created_at DESC, rowid DESC")
            .map_err(backend)?;
        let rows = stmt
            .query_map(params![cv_id.as_str()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(backend)?;
        let mut out = Vec::new();
        for row in rows {
            let (id, reason, created_at) = row.map_err(backend)?;
            out.push(SnapshotSummary {
                id: parse_id(&id)?,
                cv_id: cv_id.clone(),
                reason: parse_reason(&reason)?,
                created_at: Timestamp(created_at),
            });
        }
        Ok(out)
    }

    fn restore_snapshot(&self, snapshot_id: &Id, expected_revision: u64) -> Result<StoredCv> {
        let now = self.now();
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(backend)?;
        let (cv_id, content): (String, String) = tx
            .query_row(
                "SELECT cv_id, content FROM snapshots WHERE id = ?1",
                params![snapshot_id.as_str()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(backend)?
            .ok_or_else(|| StoreError::NotFound(snapshot_id.clone()))?;
        let mut cv = migrate_cv_json(parse_json(&content)?)?;
        if cv.id.as_str() != cv_id {
            return Err(StoreError::Invalid("snapshot belongs to another CV".into()));
        }
        // The title column is authoritative; a restore keeps the current title.
        cv.title = read_cv_row(&tx, &cv.id)?.title;
        let cv = prepare_cv(&cv)?;
        let stored = update_cv(&tx, &cv, expected_revision, &now)?;
        tx.commit().map_err(backend)?;
        Ok(stored)
    }

    fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.lock();
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .optional()
        .map_err(backend)
    }

    fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(backend)?;
        Ok(())
    }

    fn ai_cache_get(&self, key: &str) -> Result<Option<String>> {
        let conn = self.lock();
        conn.query_row(
            "SELECT response FROM ai_cache WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .optional()
        .map_err(backend)
    }

    fn ai_cache_put(&self, key: &str, kind: &str, response: &str) -> Result<()> {
        let now = self.now();
        let conn = self.lock();
        conn.execute(
            "INSERT INTO ai_cache (key, kind, response, created_at) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(key) DO UPDATE SET kind = excluded.kind, response = excluded.response, created_at = excluded.created_at",
            params![key, kind, response, now.0],
        )
        .map_err(backend)?;
        Ok(())
    }

    fn ai_cache_clear(&self) -> Result<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM ai_cache", []).map_err(backend)?;
        Ok(())
    }

    fn backup_to(&self, path: &Path) -> Result<()> {
        let target = path
            .to_str()
            .ok_or_else(|| StoreError::Invalid("the backup path is not valid UTF-8".into()))?;
        if path.exists() {
            return Err(StoreError::Invalid("the backup file already exists".into()));
        }
        let conn = self.lock();
        conn.execute("VACUUM INTO ?1", params![target])
            .map_err(backend)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Storage-specific tests that need raw SQL. Behaviour tests go through
    //! the `Store` trait in `tests/store_contract.rs`.

    use super::*;
    use crate::clock::SystemClock;

    #[test]
    fn migrations_set_user_version_and_are_idempotent() {
        let store = SqliteStore::open_in_memory(Box::new(SystemClock)).expect("open");
        let conn = store.lock();
        assert_eq!(user_version(&conn).expect("version"), DB_SCHEMA_VERSION);
        migrate_db(&conn).expect("re-run is a no-op");
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .expect("pragma");
        assert_eq!(fk, 1);
    }

    #[test]
    fn newer_database_schema_is_refused() {
        let conn = Connection::open_in_memory().expect("open");
        conn.execute_batch("PRAGMA user_version = 99").expect("set");
        assert!(matches!(
            migrate_db(&conn),
            Err(StoreError::NewerSchema {
                found: 99,
                supported: 1
            })
        ));
    }

    #[test]
    fn v0_content_in_the_database_is_migrated_on_load() {
        let store = SqliteStore::open_in_memory(Box::new(SystemClock)).expect("open");
        let v0 = include_str!("../tests/fixtures/cv_v0.json");
        store
            .lock()
            .execute(
                "INSERT INTO cvs (id, title, content, revision, created_at, updated_at) \
                 VALUES ('01HZX3M8Q4V7K2N9P5R0000001', 'Main CV', ?1, 3, 't', 't')",
                params![v0],
            )
            .expect("insert");
        let id = Id::parse("01HZX3M8Q4V7K2N9P5R0000001").expect("id");
        let stored = store.get_cv(&id).expect("load");
        assert_eq!(stored.cv.basics.full_name, "Jane Doe");
        assert_eq!(stored.cv.schema_version, cv_core::CV_SCHEMA_VERSION);
        assert_eq!(stored.revision, 3);
    }

    #[test]
    fn title_column_wins_over_json_copy() {
        let store = SqliteStore::open_in_memory(Box::new(SystemClock)).expect("open");
        let v0 = include_str!("../tests/fixtures/cv_v0.json");
        store
            .lock()
            .execute(
                "INSERT INTO cvs (id, title, content, revision, created_at, updated_at) \
                 VALUES ('01HZX3M8Q4V7K2N9P5R0000001', 'Renamed', ?1, 1, 't', 't')",
                params![v0],
            )
            .expect("insert");
        let id = Id::parse("01HZX3M8Q4V7K2N9P5R0000001").expect("id");
        assert_eq!(store.get_cv(&id).expect("load").cv.title, "Renamed");
    }

    #[test]
    fn snapshot_reasons_round_trip() {
        for r in [
            SnapshotReason::Export,
            SnapshotReason::Import,
            SnapshotReason::Manual,
            SnapshotReason::AiApply,
        ] {
            assert_eq!(parse_reason(reason_str(r)).expect("parse"), r);
        }
        assert!(parse_reason("other").is_err());
    }
}
