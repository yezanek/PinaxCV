//! M0.5 spike: Typst PDF text extraction, deterministic bytes, PNG preview latency.
use std::time::Instant;

use sha2::{Digest, Sha256};
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration, Smart};
use typst_layout::PagedDocument;
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};

struct SpikeWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    main: Source,
}

impl SpikeWorld {
    fn new(text: String) -> Self {
        // Embedded in the binary, as cv-render will do.
        const FONTS: [&[u8]; 4] = [
            include_bytes!("../../fonts/Inter-Regular.ttf"),
            include_bytes!("../../fonts/Inter-Bold.ttf"),
            include_bytes!("../../fonts/Inter-SemiBold.ttf"),
            include_bytes!("../../fonts/Inter-Italic.ttf"),
        ];
        let mut fonts = Vec::new();
        for data in FONTS {
            fonts.extend(Font::iter(Bytes::new(data)));
        }
        let book = FontBook::from_fonts(&fonts);
        let id = FileId::new(RootedPath::new(VirtualRoot::Project, VirtualPath::new("main.typ").expect("path")));
        Self {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(book),
            fonts,
            main: Source::new(id, text),
        }
    }
    fn set_text(&mut self, text: String) {
        self.main.replace(&text);
    }
}

impl World for SpikeWorld {
    fn library(&self) -> &LazyHash<Library> { &self.library }
    fn book(&self) -> &LazyHash<FontBook> { &self.book }
    fn main(&self) -> FileId { self.main.id() }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() { Ok(self.main.clone()) } else { Err(FileError::NotFound(id.vpath().get_without_slash().into())) }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        Err(FileError::NotFound(id.vpath().get_without_slash().into()))
    }
    fn font(&self, index: usize) -> Option<Font> { self.fonts.get(index).cloned() }
    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> { None }
}

fn compile(world: &SpikeWorld) -> PagedDocument {
    let result = typst::compile::<PagedDocument>(world);
    match result.output {
        Ok(doc) => doc,
        Err(errs) => panic!("{errs:?}"),
    }
}

fn pdf(doc: &PagedDocument) -> Vec<u8> {
    let options = typst_pdf::PdfOptions { ident: Smart::Custom("pinaxcv".to_owned()), timestamp: None, ..Default::default() };
    typst_pdf::pdf(doc, &options).expect("pdf")
}

fn sha(b: &[u8]) -> String {
    Sha256::digest(b).iter().take(8).map(|x| format!("{x:02x}")).collect()
}

fn ms(t: Instant) -> f64 { t.elapsed().as_secs_f64() * 1000.0 }

fn main() {
    let src = include_str!("cv.typ").to_owned();

    // --- cold compile + PDF
    let t = Instant::now();
    let mut world = SpikeWorld::new(src.clone());
    let doc = compile(&world);
    let cold = ms(t);
    let t = Instant::now();
    let bytes = pdf(&doc);
    let pdf_ms = ms(t);
    println!("pages={} cold_compile_ms={cold:.1} pdf_export_ms={pdf_ms:.1} pdf_bytes={}", doc.pages().len(), bytes.len());
    std::fs::write("out.pdf", &bytes).expect("write");

    // --- determinism: fresh world, same input
    let world2 = SpikeWorld::new(src.clone());
    let bytes2 = pdf(&compile(&world2));
    println!("deterministic_same_process={} sha={}", bytes == bytes2, sha(&bytes));

    // --- warm incremental edits (preview loop): edit one bullet, recompile, render page 1 to PNG
    let mut compile_ms = Vec::new();
    let mut render_ms = Vec::new();
    let mut png_len = 0;
    for i in 0..20 {
        world.set_text(src.replace("cutting p99 latency by 43%", &format!("cutting p99 latency by {}%", 40 + i)));
        let t = Instant::now();
        let d = compile(&world);
        compile_ms.push(ms(t));
        let t = Instant::now();
        let pixmap = typst_render::render(&d.pages()[0], &typst_render::RenderOptions { pixel_per_pt: 1.5.into(), ..Default::default() });
        let png = pixmap.encode_png().expect("png");
        render_ms.push(ms(t));
        png_len = png.len();
        if i == 0 { std::fs::write("page1.png", &png).expect("write"); }
    }
    let med = |v: &mut Vec<f64>| { v.sort_by(f64::total_cmp); (v[v.len() / 2], v[v.len() * 9 / 10]) };
    let (c50, c90) = med(&mut compile_ms);
    let (r50, r90) = med(&mut render_ms);
    println!("warm_compile_ms p50={c50:.1} p90={c90:.1}; png_render_ms(1.5x) p50={r50:.1} p90={r90:.1}; png_bytes={png_len}");

    // --- extraction
    let t = Instant::now();
    let text = pdf_extract::extract_text_from_mem(&bytes).unwrap_or_else(|e| format!("<pdf-extract error: {e}>"));
    println!("pdf_extract_ms={:.1}", ms(t));
    std::fs::write("extract-pdf-extract.txt", &text).expect("write");
    let doc = lopdf::Document::load_mem(&bytes).expect("lopdf");
    let pages: Vec<u32> = doc.get_pages().keys().copied().collect();
    let text2 = doc.extract_text(&pages).unwrap_or_else(|e| format!("<lopdf error: {e}>"));
    std::fs::write("extract-lopdf.txt", &text2).expect("write");

    let checks = [
        "Jane Doe", "jane.doe@example.com", "+49 30 1234 5678", "Work Experience",
        "Senior Backend Engineer", "Acme GmbH", "Mar 2021", "Office, efficient, affluent",
        "cutting p99 latency by 43%", "façade naïve café", "Ünïcödé",
    ];
    for (name, t) in [("pdf-extract", &text), ("lopdf", &text2)] {
        let norm = t.split_whitespace().collect::<Vec<_>>().join(" ");
        let missing: Vec<_> = checks.iter().filter(|c| !norm.contains(*c)).collect();
        let email_line = t.lines().filter(|l| !l.trim().is_empty()).position(|l| l.contains("jane.doe@example.com"));
        let bad = t.chars().filter(|c| ('\u{E000}'..='\u{F8FF}').contains(c) || *c == '\u{FFFD}' || ('\u{FB00}'..='\u{FB06}').contains(c)).count();
        println!("{name}: missing={missing:?} email_line={email_line:?} bad_codepoints={bad}");
    }
}
