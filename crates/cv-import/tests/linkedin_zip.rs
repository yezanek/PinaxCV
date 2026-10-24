//! Spec section 9.3: a LinkedIn data export ZIP maps to a CV, and unsafe
//! archives are refused. The ZIPs are built here from synthetic CSVs.

use std::io::{Cursor, Write};

use cv_core::{Id, PartialDate, Section};
use cv_import::{ImportError, MergePlan, import_any, import_linkedin_zip, merge};
use zip::write::SimpleFileOptions;

fn ids() -> impl FnMut() -> Id {
    let mut n = 0u32;
    move || {
        n += 1;
        Id::parse(&format!("01JAAAAAAAAAAAAAAAAAA{n:05}")).expect("id")
    }
}

fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in files {
        w.start_file(*name, options).expect("start");
        w.write_all(data).expect("write");
    }
    w.finish().expect("finish").into_inner()
}

const PROFILE: &[u8] = b"\xEF\xBB\xBFFirst Name,Last Name,Maiden Name,Address,Birth Date,Headline,Summary,Industry,Zip Code,Geo Location,Twitter Handles,Websites,Instant Messengers\nSam,Placeholder,,,,Site Reliability Engineer,\"Keeps things up.\nLikes pagers less.\",Software,,\"Lyon, France\",,\"[PORTFOLIO:https://sam.example.org]\",\n";
const POSITIONS: &[u8] = b"Company Name,Title,Description,Location,Started On,Finished On\nOld Hosting,Ops Engineer,Racked servers.,Paris,Mar 2015,Dec 2018\nCloud Sample,SRE,\"- Ran on-call\n- Wrote runbooks\",Lyon,Jan 2019,\n";
const EDUCATION: &[u8] = b"School Name,Start Date,End Date,Notes,Degree Name,Activities\nSample Institute,2011,2014,,BSc Networks,Robotics club\n";
const SKILLS: &[u8] = b"Name\nLinux\nTerraform\n";
const LANGUAGES: &[u8] = b"Name,Proficiency\nFrench,Native or bilingual proficiency\nEnglish,Full professional proficiency\n";
const CERTS: &[u8] = b"Name,Url,Authority,Started On,Finished On,License Number\nKubernetes Admin,https://certs.example.com/k,Example Foundation,Jun 2021,,ABC-123\n";
const EMAILS: &[u8] = b"Email Address,Confirmed,Primary,Updated On\nold@example.org,Yes,No,\nsam@example.org,Yes,Yes,\n";
const PHONES: &[u8] = b"Extension,Number,Type\n,+33 4 00 00 00 00,Mobile\n";

fn export() -> Vec<u8> {
    zip_of(&[
        ("Profile.csv", PROFILE),
        ("Positions.csv", POSITIONS),
        ("Education.csv", EDUCATION),
        ("Skills.csv", SKILLS),
        ("Languages.csv", LANGUAGES),
        ("Certifications.csv", CERTS),
        ("Email Addresses.csv", EMAILS),
        ("PhoneNumbers.csv", PHONES),
        ("Connections.csv", b"Notes:\nnot read\n"),
        ("messages.csv", b"not read"),
    ])
}

#[test]
fn a_full_export_maps_to_a_cv() {
    let r = import_any(&export(), &mut ids()).expect("imports");
    let p = &r.cv_partial;
    assert_eq!(p.full_name.as_deref(), Some("Sam Placeholder"));
    assert_eq!(p.headline.as_deref(), Some("Site Reliability Engineer"));
    assert_eq!(
        p.summary.as_deref(),
        Some("Keeps things up.\nLikes pagers less.")
    );
    assert_eq!(p.location.as_deref(), Some("Lyon, France"));
    assert_eq!(p.email.as_deref(), Some("sam@example.org"));
    assert_eq!(p.phone.as_deref(), Some("+33 4 00 00 00 00"));
    assert_eq!(p.links[0].label, "Portfolio");

    let Section::Experience(work) = &p.sections[0] else {
        panic!("experience first");
    };
    // Sorted newest first, whatever the file order.
    assert_eq!(work.entries[0].org, "Cloud Sample");
    assert!(work.entries[0].period.current);
    assert_eq!(
        work.entries[0].period.start,
        Some(PartialDate {
            year: 2019,
            month: Some(1)
        })
    );
    let bullets: Vec<&str> = work.entries[0]
        .bullets
        .iter()
        .map(|b| b.text.as_str())
        .collect();
    assert_eq!(bullets, ["Ran on-call", "Wrote runbooks"]);
    assert!(!work.entries[1].period.current);

    let Section::Education(edu) = &p.sections[1] else {
        panic!("education");
    };
    assert_eq!(edu.entries[0].title, "BSc Networks");
    assert_eq!(edu.entries[0].description.as_deref(), Some("Robotics club"));

    let Section::Certifications(certs) = &p.sections[2] else {
        panic!("certifications");
    };
    assert_eq!(certs.entries[0].org, "Example Foundation");
    let Section::Skills(skills) = &p.sections[3] else {
        panic!("skills");
    };
    assert_eq!(skills.groups[0].skills.len(), 2);
    let Section::Languages(langs) = &p.sections[4] else {
        panic!("languages");
    };
    assert_eq!(langs.items[0].name, "French");

    assert!(
        r.warnings
            .iter()
            .any(|w| w.message.contains("Connections.csv") && w.message.contains("messages.csv"))
    );

    let mut cv: cv_core::Cv =
        serde_json::from_str(include_str!("../../../testdata/cvs/minimal.json")).expect("fixture");
    merge(&mut cv, r.cv_partial, &MergePlan::default());
    let issues = cv_core::validate::for_save(&mut cv);
    assert!(!cv_core::validate::has_errors(&issues), "{issues:?}");
}

#[test]
fn files_in_a_folder_and_other_name_spellings_are_found() {
    let zip = zip_of(&[
        ("Basic_LinkedInDataExport/Positions.csv", POSITIONS),
        ("Basic_LinkedInDataExport/Email_Addresses.csv", EMAILS),
    ]);
    let r = import_linkedin_zip(&zip, &mut ids()).expect("imports");
    assert_eq!(r.cv_partial.email.as_deref(), Some("sam@example.org"));
    assert_eq!(r.cv_partial.sections.len(), 1);
}

#[test]
fn zip_slip_paths_are_refused() {
    for name in [
        "../Profile.csv",
        "/abs/Profile.csv",
        "a/../../Positions.csv",
    ] {
        let zip = zip_of(&[("Skills.csv", SKILLS), (name, PROFILE)]);
        assert!(
            matches!(import_any(&zip, &mut ids()), Err(ImportError::Malformed(m)) if m.contains("unsafe path")),
            "{name}"
        );
    }
}

#[test]
fn zip_bombs_are_refused() {
    // 6 MB of zeros deflates to a few kilobytes.
    let bomb = vec![b'0'; 6 * 1024 * 1024];
    let zip = zip_of(&[("Positions.csv", &bomb)]);
    assert!(zip.len() < 100 * 1024);
    assert!(
        matches!(import_any(&zip, &mut ids()), Err(ImportError::Malformed(m)) if m.contains("unpacks"))
    );
}

#[test]
fn other_zips_are_unsupported() {
    let zip = zip_of(&[("photo.jpg", b"jpeg"), ("notes.txt", b"hi")]);
    assert!(matches!(
        import_any(&zip, &mut ids()),
        Err(ImportError::UnsupportedFormat(_))
    ));
}
