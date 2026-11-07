//! Deterministic lint over a resolved CV (spec section 7.1). No network, no AI.
//!
//! `missing_glyphs` is the one rule in the spec's table that is not here: only
//! the renderer knows which characters the embedded fonts cover, so `cv-render`
//! reports it as a compile [`crate::Diagnostic`] and the API layer adds it to
//! the lint results (ADR-029).

use std::collections::BTreeSet;

use crate::contrast::{Rgb, TEXT_MIN_RATIO, contrast_ratio, darker_shade};
use crate::wording;
use crate::{
    CalendarDate, Id, LintIssue, Locale, PartialDate, Period, PersonalField, ResolvedCv,
    ResolvedEntry, ResolvedSection, Severity,
};

/// Stable codes for [`LintIssue::code`], in the order of the spec's table.
pub mod codes {
    pub const MISSING_NAME: &str = "missing_name";
    pub const MISSING_CONTACT: &str = "missing_contact";
    pub const INVALID_EMAIL: &str = "invalid_email";
    pub const DATE_ORDER: &str = "date_order";
    pub const DATE_MISSING: &str = "date_missing";
    pub const DATE_STYLE_MIXED: &str = "date_style_mixed";
    pub const GAP: &str = "gap";
    pub const BULLET_TOO_LONG: &str = "bullet_too_long";
    pub const BULLET_TOO_SHORT: &str = "bullet_too_short";
    pub const TOO_MANY_BULLETS: &str = "too_many_bullets";
    pub const NO_BULLETS: &str = "no_bullets";
    pub const SUMMARY_TOO_LONG: &str = "summary_too_long";
    pub const DUPLICATE_BULLET: &str = "duplicate_bullet";
    pub const WEAK_OPENING: &str = "weak_opening";
    pub const NO_NUMBERS: &str = "no_numbers";
    pub const SPECIAL_CHARS: &str = "special_chars";
    pub const LONG_URL: &str = "long_url";
    pub const UNSAFE_LINK: &str = "unsafe_link";
    pub const PAGE_COUNT: &str = "page_count";
    /// Added by the app, which knows whether a photo is stored (ADR-083).
    pub const PHOTO: &str = "photo";
    /// The layout has two columns (ADR-084).
    pub const TWO_COLUMNS: &str = "two_columns";
    /// A personal detail such as the date of birth is printed (ADR-088).
    pub const PERSONAL_DETAIL_SHOWN: &str = "personal_detail_shown";
    /// The accent colour is too light to read on white (WCAG 2.2 SC 1.4.3,
    /// issue #139).
    pub const LOW_CONTRAST: &str = "low_contrast";
    /// The phone number is not in the international form of ITU-T E.123
    /// (issue #140).
    pub const PHONE_FORMAT: &str = "phone_format";
    /// A language level is not CEFR, native, or another recognised scale
    /// (issue #140).
    pub const LANGUAGE_LEVEL: &str = "language_level";
    /// Plain-language lints after ISO 24495-1 (issue #141).
    pub const LONG_SENTENCE: &str = "long_sentence";
    pub const PASSIVE_VOICE: &str = "passive_voice";
    pub const UNDEFINED_ACRONYM: &str = "undefined_acronym";
    /// Common recruiter advice, not a standard (issue #141).
    pub const FIRST_PERSON: &str = "first_person";
    pub const TENSE_MIXED: &str = "tense_mixed";
    pub const CLICHE: &str = "cliche";
    /// Added by the app, which knows today's date (issue #141).
    pub const OLD_EXPERIENCE: &str = "old_experience";
}

/// Sentences and bullets longer than this many words get `long_sentence`.
/// ISO 24495-1 sets no number; this one is PinaxCV's (ADR-093).
pub const SENTENCE_MAX_WORDS: usize = 30;
/// Experience that ended more than this many years ago gets
/// `old_experience`.
pub const OLD_EXPERIENCE_YEARS: i32 = 15;

const PLAIN_LANGUAGE: &str = "Standard (ISO 24495-1, plain language)";
const ADVICE: &str = "Advice (common recruiter guidance, not a standard)";

/// Bullets longer than this many characters get `bullet_too_long`.
pub const BULLET_MAX_CHARS: usize = 220;
/// Bullets shorter than this many characters get `bullet_too_short`.
pub const BULLET_MIN_CHARS: usize = 25;
/// Entries with more visible bullets than this get `too_many_bullets`.
pub const MAX_BULLETS: usize = 8;
/// Summaries longer than this many characters get `summary_too_long`.
pub const SUMMARY_MAX_CHARS: usize = 600;
/// Gaps longer than this many months between experience entries get `gap`.
pub const GAP_MONTHS: i32 = 6;
/// URLs longer than this many characters get `long_url`.
pub const URL_MAX_CHARS: usize = 60;
/// More pages than this get `page_count`.
pub const MAX_PAGES: u32 = 2;

const WEAK_OPENINGS: [&str; 4] = [
    "responsible for",
    "worked on",
    "helped with",
    "duties included",
];

/// Lints `cv`. `page_count` is the last compiled page count, if known.
///
/// Findings come out grouped by rule in the order of the spec's table, and
/// within a rule in document order.
pub fn lint(cv: &ResolvedCv, page_count: Option<u32>) -> Vec<LintIssue> {
    let mut out = Lints::default();
    let entries: Vec<(&ResolvedSection, &ResolvedEntry)> = cv
        .sections
        .iter()
        .flat_map(|s| entries_of(s).iter().map(move |e| (s, e)))
        .collect();

    basics(cv, &mut out);
    for (section, entry) in &entries {
        dates(section, entry, &mut out);
    }
    mixed_date_styles(&entries, &mut out);
    gaps(&entries, &mut out);
    for (section, entry) in &entries {
        bullets(section, entry, &mut out);
    }
    if let Some(summary) = &cv.summary
        && summary.chars().count() > SUMMARY_MAX_CHARS
    {
        out.push(
            codes::SUMMARY_TOO_LONG,
            Severity::Warning,
            None,
            format!("The summary is longer than {SUMMARY_MAX_CHARS} characters."),
            Some("Keep it to three or four sentences."),
        );
    }
    duplicate_bullets(&entries, &mut out);
    weak_openings(&entries, &mut out);
    for (_, entry) in &entries {
        no_numbers(entry, &mut out);
    }
    special_chars(cv, &mut out);
    links(cv, &mut out);
    if cv.layout.columns == crate::Columns::Two {
        out.push(
            codes::TWO_COLUMNS,
            Severity::Warning,
            None,
            "This CV has two columns.".into(),
            Some(
                "Some ATS read straight across the page. For online applications, \
                 send the one-column ATS copy that Export PDF saves beside the PDF.",
            ),
        );
    }
    if let Some(pages) = page_count
        && pages > MAX_PAGES
    {
        out.push(
            codes::PAGE_COUNT,
            Severity::Info,
            None,
            format!("The CV is {pages} pages long."),
            Some("Most roles expect one or two pages. Try \"Fit to 2 pages\"."),
        );
    }
    personal_details(cv, &mut out);
    accent_contrast(cv, &mut out);
    phone_format(cv, &mut out);
    language_levels(cv, &mut out);
    wording(cv, &entries, &mut out);
    out.sort_by_rule()
}

/// One note per printed personal detail. Whether to show them depends on the
/// target country, so this is never more than information: common advice,
/// not a standard (ADR-088).
fn personal_details(cv: &ResolvedCv, out: &mut Lints) {
    let style = cv.options.date_style;
    for (field, _) in cv.basics.personal.lines(style, cv.locale) {
        let label = crate::locale::personal_label(field, cv.locale);
        let hint = match field {
            PersonalField::Nationality
            | PersonalField::BirthDate
            | PersonalField::BirthPlace
            | PersonalField::Gender
            | PersonalField::MaritalStatus => {
                "Many European employers expect it, but UK and US equality guidance \
                 (Equality Act 2010, EEOC) advises leaving it out. It depends on the \
                 country you apply in; show it only where it is expected or asked for."
            }
            PersonalField::Address | PersonalField::DrivingLicence | PersonalField::WorkPermit => {
                "Show it only when the job or the application asks for it."
            }
        };
        out.push(
            codes::PERSONAL_DETAIL_SHOWN,
            Severity::Info,
            None,
            format!("{label} is shown on this CV."),
            Some(hint),
        );
    }
}

/// Section headings print in the accent colour on a white page, so it must
/// pass WCAG 2.2 SC 1.4.3 for text (4.5:1). A standard, not advice.
fn accent_contrast(cv: &ResolvedCv, out: &mut Lints) {
    let Some(accent) = cv.options.accent.as_deref().and_then(Rgb::parse) else {
        return;
    };
    let ratio = contrast_ratio(accent, Rgb::WHITE);
    if ratio >= TEXT_MIN_RATIO {
        return;
    }
    let shade = darker_shade(accent, Rgb::WHITE, TEXT_MIN_RATIO).hex();
    let hint = format!(
        "WCAG 2.2 (1.4.3) asks for at least 4.5:1. Try {shade}, the nearest darker shade that passes, in the Style tab."
    );
    out.push(
        codes::LOW_CONTRAST,
        Severity::Warning,
        None,
        format!(
            "The accent colour {} has a contrast of {:.1}:1 on white; headings in it are hard to read and may vanish in greyscale prints.",
            accent.hex(),
            (ratio * 10.0).floor() / 10.0
        ),
        Some(&hint),
    );
}

/// ITU-T E.123 writes a number for international use as `+<country code>
/// <number>`, with spaces (and, commonly, `-`, `(`, `)` or `.`) to group the
/// digits. A standard, not advice.
fn phone_format(cv: &ResolvedCv, out: &mut Lints) {
    let Some(phone) = cv.basics.phone.as_deref().map(str::trim) else {
        return;
    };
    if phone.is_empty() {
        return;
    }
    let message = if !phone.starts_with('+') {
        format!("The phone number \"{phone}\" has no international country code.")
    } else if phone[1..]
        .chars()
        .any(|c| !(c.is_ascii_digit() || matches!(c, ' ' | '-' | '(' | ')' | '.')))
    {
        format!("The phone number \"{phone}\" contains characters other than digits and spacing.")
    } else if phone.chars().filter(char::is_ascii_digit).count() < 7 {
        format!("The phone number \"{phone}\" looks too short to dial from abroad.")
    } else {
        return;
    };
    out.push(
        codes::PHONE_FORMAT,
        Severity::Info,
        None,
        message,
        Some(
            "Standard (ITU-T E.123): write it as + country code, then the number, \
             for example +44 20 7946 0000, so recruiters abroad can dial it.",
        ),
    );
}

/// Levels a reader can place: CEFR (A1 to C2, the Council of Europe scale
/// Europass uses), native speaker, the ILR scale (0 to 5, and LinkedIn's
/// ILR-based wording) and ACTFL. Anything else gets an Info note.
fn language_levels(cv: &ResolvedCv, out: &mut Lints) {
    for section in &cv.sections {
        let ResolvedSection::Languages(s) = section else {
            continue;
        };
        for item in &s.items {
            let Some(level) = item.level.as_deref().map(str::trim) else {
                continue;
            };
            if level.is_empty() || is_recognised_level(level) {
                continue;
            }
            out.push(
                codes::LANGUAGE_LEVEL,
                Severity::Info,
                Some(&item.id),
                format!(
                    "\"{level}\" for {} is not a level every reader can place.",
                    item.name
                ),
                Some(
                    "Standard (CEFR, Council of Europe): use A1, A2, B1, B2, C1 or C2, \
                     or \"Native\". ILR and ACTFL levels are understood too.",
                ),
            );
        }
    }
}

/// True for a CEFR level (`B2`, `C1 (advanced)`, `B2+`), a native-speaker
/// phrase, an ILR level or LinkedIn's ILR wording, or an ACTFL level.
pub fn is_recognised_level(level: &str) -> bool {
    let lower = level.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !(c.is_alphanumeric() || c == '+'))
        .filter(|w| !w.is_empty())
        .collect();
    let cefr = |w: &str| {
        let w = w.strip_suffix('+').unwrap_or(w);
        let b = w.as_bytes();
        b.len() == 2 && matches!(b[0], b'a' | b'b' | b'c') && matches!(b[1], b'1' | b'2')
    };
    if words.iter().any(|w| cefr(w)) {
        return true;
    }
    // Phrases match whole words, so "alternative" is not "native". A
    // negation or hedge anywhere in the level ("non-native", "no longer
    // native", "not my first language", "near-native", "native-like") means
    // the phrase does not claim that level.
    const NEGATIONS: [&str; 8] = [
        "not", "no", "non", "never", "near", "nearly", "almost", "like",
    ];
    let negated = words.iter().any(|w| NEGATIONS.contains(w));
    let has_phrase = |phrase: &str| {
        let p: Vec<&str> = phrase.split(' ').collect();
        !negated && words.windows(p.len()).any(|w| w == p.as_slice())
    };
    const NATIVE: [&str; 5] = [
        "native",
        "mother tongue",
        "first language",
        "bilingual",
        "muttersprache",
    ];
    if NATIVE.iter().any(|p| has_phrase(p)) {
        return true;
    }
    // LinkedIn's levels follow the ILR descriptors.
    const ILR_WORDING: [&str; 4] = [
        "elementary proficiency",
        "limited working proficiency",
        "professional working proficiency",
        "full professional proficiency",
    ];
    if ILR_WORDING.iter().any(|p| has_phrase(p)) {
        return true;
    }
    let ilr_number = |w: &str| {
        let w = w.strip_suffix('+').unwrap_or(w);
        matches!(w, "0" | "1" | "2" | "3" | "4" | "5")
    };
    if words.first() == Some(&"ilr") && words.get(1).is_some_and(|w| ilr_number(w)) {
        return true;
    }
    const ACTFL: [&str; 5] = [
        "novice",
        "intermediate",
        "advanced",
        "superior",
        "distinguished",
    ];
    words.iter().any(|w| ACTFL.contains(w))
}

/// The plain-language and wording lints (issue #141). English only, like
/// `weak_opening`; never more than Info.
fn wording(cv: &ResolvedCv, entries: &[(&ResolvedSection, &ResolvedEntry)], out: &mut Lints) {
    use crate::wording::{Tense, cliche, first_person, is_passive, opening_tense, sentences};

    // The word lists are English; another language gets no wording notes.
    if cv.locale != Locale::En {
        return;
    }

    // long_sentence
    if let Some(summary) = &cv.summary
        && let Some(n) = sentences(summary)
            .iter()
            .map(|s| wording::word_count(s))
            .find(|n| *n > SENTENCE_MAX_WORDS)
    {
        out.push(
            codes::LONG_SENTENCE,
            Severity::Info,
            None,
            format!("A sentence in the summary has {n} words."),
            Some(&format!(
                "{PLAIN_LANGUAGE}: shorter sentences are easier to scan. \
                 Split sentences over {SENTENCE_MAX_WORDS} words (PinaxCV's limit)."
            )),
        );
    }
    for b in entries.iter().flat_map(|(_, e)| &e.bullets) {
        if let Some(n) = sentences(&b.text)
            .iter()
            .map(|s| wording::word_count(s))
            .find(|n| *n > SENTENCE_MAX_WORDS)
        {
            out.push(
                codes::LONG_SENTENCE,
                Severity::Info,
                Some(&b.id),
                format!("This bullet has a sentence of {n} words."),
                Some(&format!(
                    "{PLAIN_LANGUAGE}: keep each point to one short sentence, \
                     at most {SENTENCE_MAX_WORDS} words (PinaxCV's limit)."
                )),
            );
        }
    }

    // passive_voice
    for b in entries.iter().flat_map(|(_, e)| &e.bullets) {
        if is_passive(&b.text) {
            out.push(
                codes::PASSIVE_VOICE,
                Severity::Info,
                Some(&b.id),
                "This bullet uses the passive voice.".into(),
                Some(&format!(
                    "{PLAIN_LANGUAGE}: say who did it. \"The API was rebuilt\" becomes \"Rebuilt the API\"."
                )),
            );
        }
    }

    undefined_acronyms(cv, entries, out);

    // first_person
    for b in entries.iter().flat_map(|(_, e)| &e.bullets) {
        if let Some(word) = first_person(&b.text) {
            out.push(
                codes::FIRST_PERSON,
                Severity::Info,
                Some(&b.id),
                format!("This bullet says \"{word}\"."),
                Some(&format!(
                    "{ADVICE}: bullets usually leave out \"I\" and \"my\" and start with the verb."
                )),
            );
        }
    }

    // tense_mixed: a finished role in the present, or a current role that
    // mixes both. A current role told in the past is fine: those are
    // finished results, and most CVs (the sample's too) write them so.
    for (section, e) in entries {
        if !matches!(section, ResolvedSection::Experience(_)) {
            continue;
        }
        let tenses: Vec<Tense> = e
            .bullets
            .iter()
            .filter_map(|b| opening_tense(&b.text))
            .collect();
        let has = |t| tenses.contains(&t);
        let message = if !e.period.current && e.period.end.is_some() && has(Tense::Present) {
            format!(
                "\"{}\" has ended, but some bullets are in the present tense.",
                e.title
            )
        } else if e.period.current && has(Tense::Present) && has(Tense::Past) {
            format!("The bullets of \"{}\" mix present and past tense.", e.title)
        } else {
            continue;
        };
        out.push(
            codes::TENSE_MIXED,
            Severity::Info,
            Some(&e.id),
            message,
            Some(&format!(
                "{ADVICE}: past roles in the past tense; a current role in the present, \
                 or in the past for finished results."
            )),
        );
    }

    // cliche
    let mut check = |target: Option<&Id>, text: &str| {
        if let Some(phrase) = cliche(text) {
            out.push(
                codes::CLICHE,
                Severity::Info,
                target,
                format!("\"{phrase}\" is a phrase recruiters see in most CVs."),
                Some(&format!(
                    "{ADVICE}: show it with a result instead of claiming it."
                )),
            );
        }
    };
    if let Some(h) = &cv.basics.headline {
        check(None, h);
    }
    if let Some(summary) = &cv.summary {
        check(None, summary);
    }
    for (_, e) in entries {
        if let Some(d) = &e.description {
            check(Some(&e.id), d);
        }
        for b in &e.bullets {
            check(Some(&b.id), &b.text);
        }
    }
}

/// An all-caps acronym used once, not defined in brackets, and not on the
/// well-known list. One note per acronym.
fn undefined_acronyms(
    cv: &ResolvedCv,
    entries: &[(&ResolvedSection, &ResolvedEntry)],
    out: &mut Lints,
) {
    let mut texts: Vec<(Option<&Id>, &str)> = Vec::new();
    if let Some(summary) = &cv.summary {
        texts.push((None, summary));
    }
    for (_, e) in entries {
        if let Some(d) = &e.description {
            texts.push((Some(&e.id), d));
        }
        for b in &e.bullets {
            texts.push((Some(&b.id), &b.text));
        }
    }
    // Text that only counts uses: a skill or a title that names it.
    let mut elsewhere: Vec<&str> = Vec::new();
    elsewhere.extend(cv.basics.headline.as_deref());
    for (_, e) in entries {
        elsewhere.push(&e.title);
        elsewhere.push(&e.org);
    }
    for section in &cv.sections {
        if let ResolvedSection::Skills(s) = section {
            elsewhere.extend(
                s.groups
                    .iter()
                    .flat_map(|g| g.skills.iter().map(|k| k.name.as_str())),
            );
        }
    }
    // A word the CV also writes in lower case is an English word in
    // capitals for emphasis, not an acronym.
    let lower: BTreeSet<String> = texts
        .iter()
        .map(|(_, t)| *t)
        .chain(elsewhere.iter().copied())
        .flat_map(wording::words)
        .filter(|w| w.chars().any(char::is_lowercase))
        .map(str::to_lowercase)
        .collect();
    let mut uses: std::collections::BTreeMap<&str, (usize, Option<&Id>, bool)> =
        std::collections::BTreeMap::new();
    for (target, text) in &texts {
        for w in wording::words(text).filter(|w| wording::is_unknown_acronym(w)) {
            let bracketed = text.contains(&format!("({w})"));
            let e = uses.entry(w).or_insert((0, *target, false));
            e.0 += 1;
            e.2 |= bracketed || lower.contains(&w.to_lowercase());
        }
    }
    for w in elsewhere.iter().flat_map(|t| wording::words(t)) {
        if let Some(e) = uses.get_mut(w) {
            e.0 += 1;
        }
    }
    // Document order: by the first text each acronym appears in.
    let mut once: Vec<(usize, &str, Option<&Id>)> = uses
        .into_iter()
        .filter(|(_, (n, _, bracketed))| *n == 1 && !bracketed)
        .map(|(w, (_, target, _))| {
            let pos = texts
                .iter()
                .position(|(t, _)| *t == target)
                .unwrap_or(usize::MAX);
            (pos, w, target)
        })
        .collect();
    once.sort();
    for (_, w, target) in once {
        out.push(
            codes::UNDEFINED_ACRONYM,
            Severity::Info,
            target,
            format!("\"{w}\" is used once and not spelled out."),
            Some(&format!(
                "{PLAIN_LANGUAGE}: write it out the first time, for example \
                 \"Service Level Objective (SLO)\", unless every reader knows it."
            )),
        );
    }
}

/// Experience entries that ended more than [`OLD_EXPERIENCE_YEARS`] before
/// `today`, counted in months. An end date without a month counts as
/// December and today without a month as January, so a note never comes
/// early. The app adds these, since `cv-core` has no clock.
pub fn old_experience(cv: &ResolvedCv, today: CalendarDate) -> Vec<LintIssue> {
    let mut out = Lints::default();
    let now = i32::from(today.year) * 12 + i32::from(today.month.unwrap_or(1));
    for section in &cv.sections {
        let ResolvedSection::Experience(s) = section else {
            continue;
        };
        for e in &s.entries {
            let Some(end) = e.period.end.filter(|_| !e.period.current) else {
                continue;
            };
            let months = now - (i32::from(end.year) * 12 + i32::from(end.month.unwrap_or(12)));
            if months > OLD_EXPERIENCE_YEARS * 12 {
                let years = months / 12;
                out.push(
                    codes::OLD_EXPERIENCE,
                    Severity::Info,
                    Some(&e.id),
                    format!("\"{}\" ended {years} years ago.", e.title),
                    Some(&format!(
                        "{ADVICE}: many guides keep the last {OLD_EXPERIENCE_YEARS} years in detail. \
                         Collapse or hide older roles unless they matter for this job."
                    )),
                );
            }
        }
    }
    out.0
}

/// Returns the entries of an entry-shaped section, or nothing.
fn entries_of(section: &ResolvedSection) -> &[ResolvedEntry] {
    match section {
        ResolvedSection::Experience(s)
        | ResolvedSection::Education(s)
        | ResolvedSection::Projects(s)
        | ResolvedSection::Certifications(s)
        | ResolvedSection::Custom(s) => &s.entries,
        ResolvedSection::Skills(_) | ResolvedSection::Languages(_) => &[],
    }
}

fn basics(cv: &ResolvedCv, out: &mut Lints) {
    let b = &cv.basics;
    if b.full_name.trim().is_empty() {
        out.push(
            codes::MISSING_NAME,
            Severity::Error,
            None,
            "Your full name is missing.".into(),
            None,
        );
    }
    let email = b.email.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let phone = b.phone.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if email.is_none() && phone.is_none() {
        out.push(
            codes::MISSING_CONTACT,
            Severity::Error,
            None,
            "Add an email address or a phone number.".into(),
            None,
        );
    }
    if let Some(email) = email
        && !is_plausible_email(email)
    {
        out.push(
            codes::INVALID_EMAIL,
            Severity::Warning,
            None,
            format!("\"{email}\" does not look like an email address."),
            Some("Use the form name@example.com."),
        );
    }
}

/// A deliberately loose check: one `@`, a non-empty local part, a dotted
/// domain with non-empty labels, and no whitespace.
fn is_plausible_email(s: &str) -> bool {
    let Some((local, domain)) = s.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.contains('@')
        && !s.chars().any(char::is_whitespace)
        && domain.contains('.')
        && domain.split('.').all(|label| !label.is_empty())
}

fn dates(section: &ResolvedSection, entry: &ResolvedEntry, out: &mut Lints) {
    let p = &entry.period;
    if let (Some(start), Some(end), false) = (p.start, p.end, p.current)
        && is_before(end, start)
    {
        out.push(
            codes::DATE_ORDER,
            Severity::Error,
            Some(&entry.id),
            format!("\"{}\" ends before it starts.", entry.title),
            Some("Check the start and end dates."),
        );
    }
    let dated_kind = matches!(
        section,
        ResolvedSection::Experience(_) | ResolvedSection::Education(_)
    );
    if dated_kind && p.start.is_none() {
        out.push(
            codes::DATE_MISSING,
            Severity::Warning,
            Some(&entry.id),
            format!("\"{}\" has no start date.", entry.title),
            None,
        );
    }
}

/// True when `a` is certainly before `b`. A year-only date overlaps every
/// month of its year, so it is never before a date in the same year.
fn is_before(a: PartialDate, b: PartialDate) -> bool {
    match (a.month, b.month) {
        (Some(am), Some(bm)) if a.year == b.year => am < bm,
        _ => a.year < b.year,
    }
}

/// The dates an entry prints: start, and end unless the entry is current.
fn printed_dates(p: &Period) -> impl Iterator<Item = PartialDate> {
    p.start.into_iter().chain(p.end.filter(|_| !p.current))
}

fn mixed_date_styles(entries: &[(&ResolvedSection, &ResolvedEntry)], out: &mut Lints) {
    let any_month = entries
        .iter()
        .flat_map(|(_, e)| printed_dates(&e.period))
        .any(|d| d.month.is_some());
    if !any_month {
        return;
    }
    for (_, e) in entries {
        if printed_dates(&e.period).any(|d| d.month.is_none()) {
            out.push(
                codes::DATE_STYLE_MIXED,
                Severity::Info,
                Some(&e.id),
                format!(
                    "\"{}\" shows only a year while other entries show months.",
                    e.title
                ),
                Some("Add the month, or use year-only dates everywhere."),
            );
        }
    }
}

/// Month index for gap arithmetic. A missing month counts as the one that
/// makes the gap shortest: December for an end, January for a start.
fn month_index(d: PartialDate, default_month: u8) -> i32 {
    i32::from(d.year) * 12 + i32::from(d.month.unwrap_or(default_month))
}

fn gaps(entries: &[(&ResolvedSection, &ResolvedEntry)], out: &mut Lints) {
    let mut jobs: Vec<(&ResolvedEntry, i32)> = entries
        .iter()
        .filter(|(s, _)| matches!(s, ResolvedSection::Experience(_)))
        .filter_map(|(_, e)| e.period.start.map(|d| (*e, month_index(d, 1))))
        .collect();
    jobs.sort_by_key(|(_, start)| *start);
    // Latest month covered so far; current jobs cover everything after them.
    let mut covered: Option<i32> = None;
    for (entry, start) in jobs {
        if let Some(until) = covered {
            let gap = start - until - 1;
            if gap > GAP_MONTHS {
                out.push(
                    codes::GAP,
                    Severity::Info,
                    Some(&entry.id),
                    format!(
                        "There are {gap} months before \"{}\" with no experience entry.",
                        entry.title
                    ),
                    Some("This is fine. If you want, mention what you did in that time."),
                );
            }
        }
        let end = if entry.period.current {
            i32::MAX
        } else {
            match entry.period.end {
                Some(d) => month_index(d, 12),
                // No end and not current: assume it ended when it started.
                None => start,
            }
        };
        covered = Some(covered.map_or(end, |c| c.max(end)));
    }
}

fn bullets(section: &ResolvedSection, entry: &ResolvedEntry, out: &mut Lints) {
    for b in &entry.bullets {
        let len = b.text.trim().chars().count();
        if len > BULLET_MAX_CHARS {
            out.push(
                codes::BULLET_TOO_LONG,
                Severity::Warning,
                Some(&b.id),
                format!("This bullet is {len} characters long."),
                Some("Split it, or cut it below 220 characters."),
            );
        } else if len < BULLET_MIN_CHARS {
            out.push(
                codes::BULLET_TOO_SHORT,
                Severity::Info,
                Some(&b.id),
                "This bullet is very short.".into(),
                Some("Say what you did and what came of it."),
            );
        }
    }
    if entry.bullets.len() > MAX_BULLETS {
        out.push(
            codes::TOO_MANY_BULLETS,
            Severity::Warning,
            Some(&entry.id),
            format!("\"{}\" has {} bullets.", entry.title, entry.bullets.len()),
            Some("Keep the eight that matter most."),
        );
    }
    let is_experience = matches!(section, ResolvedSection::Experience(_));
    if is_experience && entry.bullets.is_empty() && !entry.collapsed {
        out.push(
            codes::NO_BULLETS,
            Severity::Info,
            Some(&entry.id),
            format!("\"{}\" has no bullets.", entry.title),
            Some("Add one or two results from this role."),
        );
    }
}

fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn duplicate_bullets(entries: &[(&ResolvedSection, &ResolvedEntry)], out: &mut Lints) {
    let mut seen = BTreeSet::new();
    for b in entries.iter().flat_map(|(_, e)| &e.bullets) {
        let key = normalize(&b.text);
        if !key.is_empty() && !seen.insert(key) {
            out.push(
                codes::DUPLICATE_BULLET,
                Severity::Warning,
                Some(&b.id),
                "This bullet appears more than once.".into(),
                Some("Remove the copy or reword it."),
            );
        }
    }
}

fn weak_openings(entries: &[(&ResolvedSection, &ResolvedEntry)], out: &mut Lints) {
    for b in entries.iter().flat_map(|(_, e)| &e.bullets) {
        let text = normalize(&b.text);
        if let Some(opening) = WEAK_OPENINGS.iter().find(|w| text.starts_with(*w)) {
            out.push(
                codes::WEAK_OPENING,
                Severity::Info,
                Some(&b.id),
                format!("This bullet opens with \"{opening}\"."),
                Some("Start with what you did: \"Built\", \"Cut\", \"Led\"."),
            );
        }
    }
}

fn no_numbers(entry: &ResolvedEntry, out: &mut Lints) {
    let none_with_digit = !entry
        .bullets
        .iter()
        .any(|b| b.text.chars().any(|c| c.is_ascii_digit()));
    if entry.bullets.len() >= 3 && none_with_digit {
        out.push(
            codes::NO_NUMBERS,
            Severity::Info,
            Some(&entry.id),
            format!("No bullet of \"{}\" has a number.", entry.title),
            Some("Numbers make results concrete: users, time saved, size of the team."),
        );
    }
}

/// True for control characters other than newline and tab, private-use
/// characters, and emoji.
fn is_special(c: char) -> bool {
    let cp = u32::from(c);
    (c.is_control() && c != '\n' && c != '\t')
        || (0xE000..=0xF8FF).contains(&cp)
        || (0xF0000..=0x10FFFF).contains(&cp)
        || (0x1F000..=0x1FAFF).contains(&cp)
        || (0x2600..=0x27BF).contains(&cp)
        || cp == 0xFE0F
        || cp == 0x200D
}

fn special_chars(cv: &ResolvedCv, out: &mut Lints) {
    let mut check = |target: Option<&Id>, field: &str, text: &str| {
        if let Some(c) = text.chars().find(|c| is_special(*c)) {
            out.push(
                codes::SPECIAL_CHARS,
                Severity::Warning,
                target,
                format!(
                    "The {field} contains U+{:04X}, which applicant tracking systems may garble.",
                    u32::from(c)
                ),
                Some("Remove emoji, symbols and invisible characters."),
            );
        }
    };
    let b = &cv.basics;
    check(None, "name", &b.full_name);
    for (field, text) in [
        ("headline", &b.headline),
        ("email", &b.email),
        ("phone", &b.phone),
        ("location", &b.location),
        ("summary", &cv.summary),
    ] {
        if let Some(text) = text {
            check(None, field, text);
        }
    }
    for link in &b.links {
        check(Some(&link.id), "link label", &link.label);
        check(Some(&link.id), "link", &link.url);
    }
    // The personal details as printed (ADR-088).
    for (field, value) in b.personal.lines(cv.options.date_style, cv.locale) {
        let label = crate::locale::personal_label(field, cv.locale).to_lowercase();
        check(None, &label, &value);
    }
    for section in &cv.sections {
        let heading = match section {
            ResolvedSection::Experience(s)
            | ResolvedSection::Education(s)
            | ResolvedSection::Projects(s)
            | ResolvedSection::Certifications(s)
            | ResolvedSection::Custom(s) => &s.heading,
            ResolvedSection::Skills(s) => &s.heading,
            ResolvedSection::Languages(s) => &s.heading,
        };
        if let Some(h) = heading {
            check(Some(section.id()), "section heading", h);
        }
        for e in entries_of(section) {
            for text in [Some(&e.title), Some(&e.org), e.location.as_ref()]
                .into_iter()
                .flatten()
            {
                check(Some(&e.id), "entry", text);
            }
            if let Some(d) = &e.description {
                check(Some(&e.id), "description", d);
            }
            if let Some(u) = &e.url {
                check(Some(&e.id), "link", u);
            }
            for bullet in &e.bullets {
                check(Some(&bullet.id), "bullet", &bullet.text);
            }
        }
        if let ResolvedSection::Skills(s) = section {
            for g in &s.groups {
                if let Some(label) = &g.label {
                    check(Some(&g.id), "skill group label", label);
                }
                for k in &g.skills {
                    check(Some(&k.id), "skill", &k.name);
                }
            }
        }
        if let ResolvedSection::Languages(s) = section {
            for item in &s.items {
                check(Some(&item.id), "language", &item.name);
                if let Some(level) = &item.level {
                    check(Some(&item.id), "language level", level);
                }
            }
        }
    }
}

/// True when the URL's scheme is `https:` or `mailto:`, ignoring case.
pub fn is_clickable_url(url: &str) -> bool {
    let Some((scheme, _)) = url.trim().split_once(':') else {
        return false;
    };
    scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("mailto")
}

fn links(cv: &ResolvedCv, out: &mut Lints) {
    let basics = cv.basics.links.iter().map(|l| (&l.id, &l.url));
    let entries = cv
        .sections
        .iter()
        .flat_map(entries_of)
        .filter_map(|e| e.url.as_ref().map(|u| (&e.id, u)));
    for (id, url) in basics.chain(entries) {
        if url.trim().is_empty() {
            continue;
        }
        if url.chars().count() > URL_MAX_CHARS {
            out.push(
                codes::LONG_URL,
                Severity::Info,
                Some(id),
                format!("This link is longer than {URL_MAX_CHARS} characters."),
                Some("Use a shorter address; it is printed in full."),
            );
        }
        if !is_clickable_url(url) {
            out.push(
                codes::UNSAFE_LINK,
                Severity::Warning,
                Some(id),
                format!(
                    "\"{url}\" is not an https: or mailto: link, so it is printed as plain text."
                ),
                Some("Use an https:// address."),
            );
        }
    }
}

/// Collects findings, then orders them by rule.
#[derive(Default)]
struct Lints(Vec<LintIssue>);

impl Lints {
    fn push(
        &mut self,
        code: &str,
        severity: Severity,
        target: Option<&Id>,
        message: String,
        fix_hint: Option<&str>,
    ) {
        self.0.push(LintIssue {
            code: code.to_owned(),
            severity,
            target: target.cloned(),
            message,
            fix_hint: fix_hint.map(str::to_owned),
        });
    }

    /// Stable sort by the rule's position in [`codes`]; document order is kept within a rule.
    fn sort_by_rule(mut self) -> Vec<LintIssue> {
        const ORDER: [&str; 31] = [
            codes::MISSING_NAME,
            codes::MISSING_CONTACT,
            codes::INVALID_EMAIL,
            codes::DATE_ORDER,
            codes::DATE_MISSING,
            codes::DATE_STYLE_MIXED,
            codes::GAP,
            codes::BULLET_TOO_LONG,
            codes::BULLET_TOO_SHORT,
            codes::TOO_MANY_BULLETS,
            codes::NO_BULLETS,
            codes::SUMMARY_TOO_LONG,
            codes::DUPLICATE_BULLET,
            codes::WEAK_OPENING,
            codes::NO_NUMBERS,
            codes::SPECIAL_CHARS,
            codes::LONG_URL,
            codes::UNSAFE_LINK,
            codes::PAGE_COUNT,
            codes::TWO_COLUMNS,
            codes::PERSONAL_DETAIL_SHOWN,
            codes::LOW_CONTRAST,
            codes::PHONE_FORMAT,
            codes::LANGUAGE_LEVEL,
            codes::LONG_SENTENCE,
            codes::PASSIVE_VOICE,
            codes::UNDEFINED_ACRONYM,
            codes::FIRST_PERSON,
            codes::TENSE_MIXED,
            codes::CLICHE,
            codes::OLD_EXPERIENCE,
        ];
        self.0
            .sort_by_key(|i| ORDER.iter().position(|c| *c == i.code));
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::sample_cv;
    use crate::test_ids::id;
    use crate::{Bullet, Cv, Entry, Section};

    #[test]
    fn a_light_accent_warns_with_a_darker_shade() {
        let mut cv = clean_cv();
        cv.options.accent = Some("#ffd700".into());
        let issues = lint(&cv.resolve(None), None);
        let found: Vec<_> = issues
            .iter()
            .filter(|i| i.code == codes::LOW_CONTRAST)
            .collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Warning);
        assert!(found[0].message.contains("#FFD700"), "{}", found[0].message);
        assert!(found[0].message.contains("1.4:1"), "{}", found[0].message);
        let hint = found[0].fix_hint.as_deref().expect("hint");
        let shade = hint
            .split_whitespace()
            .find(|w| w.starts_with('#'))
            .and_then(|w| Rgb::parse(w.trim_end_matches(',')))
            .expect("a shade in the hint");
        assert!(contrast_ratio(shade, Rgb::WHITE) >= TEXT_MIN_RATIO);

        for fine in [None, Some("#2B5CAB"), Some("#000000"), Some("bad")] {
            cv.options.accent = fine.map(Into::into);
            assert!(
                lint(&cv.resolve(None), None)
                    .iter()
                    .all(|i| i.code != codes::LOW_CONTRAST),
                "{fine:?}"
            );
        }
    }

    /// Numbers without a `+` country code, with stray characters or too few
    /// digits get a note; well-formed international numbers do not.
    #[test]
    fn phone_numbers_need_the_international_form() {
        let mut cv = clean_cv();
        let codes_for = |cv: &Cv| -> Vec<String> {
            lint(&cv.resolve(None), None)
                .into_iter()
                .filter(|i| i.code == codes::PHONE_FORMAT)
                .map(|i| i.message)
                .collect()
        };
        for good in [
            "+44 20 7946 0000",
            "+1 (555) 010-0000",
            "+49.170.1234567",
            "+33600000000",
        ] {
            cv.basics.phone = Some(good.into());
            assert!(codes_for(&cv).is_empty(), "{good}");
        }
        for (bad, why) in [
            ("020 7946 0000", "no international country code"),
            ("+44 20 7946 0000 ext 2", "other than digits"),
            ("+44 / 20 7946", "other than digits"),
            ("+44 123", "too short"),
        ] {
            cv.basics.phone = Some(bad.into());
            let found = codes_for(&cv);
            assert_eq!(found.len(), 1, "{bad}");
            assert!(found[0].contains(why), "{bad}: {}", found[0]);
        }
        cv.basics.phone = Some("  ".into());
        assert!(codes_for(&cv).is_empty());
    }

    /// CEFR, native, ILR and ACTFL levels pass; vague, negated or hedged
    /// wording does not.
    #[test]
    fn language_levels_on_a_known_scale_pass() {
        for ok in [
            "C1",
            "b2",
            "B2+",
            "C1 (Advanced)",
            "Upper intermediate - B2",
            "Native",
            "Mother tongue",
            "Native or bilingual proficiency",
            "Full professional proficiency",
            "Limited working proficiency",
            "ILR 3+",
            "ACTFL Advanced Mid",
            "Intermediate High",
        ] {
            assert!(is_recognised_level(ok), "{ok}");
        }
        for vague in [
            "good",
            "fluent-ish",
            "Fluent",
            "basic",
            "C3",
            "ILR 7",
            "conversational",
            "Non-native",
            "not native",
            "Not my first language",
            "Near-native",
            "Native-like",
            "alternatively fluent",
            "nativeish",
            "no longer native",
            "almost bilingual",
            "never my first language",
        ] {
            assert!(!is_recognised_level(vague), "{vague}");
        }
    }

    /// A vague level produces one Info note on that language item.
    #[test]
    fn a_vague_language_level_gets_an_info_note() {
        let mut cv = clean_cv();
        let lang = Id::parse("01J0Z000000000000000000099").expect("id");
        cv.sections
            .push(Section::Languages(crate::LanguagesSection {
                id: Id::parse("01J0Z000000000000000000098").expect("id"),
                title_override: None,
                items: vec![
                    crate::LanguageItem {
                        id: lang.clone(),
                        name: "German".into(),
                        level: Some("good".into()),
                    },
                    crate::LanguageItem {
                        id: Id::parse("01J0Z000000000000000000097").expect("id"),
                        name: "French".into(),
                        level: Some("B2".into()),
                    },
                ],
            }));
        let found: Vec<_> = lint(&cv.resolve(None), None)
            .into_iter()
            .filter(|i| i.code == codes::LANGUAGE_LEVEL)
            .collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Info);
        assert_eq!(found[0].target.as_ref(), Some(&lang));
        assert!(found[0].message.contains("German"));
    }

    fn good_bullet(n: u32, text: &str) -> Bullet {
        Bullet {
            id: id(n),
            text: text.into(),
        }
    }

    /// The sample CV with a realistic bullet, so it lints clean.
    fn clean_cv() -> Cv {
        let mut cv = sample_cv();
        entry_mut(&mut cv, 0).bullets[0].text = "Built a billing service used by 40 teams".into();
        cv
    }

    fn entry_mut(cv: &mut Cv, i: usize) -> &mut Entry {
        match &mut cv.sections[0] {
            Section::Experience(s) => &mut s.entries[i],
            _ => panic!("experience first"),
        }
    }

    fn push_entry(cv: &mut Cv, entry: Entry) {
        if let Section::Experience(s) = &mut cv.sections[0] {
            s.entries.push(entry);
        }
    }

    fn job(n: u32, start: (u16, Option<u8>), end: Option<(u16, Option<u8>)>) -> Entry {
        Entry {
            id: id(n),
            title: format!("Job {n}"),
            org: "Org".into(),
            location: None,
            period: Period {
                start: Some(PartialDate {
                    year: start.0,
                    month: start.1,
                }),
                end: end.map(|(year, month)| PartialDate { year, month }),
                current: false,
            },
            bullets: vec![good_bullet(
                n + 1000,
                &format!("Shipped reporting pipeline {n} on time"),
            )],
            description: None,
            url: None,
            locked: false,
        }
    }

    fn run(cv: &Cv) -> Vec<(String, Option<Id>)> {
        lint(&cv.resolve(None), None)
            .into_iter()
            .map(|i| (i.code, i.target))
            .collect()
    }

    fn codes(cv: &Cv) -> Vec<String> {
        run(cv).into_iter().map(|(c, _)| c).collect()
    }

    fn with_bullet(text: &str) -> Cv {
        let mut cv = clean_cv();
        entry_mut(&mut cv, 0).bullets[0].text = text.into();
        cv
    }

    fn hint_of(cv: &Cv, code: &str) -> String {
        lint(&cv.resolve(None), None)
            .into_iter()
            .find(|i| i.code == code)
            .and_then(|i| i.fix_hint)
            .unwrap_or_default()
    }

    #[test]
    fn long_sentences_in_bullets_and_the_summary() {
        let thirty = vec!["word"; SENTENCE_MAX_WORDS].join(" ");
        assert_eq!(run(&with_bullet(&format!("Built {thirty}"))).len(), 1);
        assert!(codes(&with_bullet(&thirty)).is_empty());
        let mut cv = with_bullet(&format!("Cut costs by 20%. Built {thirty}"));
        assert_eq!(run(&cv), [(codes::LONG_SENTENCE.into(), Some(id(5)))]);
        assert!(hint_of(&cv, codes::LONG_SENTENCE).starts_with("Standard (ISO 24495-1"));
        cv = clean_cv();
        cv.summary = Some(format!("Engineer. Built {thirty}."));
        assert_eq!(run(&cv), [(codes::LONG_SENTENCE.into(), None)]);
    }

    #[test]
    fn passive_and_first_person_bullets() {
        let cv = with_bullet("The billing service was rebuilt for 40 teams");
        assert_eq!(run(&cv), [(codes::PASSIVE_VOICE.into(), Some(id(5)))]);
        let cv = with_bullet("I built a billing service used by 40 teams");
        assert_eq!(run(&cv), [(codes::FIRST_PERSON.into(), Some(id(5)))]);
        assert!(hint_of(&cv, codes::FIRST_PERSON).starts_with("Advice ("));
    }

    #[test]
    fn acronyms_used_once_and_not_spelled_out() {
        let cv = with_bullet("Built a billing service with an SLO and a QPS target for 40 teams");
        assert_eq!(
            codes(&cv),
            [codes::UNDEFINED_ACRONYM, codes::UNDEFINED_ACRONYM]
        );
        // Spelled out in brackets, known, used twice, or also written in
        // lower case: no note.
        for fine in [
            "Set a Service Level Objective (SLO) for 40 teams",
            "Moved 40 teams to AWS and SQL",
            "Built SLO dashboards for 40 teams; SLO reviews weekly",
            "Built a billing service USED by 40 teams, used daily",
        ] {
            assert!(codes(&with_bullet(fine)).is_empty(), "{fine}");
        }
    }

    fn finish(cv: &mut Cv, year: u16) {
        let e = entry_mut(cv, 0);
        e.period.current = false;
        e.period.end = Some(PartialDate {
            year,
            month: Some(12),
        });
    }

    #[test]
    fn tense_follows_the_role() {
        // A finished role in the present.
        let mut cv = with_bullet("Manage a billing service used by 40 teams");
        assert!(codes(&cv).is_empty(), "the sample's first job is current");
        finish(&mut cv, 2025);
        assert_eq!(run(&cv), [(codes::TENSE_MIXED.into(), Some(id(4)))]);
        // A current role may be told in the past, but not both at once.
        let mut cv = clean_cv();
        assert!(
            codes(&cv).is_empty(),
            "past-tense results in a current role"
        );
        e_push(&mut cv, "Manage the on-call rota for 40 engineers");
        assert_eq!(codes(&cv), [codes::TENSE_MIXED]);
    }

    fn e_push(cv: &mut Cv, text: &str) {
        let e = entry_mut(cv, 0);
        let n = 900 + u32::try_from(e.bullets.len()).expect("few bullets");
        e.bullets.push(Bullet {
            id: id(n),
            text: text.into(),
        });
    }

    #[test]
    fn cliches_anywhere_in_the_text() {
        let cv = with_bullet("A team player who built a billing service for 40 teams");
        assert_eq!(run(&cv), [(codes::CLICHE.into(), Some(id(5)))]);
        let mut cv = clean_cv();
        cv.basics.headline = Some("Results-driven engineer".into());
        assert_eq!(run(&cv), [(codes::CLICHE.into(), None)]);
    }

    #[test]
    fn old_experience_counts_from_this_year() {
        let day = |year, month| CalendarDate {
            year,
            month: Some(month),
            day: None,
        };
        let mut cv = clean_cv();
        assert!(
            old_experience(&cv.resolve(None), day(2100, 1)).is_empty(),
            "current"
        );
        // `finish` ends the role in December 2005.
        finish(&mut cv, 2005);
        let cv = cv.resolve(None);
        // Exactly 15 years later is not yet over the line; a month more is.
        assert!(old_experience(&cv, day(2020, 12)).is_empty());
        let found = old_experience(&cv, day(2021, 1));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, codes::OLD_EXPERIENCE);
        assert_eq!(found[0].severity, Severity::Info);
        assert!(
            found[0].message.contains("15 years"),
            "{}",
            found[0].message
        );
        // Without today's month, January is assumed, so no early note.
        let undated = CalendarDate {
            year: 2021,
            month: None,
            day: None,
        };
        assert_eq!(old_experience(&cv, undated).len(), 1);
    }

    #[test]
    fn clean_cv_has_no_findings() {
        assert_eq!(run(&clean_cv()), vec![]);
    }

    #[test]
    fn missing_name_contact_and_invalid_email() {
        let mut cv = clean_cv();
        cv.basics.full_name = " ".into();
        cv.basics.email = None;
        assert_eq!(codes(&cv), [codes::MISSING_NAME, codes::MISSING_CONTACT]);
        cv.basics.email = Some("jane at example".into());
        assert_eq!(codes(&cv), [codes::MISSING_NAME, codes::INVALID_EMAIL]);
    }

    #[test]
    fn plausible_email_check() {
        for ok in ["a@b.co", "jane.doe+cv@mail.example.org"] {
            assert!(is_plausible_email(ok), "{ok}");
        }
        for bad in ["a@b", "@b.co", "a@@b.co", "a@b..co", "a b@c.de", "a@.co"] {
            assert!(!is_plausible_email(bad), "{bad}");
        }
    }

    #[test]
    fn date_order_respects_year_only_overlap() {
        let d = |year, month| PartialDate { year, month };
        assert!(is_before(d(2020, Some(5)), d(2020, Some(6))));
        assert!(is_before(d(2019, None), d(2020, Some(1))));
        assert!(!is_before(d(2020, None), d(2020, Some(6))));
        assert!(!is_before(d(2020, Some(6)), d(2020, None)));

        let mut cv = clean_cv();
        *entry_mut(&mut cv, 0) = job(4, (2021, Some(3)), Some((2020, Some(1))));
        assert_eq!(run(&cv), [(codes::DATE_ORDER.into(), Some(id(4)))]);
        entry_mut(&mut cv, 0).period.current = true;
        assert_eq!(run(&cv), [], "current entries ignore the end date");
    }

    #[test]
    fn date_missing_only_for_experience_and_education() {
        let mut cv = clean_cv();
        entry_mut(&mut cv, 0).period.start = None;
        assert_eq!(run(&cv), [(codes::DATE_MISSING.into(), Some(id(4)))]);
    }

    #[test]
    fn year_only_dates_mixed_with_month_dates() {
        let mut cv = clean_cv();
        push_entry(&mut cv, job(20, (2018, None), Some((2021, None))));
        assert_eq!(run(&cv), [(codes::DATE_STYLE_MIXED.into(), Some(id(20)))]);

        entry_mut(&mut cv, 0).period.start = Some(PartialDate {
            year: 2021,
            month: None,
        });
        assert_eq!(run(&cv), [], "year-only everywhere is consistent");
    }

    #[test]
    fn gaps_over_six_months_between_experience_entries() {
        let mut cv = clean_cv();
        // The sample job started 2021-03 and is current.
        push_entry(&mut cv, job(20, (2018, Some(1)), Some((2020, Some(8)))));
        assert_eq!(run(&cv), [], "Sep 2020 to Feb 2021 is six months");
        entry_mut(&mut cv, 1).period.end = Some(PartialDate {
            year: 2020,
            month: Some(7),
        });
        assert_eq!(run(&cv), [(codes::GAP.into(), Some(id(4)))]);

        // An overlapping longer job covers the gap.
        push_entry(&mut cv, job(21, (2019, Some(1)), Some((2021, Some(2)))));
        assert_eq!(run(&cv), []);
    }

    #[test]
    fn year_only_ends_and_starts_give_the_shortest_gap() {
        let mut cv = clean_cv();
        entry_mut(&mut cv, 0).period.start = Some(PartialDate {
            year: 2021,
            month: None,
        });
        push_entry(&mut cv, job(20, (2018, None), Some((2020, None))));
        assert_eq!(run(&cv), []);
    }

    #[test]
    fn bullet_length_count_and_missing_bullets() {
        let mut cv = clean_cv();
        let e = entry_mut(&mut cv, 0);
        e.bullets = vec![
            good_bullet(30, &"x".repeat(221)),
            good_bullet(31, "Too short"),
            good_bullet(32, &"é".repeat(220)),
        ];
        e.bullets[2].text.push('1');
        assert_eq!(
            run(&cv),
            [
                (codes::BULLET_TOO_LONG.into(), Some(id(30))),
                (codes::BULLET_TOO_LONG.into(), Some(id(32))),
                (codes::BULLET_TOO_SHORT.into(), Some(id(31))),
            ]
        );

        let e = entry_mut(&mut cv, 0);
        e.bullets = (0..9)
            .map(|n| {
                good_bullet(
                    40 + n,
                    &format!("Delivered project number {n} for a client"),
                )
            })
            .collect();
        assert_eq!(run(&cv), [(codes::TOO_MANY_BULLETS.into(), Some(id(4)))]);

        entry_mut(&mut cv, 0).bullets.clear();
        assert_eq!(run(&cv), [(codes::NO_BULLETS.into(), Some(id(4)))]);
    }

    #[test]
    fn collapsed_entries_are_not_missing_bullets() {
        let cv = clean_cv();
        let mut v = crate::Variant::empty(id(100), id(1), "V");
        v.collapsed.insert(id(4));
        assert_eq!(lint(&cv.resolve(Some(&v)), None), vec![]);
    }

    #[test]
    fn summary_too_long() {
        let mut cv = clean_cv();
        cv.summary = Some("a".repeat(600));
        assert_eq!(run(&cv), []);
        cv.summary = Some("a".repeat(601));
        assert_eq!(codes(&cv), [codes::SUMMARY_TOO_LONG]);
    }

    #[test]
    fn duplicate_bullets_ignore_case_and_spacing() {
        let mut cv = clean_cv();
        push_entry(&mut cv, job(20, (2021, Some(1)), Some((2021, Some(2)))));
        entry_mut(&mut cv, 1).bullets[0].text =
            "  built a billing   service USED by 40 teams".into();
        assert_eq!(run(&cv), [(codes::DUPLICATE_BULLET.into(), Some(id(1020)))]);
    }

    #[test]
    fn weak_openings_are_flagged() {
        let mut cv = clean_cv();
        entry_mut(&mut cv, 0).bullets[0].text =
            "Responsible for the billing service of 40 teams".into();
        assert_eq!(run(&cv), [(codes::WEAK_OPENING.into(), Some(id(5)))]);
    }

    #[test]
    fn three_bullets_without_numbers() {
        let mut cv = clean_cv();
        entry_mut(&mut cv, 0).bullets = (0..3)
            .map(|n| good_bullet(30 + n, "Improved the onboarding flow for new users"))
            .enumerate()
            .map(|(i, mut b)| {
                b.text.push_str(["", " a lot", " again"][i]);
                b
            })
            .collect();
        assert_eq!(run(&cv), [(codes::NO_NUMBERS.into(), Some(id(4)))]);
    }

    #[test]
    fn special_characters_in_any_text() {
        assert!(is_special('\u{7}'));
        assert!(is_special('\u{E000}'));
        assert!(is_special('🚀'));
        assert!(is_special('★'));
        assert!(!is_special('\n'));
        assert!(!is_special('é'));
        assert!(!is_special('–'));

        let mut cv = clean_cv();
        cv.basics.headline = Some("Engineer 🚀".into());
        entry_mut(&mut cv, 0).bullets[0].text.push('\u{E001}');
        assert_eq!(
            run(&cv),
            [
                (codes::SPECIAL_CHARS.into(), None),
                (codes::SPECIAL_CHARS.into(), Some(id(5))),
            ]
        );
    }

    #[test]
    fn long_and_unsafe_links() {
        assert!(is_clickable_url("HTTPS://example.com"));
        assert!(is_clickable_url("mailto:jane@example.com"));
        assert!(!is_clickable_url("http://example.com"));
        assert!(!is_clickable_url("javascript:alert(1)"));
        assert!(!is_clickable_url("example.com"));

        let mut cv = clean_cv();
        cv.basics.links[0].url = format!("http://example.com/{}", "a".repeat(50));
        entry_mut(&mut cv, 0).url = Some("ftp://example.com".into());
        assert_eq!(
            run(&cv),
            [
                (codes::LONG_URL.into(), Some(id(2))),
                (codes::UNSAFE_LINK.into(), Some(id(2))),
                (codes::UNSAFE_LINK.into(), Some(id(4))),
            ]
        );
    }

    #[test]
    fn page_count_only_when_known_and_over_two() {
        let r = clean_cv().resolve(None);
        assert_eq!(lint(&r, None), vec![]);
        assert_eq!(lint(&r, Some(2)), vec![]);
        let issues = lint(&r, Some(3));
        assert_eq!(issues.len(), 1);
        assert_eq!(
            (issues[0].code.as_str(), issues[0].severity),
            (codes::PAGE_COUNT, Severity::Info)
        );
    }

    #[test]
    fn two_columns_get_a_warning() {
        let mut cv = clean_cv();
        assert_eq!(lint(&cv.resolve(None), None), vec![]);
        cv.layout.columns = crate::Columns::Two;
        let issues = lint(&cv.resolve(None), None);
        assert_eq!(issues.len(), 1);
        assert_eq!(
            (issues[0].code.as_str(), issues[0].severity),
            (codes::TWO_COLUMNS, Severity::Warning)
        );
    }

    #[test]
    fn shown_personal_details_are_checked_for_special_characters() {
        let mut cv = clean_cv();
        cv.basics.personal.nationality = vec!["Italian \u{1F1EE}\u{1F1F9}".into()];
        cv.basics.personal.address = Some("Via Esempio 1\u{E000}".into());
        // Hidden: not printed, so not checked.
        assert!(lint(&cv.resolve(None), None).is_empty());
        cv.basics.show_personal = [PersonalField::Nationality, PersonalField::Address].into();
        let special: Vec<String> = lint(&cv.resolve(None), None)
            .into_iter()
            .filter(|i| i.code == codes::SPECIAL_CHARS)
            .map(|i| i.message)
            .collect();
        assert_eq!(
            special,
            [
                "The nationality contains U+1F1EE, which applicant tracking systems may garble.",
                "The address contains U+E000, which applicant tracking systems may garble.",
            ]
        );
    }

    #[test]
    fn shown_personal_details_are_info_notes() {
        let mut cv = clean_cv();
        cv.basics.personal.birth_date = crate::CalendarDate::parse("1990-03-12");
        cv.basics.personal.driving_licence = vec!["B".into()];
        cv.basics.personal.gender = Some(" ".into());
        // Filled but switched off: no note.
        assert_eq!(lint(&cv.resolve(None), None), vec![]);

        cv.basics.show_personal = [
            PersonalField::BirthDate,
            PersonalField::DrivingLicence,
            PersonalField::Gender,
        ]
        .into();
        let issues = lint(&cv.resolve(None), Some(3));
        let got: Vec<(&str, Severity, &str)> = issues
            .iter()
            .map(|i| (i.code.as_str(), i.severity, i.message.as_str()))
            .collect();
        // Blank gender prints nothing, so it gets no note.
        assert_eq!(
            got,
            [
                (codes::PAGE_COUNT, Severity::Info, "The CV is 3 pages long."),
                (
                    codes::PERSONAL_DETAIL_SHOWN,
                    Severity::Info,
                    "Date of birth is shown on this CV."
                ),
                (
                    codes::PERSONAL_DETAIL_SHOWN,
                    Severity::Info,
                    "Driving licence is shown on this CV."
                ),
            ]
        );
        assert!(
            issues[1]
                .fix_hint
                .as_deref()
                .is_some_and(|h| h.contains("Equality Act"))
        );
        assert!(
            !issues[2]
                .fix_hint
                .as_deref()
                .is_some_and(|h| h.contains("Equality Act"))
        );
    }
}
