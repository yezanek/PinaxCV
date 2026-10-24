//! Spec section 9.1: a full JSON Resume maps to every section, with fresh
//! IDs, and imports into a CV that passes validation. Synthetic data only.

use std::path::PathBuf;

use cv_core::{Id, PartialDate, Section};
use cv_import::{import_any, import_json_resume};

fn ids() -> impl FnMut() -> Id {
    let mut n = 0u32;
    move || {
        n += 1;
        Id::parse(&format!("01JAAAAAAAAAAAAAAAAAA{n:05}")).expect("id")
    }
}

fn sample() -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/import/json-resume-sample.json"),
    )
    .expect("read sample")
}

#[test]
fn the_sample_maps_every_section() {
    let r = import_json_resume(&sample(), &mut ids()).expect("imports");
    let p = &r.cv_partial;
    assert_eq!(p.full_name.as_deref(), Some("Robin Example"));
    assert_eq!(p.headline.as_deref(), Some("Data Engineer"));
    assert_eq!(p.location.as_deref(), Some("Leeds, England, GB"));
    assert_eq!(
        p.summary.as_deref(),
        Some("Builds data pipelines.\n\nLikes tidy schemas.")
    );
    let links: Vec<(&str, &str)> = p
        .links
        .iter()
        .map(|l| (l.label.as_str(), l.url.as_str()))
        .collect();
    assert_eq!(
        links,
        [
            ("robin.example.net", "https://robin.example.net/"),
            ("GitHub", "https://github.com/robin-example"),
        ]
    );

    let kinds: Vec<&str> = p
        .sections
        .iter()
        .map(|s| match s {
            Section::Experience(_) => "experience",
            Section::Education(_) => "education",
            Section::Projects(_) => "projects",
            Section::Certifications(_) => "certifications",
            Section::Custom(_) => "custom",
            Section::Skills(_) => "skills",
            Section::Languages(_) => "languages",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "experience",
            "education",
            "projects",
            "certifications",
            "custom",
            "skills",
            "languages"
        ]
    );

    let Section::Experience(work) = &p.sections[0] else {
        panic!("experience");
    };
    let current = &work.entries[0];
    assert_eq!(current.org, "Sample Analytics Ltd");
    assert_eq!(
        current.period.start,
        Some(PartialDate {
            year: 2021,
            month: Some(4)
        })
    );
    assert!(current.period.current, "no end date means current");
    assert_eq!(current.bullets.len(), 2);
    assert_eq!(current.description.as_deref(), Some("Owns the warehouse."));
    assert_eq!(work.entries[1].org, "Old Corp", "older `company` key");
    assert!(!work.entries[1].period.current);

    let Section::Education(edu) = &p.sections[1] else {
        panic!("education");
    };
    assert_eq!(edu.entries[0].title, "BSc Mathematics");
    assert_eq!(edu.entries[0].bullets.len(), 2);

    let Section::Skills(skills) = &p.sections[5] else {
        panic!("skills");
    };
    assert_eq!(skills.groups[0].label.as_deref(), Some("Languages"));
    assert_eq!(skills.groups[1].label, None);
    assert_eq!(skills.groups[1].skills[0].name, "Spark");

    let Section::Languages(langs) = &p.sections[6] else {
        panic!("languages");
    };
    assert_eq!(langs.items[1].level.as_deref(), Some("B1"));

    let messages: Vec<&str> = r.warnings.iter().map(|w| w.message.as_str()).collect();
    assert!(messages.iter().any(|m| m.contains("marked as current")));
    assert!(messages.contains(&"Awards were not imported."));
    assert!(messages.contains(&"References were not imported."));
}

#[test]
fn import_any_detects_json_resume() {
    let r = import_any(&sample(), &mut ids()).expect("imports");
    assert_eq!(r.cv_partial.full_name.as_deref(), Some("Robin Example"));
}

#[test]
fn every_id_is_unique() {
    let r = import_json_resume(&sample(), &mut ids()).expect("imports");
    let json = format!("{:?}", r.cv_partial);
    let mut seen = std::collections::HashSet::new();
    for part in json.split("Id(\"").skip(1) {
        let id = &part[..26];
        assert!(seen.insert(id.to_owned()), "duplicate id {id}");
    }
    assert!(seen.len() > 20);
}

#[test]
fn the_import_merges_into_a_valid_cv() {
    let mut cv: cv_core::Cv =
        serde_json::from_str(include_str!("../../../testdata/cvs/minimal.json")).expect("fixture");
    let r = import_json_resume(&sample(), &mut ids()).expect("imports");
    cv_import::merge(&mut cv, r.cv_partial, &cv_import::MergePlan::default());
    let issues = cv_core::validate::for_save(&mut cv);
    assert!(!cv_core::validate::has_errors(&issues), "{issues:?}");
}
