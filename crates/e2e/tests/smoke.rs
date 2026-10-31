//! End-to-end smoke test (spec section 14): create a CV, edit it, wait for
//! the preview, export a PDF. Runs only when `PINAXCV_E2E_APP` names an app
//! built with `--features e2e` and `tauri-driver` is listening; CI sets both
//! up (see `.github/workflows/ci.yml` and `docs/BUILDING.md`).

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};
use std::{env, fs, thread};

use pinaxcv_e2e::{Session, WAIT, xpath_literal};

/// The labelled input inside `<label>{label}<input/></label>`.
fn field(label: &str) -> String {
    format!(
        "//label[normalize-space(text())={}]/input",
        xpath_literal(label)
    )
}

/// Each PDF in `dir` with its modification time.
fn pdfs(dir: &Path) -> HashMap<PathBuf, SystemTime> {
    fs::read_dir(dir)
        .expect("export dir")
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "pdf"))
        .map(|e| {
            (
                e.path(),
                e.metadata().and_then(|m| m.modified()).expect("mtime"),
            )
        })
        .collect()
}

fn button(text: &str) -> String {
    format!("//button[normalize-space()={}]", xpath_literal(text))
}

#[test]
fn create_edit_preview_and_export() {
    let Some(app) = env::var_os("PINAXCV_E2E_APP").map(PathBuf::from) else {
        eprintln!("skipped: set PINAXCV_E2E_APP to an app built with --features e2e");
        return;
    };
    let export_dir = PathBuf::from(
        env::var_os("PINAXCV_E2E_EXPORT_DIR").expect("PINAXCV_E2E_EXPORT_DIR is set too"),
    );
    let driver = env::var("TAURI_DRIVER_URL").unwrap_or_else(|_| "http://127.0.0.1:4444".into());
    let s = Session::start(&driver, &app).expect("session");

    // Create a CV; the editor opens on it.
    let title = s
        .wait_for("//main[contains(@class,'list')]//input")
        .expect("title input");
    s.type_text(&title, "E2E smoke").expect("type title");
    s.click(&s.wait_for(&button("New CV")).expect("New CV"))
        .expect("create");

    // Edit, then wait for the preview to render a page.
    let name = s.wait_for(&field("Full name")).expect("name field");
    s.type_text(&name, "Jane Doe").expect("type name");
    let email = s.wait_for(&field("Email")).expect("email field");
    s.type_text(&email, "jane@example.com").expect("type email");
    s.wait_for("//section[contains(@class,'preview')]//img")
        .expect("preview page");

    // Export; the e2e build saves into the export folder without a dialog.
    // A PDF left by an earlier run counts only if this export rewrote it.
    let before = pdfs(&export_dir);
    s.click(&s.wait_for(&button("Export PDF")).expect("export button"))
        .expect("export");
    let status = s
        .wait_for("//span[contains(@class,'status')]")
        .expect("status");
    let start = Instant::now();
    loop {
        let text = s.text(&status).expect("status text");
        if text.starts_with("Exported to") {
            break;
        }
        assert!(start.elapsed() < WAIT, "export did not finish: {text}");
        thread::sleep(Duration::from_millis(250));
    }
    let pdf = pdfs(&export_dir)
        .into_iter()
        .find(|(p, t)| before.get(p) != Some(t))
        .map(|(p, _)| p)
        .expect("a PDF from this export");
    let bytes = fs::read(&pdf).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF"), "not a PDF: {}", pdf.display());

    // Style tab (ADR-084): two columns, then the name card moves to the
    // sidebar with its keyboard button.
    s.click(&s.wait_for("//button[@id='tab-style']").expect("Style tab"))
        .expect("open Style");
    s.click(
        &s.wait_for(
            "//fieldset[contains(@class,'layout')]//label[normalize-space()='Two columns']/input",
        )
        .expect("Two columns"),
    )
    .expect("two columns");
    s.click(
        &s.wait_for(&format!(
            "//div[@data-drop-zone='main']//button[@aria-label={}]",
            xpath_literal("Move Name to the other column")
        ))
        .expect("move button"),
    )
    .expect("move name");
    s.wait_for("//div[@data-drop-zone='sidebar']//li[@data-drop-key='top:name']")
        .expect("name card in the sidebar");
    // Then the summary card is dragged below it.
    let handle = s
        .wait_for("//li[@data-drop-key='top:summary']//span[contains(@class,'drag-handle')]")
        .expect("summary handle");
    let name_card = s
        .wait_for("//div[@data-drop-zone='sidebar']//li[@data-drop-key='top:name']")
        .expect("name card");
    s.drag(&handle, &name_card, 12).expect("drag");
    s.wait_for("//div[@data-drop-zone='sidebar']//li[@data-drop-key='top:summary']")
        .expect("summary card dragged to the sidebar");

    // The style is saved as a style file with the new layout.
    s.click(&s.wait_for(&button("Save style…")).expect("Save style"))
        .expect("save style");
    s.wait_for("//fieldset[contains(@class,'style-file')]//li[starts-with(normalize-space(),'Style saved')]")
        .expect("style saved message");
    let style = fs::read_dir(&export_dir)
        .expect("export dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.to_string_lossy().ends_with(".pinaxstyle.xml"))
        .expect("a style file");
    let xml = fs::read_to_string(&style).expect("read style");
    assert!(xml.contains("<layout columns=\"two\""), "{xml}");
    assert!(
        xml.contains("<block kind=\"summary\" column=\"sidebar\"/>"),
        "{xml}"
    );

    s.quit().expect("quit");
}
