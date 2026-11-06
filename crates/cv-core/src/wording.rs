//! English wording heuristics for the plain-language lints (issue #141).
//! Deterministic word lists and patterns, no AI. They are heuristics: each
//! errs towards saying nothing, since a wrong note costs the user's trust.

/// Sentences of `text`: split after `.`, `!` or `?` followed by a space or
/// the end, and at line breaks. A full stop inside a number or an
/// abbreviation without a following space ("3.5", "e.g.x") does not split.
pub fn sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    for (i, c) in text.char_indices() {
        let end = match c {
            '\n' => Some(i),
            '.' | '!' | '?' if bytes.get(i + 1).is_none_or(|b| b.is_ascii_whitespace()) => {
                Some(i + 1)
            }
            _ => None,
        };
        if let Some(end) = end {
            let s = text[start..end].trim();
            if !s.is_empty() {
                out.push(s);
            }
            start = end;
        }
    }
    let rest = text[start..].trim();
    if !rest.is_empty() {
        out.push(rest);
    }
    out
}

/// Words of `text`: runs of letters, digits, `'`, `-`, `+`, `#` and `/`.
pub fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_alphanumeric() || matches!(c, '\'' | '’' | '-' | '+' | '#' | '/')))
        .map(|w| w.trim_matches(|c| matches!(c, '\'' | '’' | '-' | '/')))
        .filter(|w| w.chars().any(char::is_alphanumeric))
}

/// The number of words in `text`.
pub fn word_count(text: &str) -> usize {
    words(text).count()
}

/// Irregular past participles common in CV bullets.
const IRREGULAR_PARTICIPLES: &[&str] = &[
    "built",
    "made",
    "led",
    "done",
    "given",
    "written",
    "run",
    "taken",
    "chosen",
    "shown",
    "known",
    "seen",
    "held",
    "kept",
    "won",
    "sent",
    "set",
    "put",
    "brought",
    "taught",
    "sold",
    "bought",
    "grown",
    "driven",
    "begun",
    "met",
    "paid",
    "told",
    "rebuilt",
    "rewritten",
    "redone",
    "overseen",
    "undertaken",
    "found",
];

/// `-ed` words after "was"/"were" that are adjectives, not passives.
const NOT_PARTICIPLES: &[&str] = &[
    "interested",
    "excited",
    "skilled",
    "talented",
    "red",
    "need",
];

/// True when `text` uses a passive ("was built", "were reduced by", "has
/// been migrated"). A heuristic: a form of "to be" followed by a word that
/// ends in "-ed" or is a common irregular participle.
pub fn is_passive(text: &str) -> bool {
    let lower = text.to_lowercase();
    let w: Vec<&str> = words(&lower).collect();
    w.windows(2).any(|pair| {
        let [be, next] = pair else { return false };
        matches!(*be, "was" | "were" | "been" | "being" | "is" | "are")
            && ((next.len() > 4 && next.ends_with("ed") && !NOT_PARTICIPLES.contains(next))
                || IRREGULAR_PARTICIPLES.contains(next))
    })
}

/// The first first-person word ("I", "I'm", "my", "me", "myself") in
/// `text`, if any. "I" counts only as a capital, so "i.e." does not.
pub fn first_person(text: &str) -> Option<&str> {
    words(text).find(|w| {
        matches!(*w, "I" | "I'm" | "I’m" | "I've" | "I’ve" | "I'd" | "I’d")
            || matches!(w.to_lowercase().as_str(), "my" | "me" | "myself")
    })
}

/// Phrases that say nothing a reader can check. Common advice, not a
/// standard.
pub const CLICHES: &[&str] = &[
    "team player",
    "hard-working",
    "hard working",
    "hardworking",
    "results-driven",
    "results driven",
    "go-getter",
    "self-starter",
    "detail-oriented",
    "detail oriented",
    "think outside the box",
    "synergy",
    "dynamic individual",
    "proven track record",
    "passionate about",
    "highly motivated",
];

/// The first cliché in `text`, as listed in [`CLICHES`].
pub fn cliche(text: &str) -> Option<&'static str> {
    let lower = text.to_lowercase();
    CLICHES.iter().copied().find(|c| {
        lower.match_indices(c).any(|(i, _)| {
            let before = lower[..i].chars().next_back();
            let after = lower[i + c.len()..].chars().next();
            !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
        })
    })
}

/// Acronyms most readers know; they need no definition.
const KNOWN_ACRONYMS: &[&str] = &[
    "ACCA", "API", "APIS", "ATS", "AWS", "BBC", "CAD", "CEFR", "CEO", "CFA", "CFO", "CIO", "CLI",
    "CMS", "COO", "CPA", "CPU", "CRM", "CSS", "CSV", "CTO", "CV", "DNS", "EMEA", "ERP", "ESG",
    "ETL", "FAQ", "GCP", "GDPR", "GIS", "GMBH", "GPU", "HTML", "HTTP", "HTTPS", "IBM", "IDE",
    "IELTS", "IOS", "IOT", "ISO", "JSON", "JWT", "KPI", "KPIS", "LAN", "LLC", "LLM", "LLMS", "LTD",
    "MBA", "MSC", "NASA", "NGO", "NHS", "NLP", "OKR", "OKRS", "PDF", "PHP", "PMP", "QA", "RAM",
    "REST", "ROI", "SAAS", "SAP", "SDK", "SEO", "SLA", "SLAS", "SMS", "SQL", "SSD", "SSH", "SSL",
    "TCP", "TLS", "TOEFL", "UCLA", "UDP", "UNICEF", "UNIX", "URL", "USA", "USB", "UTC", "VAT",
    "VPN", "WHO", "XML", "YAML", "NATO",
];

/// True for a word written in capitals that looks like an acronym: three
/// to eight characters, at least two of them letters, a letter first, digits
/// allowed after it, and not a well-known one.
pub fn is_unknown_acronym(word: &str) -> bool {
    let letters = word.chars().filter(char::is_ascii_alphabetic).count();
    (3..=8).contains(&word.len())
        && letters >= 2
        && word.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && word
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && !KNOWN_ACRONYMS.contains(&word)
        && !is_roman_numeral(word)
}

fn is_roman_numeral(word: &str) -> bool {
    word.chars()
        .all(|c| matches!(c, 'I' | 'V' | 'X' | 'L' | 'C' | 'D' | 'M'))
}

/// Which tense a bullet's first word is in, when it is clear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tense {
    Past,
    Present,
}

/// Irregular past forms of verbs that open CV bullets.
const IRREGULAR_PAST: &[&str] = &[
    "led",
    "built",
    "ran",
    "made",
    "wrote",
    "grew",
    "won",
    "drove",
    "cut",
    "set",
    "began",
    "brought",
    "taught",
    "sold",
    "bought",
    "met",
    "held",
    "kept",
    "took",
    "gave",
    "saw",
    "spoke",
    "rebuilt",
    "rewrote",
    "oversaw",
    "undertook",
    "spun",
    "chose",
    "found",
    "sped",
];

/// Present-tense forms of verbs that open CV bullets, base and third person.
const PRESENT_VERBS: &[&str] = &[
    "lead",
    "build",
    "run",
    "manage",
    "develop",
    "design",
    "maintain",
    "own",
    "coordinate",
    "support",
    "oversee",
    "drive",
    "create",
    "implement",
    "deliver",
    "write",
    "work",
    "handle",
    "mentor",
    "plan",
    "organise",
    "organize",
    "monitor",
    "train",
    "analyse",
    "analyze",
    "teach",
    "sell",
    "operate",
    "supervise",
    "direct",
    "prepare",
    "review",
    "test",
];

/// The tense of the bullet's first word, or `None` when it is not a verb
/// these lists know.
pub fn opening_tense(bullet: &str) -> Option<Tense> {
    let first = words(bullet).next()?.to_lowercase();
    if IRREGULAR_PAST.contains(&first.as_str())
        || (first.len() > 4 && first.ends_with("ed") && !first.ends_with("eed"))
    {
        return Some(Tense::Past);
    }
    let base = first
        .strip_suffix("es")
        .filter(|b| PRESENT_VERBS.contains(b))
        .or_else(|| first.strip_suffix('s'))
        .unwrap_or(&first);
    PRESENT_VERBS.contains(&base).then_some(Tense::Present)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentences_split_on_end_marks_and_lines_only() {
        assert_eq!(
            sentences("Cut costs 3.5%. Led a team! Why?\nNext line"),
            ["Cut costs 3.5%.", "Led a team!", "Why?", "Next line"]
        );
        assert_eq!(sentences("  "), Vec::<&str>::new());
        assert_eq!(word_count("Built a C++ / Rust service, then e-mail."), 7);
    }

    #[test]
    fn passives_are_found_and_actives_are_not() {
        for p in [
            "The API was rebuilt by the team",
            "Costs were reduced by 20%",
            "Has been migrated to Rust",
            "Was given the award",
        ] {
            assert!(is_passive(p), "{p}");
        }
        for a in [
            "Rebuilt the API",
            "Was interested in data",
            "The team was small",
            "Reduced costs by 20%",
        ] {
            assert!(!is_passive(a), "{a}");
        }
    }

    #[test]
    fn first_person_words_are_found() {
        assert_eq!(first_person("I built the service"), Some("I"));
        assert_eq!(first_person("Led my team of 5"), Some("my"));
        assert_eq!(first_person("I’m proud"), Some("I’m"));
        assert_eq!(first_person("Wrote docs, i.e. guides"), None);
        assert_eq!(first_person("Migrated memory caches"), None);
    }

    #[test]
    fn cliches_match_whole_phrases() {
        assert_eq!(cliche("A results-driven engineer"), Some("results-driven"));
        assert_eq!(cliche("Strong Team Player."), Some("team player"));
        assert_eq!(cliche("Built team players' tools"), None);
        assert_eq!(cliche("Shipped synergy-free code"), Some("synergy"));
        assert_eq!(cliche("Cut latency by 40%"), None);
    }

    #[test]
    fn acronyms_need_capitals_and_skip_known_ones() {
        assert!(is_unknown_acronym("SLO"));
        assert!(is_unknown_acronym("K8S"));
        assert!(!is_unknown_acronym("SQL"));
        assert!(!is_unknown_acronym("AI"), "too short");
        assert!(!is_unknown_acronym("Rust"));
        assert!(!is_unknown_acronym("III"), "a roman numeral");
        assert!(!is_unknown_acronym("8KB"));
    }

    #[test]
    fn opening_tense_reads_the_first_verb() {
        assert_eq!(opening_tense("Led a team of 5"), Some(Tense::Past));
        assert_eq!(opening_tense("Migrated billing"), Some(Tense::Past));
        assert_eq!(opening_tense("Manage a team"), Some(Tense::Present));
        assert_eq!(opening_tense("Manages a team"), Some(Tense::Present));
        assert_eq!(opening_tense("Oversees hiring"), Some(Tense::Present));
        assert_eq!(opening_tense("Teaches juniors"), Some(Tense::Present));
        assert_eq!(opening_tense("Speed matters"), None);
        assert_eq!(opening_tense("Kubernetes migration"), None);
    }
}
