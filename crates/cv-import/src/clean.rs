//! Text cleaning shared by the JSON Resume and LinkedIn importers, and the
//! record of which fields were cut short (issue #117).

use std::cell::RefCell;
use std::collections::BTreeSet;

use unicode_normalization::UnicodeNormalization;

use crate::partial::ImportWarning;

/// Characters kept from one text value.
pub const MAX_TEXT: usize = 10_000;

/// Field names listed in the "shortened" warning.
const LISTED_FIELDS: usize = 10;

thread_local! {
    /// Field names cut while [`track_cuts`] runs on this thread.
    static CUT: RefCell<Option<BTreeSet<String>>> = const { RefCell::new(None) };
}

/// NFC, with whitespace controls as spaces (line breaks kept with
/// `keep_lines`), other control characters dropped, runs of spaces collapsed,
/// at most [`MAX_TEXT`] characters. `None` when nothing is left. The flag is
/// true when characters past [`MAX_TEXT`] were dropped.
pub(crate) fn clean(s: &str, keep_lines: bool) -> (Option<String>, bool) {
    let mut chars = s.nfc().filter_map(|c| match c {
        '\n' if keep_lines => Some('\n'),
        c if c.is_whitespace() => Some(' '),
        c if c.is_control() => None,
        c => Some(c),
    });
    let nfc: String = chars.by_ref().take(MAX_TEXT).collect();
    let cut = chars.next().is_some();
    let out = if keep_lines {
        nfc.lines()
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_owned()
    } else {
        nfc.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    ((!out.is_empty()).then_some(out), cut)
}

/// [`clean`] for the value of `field`, noting the field when it was cut.
pub(crate) fn clean_field(s: &str, keep_lines: bool, field: &str) -> Option<String> {
    let (out, cut) = clean(s, keep_lines);
    if cut {
        CUT.with(|c| {
            if let Some(set) = c.borrow_mut().as_mut() {
                set.insert(field.to_owned());
            }
        });
    }
    out
}

/// Runs `f`, then returns its result with one warning naming the fields
/// that [`clean_field`] cut short during it, if any. Only field names are
/// listed, never the text itself.
pub(crate) fn track_cuts<T>(f: impl FnOnce() -> T) -> (T, Option<ImportWarning>) {
    let outer = CUT.with(|c| c.borrow_mut().replace(BTreeSet::new()));
    let value = f();
    let cut = CUT
        .with(|c| std::mem::replace(&mut *c.borrow_mut(), outer))
        .unwrap_or_default();
    (value, cut_warning(&cut))
}

fn cut_warning(fields: &BTreeSet<String>) -> Option<ImportWarning> {
    if fields.is_empty() {
        return None;
    }
    let mut names: Vec<&str> = fields
        .iter()
        .take(LISTED_FIELDS)
        .map(String::as_str)
        .collect();
    if fields.len() > LISTED_FIELDS {
        names.push("…");
    }
    // The message spells out MAX_TEXT; a test keeps the two in step.
    Some(ImportWarning::new(format!(
        "Some text was longer than 10,000 characters and was shortened: {}.",
        names.join(", ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_reports_a_cut_only_past_the_cap() {
        let exact = "x".repeat(MAX_TEXT);
        assert!(!clean(&exact, false).1);
        let over = "x".repeat(MAX_TEXT + 1);
        let (out, cut) = clean(&over, false);
        assert!(cut);
        assert_eq!(out.map(|s| s.chars().count()), Some(MAX_TEXT));
        // Dropped control characters don't count towards the cap.
        let with_controls = format!("{}{}", "\u{7}".repeat(10), exact);
        assert!(!clean(&with_controls, false).1);
    }

    #[test]
    fn track_cuts_lists_field_names_once_and_never_the_text() {
        let long = "secret ".repeat(MAX_TEXT);
        let ((), warning) = track_cuts(|| {
            clean_field(&long, true, "summary");
            clean_field(&long, true, "summary");
            clean_field("short", false, "name");
            clean_field(&long, false, "description");
        });
        let message = warning.expect("a warning").message;
        assert!(
            message.ends_with("shortened: description, summary."),
            "{message}"
        );
        assert!(!message.contains("secret"));
        assert_eq!(MAX_TEXT, 10_000, "update the warning text");
    }

    #[test]
    fn no_warning_without_cuts_or_outside_tracking() {
        let ((), warning) = track_cuts(|| {
            clean_field("short", false, "name");
        });
        assert!(warning.is_none());
        // Outside track_cuts nothing is recorded.
        clean_field(&"x".repeat(MAX_TEXT + 1), false, "name");
        let ((), warning) = track_cuts(|| ());
        assert!(warning.is_none());
    }

    #[test]
    fn many_fields_are_listed_with_an_ellipsis() {
        let long = "x".repeat(MAX_TEXT + 1);
        let ((), warning) = track_cuts(|| {
            for i in 0..LISTED_FIELDS + 2 {
                clean_field(&long, false, &format!("f{i:02}"));
            }
        });
        let message = warning.expect("a warning").message;
        assert!(message.ends_with(", …."), "{message}");
    }
}
