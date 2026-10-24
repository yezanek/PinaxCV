//! `ai_review` with `FakeLlm`: stage G on the base CV against a real
//! SQLite store, then accepting and rejecting its suggestions.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use cv_ai::{FakeLlm, Llm};
use cv_api::{
    AiConfig, AiProvider, AiReview, ErrorCode, NewCv, ReviewDecide, ReviewNoteCategory,
    ReviewSession, SaveCv,
};
use cv_app::{App, ExportKind, FileDialog, JobEvent, JobEvents, UlidIds};
use cv_core::{Cv, Id, Section, SuggestionSource, SuggestionStatus, SuggestionTarget};
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

/// IDs in `testdata/cvs/typical.json`.
const RECENT: &str = "01J02000000000000000000003";
const B1: &str = "01J02000000000000000000004";
const B2: &str = "01J02000000000000000000005";
const B3: &str = "01J02000000000000000000006";

fn id(s: &str) -> Id {
    Id::parse(s).expect("id")
}

fn setup(llm: FakeLlm) -> (Arc<App>, Id) {
    let store = SqliteStore::open_in_memory(Box::new(SystemClock)).expect("store");
    let app = App::new(
        Box::new(store),
        Box::new(NoDialog),
        Box::new(UlidIds),
        "preview://localhost",
    )
    .with_llm(Arc::new(llm) as Arc<dyn Llm>);
    app.ai_configure(AiConfig {
        provider: AiProvider::OpenaiCompatible,
        model: "local-model".into(),
        base_url: Some("http://localhost:11434/v1".into()),
        api_key: None,
        send_name: false,
        send_location: true,
    })
    .expect("configure");
    app.ai_consent().expect("consent");
    let created = app
        .create_cv(NewCv {
            title: "Main".into(),
        })
        .expect("cv");
    let mut cv: Cv =
        serde_json::from_str(include_str!("../../../testdata/cvs/typical.json")).expect("fixture");
    cv.id = created.cv.id.clone();
    app.save_cv(SaveCv {
        cv,
        expected_revision: created.revision,
    })
    .expect("save");
    (Arc::new(app), created.cv.id)
}

async fn review(app: &Arc<App>, cv_id: &Id) -> Result<ReviewSession, ErrorCode> {
    let events = Arc::new(Recorded::default());
    let (_, job) = app
        .ai_review(
            events.clone(),
            AiReview {
                cv_id: cv_id.clone(),
            },
        )
        .expect("started");
    job.await;
    let last = events.0.lock().expect("lock").last().cloned();
    match last {
        Some(JobEvent::Done(d)) => Ok(serde_json::from_value(d.result).expect("result")),
        Some(JobEvent::Failed(f)) => Err(f.error.code),
        _ => panic!("no final event"),
    }
}

fn bullet_text(cv: &Cv, bullet: &str) -> String {
    cv.sections
        .iter()
        .find_map(|s| match s {
            Section::Experience(s) => s
                .entries
                .iter()
                .flat_map(|e| &e.bullets)
                .find(|b| b.id.as_str() == bullet)
                .map(|b| b.text.clone()),
            _ => None,
        })
        .expect("bullet")
}

#[tokio::test]
async fn reviews_are_saved_and_accepted_suggestions_change_the_base_cv() {
    let llm = FakeLlm::new().reply(
        "ReviewResult",
        json!({
            "rewrites": [
                { "bullet_id": B1, "new_text": "Migrated the billing platform to Rust, cutting p99 latency by 43%.",
                  "change_type": "Tighten", "reason": "Starts with the result." },
                { "bullet_id": B2, "new_text": "Designed an event-sourced ledger for 5 million transactions a day.",
                  "change_type": "Impact", "reason": "Rounder." },
                { "bullet_id": B3, "new_text": "Mentored four engineers and brought property-based testing to three services.",
                  "change_type": "Clarity", "reason": "Clearer verb." }
            ],
            "notes": [
                { "target_id": RECENT, "category": "Impact", "message": "Say how many teams used the ledger, if you know." },
                { "target_id": null, "category": "Consistency", "message": "Use one date style throughout." }
            ],
            "summary_suggestion": "Backend engineer who led a billing migration to Rust."
        }),
    );
    let (app, cv_id) = setup(llm);
    assert_eq!(app.get_review(&cv_id).expect("none yet"), None);

    let session = review(&app, &cv_id).await.expect("reviewed");
    let targets: Vec<_> = session
        .suggestions
        .iter()
        .map(|s| s.target.clone())
        .collect();
    assert_eq!(
        targets,
        [
            SuggestionTarget::Bullet { id: id(B1) },
            SuggestionTarget::Bullet { id: id(B3) },
            SuggestionTarget::Summary
        ]
    );
    assert!(
        session
            .suggestions
            .iter()
            .all(|s| s.source == SuggestionSource::Review && s.status == SuggestionStatus::Pending)
    );
    // The new number dropped one rewrite.
    assert_eq!(session.discarded.len(), 1);
    assert_eq!(session.discarded[0].bullet_id, Some(id(B2)));
    let notes: Vec<_> = session
        .notes
        .iter()
        .map(|n| (n.target.clone(), n.category))
        .collect();
    assert_eq!(
        notes,
        [
            (Some(id(RECENT)), ReviewNoteCategory::Impact),
            (None, ReviewNoteCategory::Consistency)
        ]
    );
    assert_eq!(
        app.get_review(&cv_id).expect("saved"),
        Some(session.clone())
    );

    // Edit then accept: the edited text goes into the base CV.
    let decide = |sid: &Id, accept: bool, text: Option<&str>| {
        app.review_decide(ReviewDecide {
            cv_id: cv_id.clone(),
            suggestion_id: sid.clone(),
            accept,
            text: text.map(str::to_owned),
        })
    };
    let first = session.suggestions[0].id.clone();
    let edited = "Migrated billing to Rust, cutting p99 latency by 43%.";
    let decided = decide(&first, true, Some(edited)).expect("accepted");
    let saved = decided.cv.expect("cv saved");
    assert_eq!(bullet_text(&saved.cv, B1), edited);
    assert_eq!(
        decided.session.suggestions[0].status,
        SuggestionStatus::Accepted
    );
    assert_eq!(decided.session.created_at, session.created_at);
    assert_eq!(
        decide(&first, true, None).map_err(|e| e.code).err(),
        Some(ErrorCode::NotFound)
    );

    // Rejecting changes nothing in the CV.
    let summary = session.suggestions[2].id.clone();
    let decided = decide(&summary, false, None).expect("rejected");
    assert!(decided.cv.is_none());
    assert_eq!(
        decided.session.suggestions[2].status,
        SuggestionStatus::Rejected
    );

    // A bullet edited since the review is not overwritten.
    let mut cv = app.get_cv(&cv_id).expect("cv");
    for s in &mut cv.cv.sections {
        if let Section::Experience(s) = s {
            for b in s.entries.iter_mut().flat_map(|e| &mut e.bullets) {
                if b.id.as_str() == B3 {
                    b.text = "Mentored four engineers.".into();
                }
            }
        }
    }
    app.save_cv(SaveCv {
        cv: cv.cv,
        expected_revision: cv.revision,
    })
    .expect("edit");
    let third = session.suggestions[1].id.clone();
    assert_eq!(
        decide(&third, true, None).map_err(|e| e.code).err(),
        Some(ErrorCode::Conflict)
    );
    let cv = app.get_cv(&cv_id).expect("cv").cv;
    assert_eq!(bullet_text(&cv, B3), "Mentored four engineers.");
    let still = app.get_review(&cv_id).expect("get").expect("some");
    assert_eq!(still.suggestions[1].status, SuggestionStatus::Pending);
}

#[tokio::test]
async fn a_failed_review_keeps_the_last_one() {
    let llm = FakeLlm::new().fail("ReviewResult", cv_ai::LlmError::Http { status: 500 });
    let (app, cv_id) = setup(llm);
    assert_eq!(
        review(&app, &cv_id).await.err(),
        Some(ErrorCode::AiProvider)
    );
    assert_eq!(app.get_review(&cv_id).expect("get"), None);
    assert_eq!(
        app.ai_review(Arc::new(Recorded::default()), AiReview { cv_id: id(B1) })
            .map(|_| ())
            .map_err(|e| e.code),
        Err(ErrorCode::NotFound)
    );
}
