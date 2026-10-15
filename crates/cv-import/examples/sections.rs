//! Prints only the section headings and line counts found in a PDF, so a
//! local file can be checked without printing its content.
fn main() {
    let path = std::env::args().nth(1).expect("usage: sections <file.pdf>");
    let bytes = std::fs::read(path).expect("read");
    let text = cv_import::page_text(&bytes).expect("readable PDF");
    for (h, lines) in cv_import::split_sections(&text) {
        println!("{h:?}: {} lines", lines.len());
    }
    let mut n = 0u64;
    let mut fresh = || {
        n += 1;
        cv_core::Id::parse(&format!("01HZX3M8Q4V7K2N9P5R6T{n:05}")).expect("id")
    };
    let r = cv_import::text_to_partial(&text, &mut fresh);
    let p = &r.cv_partial;
    println!(
        "name {} headline {} email {} phone {} summary {}",
        p.full_name.is_some(),
        p.headline.is_some(),
        p.email.is_some(),
        p.phone.is_some(),
        p.summary.is_some()
    );
    for s in &p.sections {
        let n = match s {
            cv_core::Section::Experience(x) | cv_core::Section::Education(x) => x.entries.len(),
            cv_core::Section::Languages(l) => l.items.len(),
            _ => 0,
        };
        println!(
            "{}: {n} items",
            match s {
                cv_core::Section::Experience(_) => "Experience",
                cv_core::Section::Education(_) => "Education",
                cv_core::Section::Languages(_) => "Languages",
                _ => "Other",
            }
        );
    }
    println!("{} warnings", r.warnings.len());
}
