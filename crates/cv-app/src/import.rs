//! The import wizard's services (spec section 9.5): `import_parse` reads a
//! file the user picks into a preview kept in memory, `import_match` lines it
//! up with an existing CV, and `import_commit` saves it as a new CV or merges
//! it into one after a snapshot.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use cv_api::{
    ApiError, EntryChoice, EntryMatch, ErrorCode, ImportCommit, ImportConflict, ImportField,
    ImportMatch, ImportMatchRequest, ImportPreview, ImportTarget,
};
use cv_core::{Id, SnapshotReason, StoredCv, validate};
use cv_import::{
    CvPartial, EntryDecision, Field, ImportError, ImportResult, MAX_INPUT, MergePlan,
    field_conflicts, import_any, merge, suggest_matches,
};

use crate::{App, Result, blank_cv, store_error, validation_error};

/// File extensions the open dialog offers.
pub const IMPORT_EXTENSIONS: &[&str] = &["pdf", "xml"];

/// Previews kept at once; opening another drops the oldest.
const KEPT_PREVIEWS: usize = 4;

/// Parsed imports waiting for `import_commit`, by preview ID.
#[derive(Default)]
pub struct ImportCache {
    inner: Mutex<(u64, Vec<(u64, CvPartial)>)>,
}

impl ImportCache {
    fn put(&self, partial: CvPartial) -> u64 {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner.0 += 1;
        let id = inner.0;
        inner.1.push((id, partial));
        if inner.1.len() > KEPT_PREVIEWS {
            inner.1.remove(0);
        }
        id
    }

    fn get(&self, id: u64) -> Result<CvPartial> {
        let inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner
            .1
            .iter()
            .find(|(k, _)| *k == id)
            .map(|(_, p)| p.clone())
            .ok_or_else(expired)
    }

    fn remove(&self, id: u64) {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner.1.retain(|(k, _)| *k != id);
    }
}

/// `Import`, not `NotFound`, so the UI shows this message rather than the
/// generic "no longer exists" one.
fn expired() -> ApiError {
    ApiError::new(
        ErrorCode::Import,
        "This import is no longer open. Choose the file again.",
    )
}

impl App {
    /// Asks for a file and reads it into a preview. Nothing is saved.
    pub fn import_parse(&self) -> Result<ImportPreview> {
        let path = self
            .dialog
            .choose_open_path(IMPORT_EXTENSIONS)
            .ok_or_else(|| ApiError::new(ErrorCode::Cancelled, "Import cancelled."))?;
        let bytes = read_capped(&path)?;
        let result = self
            .with_ids(|ids| import_any(&bytes, &mut || ids.next_id()))
            .map_err(import_error)?;
        Ok(self.keep_preview(result, Vec::new()))
    }

    pub(crate) fn keep_preview(
        &self,
        result: ImportResult,
        needs_review: Vec<Id>,
    ) -> ImportPreview {
        let ImportResult {
            cv_partial: p,
            warnings,
        } = result;
        let preview = ImportPreview {
            preview_id: 0,
            full_name: p.full_name.clone(),
            headline: p.headline.clone(),
            email: p.email.clone(),
            phone: p.phone.clone(),
            location: p.location.clone(),
            links: p.links.clone(),
            summary: p.summary.clone(),
            sections: p.sections.clone(),
            warnings: warnings.into_iter().map(|w| w.message).collect(),
            needs_review,
        };
        ImportPreview {
            preview_id: self.imports.put(p),
            ..preview
        }
    }

    /// The conflicting fields and the suggested entry matches against `cv_id`.
    pub fn import_match(&self, req: ImportMatchRequest) -> Result<ImportMatch> {
        let partial = self.imports.get(req.preview_id)?;
        let base = self.store.get_cv(&req.cv_id).map_err(store_error)?.cv;
        let conflicts = field_conflicts(&base, &partial)
            .into_iter()
            .map(|c| ImportConflict {
                field: api_field(c.field),
                mine: c.mine,
                imported: c.imported,
            })
            .collect();
        let mut matches: Vec<EntryMatch> = suggest_matches(&base, &partial)
            .into_iter()
            .map(|(imported, base)| EntryMatch { imported, base })
            .collect();
        matches.sort_by(|a, b| a.imported.cmp(&b.imported));
        Ok(ImportMatch { conflicts, matches })
    }

    /// Saves the preview as a new CV, or merges it into an existing one after
    /// taking a snapshot of it. The preview is dropped once saved.
    pub fn import_commit(&self, req: ImportCommit) -> Result<StoredCv> {
        let partial = self.imports.get(req.preview_id)?;
        let stored = match req.target {
            ImportTarget::New { title } => {
                let title = match title.trim() {
                    "" => partial
                        .full_name
                        .clone()
                        .unwrap_or_else(|| "Imported CV".into()),
                    t => t.to_owned(),
                };
                let mut cv = self.with_defaults(blank_cv(self.next_id(), &title))?;
                merge(&mut cv, partial, &MergePlan::default());
                let issues = validate::for_save(&mut cv);
                if validate::has_errors(&issues) {
                    return Err(validation_error(&issues));
                }
                self.store.create_cv(&cv).map_err(store_error)?
            }
            ImportTarget::Merge {
                cv_id,
                expected_revision,
            } => {
                let current = self.store.get_cv(&cv_id).map_err(store_error)?;
                if current.revision != expected_revision {
                    return Err(ApiError::new(
                        ErrorCode::Conflict,
                        "This CV changed since the import started. Open the import again.",
                    )
                    .with_details(serde_json::json!({ "current_revision": current.revision })));
                }
                let plan = MergePlan {
                    use_imported: req.use_imported.into_iter().map(core_field).collect(),
                    entries: req
                        .entries
                        .into_iter()
                        .map(|d| (d.imported, decision(d.choice)))
                        .collect::<HashMap<Id, EntryDecision>>(),
                };
                let mut cv = current.cv;
                merge(&mut cv, partial, &plan);
                let issues = validate::for_save(&mut cv);
                if validate::has_errors(&issues) {
                    return Err(validation_error(&issues));
                }
                self.with_ids(|ids| self.store.add_snapshot(&cv_id, SnapshotReason::Import, ids))
                    .map_err(store_error)?;
                self.store
                    .save_cv(&cv, expected_revision)
                    .map_err(store_error)?
            }
        };
        self.imports.remove(req.preview_id);
        Ok(stored)
    }
}

/// Reads at most [`MAX_INPUT`] bytes, failing on anything larger without
/// reading it all.
pub(crate) fn read_capped(path: &Path) -> Result<Vec<u8>> {
    // The io::Error text can carry the path, which holds the user's name.
    let unreadable = |e: std::io::Error| {
        ApiError::new(
            ErrorCode::Io,
            "Could not read the file. Check that it still exists and you can open it.",
        )
        .with_details(serde_json::json!({ "kind": format!("{:?}", e.kind()) }))
    };
    let file = File::open(path).map_err(unreadable)?;
    let mut bytes = Vec::new();
    file.take(MAX_INPUT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(unreadable)?;
    if bytes.len() > MAX_INPUT {
        return Err(import_error(ImportError::TooLarge));
    }
    Ok(bytes)
}

pub(crate) fn import_error(e: ImportError) -> ApiError {
    match e {
        ImportError::TooLarge => ApiError::new(
            ErrorCode::Import,
            "That file is larger than 20 MB, which is too big for a CV.",
        ),
        ImportError::UnsupportedFormat(m) => ApiError::new(ErrorCode::UnsupportedFormat, m),
        ImportError::Malformed(m) => ApiError::new(ErrorCode::Import, m),
    }
}

fn decision(choice: EntryChoice) -> EntryDecision {
    match choice {
        EntryChoice::Skip => EntryDecision::Skip,
        EntryChoice::AddNew => EntryDecision::AddNew,
        EntryChoice::Replace { base } => EntryDecision::Replace(base),
    }
}

fn api_field(f: Field) -> ImportField {
    match f {
        Field::FullName => ImportField::FullName,
        Field::Headline => ImportField::Headline,
        Field::Email => ImportField::Email,
        Field::Phone => ImportField::Phone,
        Field::Location => ImportField::Location,
        Field::Summary => ImportField::Summary,
    }
}

fn core_field(f: ImportField) -> Field {
    match f {
        ImportField::FullName => Field::FullName,
        ImportField::Headline => Field::Headline,
        ImportField::Email => Field::Email,
        ImportField::Phone => Field::Phone,
        ImportField::Location => Field::Location,
        ImportField::Summary => Field::Summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cache_keeps_the_latest_previews() {
        let cache = ImportCache::default();
        let ids: Vec<u64> = (0..=KEPT_PREVIEWS)
            .map(|_| cache.put(CvPartial::default()))
            .collect();
        assert!(cache.get(ids[0]).is_err());
        assert!(cache.get(ids[KEPT_PREVIEWS]).is_ok());
        cache.remove(ids[KEPT_PREVIEWS]);
        assert_eq!(
            cache.get(ids[KEPT_PREVIEWS]).map_err(|e| e.code),
            Err(ErrorCode::Import)
        );
    }

    #[test]
    fn fields_map_both_ways() {
        for f in [
            Field::FullName,
            Field::Headline,
            Field::Email,
            Field::Phone,
            Field::Location,
            Field::Summary,
        ] {
            assert_eq!(core_field(api_field(f)), f);
        }
    }

    #[test]
    fn oversized_files_are_refused() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("big.pdf");
        std::fs::write(&path, vec![b' '; MAX_INPUT + 1]).expect("write");
        assert_eq!(
            read_capped(&path).map_err(|e| e.code),
            Err(ErrorCode::Import)
        );
        let gone = read_capped(&dir.path().join("missing.pdf")).expect_err("missing");
        assert_eq!(gone.code, ErrorCode::Io);
        assert!(!gone.message.contains("missing.pdf"));
    }
}
