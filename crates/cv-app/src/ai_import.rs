//! Text import (spec sections 9.4 and 11.12): pasted text, or a PDF or text
//! file, read with AI (`ai_import`) or with the heading and date heuristics
//! (`import_text_basic`). The user sees the exact text before it is sent;
//! email addresses, phone numbers and links are taken out of it and found
//! again here. Both end in the same preview and merge review as a file
//! import.

use std::sync::{Arc, Mutex, PoisonError};

use cv_ai::StageContext;
use cv_ai::extract::{
    ExtractedCv, ExtractedDate, ExtractedKind, PrepareError, extract_cv, find_contacts,
    is_verbatim, prepare_text,
};
use cv_api::{
    AiImport, AiStage, ApiError, ErrorCode, ImportPreview, ImportText, ImportTextSource, JobId,
};
use cv_core::{
    Bullet, CustomSection, Entry, EntrySection, Id, IdSource, LanguageItem, LanguagesSection, Link,
    PartialDate, Period, Section, Skill, SkillGroup, SkillsSection,
};
use cv_import::{CvPartial, ImportResult, ImportWarning, page_text, text_to_partial};

use crate::import::{import_error, read_capped};
use crate::tailor::StoreCache;
use crate::{App, JobContext, JobEvents, Result, llm_error};

/// File extensions the text import's open dialog offers.
pub const TEXT_EXTENSIONS: &[&str] = &["pdf", "txt"];

/// Texts kept at once; preparing another drops the oldest.
const KEPT_TEXTS: usize = 2;

struct Prepared {
    id: u64,
    /// The text as read, for the contact details and the heuristics.
    original: String,
    /// The text as sent.
    sent: String,
}

/// Prepared texts waiting for `ai_import` or `import_text_basic`. Kept in
/// memory only.
#[derive(Default)]
pub struct TextCache {
    inner: Mutex<(u64, Vec<Prepared>)>,
}

impl TextCache {
    fn put(&self, original: String, sent: String) -> u64 {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner.0 += 1;
        let id = inner.0;
        inner.1.push(Prepared { id, original, sent });
        if inner.1.len() > KEPT_TEXTS {
            inner.1.remove(0);
        }
        id
    }

    fn get(&self, id: u64) -> Result<(String, String)> {
        let inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner
            .1
            .iter()
            .find(|p| p.id == id)
            .map(|p| (p.original.clone(), p.sent.clone()))
            .ok_or_else(|| {
                ApiError::new(
                    ErrorCode::Import,
                    "This text is no longer open. Paste or choose it again.",
                )
            })
    }
}

fn date(d: Option<ExtractedDate>) -> Option<PartialDate> {
    let d = d?;
    Some(PartialDate {
        year: u16::try_from(d.year)
            .ok()
            .filter(|y| (1900..=2100).contains(y))?,
        month: d.month.filter(|m| (1..=12).contains(m)),
    })
}

fn clean(s: Option<String>) -> Option<String> {
    s.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

/// A bullet without its bullet mark.
fn bullet_text(s: &str) -> &str {
    s.trim()
        .trim_start_matches(['•', '-', '*', '·', '–'])
        .trim()
}

/// The model's reading as a partial CV with fresh IDs, the contact details
/// found in `original`, and the bullets not found word for word in `sent`.
pub fn to_partial(
    cv: ExtractedCv,
    original: &str,
    sent: &str,
    ids: &mut dyn IdSource,
) -> (ImportResult, Vec<Id>) {
    let contacts = find_contacts(original);
    let mut needs_review = Vec::new();
    let mut partial = CvPartial {
        full_name: clean(cv.full_name),
        headline: clean(cv.headline),
        email: contacts.emails.first().cloned(),
        phone: contacts.phones.first().cloned(),
        location: clean(cv.location),
        links: contacts
            .urls
            .iter()
            .map(|u| Link {
                id: ids.next_id(),
                label: String::new(),
                url: u.clone(),
            })
            .collect(),
        summary: clean(cv.summary),
        sections: Vec::new(),
        // The AI import does not read personal details (ADR-088).
        personal: Default::default(),
    };
    for s in cv.sections {
        let entries: Vec<Entry> = s
            .entries
            .into_iter()
            .map(|e| {
                let bullets = e
                    .bullets
                    .iter()
                    .map(|b| bullet_text(b))
                    .filter(|b| !b.is_empty())
                    .map(|b| {
                        let id = ids.next_id();
                        if !is_verbatim(sent, b) {
                            needs_review.push(id.clone());
                        }
                        Bullet {
                            id,
                            text: b.to_owned(),
                        }
                    })
                    .collect();
                Entry {
                    id: ids.next_id(),
                    title: e.title.trim().to_owned(),
                    org: e.organization.unwrap_or_default().trim().to_owned(),
                    location: clean(e.location),
                    period: Period {
                        start: date(e.start),
                        end: if e.current { None } else { date(e.end) },
                        current: e.current,
                    },
                    bullets,
                    description: None,
                    url: None,
                    locked: false,
                }
            })
            .collect();
        if entries.is_empty() {
            continue;
        }
        let id = ids.next_id();
        let section = |entries| EntrySection {
            id: id.clone(),
            title_override: None,
            entries,
        };
        partial.sections.push(match s.kind {
            ExtractedKind::Experience => Section::Experience(section(entries)),
            ExtractedKind::Education => Section::Education(section(entries)),
            ExtractedKind::Projects => Section::Projects(section(entries)),
            ExtractedKind::Certifications => Section::Certifications(section(entries)),
            ExtractedKind::Other => Section::Custom(CustomSection {
                id: id.clone(),
                title: clean(s.heading).unwrap_or_else(|| "Other".into()),
                entries,
            }),
        });
    }
    let groups: Vec<SkillGroup> = cv
        .skills
        .into_iter()
        .map(|g| SkillGroup {
            id: ids.next_id(),
            label: clean(g.label),
            skills: g
                .skills
                .iter()
                .map(|k| k.trim())
                .filter(|k| !k.is_empty())
                .map(|k| Skill {
                    id: ids.next_id(),
                    name: k.to_owned(),
                })
                .collect(),
        })
        .filter(|g| !g.skills.is_empty())
        .collect();
    if !groups.is_empty() {
        partial.sections.push(Section::Skills(SkillsSection {
            id: ids.next_id(),
            title_override: None,
            groups,
        }));
    }
    let items: Vec<LanguageItem> = cv
        .languages
        .into_iter()
        .filter(|l| !l.name.trim().is_empty())
        .map(|l| LanguageItem {
            id: ids.next_id(),
            name: l.name.trim().to_owned(),
            level: clean(l.level),
        })
        .collect();
    if !items.is_empty() {
        partial.sections.push(Section::Languages(LanguagesSection {
            id: ids.next_id(),
            title_override: None,
            items,
        }));
    }
    let mut warnings: Vec<ImportWarning> = cv
        .import_notes
        .into_iter()
        .filter(|n| !n.trim().is_empty())
        .map(ImportWarning::new)
        .collect();
    if !needs_review.is_empty() {
        warnings.push(ImportWarning::new(format!(
            "Bullets not found word for word in your text: {}. They are marked below; check them.",
            needs_review.len()
        )));
    }
    (
        ImportResult {
            cv_partial: partial,
            warnings,
        },
        needs_review,
    )
}

impl App {
    /// Reads the text to import and returns it exactly as `ai_import`
    /// would send it. Nothing is sent or saved.
    pub fn import_text_prepare(&self, source: ImportTextSource) -> Result<ImportText> {
        let original = match source {
            ImportTextSource::Pasted { text } => text,
            ImportTextSource::File => {
                let path = self
                    .dialog
                    .choose_open_path(TEXT_EXTENSIONS)
                    .ok_or_else(|| ApiError::new(ErrorCode::Cancelled, "Import cancelled."))?;
                let bytes = read_capped(&path)?;
                if bytes.starts_with(b"%PDF") {
                    page_text(&bytes).map_err(|_| {
                        import_error(cv_import::ImportError::Malformed(
                            "The PDF file could not be read.".into(),
                        ))
                    })?
                } else {
                    String::from_utf8(bytes).map_err(|_| {
                        ApiError::new(
                            ErrorCode::UnsupportedFormat,
                            "The file is not plain text. Choose a PDF or a .txt file.",
                        )
                    })?
                }
            }
        };
        let prepared = prepare_text(&original).map_err(|e| match e {
            PrepareError::Empty => ApiError::new(
                ErrorCode::Validation,
                "There is no text to import. A scanned PDF has no text layer.",
            ),
            PrepareError::TooLong => ApiError::new(
                ErrorCode::Validation,
                "The text is too long for a CV. Import at most about ten pages.",
            ),
        })?;
        let text_id = self.texts.put(original, prepared.sent.clone());
        Ok(ImportText {
            text_id,
            text: prepared.sent,
            redacted: prepared.redacted,
        })
    }

    /// Reads a prepared text with the heading and date heuristics, for use
    /// without AI. The result is marked as low confidence.
    pub fn import_text_basic(&self, text_id: u64) -> Result<ImportPreview> {
        let (original, _) = self.texts.get(text_id)?;
        let mut result = self.with_ids(|ids| text_to_partial(&original, &mut || ids.next_id()));
        result.warnings.insert(
            0,
            ImportWarning::new(
                "Read without AI from headings and dates: check everything before saving.",
            ),
        );
        Ok(self.keep_preview(result, Vec::new()))
    }

    /// Starts extracting a prepared text with AI and returns the job's ID
    /// and the future to spawn; the job's result is an `ImportPreview`.
    pub fn ai_import(
        self: &Arc<Self>,
        events: Arc<dyn JobEvents>,
        req: AiImport,
    ) -> Result<(JobId, impl Future<Output = ()> + Send + 'static)> {
        let generation = self.ai_generation();
        self.llm()?;
        self.texts.get(req.text_id)?;
        Ok(self.start_job(events, move |app, ctx| async move {
            let preview = app.extract(&ctx, req, generation).await?;
            serde_json::to_value(preview).map_err(|_| {
                ApiError::new(ErrorCode::Internal, "The import result could not be sent.")
            })
        }))
    }

    async fn extract(
        self: &Arc<Self>,
        ctx: &JobContext,
        req: AiImport,
        generation: u64,
    ) -> Result<ImportPreview> {
        let (llm, status) = self
            .blocking(|app| Ok((app.llm()?, app.ai_status()?)))
            .await?;
        let model = status.model;
        let (original, sent) = self.texts.get(req.text_id)?;
        let cache = StoreCache::new(Arc::clone(self), generation);
        let stage = StageContext {
            llm: llm.as_ref(),
            cache: &cache,
            model: &model,
            cancel: ctx.cancel.clone(),
        };
        ctx.progress(AiStage::Import, 10);
        let cv = extract_cv(&stage, &sent).await.map_err(llm_error)?;
        ctx.check()?;
        let (result, needs_review) = self.with_ids(|ids| to_partial(cv, &original, &sent, ids));
        ctx.progress(AiStage::Import, 100);
        Ok(self.keep_preview(result, needs_review))
    }
}
