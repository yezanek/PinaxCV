//! Measures the live-preview path with the real `classic` template (spec
//! section 16, ADR-035): edit one bullet, resolve, compile and render every
//! page to PNG, as the app does after the debounce. Also times PDF and HTML
//! export.
//!
//! Run with: `cargo run --release -p cv-render --example preview_latency`

use std::path::PathBuf;
use std::time::{Duration, Instant};

use cv_core::{Cv, Section};
use cv_render::{ExportOptions, Renderer, to_html};

const ROUNDS: usize = 40;

fn fixture(name: &str) -> Cv {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/cvs")
        .join(format!("{name}.json"));
    let json = std::fs::read_to_string(&path).expect("fixture");
    serde_json::from_str(&json).expect("fixture parses")
}

/// p50 and p95 in milliseconds.
fn percentiles(mut samples: Vec<Duration>) -> (f64, f64) {
    samples.sort();
    let at = |q: f64| {
        let i = ((samples.len() - 1) as f64 * q).round() as usize;
        samples[i].as_secs_f64() * 1000.0
    };
    (at(0.5), at(0.95))
}

fn main() {
    let renderer = Renderer::new();
    let options = ExportOptions::default();
    let mut cv = fixture("long");
    let pages = renderer
        .compile(&cv.resolve(None), &options)
        .expect("compiles")
        .page_count();

    let mut preview = Vec::with_capacity(ROUNDS);
    for round in 0..ROUNDS {
        if let Some(Section::Experience(s)) = cv.sections.first_mut()
            && let Some(b) = s.entries.first_mut().and_then(|e| e.bullets.first_mut())
        {
            b.text = format!(
                "Cut p99 latency by {}% across the billing platform.",
                30 + round
            );
        }
        let t = Instant::now();
        let compiled = renderer
            .compile(&cv.resolve(None), &options)
            .expect("compiles");
        let pngs = compiled.preview_png(1.5).expect("renders");
        preview.push(t.elapsed());
        assert_eq!(pngs.len(), pages as usize);
    }

    let typical = fixture("typical").resolve(None);
    let mut pdf = Vec::with_capacity(ROUNDS);
    let mut html = Vec::with_capacity(ROUNDS);
    for _ in 0..ROUNDS {
        let t = Instant::now();
        let bytes = renderer
            .compile(&typical, &options)
            .expect("compiles")
            .to_pdf()
            .expect("pdf");
        pdf.push(t.elapsed());
        assert!(!bytes.is_empty());
        let t = Instant::now();
        let doc = to_html(&typical, &options);
        html.push(t.elapsed());
        assert!(!doc.is_empty());
    }

    let (p50, p95) = percentiles(preview);
    println!(
        "preview ({pages}-page `long` CV, all pages at 1.5 px/pt): p50 {p50:.1} ms, p95 {p95:.1} ms (target p95 < 250 ms)"
    );
    let (p50, p95) = percentiles(pdf);
    println!("PDF export (`typical`): p50 {p50:.1} ms, p95 {p95:.1} ms (target < 1000 ms)");
    let (p50, p95) = percentiles(html);
    println!("HTML export (`typical`): p50 {p50:.2} ms, p95 {p95:.2} ms (target < 250 ms)");
}
