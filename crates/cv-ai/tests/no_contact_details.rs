//! Spec section 17, M7 acceptance: no contact details appear in any request
//! of any stage; `IMPORT_EXTRACT` sends the name and street but no email,
//! phone or links (ADR-076). Every stage's prompt is built from
//! a CV whose free text also hides contact details, sent through a
//! recording `Llm`, and the captured requests are searched.

use cv_ai::payload::Redactor;
use cv_ai::{FakeLlm, Kind, Llm, PayloadOptions, build_cv_payload, prompts};
use cv_core::{Cv, JobRequirements, LintIssue, Requirement, RequirementKind, Severity};
use serde_json::json;

const EMAIL: &str = "jane@example.com";
const PHONE: &str = "+49 30 1234 5678";
const LINK: &str = "https://jane.example.com";
const STREET: &str = "Hauptstraße 5";
const POSTCODE: &str = "10115";
const NAME: &str = "Jane Doe";

fn cv() -> Cv {
    let json = include_str!("../../../testdata/cvs/typical.json");
    let mut cv: Cv = serde_json::from_str(json).expect("fixture");
    assert_eq!(cv.basics.email.as_deref(), Some(EMAIL));
    assert_eq!(cv.basics.phone.as_deref(), Some(PHONE));
    assert_eq!(cv.basics.full_name, NAME);
    cv.basics.location = Some(format!("{STREET}, {POSTCODE} Berlin, Germany"));
    cv.summary = Some(format!(
        "{NAME} is reachable at {EMAIL} or {PHONE}; portfolio at {LINK}/work."
    ));
    // Contact details hidden in an entry's description and bullet too.
    let entry = cv
        .sections
        .iter_mut()
        .find_map(|s| match s {
            cv_core::Section::Experience(e) => e.entries.first_mut(),
            _ => None,
        })
        .expect("an experience entry");
    entry.description = Some("Write to JANE@EXAMPLE.COM, see www.jane.example.com".into());
    entry.bullets[0].text = format!("{} (call 030 12345678)", entry.bullets[0].text);
    cv
}

fn requirements() -> JobRequirements {
    JobRequirements {
        title: Some("Backend Engineer".into()),
        seniority: Some("senior".into()),
        language: "en".into(),
        must_have: vec![Requirement {
            id: "r1".into(),
            text: "Rust".into(),
            kind: RequirementKind::Skill,
            keywords: vec!["Rust".into()],
        }],
        nice_to_have: vec![Requirement {
            id: "r2".into(),
            text: "Kubernetes".into(),
            kind: RequirementKind::Skill,
            keywords: vec!["Kubernetes".into()],
        }],
        responsibilities: vec![],
        keywords: vec!["Rust".into()],
        domain: None,
    }
}

/// Every stage, as the pipeline will call it. Returns the kinds sent.
async fn run_every_stage(llm: &FakeLlm, opts: PayloadOptions) -> Vec<Kind> {
    let cv = cv();
    let resolved = cv.resolve(None);
    let payload = build_cv_payload(&resolved, opts);
    let req = requirements();
    let entry = &payload.entries[0];
    let bullet_ids: Vec<&str> = entry.bullets.iter().map(|b| b.id.as_str()).collect();
    let lint = vec![LintIssue {
        code: "invalid_email".into(),
        severity: Severity::Error,
        target: None,
        message: format!("{EMAIL} is not a valid address"),
        fix_hint: None,
    }];
    let text = format!("{NAME}\n{EMAIL}\n{PHONE}\n{STREET}\nRust engineer");
    let prompts = [
        prompts::job_parse("We need Rust. Contact recruiter@agency.example."),
        prompts::relevance(&req, &payload),
        prompts::rewrite(&req, entry, &bullet_ids),
        prompts::summary(&req, &payload),
        prompts::gaps(&req, &req.nice_to_have, &payload),
        prompts::review(&lint, &payload),
        prompts::import_extract(&cv_ai::extract::prepare_text(&text).expect("text").sent),
    ];
    let mut kinds = Vec::new();
    for p in prompts {
        let p = p.expect("prompt");
        kinds.push(p.kind);
        let _ = llm.generate_json(p.request(json!({}), "Out")).await;
    }
    kinds
}

fn leaks(text: &str, name_allowed: bool) -> Vec<&'static str> {
    let lower = text.to_lowercase();
    let mut found: Vec<&'static str> = [
        EMAIL,
        PHONE,
        LINK,
        STREET,
        POSTCODE,
        "12345678",
        "jane.example",
    ]
    .into_iter()
    .filter(|needle| lower.contains(&needle.to_lowercase()))
    .collect();
    if !name_allowed && text.contains(NAME) {
        found.push(NAME);
    }
    found
}

#[tokio::test]
async fn no_stage_sends_contact_details() {
    let llm = FakeLlm::new();
    let kinds = run_every_stage(&llm, PayloadOptions::default()).await;
    let seen = llm.seen();
    assert_eq!(seen.len(), Kind::ALL.len(), "every stage was sent");
    for (kind, request) in kinds.iter().zip(&seen) {
        let sent = format!("{}\n{}", request.system, request.user);
        let found = leaks(&sent, false);
        if *kind == Kind::ImportExtract {
            // The import sends the text as written, name and street
            // included, after the user has seen it; email, phone and links
            // are found again locally (ADR-076).
            let found: Vec<_> = found
                .into_iter()
                .filter(|f| ![NAME, STREET, POSTCODE].contains(f))
                .collect();
            assert!(found.is_empty(), "import sent {found:?}");
        } else {
            assert!(found.is_empty(), "{} sent {found:?}", kind.as_str());
        }
    }
    // City and country still go, as the default settings allow.
    assert!(seen[1].user.contains("Berlin, Germany"));
    // And what was taken out is marked, not silently dropped.
    assert!(seen[1].user.contains(cv_ai::payload::REDACTED));
}

#[tokio::test]
async fn name_and_location_follow_the_settings() {
    let llm = FakeLlm::new();
    let opts = PayloadOptions {
        send_name: true,
        send_location: false,
    };
    run_every_stage(&llm, opts).await;
    let relevance = &llm.seen()[1];
    assert!(relevance.user.contains(NAME));
    assert!(!relevance.user.contains("Berlin"));
    assert!(leaks(&relevance.user, true).is_empty());
}

#[test]
fn the_redactor_is_what_the_payload_uses() {
    let cv = cv();
    let r = Redactor::new(&cv.basics, PayloadOptions::default());
    let payload = build_cv_payload(&cv.resolve(None), PayloadOptions::default());
    assert_eq!(payload.summary, cv.summary.as_deref().map(|s| r.text(s)));
}

#[test]
fn phone_numbers_in_pairs_of_digits_are_redacted() {
    let r = Redactor::patterns();
    for phone in ["06 12 34 56 78", "+33 1 23 45 67 89", "01.23.45.67.89"] {
        let sent = r.text(&format!("Call {phone} after 6"));
        assert!(!sent.contains(phone), "{phone} was sent");
        assert!(sent.contains(cv_ai::payload::REDACTED));
    }
    // Months and years stay.
    let dates = "01/2019 - 03/2021, 2019 - 2021";
    assert_eq!(r.text(dates), dates);
}
