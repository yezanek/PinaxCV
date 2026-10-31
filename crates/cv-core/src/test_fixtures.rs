//! Shared test fixtures.

use crate::test_ids::id;
use crate::*;

/// A small valid CV: one experience entry with one bullet and one skill group.
///
/// IDs: CV 1, link 2, experience section 3, entry 4, bullet 5, skills section 6,
/// group 7, skill 8.
pub fn sample_cv() -> Cv {
    Cv {
        schema_version: CV_SCHEMA_VERSION,
        id: id(1),
        title: "Main CV".into(),
        locale: Locale::En,
        paper: Paper::A4,
        template: TemplateId::default(),
        options: TemplateOptions::default(),
        basics: Basics {
            full_name: "Jane Doe".into(),
            headline: Some("Backend Engineer".into()),
            email: Some("jane@example.com".into()),
            phone: None,
            location: Some("Berlin, Germany".into()),
            links: vec![Link {
                id: id(2),
                label: "Website".into(),
                url: "https://example.com".into(),
            }],
            show_photo: false,
        },
        summary: None,
        sections: vec![
            Section::Experience(EntrySection {
                id: id(3),
                title_override: None,
                entries: vec![Entry {
                    id: id(4),
                    title: "Engineer".into(),
                    org: "Acme".into(),
                    location: None,
                    period: Period {
                        start: Some(PartialDate {
                            year: 2021,
                            month: Some(3),
                        }),
                        end: None,
                        current: true,
                    },
                    bullets: vec![Bullet {
                        id: id(5),
                        text: "Built things".into(),
                    }],
                    description: None,
                    url: None,
                    locked: false,
                }],
            }),
            Section::Skills(SkillsSection {
                id: id(6),
                title_override: None,
                groups: vec![SkillGroup {
                    id: id(7),
                    label: None,
                    skills: vec![Skill {
                        id: id(8),
                        name: "Rust".into(),
                    }],
                }],
            }),
        ],
        layout: Layout::default(),
    }
}
