//! Typst rendering: PDF, PNG preview and page counting (spec sections 6.1 and 6.2).
//!
//! One [`Renderer`] lives for the whole process. Its World holds the embedded
//! fonts and template; only the virtual `data.json` changes between
//! compiles, so Typst's incremental cache stays warm. There is no system font,
//! filesystem, network or package access.

use std::collections::BTreeSet;
use std::sync::{Mutex, PoisonError};

use cv_core::{Diagnostic, PageCountError, PageCounter, ResolvedCv, Severity};
use typst::diag::{FileError, FileResult, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime, Duration, Smart};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;

use crate::data::{SectionData, TypstData, to_typst_data};
use crate::{ExportOptions, ExportedDocument, Exporter, RenderError, suggested_file_name};

const FONTS: [&[u8]; 4] = [
    include_bytes!("../fonts/Inter-Regular.ttf"),
    include_bytes!("../fonts/Inter-Bold.ttf"),
    include_bytes!("../fonts/Inter-SemiBold.ttf"),
    include_bytes!("../fonts/Inter-Italic.ttf"),
];

const CLASSIC: &str = include_str!("../templates/classic/template.typ");

/// The embedded manifest of the `classic` template, for the UI.
pub const CLASSIC_MANIFEST: &str = include_str!("../templates/classic/manifest.json");

/// The largest preview scale accepted; an A4 page is then about 4800 px wide.
pub const MAX_PIXELS_PER_PT: f32 = 8.0;

/// Diagnostic code for characters the embedded fonts cannot display.
pub const MISSING_GLYPHS: &str = "missing_glyphs";

/// The long-lived Typst renderer.
pub struct Renderer {
    world: Mutex<CvWorld>,
}

/// A compiled CV, ready for PDF or PNG output.
pub struct Compiled {
    document: PagedDocument,
    warnings: Vec<Diagnostic>,
}

impl Renderer {
    /// Loads the embedded fonts and the `classic` template.
    pub fn new() -> Self {
        Self {
            world: Mutex::new(CvWorld::new(CLASSIC)),
        }
    }

    /// Compiles `cv`. Warnings, including [`MISSING_GLYPHS`], come back on the
    /// result; errors come back as diagnostics.
    pub fn compile(
        &self,
        cv: &ResolvedCv,
        options: &ExportOptions,
    ) -> Result<Compiled, Vec<Diagnostic>> {
        let data = to_typst_data(cv, options);
        let mut warnings = Vec::new();
        let json = serde_json::to_vec(&data).map_err(|e| {
            vec![error_diagnostic(
                "data",
                &format!("cannot encode data.json: {e}"),
            )]
        })?;
        let mut world = self.world.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(missing) = world.missing_glyphs(&data) {
            warnings.push(missing);
        }
        world.set_data(json, options);
        let result = typst::compile::<PagedDocument>(&*world);
        warnings.extend(result.warnings.iter().map(diagnostic));
        match result.output {
            Ok(document) => Ok(Compiled { document, warnings }),
            Err(errors) => Err(errors.iter().map(diagnostic).collect()),
        }
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Compiled {
    pub fn page_count(&self) -> u32 {
        u32::try_from(self.document.pages().len()).unwrap_or(u32::MAX)
    }

    /// Warnings from the glyph check and from Typst.
    pub fn warnings(&self) -> &[Diagnostic] {
        &self.warnings
    }

    /// Exports a tagged PDF that declares PDF/UA-1 conformance. When Typst
    /// cannot meet PDF/UA-1, as with glyphs missing from the font (ADR-033),
    /// it exports the same tagged PDF without the claim (ADR-061). The bytes
    /// depend only on the input: no creation date, a fixed document
    /// identifier and no creator string.
    pub fn to_pdf(&self) -> Result<Vec<u8>, RenderError> {
        let options = |standards| typst_pdf::PdfOptions {
            ident: Smart::Custom("pinaxcv".to_owned()),
            creator: Smart::Custom(None),
            timestamp: None,
            tagged: true,
            standards,
            ..Default::default()
        };
        let ua = typst_pdf::PdfStandards::new(&[typst_pdf::PdfStandard::Ua_1])
            .map_err(|e| RenderError::Export(e.message().to_string()))?;
        typst_pdf::pdf(&self.document, &options(ua))
            .or_else(|_| typst_pdf::pdf(&self.document, &options(Default::default())))
            .map_err(|errors| {
                let message: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
                RenderError::Export(message.join("; "))
            })
    }

    /// Renders every page to PNG at `pixels_per_pt` (1.5 suits the preview).
    /// Scales that are not finite, not positive or above
    /// [`MAX_PIXELS_PER_PT`] are refused.
    pub fn preview_png(&self, pixels_per_pt: f32) -> Result<Vec<Vec<u8>>, RenderError> {
        if !pixels_per_pt.is_finite() || pixels_per_pt <= 0.0 || pixels_per_pt > MAX_PIXELS_PER_PT {
            return Err(RenderError::Export(format!(
                "preview scale {pixels_per_pt} is outside 0 to {MAX_PIXELS_PER_PT}"
            )));
        }
        let options = typst_render::RenderOptions {
            pixel_per_pt: f64::from(pixels_per_pt).into(),
            ..Default::default()
        };
        self.document
            .pages()
            .iter()
            .map(|page| {
                typst_render::render(page, &options)
                    .encode_png()
                    .map_err(|e| RenderError::Export(format!("PNG encoding failed: {e}")))
            })
            .collect()
    }
}

impl PageCounter for Renderer {
    fn pages(&self, cv: &ResolvedCv) -> Result<u32, PageCountError> {
        self.compile(cv, &ExportOptions::default())
            .map(|c| c.page_count())
            .map_err(|diags| {
                let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
                PageCountError(messages.join("; "))
            })
    }
}

/// Exports PDFs through a shared [`Renderer`].
pub struct PdfExporter<'a> {
    pub renderer: &'a Renderer,
}

impl Exporter for PdfExporter<'_> {
    fn export(
        &self,
        cv: &ResolvedCv,
        options: &ExportOptions,
    ) -> Result<ExportedDocument, RenderError> {
        let compiled = self
            .renderer
            .compile(cv, options)
            .map_err(RenderError::Compile)?;
        Ok(ExportedDocument {
            bytes: compiled.to_pdf()?,
            media_type: "application/pdf",
            suggested_file_name: suggested_file_name(&cv.basics.full_name, "pdf"),
            page_count: Some(compiled.page_count()),
            warnings: compiled.warnings().to_vec(),
        })
    }
}

fn diagnostic(d: &SourceDiagnostic) -> Diagnostic {
    let severity = match d.severity {
        typst::diag::Severity::Error => Severity::Error,
        typst::diag::Severity::Warning => Severity::Warning,
    };
    Diagnostic {
        severity,
        code: "typst".to_owned(),
        message: d.message.to_string(),
    }
}

fn error_diagnostic(code: &str, message: &str) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

/// The Typst World: embedded fonts, one template and a virtual `data.json`.
struct CvWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    main: Source,
    data_id: FileId,
    data: Bytes,
    photo_id: FileId,
    photo: Option<Bytes>,
    signature_id: FileId,
    signature: Option<Bytes>,
}

fn file_id(name: &str) -> FileId {
    // Both names are static and valid.
    let path = VirtualPath::new(name).unwrap_or_else(|_| unreachable!("valid virtual path"));
    FileId::new(RootedPath::new(VirtualRoot::Project, path))
}

impl CvWorld {
    fn new(template: &str) -> Self {
        let mut fonts = Vec::new();
        for data in FONTS {
            fonts.extend(Font::iter(Bytes::new(data)));
        }
        Self {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(FontBook::from_fonts(&fonts)),
            fonts,
            main: Source::new(file_id("template.typ"), template.to_owned()),
            data_id: file_id("data.json"),
            data: Bytes::new(b"{}".to_vec()),
            photo_id: file_id("photo.jpg"),
            photo: None,
            signature_id: file_id("signature.png"),
            signature: None,
        }
    }

    fn set_data(&mut self, json: Vec<u8>, options: &ExportOptions) {
        self.data = Bytes::new(json);
        self.photo = options.photo.clone().map(Bytes::new);
        self.signature = options.signature.clone().map(Bytes::new);
    }

    /// Lists the printed characters no embedded font covers.
    fn missing_glyphs(&self, data: &TypstData) -> Option<Diagnostic> {
        let mut missing = BTreeSet::new();
        for text in printed_strings(data) {
            for c in text.chars() {
                if !c.is_whitespace()
                    && !self
                        .fonts
                        .iter()
                        .any(|f| f.info().coverage.contains(u32::from(c)))
                {
                    missing.insert(c);
                }
            }
        }
        if missing.is_empty() {
            return None;
        }
        let list: String = missing.iter().take(20).collect();
        let more = if missing.len() > 20 { " and more" } else { "" };
        Some(Diagnostic {
            severity: Severity::Warning,
            code: MISSING_GLYPHS.to_owned(),
            message: format!(
                "The font cannot display {} character(s): {list}{more}. They will be missing from the PDF.",
                missing.len()
            ),
        })
    }
}

/// Every string the template prints.
fn printed_strings(d: &TypstData) -> Vec<&str> {
    let mut out: Vec<&str> = vec![d.basics.full_name.as_str()];
    let b = &d.basics;
    out.extend(
        [&b.headline, &b.email, &b.phone, &b.location, &d.summary]
            .into_iter()
            .flatten()
            .map(String::as_str),
    );
    for l in &b.links {
        out.push(&l.url);
        out.extend(l.label.as_deref());
    }
    for s in &d.sections {
        match s {
            SectionData::Experience(e)
            | SectionData::Education(e)
            | SectionData::Projects(e)
            | SectionData::Certifications(e)
            | SectionData::Custom(e) => {
                out.push(&e.heading);
                for entry in &e.entries {
                    out.extend(
                        [
                            &entry.title,
                            &entry.org,
                            &entry.location,
                            &entry.period,
                            &entry.description,
                        ]
                        .into_iter()
                        .flatten()
                        .map(String::as_str),
                    );
                    if let Some(l) = &entry.link {
                        out.push(&l.url);
                    }
                    out.extend(entry.bullets.iter().map(String::as_str));
                }
            }
            SectionData::Skills { heading, groups } => {
                out.push(heading);
                for g in groups {
                    out.extend(g.label.as_deref());
                    out.extend(g.skills.iter().map(String::as_str));
                }
            }
            SectionData::Languages { heading, items } => {
                out.push(heading);
                for i in items {
                    out.push(&i.name);
                    out.extend(i.level.as_deref());
                }
            }
        }
    }
    out
}

impl World for CvWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            Err(FileError::AccessDenied)
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.data_id {
            Ok(self.data.clone())
        } else if id == self.photo_id {
            self.photo.clone().ok_or(FileError::AccessDenied)
        } else if id == self.signature_id {
            self.signature.clone().ok_or(FileError::AccessDenied)
        } else {
            Err(FileError::AccessDenied)
        }
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdata::{FIXTURES, fixture};

    fn compile(name: &str) -> Compiled {
        Renderer::new()
            .compile(&fixture(name).resolve(None), &ExportOptions::default())
            .unwrap_or_else(|e| panic!("{name}: {e:?}"))
    }

    #[test]
    fn every_fixture_compiles() {
        let renderer = Renderer::new();
        for name in FIXTURES {
            let compiled = renderer
                .compile(&fixture(name).resolve(None), &ExportOptions::default())
                .unwrap_or_else(|e| panic!("{name}: {e:?}"));
            assert!(compiled.page_count() >= 1, "{name}");
            let pdf = compiled.to_pdf().expect("pdf");
            assert!(pdf.starts_with(b"%PDF-"), "{name}");
        }
    }

    #[test]
    fn pdf_bytes_are_deterministic() {
        let a = compile("typical").to_pdf().expect("pdf");
        let b = compile("typical").to_pdf().expect("pdf");
        assert_eq!(a, b);
    }

    #[test]
    fn long_fixture_spans_several_pages_and_counter_agrees() {
        let cv = fixture("long").resolve(None);
        let renderer = Renderer::new();
        let pages = renderer.pages(&cv).expect("count");
        assert!(pages >= 3, "long fixture is {pages} pages");
        assert_eq!(compile("long").page_count(), pages);
        assert_eq!(compile("minimal").page_count(), 1);
    }

    #[test]
    fn pdfs_claim_pdf_ua_when_typst_can_meet_it() {
        let contains = |pdf: &[u8], needle: &[u8]| pdf.windows(needle.len()).any(|w| w == needle);
        let typical = compile("typical").to_pdf().expect("typical exports");
        assert!(contains(&typical, b"pdfuaid"), "PDF/UA-1 declared");
        // Missing glyphs break PDF/UA-1; the export still succeeds without it.
        let cjk = compile("cjk").to_pdf().expect("cjk still exports");
        assert!(cjk.starts_with(b"%PDF") && !contains(&cjk, b"pdfuaid"));
    }

    #[test]
    fn cjk_reports_missing_glyphs() {
        let compiled = compile("cjk");
        let missing: Vec<_> = compiled
            .warnings()
            .iter()
            .filter(|w| w.code == MISSING_GLYPHS)
            .collect();
        assert_eq!(missing.len(), 1, "{:?}", compiled.warnings());
        assert!(missing[0].message.contains('ソ'), "{}", missing[0].message);
        assert!(
            compile("unicode")
                .warnings()
                .iter()
                .all(|w| w.code != MISSING_GLYPHS),
            "Latin, Greek and Cyrillic are covered"
        );
    }

    #[test]
    fn preview_renders_one_png_per_page() {
        let compiled = compile("long");
        let pngs = compiled.preview_png(1.0).expect("png");
        assert_eq!(pngs.len(), compiled.page_count() as usize);
        assert!(pngs.iter().all(|p| p.starts_with(b"\x89PNG")));
    }

    #[test]
    fn preview_refuses_bad_scales() {
        let compiled = compile("minimal");
        for scale in [0.0, -1.0, f32::NAN, f32::INFINITY, MAX_PIXELS_PER_PT * 2.0] {
            assert!(compiled.preview_png(scale).is_err(), "{scale}");
        }
        assert!(compiled.preview_png(MAX_PIXELS_PER_PT).is_ok());
    }

    #[test]
    fn pdf_export_keeps_missing_glyph_warnings() {
        let renderer = Renderer::new();
        let doc = PdfExporter {
            renderer: &renderer,
        }
        .export(&fixture("cjk").resolve(None), &ExportOptions::default())
        .expect("export is still allowed");
        assert!(doc.warnings.iter().any(|w| w.code == MISSING_GLYPHS));
    }

    #[test]
    fn pdf_exporter_names_file_and_counts_pages() {
        let renderer = Renderer::new();
        let doc = PdfExporter {
            renderer: &renderer,
        }
        .export(&fixture("typical").resolve(None), &ExportOptions::default())
        .expect("export");
        assert_eq!(doc.media_type, "application/pdf");
        assert_eq!(doc.suggested_file_name, "Jane-Doe-CV.pdf");
        assert_eq!(doc.page_count, Some(1));
    }

    #[test]
    fn templates_cannot_read_other_files() {
        let world = CvWorld::new("#read(\"/etc/passwd\")");
        let result = typst::compile::<PagedDocument>(&world);
        assert!(result.output.is_err());
    }

    #[test]
    fn manifest_is_valid_json_for_classic() {
        let m: serde_json::Value = serde_json::from_str(CLASSIC_MANIFEST).expect("json");
        assert_eq!(m["id"], "classic");
        assert_eq!(m["ats_safe"], true);
    }
}
