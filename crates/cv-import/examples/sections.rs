//! Prints only the section headings and line counts found in a PDF, so a
//! local file can be checked without printing its content.
fn main() {
    let path = std::env::args().nth(1).expect("usage: sections <file.pdf>");
    let bytes = std::fs::read(path).expect("read");
    let text = cv_import::page_text(&bytes).expect("readable PDF");
    for (h, lines) in cv_import::split_sections(&text) {
        println!("{h:?}: {} lines", lines.len());
    }
}
