//! M4 acceptance: a Europass PDF export imports into an existing CV through
//! the manual merge, without losing or duplicating anything. Synthetic data
//! only.

use std::collections::HashMap;
use std::path::PathBuf;

use cv_core::{Cv, Id, Section, validate};
use cv_import::{EntryDecision, ImportError, MergePlan, import_europass, merge, suggest_matches};
use lopdf::{Document, Object, Stream, dictionary};

const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Candidate xmlns="http://www.europass.eu/1.0" xmlns:oa="http://www.openapplications.org/oagis/9" xmlns:hr="http://www.hr-xml.org/3" xmlns:eures="http://www.europass_eures.eu/1.0">
  <CandidatePerson>
    <PersonName><oa:GivenName>Alex</oa:GivenName><hr:FamilyName>Sample</hr:FamilyName></PersonName>
    <Communication><ChannelCode>Email</ChannelCode><oa:URI>alex@example.org</oa:URI></Communication>
  </CandidatePerson>
  <CandidateProfile>
    <EmploymentHistory>
      <EmployerHistory>
        <hr:OrganizationName>Example Works</hr:OrganizationName>
        <PositionHistory>
          <PositionTitle typeCode="FREETEXT">Platform Engineer</PositionTitle>
          <EmploymentPeriod>
            <eures:StartDate><hr:FormattedDateTime>2019-05</hr:FormattedDateTime></eures:StartDate>
            <eures:EndDate><hr:FormattedDateTime>2022-01</hr:FormattedDateTime></eures:EndDate>
          </EmploymentPeriod>
          <oa:Description>&lt;ul&gt;&lt;li&gt;Ran the build farm.&lt;/li&gt;&lt;/ul&gt;</oa:Description>
        </PositionHistory>
      </EmployerHistory>
    </EmploymentHistory>
  </CandidateProfile>
</Candidate>"#;

/// A one-page PDF with `xml` attached the way the Europass editor does it.
fn europass_pdf(xml: Option<&str>) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let mut catalog = dictionary! { "Type" => "Catalog", "Pages" => pages_id };
    if let Some(xml) = xml {
        let mut stream = Stream::new(
            dictionary! { "Type" => "EmbeddedFile" },
            xml.as_bytes().to_vec(),
        );
        stream.compress().expect("compress");
        let file_id = doc.add_object(stream);
        let spec_id = doc.add_object(dictionary! {
            "Type" => "Filespec",
            "F" => Object::string_literal("attachment.xml"),
            "EF" => dictionary! { "F" => file_id },
        });
        catalog.set(
            "Names",
            dictionary! {
                "EmbeddedFiles" => dictionary! {
                    "Names" => vec![Object::string_literal("attachment.xml"), spec_id.into()],
                },
            },
        );
    }
    let catalog_id = doc.add_object(catalog);
    doc.trailer.set("Root", catalog_id);
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save");
    out
}

fn ids() -> impl FnMut() -> Id {
    let mut n = 0u8;
    move || {
        n += 1;
        Id::parse(&format!("01HZX3M8Q4V7K2N9P5R6T0W2{n:02}")).expect("id")
    }
}

fn typical() -> Cv {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cvs/typical.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("read")).expect("parse")
}

fn entry_count(cv: &Cv) -> usize {
    cv.sections
        .iter()
        .map(|s| match s {
            Section::Experience(s)
            | Section::Education(s)
            | Section::Projects(s)
            | Section::Certifications(s) => s.entries.len(),
            Section::Custom(s) => s.entries.len(),
            _ => 0,
        })
        .sum()
}

#[test]
fn pdf_import_merges_into_an_existing_cv() {
    let mut cv = typical();
    let before = entry_count(&cv);
    let name = cv.basics.full_name.clone();

    let first = import_europass(&europass_pdf(Some(XML)), &mut ids()).expect("import");
    assert!(first.warnings.is_empty(), "{:?}", first.warnings);
    assert_eq!(first.cv_partial.full_name.as_deref(), Some("Alex Sample"));

    // Default plan: keep my basics, add every new entry.
    merge(&mut cv, first.cv_partial, &MergePlan::default());
    assert_eq!(cv.basics.full_name, name);
    assert_eq!(entry_count(&cv), before + 1);
    assert!(!validate::has_errors(&validate::for_save(&mut cv)));

    // Importing the same file again matches the entry; Replace keeps one copy.
    let again = import_europass(&europass_pdf(Some(XML)), &mut ids()).expect("import");
    let matches = suggest_matches(&cv, &again.cv_partial);
    assert_eq!(matches.len(), 1);
    let entries: HashMap<Id, EntryDecision> = matches
        .into_iter()
        .map(|(incoming, base)| (incoming, EntryDecision::Replace(base)))
        .collect();
    merge(
        &mut cv,
        again.cv_partial,
        &MergePlan {
            use_imported: Vec::new(),
            entries,
        },
    );
    assert_eq!(entry_count(&cv), before + 1);
    assert!(!validate::has_errors(&validate::for_save(&mut cv)));
}

#[test]
fn a_pdf_without_europass_data_is_refused_with_advice() {
    match import_europass(&europass_pdf(None), &mut ids()) {
        Err(ImportError::UnsupportedFormat(msg)) => assert!(msg.contains("Europass")),
        other => panic!("unexpected {other:?}"),
    }
}
