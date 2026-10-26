//! Rewrite verification (spec section 11.8): deterministic checks that a
//! rewrite adds no facts. Errors drop the suggestion; warnings and notes
//! travel with it as badges.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use cv_core::{Severity, VerificationKind, VerificationWarning};
use regex::Regex;

/// Shortest and longest rewrite, as a share of the original's length.
pub const LENGTH_RANGE: (f64, f64) = (0.4, 1.1);
/// Longest rewrite in characters.
pub const MAX_CHARS: usize = 200;

/// What a rewrite is checked against besides its original.
#[derive(Clone, Debug, Default)]
pub struct Context<'a> {
    /// The entry's other bullets, its title and organization.
    pub entry: Vec<&'a str>,
    /// Every skill name in the CV.
    pub skills: Vec<&'a str>,
    /// The job's keywords, which a declared terminology change may bring in.
    pub job_keywords: Vec<&'a str>,
    /// The model said it aligned terminology with the job.
    pub terminology_declared: bool,
}

/// The outcome of [`verify_rewrite`].
#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    /// Show the rewrite with these warnings (possibly none).
    Keep(Vec<VerificationWarning>),
    /// Drop it: an error check failed. The findings say why.
    Drop(Vec<VerificationWarning>),
    /// The rewrite only changes spacing or case: drop it silently.
    Same,
}

/// Checks `rewrite` of `original` (spec section 11.8, checks 1 to 6).
pub fn verify_rewrite(original: &str, rewrite: &str, ctx: &Context<'_>) -> Verdict {
    if normalize(original) == normalize(rewrite) {
        return Verdict::Same;
    }
    let mut found = Vec::new();
    let finding = |kind, severity, detail: String| VerificationWarning {
        kind,
        severity,
        detail,
    };

    // 1. Numbers, digits or spelled out, by value.
    let known = numbers(original);
    for (token, value) in number_tokens(rewrite) {
        if !known.iter().any(|k| same_number(*k, value)) {
            found.push(finding(VerificationKind::NewNumber, Severity::Error, token));
        }
    }

    // 2 and 3. Names, technologies and job terms the sources do not have.
    let mut sources = String::from(original);
    for s in ctx.entry.iter().chain(&ctx.skills) {
        sources.push('\n');
        sources.push_str(s);
    }
    let sources_lower = sources.to_lowercase();
    let mut seen = BTreeSet::new();
    for term in terms(rewrite) {
        let lower = term.to_lowercase();
        if contains_word(&sources_lower, &lower) || !seen.insert(lower.clone()) {
            continue;
        }
        let from_job = ctx.job_keywords.iter().any(|k| k.to_lowercase() == lower);
        if from_job && ctx.terminology_declared {
            found.push(finding(
                VerificationKind::TerminologyChanged,
                Severity::Info,
                term,
            ));
        } else {
            found.push(finding(VerificationKind::NewTerm, Severity::Warning, term));
        }
    }

    // 4. Length.
    let (a, b) = (original.chars().count(), rewrite.chars().count());
    #[allow(clippy::cast_precision_loss)] // Bullet lengths are small.
    let ratio = if a == 0 {
        f64::INFINITY
    } else {
        b as f64 / a as f64
    };
    if b > MAX_CHARS || ratio < LENGTH_RANGE.0 || ratio > LENGTH_RANGE.1 {
        found.push(finding(
            VerificationKind::LengthOutOfRange,
            Severity::Error,
            format!("{b} characters, was {a}"),
        ));
    }

    // 5. Language, when both texts are long enough to tell.
    if let (Some(x), Some(y)) = (language(original), language(rewrite))
        && x != y
    {
        found.push(finding(
            VerificationKind::LanguageChanged,
            Severity::Error,
            format!("{} → {}", x.code(), y.code()),
        ));
    }

    if found.iter().any(|f| f.severity == Severity::Error) {
        Verdict::Drop(found)
    } else {
        Verdict::Keep(found)
    }
}

/// Longest summary in characters (spec section 11.9).
pub const SUMMARY_MAX_CHARS: usize = 420;

/// Checks a proposed summary against the whole CV (spec section 11.9):
/// `cv_text` is every text of the CV, `current` the summary it would
/// replace, and `max_years` the longest experience span its periods allow,
/// so "8 years" may be derived from the dates. New numbers, a summary over
/// [`SUMMARY_MAX_CHARS`] and a language change drop it; new terms are
/// warnings.
pub fn verify_summary(
    summary: &str,
    current: Option<&str>,
    cv_text: &str,
    max_years: Option<u32>,
) -> Verdict {
    if summary.trim().is_empty() || current.is_some_and(|c| normalize(c) == normalize(summary)) {
        return Verdict::Same;
    }
    let mut found = Vec::new();
    let finding = |kind, severity, detail: String| VerificationWarning {
        kind,
        severity,
        detail,
    };
    let known = numbers(cv_text);
    for (token, value) in number_tokens(summary) {
        let years = max_years.is_some_and(|m| {
            value >= 1.0
                && value <= f64::from(m)
                && value.fract() == 0.0
                && is_year_count(summary, &token)
        });
        if !years && !known.iter().any(|k| same_number(*k, value)) {
            found.push(finding(VerificationKind::NewNumber, Severity::Error, token));
        }
    }
    let sources_lower = cv_text.to_lowercase();
    let mut seen = BTreeSet::new();
    for term in terms(summary) {
        let lower = term.to_lowercase();
        if !contains_word(&sources_lower, &lower) && seen.insert(lower) {
            found.push(finding(VerificationKind::NewTerm, Severity::Warning, term));
        }
    }
    let n = summary.chars().count();
    if n > SUMMARY_MAX_CHARS {
        found.push(finding(
            VerificationKind::LengthOutOfRange,
            Severity::Error,
            format!("{n} characters, at most {SUMMARY_MAX_CHARS}"),
        ));
    }
    if let (Some(x), Some(y)) = (language(cv_text), language(summary))
        && x != y
    {
        found.push(finding(
            VerificationKind::LanguageChanged,
            Severity::Error,
            format!("{} → {}", x.code(), y.code()),
        ));
    }
    if found.iter().any(|f| f.severity == Severity::Error) {
        Verdict::Drop(found)
    } else {
        Verdict::Keep(found)
    }
}

fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .trim_end_matches('.')
        .to_owned()
}

fn language(text: &str) -> Option<whatlang::Lang> {
    if text.chars().count() < 40 {
        return None;
    }
    whatlang::detect(text)
        .filter(|i| i.is_reliable() && i.confidence() > 0.8)
        .map(|i| i.lang())
}

static NUMBER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[$€£¥]?\d+(?:[.,']\d+)*(?:\s?(?:%|x|k|m|bn|mn|million|billion|thousand|tausend|millionen|milliarden))?\b%?")
        .expect("number pattern")
});

/// Numbers in `text`, as written and by value.
fn number_tokens(text: &str) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = NUMBER
        .find_iter(text)
        .filter_map(|m| number_value(m.as_str()).map(|v| (m.as_str().trim().to_owned(), v)))
        .collect();
    let words = alphabetic_words(text);
    let mut i = 0;
    while i < words.len() {
        let (start, end) = words[i];
        let word = text[start..end].to_lowercase();
        // "twenty-one", "twenty one": tens then a unit.
        if let (Some(tens), Some(&(next_start, next_end))) = (tens_value(&word), words.get(i + 1)) {
            let gap = &text[end..next_start];
            let joined = !gap.is_empty() && gap.chars().all(|c| c == '-' || c.is_whitespace());
            if let Some(unit) = joined
                .then(|| unit_value(&text[next_start..next_end].to_lowercase()))
                .flatten()
            {
                out.push((text[start..next_end].to_owned(), tens + unit));
                i += 2;
                continue;
            }
        }
        if let Some(v) = spelled(&word) {
            out.push((text[start..end].to_owned(), v));
        }
        i += 1;
    }
    out
}

/// Byte ranges of the alphabetic runs in `text`.
fn alphabetic_words(text: &str) -> Vec<(usize, usize)> {
    let mut words = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        match (c.is_alphabetic(), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                words.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        words.push((s, text.len()));
    }
    words
}

const UNITS: &[(&str, f64)] = &[
    ("one", 1.0),
    ("two", 2.0),
    ("three", 3.0),
    ("four", 4.0),
    ("five", 5.0),
    ("six", 6.0),
    ("seven", 7.0),
    ("eight", 8.0),
    ("nine", 9.0),
];

const TENS: &[(&str, f64)] = &[
    ("twenty", 20.0),
    ("thirty", 30.0),
    ("forty", 40.0),
    ("fifty", 50.0),
    ("sixty", 60.0),
    ("seventy", 70.0),
    ("eighty", 80.0),
    ("ninety", 90.0),
];

const GERMAN_UNITS: &[(&str, f64)] = &[
    ("ein", 1.0),
    ("zwei", 2.0),
    ("drei", 3.0),
    ("vier", 4.0),
    ("fünf", 5.0),
    ("sechs", 6.0),
    ("sieben", 7.0),
    ("acht", 8.0),
    ("neun", 9.0),
];

const GERMAN_TENS: &[(&str, f64)] = &[
    ("zwanzig", 20.0),
    ("dreißig", 30.0),
    ("vierzig", 40.0),
    ("fünfzig", 50.0),
    ("sechzig", 60.0),
    ("siebzig", 70.0),
    ("achtzig", 80.0),
    ("neunzig", 90.0),
];

fn lookup(table: &[(&str, f64)], word: &str) -> Option<f64> {
    table.iter().find(|(w, _)| *w == word).map(|(_, v)| *v)
}

fn unit_value(word: &str) -> Option<f64> {
    lookup(UNITS, word)
}

fn tens_value(word: &str) -> Option<f64> {
    lookup(TENS, word)
}

/// German unit-plus-tens words: "einundzwanzig" is 21.
fn german_compound(word: &str) -> Option<f64> {
    let (unit, tens) = word.split_once("und")?;
    Some(lookup(GERMAN_UNITS, unit)? + lookup(GERMAN_TENS, tens)?)
}

fn numbers(text: &str) -> Vec<f64> {
    number_tokens(text).into_iter().map(|(_, v)| v).collect()
}

fn same_number(a: f64, b: f64) -> bool {
    (a - b).abs() <= f64::EPSILON * a.abs().max(b.abs()).max(1.0)
}

/// The value of a number token: separators resolved, suffixes applied.
fn number_value(token: &str) -> Option<f64> {
    let lower = token.to_lowercase();
    let digits_end = lower
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_ascii_digit())
        .map(|(i, c)| i + c.len_utf8())?;
    let start = lower.find(|c: char| c.is_ascii_digit())?;
    let raw = &lower[start..digits_end];
    let suffix = lower[digits_end..].trim();
    let commas = raw.matches(',').count();
    let dots = raw.matches('.').count();
    let plain: String = if commas > 0 && dots > 0 {
        // "1,234.5" or "1.234,5": the last separator is the decimal one.
        let decimal = if raw.rfind(',') > raw.rfind('.') {
            ','
        } else {
            '.'
        };
        raw.chars()
            .filter_map(|c| match c {
                c if c == decimal => Some('.'),
                ',' | '.' | '\'' => None,
                c => Some(c),
            })
            .collect()
    } else if commas + dots > 0 {
        let sep = if commas > 0 { ',' } else { '.' };
        let groups: Vec<&str> = raw.split(sep).collect();
        let thousands = groups.len() > 2 || groups.last().is_some_and(|g| g.len() == 3);
        if thousands {
            raw.replace([sep, '\''], "")
        } else {
            raw.replace('\'', "").replace(sep, ".")
        }
    } else {
        raw.replace('\'', "")
    };
    let value: f64 = plain.parse().ok()?;
    let factor = match suffix.trim_end_matches('%') {
        "k" | "thousand" | "tausend" => 1e3,
        "m" | "mn" | "million" | "millionen" => 1e6,
        "bn" | "billion" | "milliarden" => 1e9,
        _ => 1.0,
    };
    Some(value * factor)
}

/// Spelled-out numbers in English and German.
fn spelled(word: &str) -> Option<f64> {
    const WORDS: &[(&str, f64)] = &[
        ("ten", 10.0),
        ("eleven", 11.0),
        ("twelve", 12.0),
        ("thirteen", 13.0),
        ("fourteen", 14.0),
        ("fifteen", 15.0),
        ("sixteen", 16.0),
        ("seventeen", 17.0),
        ("eighteen", 18.0),
        ("nineteen", 19.0),
        ("hundred", 100.0),
        ("thousand", 1e3),
        ("million", 1e6),
        ("billion", 1e9),
        ("dozen", 12.0),
        ("half", 0.5),
        ("twice", 2.0),
        ("double", 2.0),
        ("triple", 3.0),
        ("tripled", 3.0),
        ("doubled", 2.0),
        ("halved", 0.5),
        ("eins", 1.0),
        ("zwei", 2.0),
        ("drei", 3.0),
        ("vier", 4.0),
        ("fünf", 5.0),
        ("sechs", 6.0),
        ("sieben", 7.0),
        ("acht", 8.0),
        ("neun", 9.0),
        ("zehn", 10.0),
        ("elf", 11.0),
        ("zwölf", 12.0),
        ("hundert", 100.0),
        ("tausend", 1e3),
        ("verdoppelt", 2.0),
        ("halbiert", 0.5),
    ];
    lookup(WORDS, word)
        .or_else(|| lookup(UNITS, word))
        .or_else(|| lookup(TENS, word))
        .or_else(|| lookup(GERMAN_TENS, word))
        .or_else(|| german_compound(word))
}

/// Whether every whole-word use of `token` in `text` counts years:
/// "8 years", "8+ yrs", "acht Jahre".
fn is_year_count(text: &str, token: &str) -> bool {
    let lower = text.to_lowercase();
    let token = token.to_lowercase();
    let mut found = false;
    for (i, _) in lower.match_indices(&token) {
        let before = lower[..i].chars().next_back();
        let rest = &lower[i + token.len()..];
        if before.is_some_and(char::is_alphanumeric)
            || rest.chars().next().is_some_and(char::is_alphanumeric)
        {
            continue;
        }
        let rest = rest.trim_start_matches(['+', ' ', '-', '\u{a0}']);
        if !["year", "yr", "jahr"].iter().any(|w| rest.starts_with(w)) {
            return false;
        }
        found = true;
    }
    found
}

/// The technology dictionary (spec section 11.8), one term per line.
static TERMS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    include_str!("../data/tech_terms.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
});

/// Tokens that name something: capitalized mid-sentence, holding digits or
/// `.`, `/`, `+` or `#` inside, or in the dictionary (case as listed, so
/// "Go" counts and "go" does not).
fn terms(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut sentence_start = true;
    for raw in text.split_whitespace() {
        let token = raw
            .trim_matches(|c: char| !(c.is_alphanumeric() || matches!(c, '+' | '#' | '.' | '/')));
        let token = token.trim_end_matches(['.', '/']);
        let ends_sentence = raw.ends_with(['.', '!', '?', ':']);
        if token.is_empty() || is_number(token) {
            sentence_start = ends_sentence || sentence_start && token.is_empty();
            continue;
        }
        let inner = token
            .chars()
            .skip(1)
            .any(|c| matches!(c, '.' | '/' | '+' | '#'))
            || (token.chars().any(|c| c.is_ascii_digit())
                && token.chars().any(char::is_alphabetic));
        let capital = !sentence_start && token.chars().next().is_some_and(char::is_uppercase);
        let listed = TERMS.contains(&token);
        if inner || capital || listed {
            out.push(token.to_owned());
        }
        sentence_start = ends_sentence;
    }
    // Multi-word dictionary terms ("Spring Boot", "GitHub Actions").
    for term in TERMS.iter().filter(|t| t.contains(' ')) {
        if contains_word(text, term) {
            out.push((*term).to_owned());
        }
    }
    out
}

/// Whether `token` is a number as a whole ("40%", "2.1m", "€300k").
fn is_number(token: &str) -> bool {
    NUMBER
        .find(token)
        .is_some_and(|m| m.start() == 0 && m.end() == token.len())
}

/// Whether `hay` holds `needle` with no letter or digit right before or after.
fn contains_word(hay: &str, needle: &str) -> bool {
    hay.match_indices(needle).any(|(i, _)| {
        let before = hay[..i].chars().next_back();
        let after = hay[i + needle.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(v: &Verdict) -> Vec<VerificationKind> {
        match v {
            Verdict::Keep(w) | Verdict::Drop(w) => w.iter().map(|w| w.kind).collect(),
            Verdict::Same => vec![],
        }
    }

    const ORIGINAL: &str = "Led the migration of the billing platform to Rust, cutting p99 latency by 40% for four teams";

    #[test]
    fn summaries_are_checked_against_the_whole_cv() {
        let cv = "Backend Engineer at Acme. Led the migration of billing to Rust for four teams. Skills: Rust, PostgreSQL";
        let ok = "Backend engineer with 6 years of experience in Rust and PostgreSQL, who led a billing migration for four teams.";
        assert_eq!(verify_summary(ok, None, cv, Some(8)), Verdict::Keep(vec![]));
        // More years than the dates allow, and a number the CV lacks.
        let v = verify_summary(ok, None, cv, Some(5));
        assert_eq!(kinds(&v), [VerificationKind::NewNumber]);
        let v = verify_summary(
            "Engineer who cut costs by 30% using Kubernetes.",
            None,
            cv,
            None,
        );
        assert!(matches!(&v, Verdict::Drop(_)));
        assert_eq!(
            kinds(&v),
            [VerificationKind::NewNumber, VerificationKind::NewTerm]
        );
        let long = "Backend engineer. ".repeat(30);
        assert_eq!(
            kinds(&verify_summary(&long, None, cv, None)),
            [VerificationKind::LengthOutOfRange]
        );
        // Only year counts are exempt: "5 engineers" is not in the CV.
        let v = verify_summary(
            "Backend engineer who led 5 engineers on a billing migration to Rust.",
            None,
            cv,
            Some(8),
        );
        assert!(matches!(&v, Verdict::Drop(_)));
        assert_eq!(kinds(&v), [VerificationKind::NewNumber]);
        assert!(is_year_count("8+ years of Rust", "8"));
        assert!(is_year_count("acht Jahre Erfahrung", "acht"));
        assert!(!is_year_count("8 years and 8 teams", "8"));
        assert!(is_year_count("18 teams, 8 years", "8"));
        assert_eq!(verify_summary(ok, Some(ok), cv, Some(8)), Verdict::Same);
        assert_eq!(verify_summary("  ", None, cv, None), Verdict::Same);
    }

    #[test]
    fn faithful_rewrites_pass() {
        let v = verify_rewrite(
            ORIGINAL,
            "Migrated the billing platform to Rust, cutting p99 latency 40% for 4 teams",
            &Context::default(),
        );
        assert_eq!(v, Verdict::Keep(vec![]));
    }

    #[test]
    fn new_numbers_drop_the_rewrite() {
        let v = verify_rewrite(
            ORIGINAL,
            "Migrated the billing platform to Rust, cutting p99 latency by 45% for four teams",
            &Context::default(),
        );
        assert!(matches!(&v, Verdict::Drop(_)));
        assert_eq!(kinds(&v), [VerificationKind::NewNumber]);
        let v = verify_rewrite(
            "Grew the revenue of the team",
            "Doubled revenue of the team",
            &Context::default(),
        );
        assert_eq!(kinds(&v), [VerificationKind::NewNumber]);
    }

    #[test]
    fn compound_number_words_compare_by_value() {
        let words = |t: &str| numbers(t);
        assert_eq!(words("twenty-one teams"), [21.0]);
        assert_eq!(words("forty two teams"), [42.0]);
        assert_eq!(words("einundzwanzig Teams"), [21.0]);
        assert_eq!(words("twenty teams"), [20.0]);
        for (original, rewrite) in [
            (
                "Coached twenty-one engineers across the payments org",
                "Coached 21 engineers across the payments org",
            ),
            (
                "Coached thirty five engineers across the payments org",
                "Coached 35 engineers across the payments org",
            ),
            (
                "Betreute einundzwanzig Entwickler im Zahlungsbereich",
                "Betreute 21 Entwickler im Zahlungsbereich",
            ),
        ] {
            assert_eq!(
                kinds(&verify_rewrite(original, rewrite, &Context::default())),
                []
            );
        }
    }

    #[test]
    fn numbers_compare_by_value() {
        assert_eq!(number_value("1,200"), Some(1200.0));
        assert_eq!(number_value("1.200"), Some(1200.0));
        assert_eq!(number_value("2,5"), Some(2.5));
        assert_eq!(number_value("1,234.5"), Some(1234.5));
        assert_eq!(number_value("10k"), Some(10_000.0));
        assert_eq!(number_value("€2.1m"), Some(2_100_000.0));
        assert_eq!(number_value("40%"), Some(40.0));
        let v = verify_rewrite(
            "Processed 2,100,000 events each day across the platform",
            "Processed 2.1m events daily across the platform",
            &Context::default(),
        );
        assert_eq!(kinds(&v), []);
    }

    #[test]
    fn new_terms_are_flagged_and_job_terms_noted_when_declared() {
        let ctx = Context {
            entry: vec!["Ran the CI/CD pipelines"],
            skills: vec!["Kafka"],
            job_keywords: vec!["continuous delivery", "Kubernetes"],
            terminology_declared: true,
        };
        let v = verify_rewrite(
            "Built the event pipeline on Kafka for the payments team",
            "Built the Kafka event pipeline on Kubernetes for payments",
            &ctx,
        );
        assert_eq!(
            v,
            Verdict::Keep(vec![VerificationWarning {
                kind: VerificationKind::TerminologyChanged,
                severity: Severity::Info,
                detail: "Kubernetes".into(),
            }])
        );
        let ctx = Context {
            terminology_declared: false,
            ..ctx
        };
        let v = verify_rewrite(
            "Built the event pipeline on Kafka for the payments team",
            "Built the Kafka event pipeline with Node.js for payments",
            &ctx,
        );
        assert_eq!(kinds(&v), [VerificationKind::NewTerm]);
    }

    #[test]
    fn sentence_starts_and_common_words_are_not_terms() {
        assert!(terms("Reduced costs. Mentored the go team").is_empty());
        assert_eq!(terms("Shipped Go services and npm packages"), ["Go", "npm"]);
    }

    #[test]
    fn length_and_language_are_errors() {
        let v = verify_rewrite(ORIGINAL, "Migrated billing to Rust", &Context::default());
        assert_eq!(kinds(&v), [VerificationKind::LengthOutOfRange]);
        let v = verify_rewrite(
            "Led the migration of the billing platform to a new system for the whole company",
            "Leitete die Migration der Abrechnungsplattform auf ein neues System im Unternehmen",
            &Context::default(),
        );
        assert!(
            kinds(&v).contains(&VerificationKind::LanguageChanged),
            "{v:?}"
        );
    }

    #[test]
    fn identical_rewrites_are_dropped_silently() {
        assert_eq!(
            verify_rewrite("Built  APIs.", "built APIs", &Context::default()),
            Verdict::Same
        );
    }

    #[test]
    fn the_dictionary_is_big_enough() {
        assert!(TERMS.len() >= 500, "{}", TERMS.len());
    }
}
