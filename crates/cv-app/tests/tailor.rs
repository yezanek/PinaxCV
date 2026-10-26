//! `ai_tailor` with `FakeLlm`: stages A to C end to end against a real
//! SQLite store, with no network.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use cv_ai::{FakeLlm, Llm};
use cv_api::{
    AiConfig, AiProvider, AiStage, AiTailor, ErrorCode, NewCv, NewVariant, SaveCv, TailorResult,
};
use cv_app::{App, ExportKind, FileDialog, JobEvent, JobEvents, UlidIds};
use cv_core::selection::TailorOptions;
use cv_core::{
    Cv, Id, JobTarget, SuggestionAction, SuggestionKind, SuggestionSource, SuggestionStatus,
    SuggestionTarget,
};
use cv_store::{SqliteStore, SystemClock};
use serde_json::{Value, json};

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
const OLDER: &str = "01J02000000000000000000007";
const EDUCATION: &str = "01J02000000000000000000011";
const GO: &str = "01J02000000000000000000016";
const SQL: &str = "01J02000000000000000000017";

fn id(s: &str) -> Id {
    Id::parse(s).expect("id")
}

fn requirements() -> Value {
    json!({
        "title": "Platform Engineer", "seniority": "senior", "language": "en",
        "must_have": [{ "id": "r1", "text": "SQL", "kind": "skill", "keywords": ["SQL"] }],
        "nice_to_have": [], "responsibilities": [], "keywords": ["SQL"], "domain": null
    })
}

fn relevance() -> Value {
    json!({
        "entries": [
            { "entry_id": RECENT, "score": 90, "matched_requirements": ["r1"], "reason": "Strong match." },
            { "entry_id": OLDER, "score": 10, "matched_requirements": [], "reason": "Unrelated stack." },
            { "entry_id": EDUCATION, "score": 15, "matched_requirements": [], "reason": "Not asked for." }
        ],
        "bullets": [],
        "skills": [{ "skill_id": SQL, "score": 95 }, { "skill_id": GO, "score": 5 }]
    })
}

struct Setup {
    app: Arc<App>,
    llm: Arc<FakeLlm>,
    variant: Id,
}

fn setup(llm: FakeLlm, description: &str, consent: bool) -> Setup {
    let llm = Arc::new(llm);
    let store = SqliteStore::open_in_memory(Box::new(SystemClock)).expect("store");
    let app = App::new(
        Box::new(store),
        Box::new(NoDialog),
        Box::new(UlidIds),
        "preview://localhost",
    )
    .with_llm(llm.clone() as Arc<dyn Llm>);
    app.ai_configure(AiConfig {
        provider: AiProvider::OpenaiCompatible,
        model: "local-model".into(),
        base_url: Some("http://localhost:11434/v1".into()),
        api_key: None,
        send_name: false,
        send_location: true,
    })
    .expect("configure");
    if consent {
        app.ai_consent().expect("consent");
    }
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
    let variant = app
        .create_variant(NewVariant {
            cv_id: created.cv.id,
            name: "Platform".into(),
            job: JobTarget {
                title: Some("Platform Engineer".into()),
                company: Some("Acme".into()),
                description_text: description.into(),
                ..JobTarget::default()
            },
        })
        .expect("variant");
    Setup {
        app: Arc::new(app),
        llm,
        variant: variant.variant.id,
    }
}

async fn run(
    s: &Setup,
    options: TailorOptions,
) -> (Vec<JobEvent>, Result<TailorResult, ErrorCode>) {
    let events = Arc::new(Recorded::default());
    let (_, job) = s
        .app
        .ai_tailor(
            events.clone(),
            AiTailor {
                variant_id: s.variant.clone(),
                options,
            },
        )
        .expect("started");
    job.await;
    let events = events.0.lock().expect("lock").clone();
    let outcome = match events.last() {
        Some(JobEvent::Done(d)) => Ok(serde_json::from_value(d.result.clone()).expect("result")),
        Some(JobEvent::Failed(f)) => Err(f.error.code),
        _ => panic!("no final event"),
    };
    (events, outcome)
}

#[tokio::test]
async fn tailoring_saves_pending_suggestions_and_caches_the_parse() {
    let llm = FakeLlm::new()
        .reply("JobRequirements", requirements())
        .reply("RelevanceResult", relevance());
    let s = setup(llm, "We need SQL. Kubernetes is a plus.", true);
    let (events, outcome) = run(&s, TailorOptions::default()).await;
    let result = outcome.expect("tailored");

    let stages: Vec<(AiStage, u8)> = events
        .iter()
        .filter_map(|e| match e {
            JobEvent::Progress(p) => Some((p.stage, p.pct)),
            _ => None,
        })
        .collect();
    assert_eq!(stages.first(), Some(&(AiStage::Starting, 0)));
    assert_eq!(stages.last(), Some(&(AiStage::Selection, 100)));
    assert!(stages.windows(2).all(|w| w[0].1 <= w[1].1));

    let v = &result.variant.variant;
    let picked: Vec<_> = v
        .suggestions
        .iter()
        .map(|s| (s.kind, s.target.clone()))
        .collect();
    assert_eq!(
        picked,
        [
            (
                SuggestionKind::Hide,
                SuggestionTarget::Entry { id: id(OLDER) }
            ),
            (
                SuggestionKind::Collapse,
                SuggestionTarget::Entry { id: id(EDUCATION) }
            ),
            (
                SuggestionKind::SkillOrder,
                SuggestionTarget::SkillGroup {
                    id: id("01J02000000000000000000014")
                }
            ),
        ]
    );
    assert_eq!(result.suggestions, 3);
    assert!(
        v.suggestions
            .iter()
            .all(|s| s.status == SuggestionStatus::Pending && s.source == SuggestionSource::Tailor)
    );
    // Nothing is applied until the user accepts.
    assert!(v.hidden.is_empty() && v.collapsed.is_empty());
    assert!(v.job.requirements.is_some() && v.job.description_hash.is_some());
    assert_eq!(
        v.suggestions[2].action,
        Some(SuggestionAction::Reorder {
            // Rust has no score, so it goes after the scored skills.
            order: vec![id(SQL), id(GO), id("01J02000000000000000000015")]
        })
    );

    // No contact details were sent.
    for sent in s.llm.seen() {
        for secret in [
            "jane@example.com",
            "+49 30 1234 5678",
            "jane.example.com",
            "Jane Doe",
        ] {
            assert!(
                !sent.user.contains(secret),
                "{secret} sent in {}",
                sent.schema_name
            );
        }
    }

    // A rerun reuses the saved parse and the cached scores, so nothing is
    // sent, and replaces the pending suggestions instead of adding to them.
    let (_, again) = run(&s, TailorOptions::default()).await;
    let again = again.expect("rerun");
    assert_eq!(again.variant.variant.suggestions.len(), 3);
    let names: Vec<_> = s.llm.seen().iter().map(|r| r.schema_name).collect();
    assert_eq!(names, ["JobRequirements", "RelevanceResult"]);
}

#[tokio::test]
async fn a_page_target_adds_fit_cuts() {
    let llm = FakeLlm::new()
        .reply("JobRequirements", requirements())
        .reply("RelevanceResult", relevance());
    let s = setup(llm, "We need SQL.", true);
    let (_, outcome) = run(
        &s,
        TailorOptions {
            page_target: Some(1),
            ..TailorOptions::default()
        },
    )
    .await;
    let result = outcome.expect("tailored");
    // The typical CV already fits one page.
    assert_eq!((result.pages, result.fits), (Some(1), Some(true)));
    assert_eq!(result.variant.variant.page_target, Some(1));
}

#[tokio::test]
async fn provider_failures_end_the_job_with_an_error() {
    let llm = FakeLlm::new().fail("JobRequirements", cv_ai::LlmError::Http { status: 500 });
    let s = setup(llm, "We need SQL.", true);
    let (_, outcome) = run(&s, TailorOptions::default()).await;
    assert_eq!(outcome.err(), Some(ErrorCode::AiProvider));
    let v = s.app.get_variant(&s.variant).expect("variant").variant;
    assert!(v.suggestions.is_empty());
}

#[test]
fn tailoring_needs_consent_and_a_job_description() {
    let s = setup(FakeLlm::new(), "We need SQL.", false);
    let start = |s: &Setup| {
        s.app
            .ai_tailor(
                Arc::new(Recorded::default()),
                AiTailor {
                    variant_id: s.variant.clone(),
                    options: TailorOptions::default(),
                },
            )
            .map(|_| ())
            .map_err(|e| e.code)
    };
    assert_eq!(start(&s), Err(ErrorCode::AiConsentRequired));
    let s = setup(FakeLlm::new(), "   ", true);
    assert_eq!(start(&s), Err(ErrorCode::Validation));
    assert!(s.llm.seen().is_empty());
}
