//! Every user-facing UI string (spec section 2, ADR-050). English is the only
//! locale in v1; another locale is one more `Strings` value. CV content,
//! headings and dates go through `cv-core::locale` instead.
//!
//! Strings with values in them are templates: `{name}` placeholders are
//! filled by [`fill`], so a translation can move them around.

use cv_api::{ApiError, ErrorCode};

/// The UI text for one locale.
pub struct Strings {
    pub app_name: &'static str,

    // CV list
    pub your_cvs: &'static str,
    pub delete: &'static str,
    pub new_cv: &'static str,
    pub new_cv_title_placeholder: &'static str,
    pub new_cv_title: &'static str,
    pub no_cvs_yet: &'static str,
    pub duplicate: &'static str,
    /// `{title}`: the title of a duplicated CV.
    pub copy_title: &'static str,
    /// `{title}`.
    pub deleted_cv: &'static str,
    /// `{date}`.
    pub edited: &'static str,
    /// `{day}`, `{month}` (from `months`), `{year}` and `{time}` (HH:MM).
    pub date_time: &'static str,
    pub months: [&'static str; 12],
    pub one_page: &'static str,
    /// `{n}`.
    pub n_pages: &'static str,
    pub one_variant: &'static str,
    /// `{n}`.
    pub n_variants: &'static str,

    // Editor bar and status
    pub back_to_cvs: &'static str,
    pub export_pdf: &'static str,
    pub export_html: &'static str,
    pub export_pdf_hint: &'static str,
    pub undo: &'static str,
    pub undo_hint: &'static str,
    pub redo: &'static str,
    pub redo_hint: &'static str,
    pub loading: &'static str,
    pub editing: &'static str,
    pub saving: &'static str,
    pub saved: &'static str,
    pub not_saved: &'static str,
    pub exporting: &'static str,
    pub export_cancelled: &'static str,
    pub export_failed: &'static str,
    /// `{path}`, then `{ats}` (empty, or [`Strings::ats_score`]).
    pub exported_to: &'static str,
    /// `{score}`.
    pub ats_score: &'static str,

    // Editor form and preview
    pub content: &'static str,
    pub preview: &'static str,
    pub updating_preview: &'static str,
    pub page_preview: &'static str,
    pub cv_title: &'static str,
    pub full_name: &'static str,
    pub headline: &'static str,
    pub email: &'static str,
    pub phone: &'static str,
    pub location: &'static str,
    pub summary: &'static str,

    // Style tab
    pub style: &'static str,
    pub template: &'static str,
    pub paper: &'static str,
    pub paper_a4: &'static str,
    pub paper_letter: &'static str,
    pub cv_language: &'static str,
    pub english: &'static str,
    pub date_style: &'static str,
    pub date_month_name: &'static str,
    pub date_numeric: &'static str,
    pub date_year_only: &'static str,
    pub font_size: &'static str,
    pub line_spacing: &'static str,
    pub margins: &'static str,
    /// Units after a slider's value, with their leading space.
    pub unit_pt: &'static str,
    pub unit_mm: &'static str,
    pub accent: &'static str,
    pub accent_none: &'static str,
    pub accent_reset: &'static str,

    // Section kinds
    pub experience: &'static str,
    pub work_experience: &'static str,
    pub education: &'static str,
    pub projects: &'static str,
    pub certifications: &'static str,
    pub skills: &'static str,
    pub languages: &'static str,

    // Adding and collapsing sections
    pub add_section: &'static str,
    pub add_section_choose: &'static str,
    pub custom_section: &'static str,
    pub custom_title: &'static str,
    pub collapse_section: &'static str,
    pub expand_section: &'static str,
    pub locked: &'static str,
    pub locked_hint: &'static str,
    /// `{n}` characters of at most `{max}`.
    pub summary_count: &'static str,

    // Sections and entries
    pub move_section_up: &'static str,
    pub move_section_down: &'static str,
    pub remove_section: &'static str,
    pub drag_to_reorder: &'static str,
    pub add_entry: &'static str,
    pub remove_entry: &'static str,
    pub move_entry_up: &'static str,
    pub move_entry_down: &'static str,
    pub entry_title: &'static str,
    pub organisation: &'static str,
    pub start: &'static str,
    pub end: &'static str,
    pub date_placeholder: &'static str,
    pub current_job: &'static str,
    pub description: &'static str,
    pub link: &'static str,

    // Bullets
    pub bullet: &'static str,
    pub add_bullet: &'static str,
    pub move_bullet_up: &'static str,
    pub move_bullet_down: &'static str,
    pub remove_bullet: &'static str,

    // Skills and languages
    pub add_skill_group: &'static str,
    pub group_label: &'static str,
    pub skills_line: &'static str,
    pub remove_group: &'static str,
    pub move_group_up: &'static str,
    pub move_group_down: &'static str,
    pub language: &'static str,
    pub level: &'static str,
    pub level_placeholder: &'static str,
    pub add_language: &'static str,
    pub move_language_up: &'static str,
    pub move_language_down: &'static str,
    pub remove_language: &'static str,

    // Variants
    pub version: &'static str,
    pub base_cv: &'static str,
    pub new_variant: &'static str,
    /// `{n}`: the default name of a new variant.
    pub variant_default_name: &'static str,
    /// `{name}`.
    pub showing_variant: &'static str,
    pub variant_name: &'static str,
    pub delete_variant: &'static str,
    pub delete_variant_confirm: &'static str,
    pub cancel: &'static str,
    pub shown_in_variant: &'static str,
    pub one_line: &'static str,
    pub untitled_entry: &'static str,
    pub empty_bullet: &'static str,
    pub unlabelled_group: &'static str,

    // Errors raised by the UI itself
    pub no_secure_random: &'static str,
    pub app_not_answering: &'static str,
    /// `{error}`.
    pub unexpected_data: &'static str,

    // Errors from the backend, by code (see [`error_message`])
    pub error_not_found: &'static str,
    pub error_conflict: &'static str,
    pub error_cancelled: &'static str,
    pub error_keychain: &'static str,
    pub error_ai_not_configured: &'static str,
    pub error_ai_consent_required: &'static str,
    pub error_ai_cancelled: &'static str,
}

/// English (v1's only locale).
pub static EN: Strings = Strings {
    app_name: "PinaxCV",

    your_cvs: "Your CVs",
    delete: "Delete",
    new_cv: "New CV",
    new_cv_title_placeholder: "Title, for example Main CV",
    new_cv_title: "Title of the new CV",
    no_cvs_yet: "No CVs yet. Give your first one a title and choose New CV.",
    duplicate: "Duplicate",
    copy_title: "{title} (copy)",
    deleted_cv: "Deleted \u{201c}{title}\u{201d}.",
    edited: "Edited {date}",
    date_time: "{day} {month} {year}, {time}",
    months: [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ],
    one_page: "1 page",
    n_pages: "{n} pages",
    one_variant: "1 variant",
    n_variants: "{n} variants",

    back_to_cvs: "← CVs",
    export_pdf: "Export PDF",
    export_html: "Export HTML",
    export_pdf_hint: "Export PDF (Ctrl+E)",
    undo: "Undo",
    undo_hint: "Undo (Ctrl+Z)",
    redo: "Redo",
    redo_hint: "Redo (Ctrl+Shift+Z)",
    loading: "Loading…",
    editing: "Editing…",
    saving: "Saving…",
    saved: "Saved",
    not_saved: "Not saved",
    exporting: "Exporting…",
    export_cancelled: "Export cancelled",
    export_failed: "Export failed",
    exported_to: "Exported to {path}{ats}",
    ats_score: ", ATS score {score}",

    content: "Content",
    preview: "Preview",
    updating_preview: "Updating preview",
    page_preview: "CV page preview",
    cv_title: "CV title (only you see this)",
    full_name: "Full name",
    headline: "Headline",
    email: "Email",
    phone: "Phone",
    location: "Location",
    summary: "Summary",

    style: "Style",
    template: "Template",
    paper: "Paper size",
    paper_a4: "A4",
    paper_letter: "US Letter",
    cv_language: "CV language (headings and dates)",
    english: "English",
    date_style: "Date style",
    date_month_name: "Mar 2021",
    date_numeric: "03/2021",
    date_year_only: "2021",
    font_size: "Font size",
    line_spacing: "Line spacing",
    margins: "Margins",
    unit_pt: " pt",
    unit_mm: " mm",
    accent: "Accent colour for headings and rules",
    accent_none: "None (black)",
    accent_reset: "Use black",

    experience: "Experience",
    work_experience: "Work experience",
    education: "Education",
    projects: "Projects",
    certifications: "Certifications",
    skills: "Skills",
    languages: "Languages",

    add_section: "Add section",
    add_section_choose: "Choose a section to add…",
    custom_section: "Custom section",
    custom_title: "Section title",
    collapse_section: "Collapse section",
    expand_section: "Expand section",
    locked: "Locked",
    locked_hint: "Protects this entry's facts (title, organisation, location and dates) from AI suggestions.",
    summary_count: "{n} / {max} characters",

    move_section_up: "Move section up",
    move_section_down: "Move section down",
    remove_section: "Remove section",
    drag_to_reorder: "Drag to reorder",
    add_entry: "+ Entry",
    remove_entry: "Remove entry",
    move_entry_up: "Move entry up",
    move_entry_down: "Move entry down",
    entry_title: "Title",
    organisation: "Organisation",
    start: "Start",
    end: "End",
    date_placeholder: "2021 or 2021-03",
    current_job: "I work here now",
    description: "Description (optional)",
    link: "Link (optional, https:// links are clickable)",

    bullet: "Bullet",
    add_bullet: "+ Bullet",
    move_bullet_up: "Move bullet up",
    move_bullet_down: "Move bullet down",
    remove_bullet: "Remove bullet",

    add_skill_group: "+ Skill group",
    group_label: "Group label (optional)",
    skills_line: "Skills, separated by commas",
    remove_group: "Remove group",
    move_group_up: "Move skill group up",
    move_group_down: "Move skill group down",
    language: "Language",
    level: "Level",
    level_placeholder: "Level, for example C1",
    add_language: "+ Language",
    move_language_up: "Move language up",
    move_language_down: "Move language down",
    remove_language: "Remove language",

    version: "Version",
    base_cv: "Base CV",
    new_variant: "New variant",
    variant_default_name: "Variant {n}",
    showing_variant: "Showing variant \u{201c}{name}\u{201d}. Text edits below change the base CV; choose here what this variant shows.",
    variant_name: "Variant name",
    delete_variant: "Delete variant",
    delete_variant_confirm: "Delete this variant? The base CV is not changed.",
    cancel: "Cancel",
    shown_in_variant: "Shown in this variant",
    one_line: "one line",
    untitled_entry: "(untitled entry)",
    empty_bullet: "(empty bullet)",
    unlabelled_group: "(unlabelled group)",

    no_secure_random: "Could not add the item: the system's secure random source is unavailable.",
    app_not_answering: "The app did not answer. Restart PinaxCV.",
    unexpected_data: "Unexpected data: {error}",

    error_not_found: "That CV or variant no longer exists. It may have been deleted.",
    error_conflict: "This CV was changed somewhere else, so your latest edits were not saved. Copy anything you need, then reopen the CV.",
    error_cancelled: "Cancelled.",
    error_keychain: "The system keychain could not be used. Check that it is unlocked and try again.",
    error_ai_not_configured: "No AI provider is set up yet. Add one in Settings.",
    error_ai_consent_required: "Allow sending this CV to the AI provider first.",
    error_ai_cancelled: "The AI request was cancelled.",
};

/// The strings for the current locale.
pub fn t() -> &'static Strings {
    &EN
}

/// Replaces each `{name}` in `template` with its value. Unknown placeholders
/// are left as they are, so a missing value shows up instead of vanishing.
pub fn fill(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let value = after.find('}').and_then(|close| {
            let name = &after[..close];
            args.iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| (*v, close))
        });
        match value {
            Some((v, close)) => {
                out.push_str(v);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The text shown for a backend error (spec section 15). Codes whose meaning
/// is complete on its own map to a fixed string here. For the others the
/// backend message is the specific, actionable part (which fields failed
/// validation, what was wrong with an imported file), so it is shown as sent.
pub fn error_message(e: &ApiError) -> String {
    let s = t();
    // No wildcard: a new code must decide here whether it gets its own text.
    let fixed = match e.code {
        ErrorCode::NotFound => Some(s.error_not_found),
        ErrorCode::Conflict => Some(s.error_conflict),
        ErrorCode::Cancelled => Some(s.error_cancelled),
        ErrorCode::Keychain => Some(s.error_keychain),
        ErrorCode::AiNotConfigured => Some(s.error_ai_not_configured),
        ErrorCode::AiConsentRequired => Some(s.error_ai_consent_required),
        ErrorCode::AiCancelled => Some(s.error_ai_cancelled),
        ErrorCode::Validation
        | ErrorCode::Io
        | ErrorCode::Db
        | ErrorCode::Render
        | ErrorCode::Import
        | ErrorCode::UnsupportedFormat
        | ErrorCode::NoTextLayer
        | ErrorCode::AiProvider
        | ErrorCode::AiInvalidOutput
        | ErrorCode::Internal => None,
    };
    match fixed {
        Some(text) => text.to_owned(),
        None => e.message.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_replaces_named_placeholders() {
        assert_eq!(
            fill(t().exported_to, &[("path", "/tmp/cv.pdf"), ("ats", "")]),
            "Exported to /tmp/cv.pdf"
        );
        assert_eq!(fill("{a}{b}{a}", &[("a", "x"), ("b", "y")]), "xyx");
        // Unknown or unclosed placeholders stay visible.
        assert_eq!(fill("{x} and {", &[]), "{x} and {");
        // Values are not filled again.
        assert_eq!(fill("{a}", &[("a", "{a}")]), "{a}");
        assert_eq!(fill("Variant {n}", &[("n", "é")]), "Variant é");
    }

    #[test]
    fn fixed_codes_use_i18n_text_and_others_keep_the_backend_message() {
        let e = ApiError::new(ErrorCode::NotFound, "backend text");
        assert_eq!(error_message(&e), t().error_not_found);
        let e = ApiError::conflict(3);
        assert_eq!(error_message(&e), t().error_conflict);
        let e = ApiError::new(ErrorCode::Validation, "Add your full name.");
        assert_eq!(error_message(&e), "Add your full name.");
    }

    /// Finds string literals in Rust source outside `#[cfg(test)]` modules
    /// and comments. Good enough for this crate's code, which has no raw
    /// strings or quote characters in char literals.
    fn literals(src: &str) -> Vec<String> {
        let code = src.split("#[cfg(test)]").next().unwrap_or(src);
        let mut out = Vec::new();
        for line in code.lines() {
            let line = line.trim_start();
            if line.starts_with("//") {
                continue;
            }
            let mut chars = line.chars().peekable();
            let mut prev = ' ';
            while let Some(c) = chars.next() {
                if c == '/' && chars.peek() == Some(&'/') {
                    break;
                }
                if c == '"' {
                    let byte_string = prev == 'b';
                    let mut lit = String::new();
                    while let Some(c) = chars.next() {
                        match c {
                            '\\' => {
                                chars.next();
                            }
                            '"' => break,
                            c => lit.push(c),
                        }
                    }
                    if !byte_string {
                        out.push(lit);
                    }
                }
                prev = c;
            }
        }
        out
    }

    /// Text a user could read: a letter outside `{…}` placeholders, and
    /// either a capital letter or a space between words.
    fn looks_like_ui_text(lit: &str) -> bool {
        let mut depth = 0;
        let mut plain = String::new();
        for c in lit.chars() {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                c if depth == 0 => plain.push(c),
                _ => {}
            }
        }
        let words = plain.trim();
        words.chars().any(char::is_alphabetic)
            && (words.chars().any(char::is_uppercase) || words.contains(' '))
    }

    /// Every other source file in `src/`.
    const FILES: &[(&str, &str)] = &[
        ("dates.rs", include_str!("dates.rs")),
        ("drag.rs", include_str!("drag.rs")),
        ("editor.rs", include_str!("editor.rs")),
        ("entries.rs", include_str!("entries.rs")),
        ("history.rs", include_str!("history.rs")),
        ("ids.rs", include_str!("ids.rs")),
        ("ipc.rs", include_str!("ipc.rs")),
        ("list.rs", include_str!("list.rs")),
        ("lists.rs", include_str!("lists.rs")),
        ("main.rs", include_str!("main.rs")),
        ("order.rs", include_str!("order.rs")),
        ("style.rs", include_str!("style.rs")),
        ("variants.rs", include_str!("variants.rs")),
    ];

    #[test]
    fn no_ui_strings_outside_this_module() {
        // The `extern "C"` ABI and identifiers passed to serde or wasm-bindgen.
        const ALLOWED: &[&str] = &[
            "C",
            "camelCase",
            "__TAURI__",
            // `aria-keyshortcuts` values are key names, not prose.
            "Control+Z",
            "Control+Shift+Z",
            "Control+E",
        ];
        let mut found = Vec::new();
        for (name, src) in FILES {
            for lit in literals(src) {
                if looks_like_ui_text(&lit) && !ALLOWED.contains(&lit.as_str()) {
                    found.push(format!("{name}: {lit:?}"));
                }
            }
        }
        assert!(found.is_empty(), "move these into i18n.rs: {found:#?}");
    }

    #[test]
    fn every_module_is_scanned() {
        for line in include_str!("main.rs").lines() {
            let Some(name) = line.strip_prefix("mod ").and_then(|l| l.strip_suffix(';')) else {
                continue;
            };
            let file = format!("{name}.rs");
            assert!(
                name == "i18n" || FILES.iter().any(|(f, _)| *f == file),
                "add {file} to FILES"
            );
        }
    }

    #[test]
    fn literal_scanner_finds_ui_text() {
        let src = "let a = \"Save now\"; // \"Comment\"\nlet b = b\"ABC\"; let c = \"list_cvs\";\n#[cfg(test)]\n\"Test\"";
        assert_eq!(literals(src), ["Save now", "list_cvs"]);
        assert!(looks_like_ui_text("Save now"));
        assert!(looks_like_ui_text("one line"));
        assert!(!looks_like_ui_text("list_cvs"));
        assert!(!looks_like_ui_text("{year}-{m:02}"));
        assert!(!looks_like_ui_text("↑"));
        assert!(!looks_like_ui_text(" "));
    }
}
