//! Behaviour of the storage interface, tested through `&dyn Store` only, so
//! nothing here depends on SQLite connection types (spec section 17, M1).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cv_core::{
    Basics, Bullet, CV_SCHEMA_VERSION, Cv, Entry, EntrySection, Id, IdSource, Locale, Paper,
    Period, Section, SnapshotReason, TemplateId, TemplateOptions, Variant,
};
use cv_store::{Clock, SNAPSHOTS_PER_CV, SqliteStore, Store, StoreError, StoredPhoto};

/// A clock tests move by hand, in milliseconds since the epoch.
#[derive(Clone)]
struct TestClock(Arc<AtomicU64>);

impl TestClock {
    fn new() -> Self {
        // 2026-10-13T12:00:00Z
        Self(Arc::new(AtomicU64::new(1_791_892_800_000)))
    }

    fn advance(&self, d: Duration) {
        let ms = u64::try_from(d.as_millis()).expect("small");
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for TestClock {
    fn now(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_millis(self.0.load(Ordering::SeqCst))
    }
}

struct Ids(u32);

impl IdSource for Ids {
    fn next_id(&mut self) -> Id {
        self.0 += 1;
        id(self.0)
    }
}

fn id(n: u32) -> Id {
    Id::parse(&format!("01HZX3M8Q4V7K2N9P5R{n:07}")).expect("test ULID")
}

fn cv(n: u32) -> Cv {
    Cv {
        schema_version: CV_SCHEMA_VERSION,
        id: id(n),
        title: format!("CV {n}"),
        locale: Locale::En,
        paper: Paper::A4,
        template: TemplateId::default(),
        options: TemplateOptions::default(),
        basics: Basics {
            full_name: "Jane Doe".into(),
            headline: None,
            email: Some("jane@example.com".into()),
            phone: None,
            location: None,
            links: vec![],
            show_photo: false,
            personal: Default::default(),
            show_personal: Default::default(),
        },
        summary: None,
        sections: vec![Section::Experience(EntrySection {
            id: id(n + 1),
            title_override: None,
            entries: vec![Entry {
                id: id(n + 2),
                title: "Engineer".into(),
                org: "Acme".into(),
                location: None,
                period: Period {
                    start: None,
                    end: None,
                    current: true,
                },
                bullets: vec![Bullet {
                    id: id(n + 3),
                    text: "Built a billing service".into(),
                }],
                description: None,
                url: None,
                locked: false,
            }],
        })],
        layout: cv_core::Layout::default(),
    }
}

fn store() -> (Box<dyn Store>, TestClock) {
    let clock = TestClock::new();
    let store = SqliteStore::open_in_memory(Box::new(clock.clone())).expect("open");
    (Box::new(store), clock)
}

#[test]
fn create_get_and_list() {
    let (s, clock) = store();
    let created = s.create_cv(&cv(10)).expect("create");
    assert_eq!(created.revision, 1);
    assert_eq!(created.created_at.0, "2026-10-13T12:00:00.000Z");
    clock.advance(Duration::from_secs(1));
    s.create_cv(&cv(20)).expect("create");

    let got = s.get_cv(&id(10)).expect("get");
    assert_eq!(got.cv, cv(10));
    let list = s.list_cvs().expect("list");
    let titles: Vec<_> = list.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, ["CV 20", "CV 10"], "most recently updated first");
    assert!(matches!(s.get_cv(&id(99)), Err(StoreError::NotFound(_))));
    assert!(matches!(s.create_cv(&cv(10)), Err(StoreError::Invalid(_))));
}

#[test]
fn save_increments_revision_and_detects_conflicts() {
    let (s, clock) = store();
    s.create_cv(&cv(10)).expect("create");
    let mut edited = cv(10);
    edited.title = "Renamed".into();
    clock.advance(Duration::from_secs(5));
    let saved = s.save_cv(&edited, 1).expect("save");
    assert_eq!(saved.revision, 2);
    assert_eq!(saved.updated_at.0, "2026-10-13T12:00:05.000Z");
    assert_eq!(s.get_cv(&id(10)).expect("get").cv.title, "Renamed");
    assert_eq!(s.list_cvs().expect("list")[0].title, "Renamed");

    // A second window still holding revision 1 loses.
    match s.save_cv(&cv(10), 1) {
        Err(StoreError::Conflict { current_revision }) => assert_eq!(current_revision, 2),
        other => panic!("expected conflict, got {other:?}"),
    }
    assert_eq!(s.get_cv(&id(10)).expect("get").revision, 2);
}

#[test]
fn save_normalizes_options_and_refuses_structural_errors() {
    let (s, _) = store();
    let mut c = cv(10);
    c.options.font_size_pt = 40.0;
    let stored = s.create_cv(&c).expect("create");
    assert_eq!(stored.cv.options.font_size_pt, 12.0);
    assert_eq!(
        s.get_cv(&id(10)).expect("get").cv.options.font_size_pt,
        12.0
    );

    let mut dup = cv(10);
    if let Section::Experience(sec) = &mut dup.sections[0] {
        sec.entries[0].bullets[0].id = id(12);
    }
    match s.save_cv(&dup, 1) {
        Err(StoreError::Invalid(msg)) => assert!(msg.contains("duplicate_id"), "{msg}"),
        other => panic!("expected invalid, got {other:?}"),
    }
}

#[test]
fn soft_delete_restore_and_purge() {
    let (s, clock) = store();
    s.create_cv(&cv(10)).expect("create");
    s.create_cv(&cv(20)).expect("create");
    s.delete_cv(&id(10)).expect("delete");
    assert!(matches!(s.get_cv(&id(10)), Err(StoreError::NotFound(_))));
    assert_eq!(s.list_cvs().expect("list").len(), 1);
    assert!(matches!(s.delete_cv(&id(10)), Err(StoreError::NotFound(_))));

    s.restore_cv(&id(10)).expect("restore");
    assert_eq!(s.get_cv(&id(10)).expect("get").cv, cv(10));
    assert!(matches!(
        s.restore_cv(&id(10)),
        Err(StoreError::NotFound(_))
    ));

    s.delete_cv(&id(10)).expect("delete");
    let thirty_days = Duration::from_secs(30 * 86_400);
    clock.advance(Duration::from_secs(29 * 86_400));
    assert_eq!(s.purge_deleted(thirty_days).expect("purge"), 0);
    clock.advance(Duration::from_secs(2 * 86_400));
    assert_eq!(s.purge_deleted(thirty_days).expect("purge"), 1);
    assert!(matches!(
        s.restore_cv(&id(10)),
        Err(StoreError::NotFound(_))
    ));
    assert_eq!(s.list_cvs().expect("list").len(), 1);
}

#[test]
fn duplicate_cv_gets_fresh_ids() {
    let (s, _) = store();
    s.create_cv(&cv(10)).expect("create");
    let copy = s
        .duplicate_cv(&id(10), "Copy", &mut Ids(500))
        .expect("duplicate");
    assert_eq!(copy.cv.id, id(501));
    assert_eq!(copy.cv.title, "Copy");
    assert_eq!(copy.cv.basics, cv(10).basics);
    assert!(
        copy.cv
            .entity_ids()
            .iter()
            .all(|i| i.as_str() > id(500).as_str())
    );
    assert_eq!(s.list_cvs().expect("list").len(), 2);
}

#[test]
fn variants_crud_conflict_and_cascade() {
    let (s, _) = store();
    s.create_cv(&cv(10)).expect("create");
    let mut v = Variant::empty(id(100), id(10), "Acme");
    v.hidden.insert(id(13));
    let created = s.create_variant(&v).expect("create variant");
    assert_eq!(created.revision, 1);
    assert_eq!(s.list_cvs().expect("list")[0].variant_count, 1);

    v.name = "Acme, senior".into();
    let saved = s.save_variant(&v, 1).expect("save");
    assert_eq!((saved.revision, saved.variant.revision), (2, 2));
    assert!(matches!(
        s.save_variant(&v, 1),
        Err(StoreError::Conflict {
            current_revision: 2
        })
    ));
    let got = s.get_variant(&id(100)).expect("get");
    assert_eq!(got.variant.name, "Acme, senior");
    assert!(got.variant.hidden.contains(&id(13)));

    let copy = s
        .duplicate_variant(&id(100), "Copy", &mut Ids(700))
        .expect("dup");
    assert_eq!(
        (copy.variant.id.clone(), copy.variant.cv_id.clone()),
        (id(701), id(10))
    );
    let names: Vec<_> = s
        .list_variants(&id(10))
        .expect("list")
        .into_iter()
        .map(|v| v.name)
        .collect();
    assert_eq!(names.len(), 2);

    s.delete_variant(&id(701)).expect("delete");
    assert!(matches!(
        s.get_variant(&id(701)),
        Err(StoreError::NotFound(_))
    ));

    let mut moved = v.clone();
    moved.cv_id = id(99);
    assert!(matches!(
        s.save_variant(&moved, 2),
        Err(StoreError::Invalid(_))
    ));
    assert!(matches!(
        s.create_variant(&Variant::empty(id(200), id(99), "Orphan")),
        Err(StoreError::NotFound(_))
    ));

    // Variants of a soft-deleted CV are hidden; purging removes them.
    s.delete_cv(&id(10)).expect("delete cv");
    assert!(matches!(
        s.get_variant(&id(100)),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn snapshots_restore_throttle_and_retention() {
    let (s, clock) = store();
    s.create_cv(&cv(10)).expect("create");
    let mut ids = Ids(1000);
    s.add_snapshot(&id(10), SnapshotReason::Manual, &mut ids)
        .expect("snapshot");

    let mut edited = cv(10);
    edited.basics.full_name = "Jane Q. Doe".into();
    edited.title = "Edited title".into();
    s.save_cv(&edited, 1).expect("save");

    let snaps = s.list_snapshots(&id(10)).expect("list");
    assert_eq!(snaps.len(), 1);
    assert_eq!(snaps[0].reason, SnapshotReason::Manual);
    let restored = s.restore_snapshot(&snaps[0].id, 2).expect("restore");
    assert_eq!(restored.revision, 3);
    assert_eq!(restored.cv.basics.full_name, "Jane Doe");
    assert_eq!(restored.cv.title, "Edited title", "title column is kept");
    assert!(matches!(
        s.restore_snapshot(&snaps[0].id, 2),
        Err(StoreError::Conflict {
            current_revision: 3
        })
    ));

    // Export snapshots: at most one per hour.
    s.add_snapshot(&id(10), SnapshotReason::Export, &mut ids)
        .expect("export");
    clock.advance(Duration::from_secs(59 * 60));
    s.add_snapshot(&id(10), SnapshotReason::Export, &mut ids)
        .expect("throttled");
    assert_eq!(s.list_snapshots(&id(10)).expect("list").len(), 2);
    clock.advance(Duration::from_secs(2 * 60));
    s.add_snapshot(&id(10), SnapshotReason::Export, &mut ids)
        .expect("export");
    assert_eq!(s.list_snapshots(&id(10)).expect("list").len(), 3);

    // Retention: only the latest 50 stay, newest first.
    for _ in 0..60 {
        clock.advance(Duration::from_millis(1));
        s.add_snapshot(&id(10), SnapshotReason::Import, &mut ids)
            .expect("snapshot");
    }
    let snaps = s.list_snapshots(&id(10)).expect("list");
    assert_eq!(snaps.len(), SNAPSHOTS_PER_CV as usize);
    assert!(snaps.windows(2).all(|w| w[0].created_at >= w[1].created_at));
    assert!(snaps.iter().all(|s| s.reason == SnapshotReason::Import));

    assert!(matches!(
        s.add_snapshot(&id(99), SnapshotReason::Manual, &mut ids),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn settings_and_ai_cache() {
    let (s, _) = store();
    assert_eq!(s.get_setting("ai.provider").expect("get"), None);
    s.set_setting("ai.provider", "none").expect("set");
    s.set_setting("ai.provider", "anthropic")
        .expect("overwrite");
    assert_eq!(
        s.get_setting("ai.provider").expect("get").as_deref(),
        Some("anthropic")
    );

    assert_eq!(s.ai_cache_get("k").expect("get"), None);
    s.ai_cache_put("k", "job_parse", "{}").expect("put");
    s.ai_cache_put("k", "job_parse", "{\"a\":1}")
        .expect("overwrite");
    assert_eq!(
        s.ai_cache_get("k").expect("get").as_deref(),
        Some("{\"a\":1}")
    );
    s.ai_cache_clear().expect("clear");
    assert_eq!(s.ai_cache_get("k").expect("get"), None);
}

#[test]
fn file_database_survives_reopen_backup_and_restore() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("pinaxcv.sqlite3");
    let clock = TestClock::new();
    {
        let s = SqliteStore::open_with_clock(&db, Box::new(clock.clone())).expect("open");
        s.create_cv(&cv(10)).expect("create");
    }
    let s = SqliteStore::open_with_clock(&db, Box::new(clock.clone())).expect("reopen");
    assert_eq!(
        s.get_cv(&id(10)).expect("get").cv,
        cv(10),
        "data survives a restart"
    );

    let backup = dir.path().join("backup.sqlite3");
    s.backup_to(&backup).expect("backup");
    assert!(
        matches!(s.backup_to(&backup), Err(StoreError::Invalid(_))),
        "never overwrites"
    );
    let empty = dir.path().join("empty.sqlite3");
    std::fs::write(&empty, b"").expect("empty file");
    s.backup_to(&empty).expect("fills an empty file");
    assert!(std::fs::metadata(&empty).expect("meta").len() > 0);

    s.create_cv(&cv(20)).expect("create after backup");
    clock.advance(Duration::from_secs(1));
    let safety = s.restore_from(&backup).expect("restore");
    assert!(safety.exists());
    let titles: Vec<_> = s
        .list_cvs()
        .expect("list")
        .into_iter()
        .map(|c| c.title)
        .collect();
    assert_eq!(titles, ["CV 10"], "restored to the backup");

    // The safety copy holds the pre-restore state.
    let before =
        SqliteStore::open_with_clock(&safety, Box::new(clock.clone())).expect("open safety");
    assert_eq!(before.list_cvs().expect("list").len(), 2);

    // Garbage is refused and the live database is untouched.
    let junk = dir.path().join("junk.sqlite3");
    std::fs::write(&junk, b"not a database").expect("write");
    assert!(matches!(s.restore_from(&junk), Err(StoreError::Invalid(_))));
    assert_eq!(s.list_cvs().expect("list").len(), 1);

    // The live database itself, or a hard link to it, is refused or restored
    // without truncating it.
    assert!(matches!(s.restore_from(&db), Err(StoreError::Invalid(_))));
    assert_eq!(s.list_cvs().expect("list").len(), 1);
    let alias = dir.path().join("alias.sqlite3");
    if std::fs::hard_link(&db, &alias).is_ok() {
        clock.advance(Duration::from_secs(1));
        let _ = s.restore_from(&alias);
        assert_eq!(s.list_cvs().expect("list after alias").len(), 1);
    }

    let memory = SqliteStore::open_in_memory(Box::new(clock)).expect("open");
    assert!(matches!(
        memory.restore_from(&backup),
        Err(StoreError::Invalid(_))
    ));
}

#[test]
fn delete_all_data_leaves_an_empty_working_store() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("pinaxcv.sqlite3");
    let clock = TestClock::new();
    let s = SqliteStore::open_with_clock(&db, Box::new(clock.clone())).expect("open");
    let mut secret = cv(10);
    secret.title = "Unmistakable-Title-4711".into();
    s.create_cv(&secret).expect("create");
    s.set_setting("ai.model", "m").expect("setting");
    s.ai_cache_put("k", "job_parse", "{\"cached\":\"Unmistakable-Title-4711\"}")
        .expect("cache");
    s.photo_put(&secret.id, &photo(b"Unmistakable-Photo-0815"))
        .expect("photo");
    let backup = dir.path().join("backup.sqlite3");
    s.backup_to(&backup).expect("backup");
    clock.advance(Duration::from_secs(1));
    let copy = s.restore_from(&backup).expect("restore");
    assert!(copy.exists());

    s.delete_all_data().expect("delete");
    assert!(s.list_cvs().expect("list").is_empty());
    assert_eq!(s.get_setting("ai.model").expect("get"), None);
    assert_eq!(s.ai_cache_get("k").expect("get"), None);
    assert_eq!(s.photo_get(&secret.id).expect("get"), None);
    assert!(!copy.exists(), "restore copies are removed");
    assert!(backup.exists(), "files elsewhere are not touched");
    // Nothing of the deleted CV is left in the database or its WAL.
    for file in [db.clone(), dir.path().join("pinaxcv.sqlite3-wal")] {
        if let Ok(bytes) = std::fs::read(&file) {
            for needle in [&b"Unmistakable-Title-4711"[..], b"Unmistakable-Photo-0815"] {
                let found = bytes.windows(needle.len()).any(|w| w == needle);
                assert!(!found, "{} still holds deleted data", file.display());
            }
        }
    }
    // The store keeps working.
    s.create_cv(&cv(20)).expect("create after delete");
    assert_eq!(s.list_cvs().expect("list").len(), 1);
}

#[test]
fn store_is_shareable_across_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SqliteStore>();
    let (s, _) = store();
    let s: Arc<dyn Store> = Arc::from(s);
    let handles: Vec<_> = (0..4)
        .map(|i| {
            let s = Arc::clone(&s);
            std::thread::spawn(move || s.create_cv(&cv(100 * (i + 1))).expect("create"))
        })
        .collect();
    for h in handles {
        h.join().expect("thread");
    }
    assert_eq!(s.list_cvs().expect("list").len(), 4);
}

#[test]
fn review_sessions_keep_the_latest_per_live_cv() {
    let (s, clock) = store();
    s.create_cv(&cv(10)).expect("create");
    assert_eq!(s.review_session_get(&id(10)).expect("get"), None);
    s.review_session_put(&id(10), "{\"a\":1}").expect("put");
    clock.advance(Duration::from_secs(2));
    let at = s.review_session_put(&id(10), "{\"a\":2}").expect("replace");
    assert_eq!(at.0, "2026-10-13T12:00:02.000Z");
    let (content, saved) = s.review_session_get(&id(10)).expect("get").expect("some");
    assert_eq!((content.as_str(), saved), ("{\"a\":2}", at.clone()));
    clock.advance(Duration::from_secs(2));
    s.review_session_update(&id(10), "{\"a\":3}")
        .expect("update");
    let (content, saved) = s.review_session_get(&id(10)).expect("get").expect("some");
    assert_eq!((content.as_str(), saved), ("{\"a\":3}", at), "time kept");
    assert!(matches!(
        s.review_session_update(&id(20), "{}"),
        Err(StoreError::NotFound(_))
    ));
    assert!(matches!(
        s.review_session_put(&id(99), "{}"),
        Err(StoreError::NotFound(_))
    ));
    s.delete_cv(&id(10)).expect("delete");
    assert_eq!(s.review_session_get(&id(10)).expect("get"), None);
}

fn photo(marker: &[u8]) -> StoredPhoto {
    StoredPhoto {
        original: [b"original-".as_slice(), marker].concat(),
        image: [b"image-".as_slice(), marker].concat(),
        crop: "{\"x\":0,\"y\":0,\"width\":10,\"height\":10}".into(),
    }
}

#[test]
fn photos_belong_to_live_cvs_and_follow_duplicates() {
    let (s, _clock) = store();
    let base = s.create_cv(&cv(1)).expect("create").cv;
    assert_eq!(s.photo_get(&base.id).expect("get"), None);
    s.photo_put(&base.id, &photo(b"a")).expect("put");
    assert_eq!(s.photo_get(&base.id).expect("get"), Some(photo(b"a")));
    s.photo_put(&base.id, &photo(b"b")).expect("replace");
    assert_eq!(s.photo_get(&base.id).expect("get"), Some(photo(b"b")));

    let copy = s
        .duplicate_cv(&base.id, "Copy", &mut Ids(100))
        .expect("duplicate")
        .cv;
    assert_eq!(s.photo_get(&copy.id).expect("get"), Some(photo(b"b")));
    s.photo_delete(&copy.id).expect("delete");
    assert_eq!(s.photo_get(&copy.id).expect("get"), None);
    assert_eq!(
        s.photo_get(&base.id).expect("get"),
        Some(photo(b"b")),
        "the original keeps its photo"
    );

    // A deleted CV hides its photo until it is restored, and takes none.
    s.delete_cv(&base.id).expect("delete cv");
    assert_eq!(s.photo_get(&base.id).expect("get"), None);
    assert!(matches!(
        s.photo_put(&base.id, &photo(b"c")),
        Err(StoreError::NotFound(_))
    ));
    s.restore_cv(&base.id).expect("restore cv");
    assert_eq!(s.photo_get(&base.id).expect("get"), Some(photo(b"b")));
    assert!(matches!(
        s.photo_put(&id(999), &photo(b"c")),
        Err(StoreError::NotFound(_))
    ));
}
