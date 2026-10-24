//! End-to-end smoke test (spec section 14): create a CV, edit it, wait for
//! the preview, export a PDF. Runs only when `PINAXCV_E2E_APP` names an app
//! built with `--features e2e` and `tauri-driver` is listening; CI sets both
//! up (see `.github/workflows/ci.yml` and `docs/BUILDING.md`).

use std::path::PathBuf;
use std::time::{Duration, Instant};
use std::{env, fs, thread};

use pinaxcv_e2e::{Session, WAIT, xpath_literal};

/// The labelled input inside `<label>{label}<input/></label>`.
fn field(label: &str) -> String {
    format!(
        "//label[normalize-space(text())={}]/input",
        xpath_literal(label)
    )
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
    let pdf = fs::read_dir(&export_dir)
        .expect("export dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "pdf"))
        .expect("an exported PDF");
    let bytes = fs::read(&pdf).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF"), "not a PDF: {}", pdf.display());

    s.quit().expect("quit");
}
