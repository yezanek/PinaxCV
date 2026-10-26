//! Text import (`import_text_prepare`, `ai_import`, `import_text_basic`)
//! with `FakeLlm` against a real SQLite store, with no network.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use cv_ai::{FakeLlm, Llm};
use cv_api::{
    AiConfig, AiImport, AiProvider, ErrorCode, ImportCommit, ImportPreview, ImportTarget,
    ImportText, ImportTextSource,
};
use cv_app::{App, ExportKind, FileDialog, JobEvent, JobEvents, UlidIds};
use cv_core::Section;
use cv_store::{SqliteStore, SystemClock};
use serde_json::json;

/// Opens `path` when asked for a file to open.
struct OpenDialog(Option<PathBuf>);

impl FileDialog for OpenDialog {
    fn choose_save_path(&self, _: &str, _: ExportKind) -> Option<PathBuf> {
        None
    }

    fn choose_open_path(&self, _: &[&str]) -> Option<PathBuf> {
        self.0.clone()
    }
}

#[derive(Default)]
struct Recorded(Mutex<Vec<JobEvent>>);

impl JobEvents for Recorded {
    fn emit(&self, event: JobEvent) {
        self.0.lock().expect("lock").push(event);
    }
}

const TEXT: &str = "Alex Sample
alex@example.org | +49 170 1234567 | https://alex.example.org
Backend Engineer, Berlin

Experience
Senior Engineer, Example AG
01/2020 - present
• Built the billing service in Rust.
• Ran the on-call rota for six engineers.

Languages
German (native), English (C1)
";

fn extracted() -> serde_json::Value {
    json!({
        "full_name": "Alex Sample", "headline": "Backend Engineer", "location": "Berlin", "summary": null,
        "sections": [{ "kind": "experience", "heading": null, "entries": [{
            "title": "Senior Engineer", "organization": "Example AG", "location": null,
            "start": { "year": 2020, "month": 1 }, "end": { "year": 2023, "month": 13 }, "current": true,
            "bullets": ["• Built the billing service in Rust.", "Led the on-call rota for six engineers."]
        }]}],
        "skills": [],
        "languages": [{ "name": "German", "level": "native" }, { "name": "English", "level": "C1" }],
        "import_notes": ["The languages line was split by commas."]
    })
}

fn app(llm: FakeLlm, file: Option<PathBuf>, consent: bool) -> (Arc<App>, Arc<FakeLlm>) {
    let llm = Arc::new(llm);
    let store = SqliteStore::open_in_memory(Box::new(SystemClock)).expect("store");
    let app = App::new(
        Box::new(store),
        Box::new(OpenDialog(file)),
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
    (Arc::new(app), llm)
}

async fn extract(app: &Arc<App>, text_id: u64) -> Result<ImportPreview, ErrorCode> {
    let events = Arc::new(Recorded::default());
    let (_, job) = app
        .ai_import(events.clone(), AiImport { text_id })
        .map_err(|e| e.code)?;
    job.await;
    let last = events.0.lock().expect("lock").last().cloned();
    match last {
        Some(JobEvent::Done(d)) => Ok(serde_json::from_value(d.result).expect("preview")),
        Some(JobEvent::Failed(f)) => Err(f.error.code),
        _ => panic!("no final event"),
    }
}

#[tokio::test]
async fn pasted_text_is_extracted_without_sending_contact_details() {
    let (app, llm) = app(FakeLlm::new().reply("ExtractedCv", extracted()), None, true);
    let prepared: ImportText = app
        .import_text_prepare(ImportTextSource::Pasted { text: TEXT.into() })
        .expect("prepared");
    assert_eq!(prepared.redacted, 3);
    assert!(prepared.text.contains("01/2020 - present"));

    let preview = extract(&app, prepared.text_id).await.expect("extracted");
    // Exactly the shown text was sent, and no contact detail.
    let sent = &llm.seen()[0].user;
    assert!(sent.contains(&prepared.text));
    for secret in ["alex@example.org", "1234567", "alex.example.org"] {
        assert!(!sent.contains(secret), "{secret} was sent");
    }
    // They come back from the text on this computer.
    assert_eq!(preview.email.as_deref(), Some("alex@example.org"));
    assert_eq!(preview.phone.as_deref(), Some("+49 170 1234567"));
    assert_eq!(preview.links[0].url, "https://alex.example.org");

    let Section::Experience(exp) = &preview.sections[0] else {
        panic!("experience first");
    };
    let entry = &exp.entries[0];
    assert_eq!(entry.org, "Example AG");
    assert!(entry.period.current && entry.period.end.is_none());
    assert_eq!(
        entry.period.start.map(|d| (d.year, d.month)),
        Some((2020, Some(1)))
    );
    // The bullet mark is dropped; the reworded bullet is marked for review.
    assert_eq!(entry.bullets[0].text, "Built the billing service in Rust.");
    assert_eq!(preview.needs_review, [entry.bullets[1].id.clone()]);
    assert!(
        preview
            .warnings
            .iter()
            .any(|w| w.contains("split by commas"))
    );
    assert!(
        preview
            .warnings
            .iter()
            .any(|w| w.contains("word for word in your text: 1"))
    );
    assert!(matches!(&preview.sections[1], Section::Languages(l) if l.items.len() == 2));

    // The same merge review saves it.
    let saved = app
        .import_commit(ImportCommit {
            preview_id: preview.preview_id,
            target: ImportTarget::New {
                title: String::new(),
            },
            use_imported: Vec::new(),
            entries: Vec::new(),
        })
        .expect("saved");
    assert_eq!(saved.cv.title, "Alex Sample");
    assert_eq!(saved.cv.basics.email.as_deref(), Some("alex@example.org"));
}

#[tokio::test]
async fn a_text_file_can_be_read_without_ai() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("cv.txt");
    std::fs::write(&path, TEXT).expect("write");
    let (app, llm) = app(FakeLlm::new(), Some(path), false);
    let prepared = app
        .import_text_prepare(ImportTextSource::File)
        .expect("prepared");
    // AI needs consent first; the basic reader does not.
    assert_eq!(
        extract(&app, prepared.text_id).await.err(),
        Some(ErrorCode::AiConsentRequired)
    );
    let preview = app.import_text_basic(prepared.text_id).expect("basic");
    assert!(preview.warnings[0].contains("without AI"));
    assert_eq!(preview.full_name.as_deref(), Some("Alex Sample"));
    assert!(llm.seen().is_empty());

    assert_eq!(
        app.import_text_prepare(ImportTextSource::Pasted {
            text: " \n ".into()
        })
        .map_err(|e| e.code)
        .err(),
        Some(ErrorCode::Validation)
    );
    assert_eq!(
        app.import_text_basic(99).map_err(|e| e.code).err(),
        Some(ErrorCode::Import)
    );
}

#[tokio::test]
async fn a_job_started_before_the_cache_was_cleared_does_not_fill_it() {
    let (app, llm) = app(
        FakeLlm::new()
            .reply("ExtractedCv", extracted())
            .reply("ExtractedCv", extracted()),
        None,
        true,
    );
    let prepared = app
        .import_text_prepare(ImportTextSource::Pasted { text: TEXT.into() })
        .expect("prepared");
    let events = Arc::new(Recorded::default());
    let (_, job) = app
        .ai_import(
            events,
            AiImport {
                text_id: prepared.text_id,
            },
        )
        .expect("started");
    // Cleared after the job started but before it ran.
    app.clear_ai_cache().expect("clear");
    job.await;
    assert_eq!(llm.seen().len(), 1);
    // Its answer was not cached, so the same import asks the provider again.
    extract(&app, prepared.text_id).await.expect("extracted");
    assert_eq!(llm.seen().len(), 2);
}
