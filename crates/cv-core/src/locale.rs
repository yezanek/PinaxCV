//! Locale tables: default section headings and date formatting (spec section 6.3).
//!
//! v1 ships English only; other locales are added here with their tables.

use crate::{DateStyle, Locale, PartialDate, Period, ResolvedSection};

/// Fixed labels the template prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Labels {
    pub experience: &'static str,
    pub education: &'static str,
    pub projects: &'static str,
    pub skills: &'static str,
    pub languages: &'static str,
    pub certifications: &'static str,
    pub summary: &'static str,
    pub present: &'static str,
}

const EN: Labels = Labels {
    experience: "Work Experience",
    education: "Education",
    projects: "Projects",
    skills: "Skills",
    languages: "Languages",
    certifications: "Certifications",
    summary: "Summary",
    present: "Present",
};

const EN_MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Returns the labels for `locale`.
pub fn labels(locale: Locale) -> Labels {
    match locale {
        Locale::En => EN,
    }
}

/// Returns the heading a section prints: its own heading (custom title or
/// `title_override`) if set and not blank, else the locale's default. Custom
/// sections without a title get an empty heading.
pub fn heading(section: &ResolvedSection, locale: Locale) -> String {
    let l = labels(locale);
    let (own, default) = match section {
        ResolvedSection::Experience(s) => (&s.heading, l.experience),
        ResolvedSection::Education(s) => (&s.heading, l.education),
        ResolvedSection::Projects(s) => (&s.heading, l.projects),
        ResolvedSection::Certifications(s) => (&s.heading, l.certifications),
        ResolvedSection::Custom(s) => (&s.heading, ""),
        ResolvedSection::Skills(s) => (&s.heading, l.skills),
        ResolvedSection::Languages(s) => (&s.heading, l.languages),
    };
    match own.as_deref().map(str::trim) {
        Some(h) if !h.is_empty() => h.to_owned(),
        _ => default.to_owned(),
    }
}

/// Formats one date in `style`. A missing month prints the year only.
pub fn format_date(date: PartialDate, style: DateStyle, locale: Locale) -> String {
    let month = date.month.filter(|m| (1..=12).contains(m));
    match (style, month) {
        (DateStyle::YearOnly, _) | (_, None) => date.year.to_string(),
        (DateStyle::MonthNameYear, Some(m)) => {
            let names = match locale {
                Locale::En => EN_MONTHS,
            };
            format!("{} {}", names[usize::from(m - 1)], date.year)
        }
        (DateStyle::NumericMonthYear, Some(m)) => format!("{m:02}/{}", date.year),
    }
}

/// Formats a period as "Mar 2021 – Present", "2019 – 2021" or a single date.
/// Returns `None` when there is nothing to print.
pub fn format_period(period: &Period, style: DateStyle, locale: Locale) -> Option<String> {
    let start = period.start.map(|d| format_date(d, style, locale));
    let end = if period.current {
        Some(labels(locale).present.to_owned())
    } else {
        period.end.map(|d| format_date(d, style, locale))
    };
    match (start, end) {
        (Some(s), Some(e)) if s == e => Some(s),
        (Some(s), Some(e)) => Some(format!("{s} – {e}")),
        (Some(s), None) => Some(s),
        (None, Some(e)) => Some(e),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_ids::id;
    use crate::{ResolvedEntrySection, ResolvedSkillsSection};

    fn d(year: u16, month: Option<u8>) -> PartialDate {
        PartialDate { year, month }
    }

    #[test]
    fn english_labels() {
        let l = labels(Locale::En);
        assert_eq!(l.experience, "Work Experience");
        assert_eq!(l.present, "Present");
    }

    #[test]
    fn headings_prefer_override_then_default() {
        let exp = |heading: Option<&str>| {
            ResolvedSection::Experience(ResolvedEntrySection {
                id: id(1),
                heading: heading.map(str::to_owned),
                entries: vec![],
            })
        };
        assert_eq!(heading(&exp(None), Locale::En), "Work Experience");
        assert_eq!(heading(&exp(Some("  ")), Locale::En), "Work Experience");
        assert_eq!(heading(&exp(Some("Career")), Locale::En), "Career");
        let skills = ResolvedSection::Skills(ResolvedSkillsSection {
            id: id(2),
            heading: None,
            groups: vec![],
        });
        assert_eq!(heading(&skills, Locale::En), "Skills");
        let custom = ResolvedSection::Custom(ResolvedEntrySection {
            id: id(3),
            heading: Some("Volunteering".into()),
            entries: vec![],
        });
        assert_eq!(heading(&custom, Locale::En), "Volunteering");
    }

    #[test]
    fn date_styles() {
        let en = Locale::En;
        assert_eq!(
            format_date(d(2021, Some(3)), DateStyle::MonthNameYear, en),
            "Mar 2021"
        );
        assert_eq!(
            format_date(d(2021, Some(3)), DateStyle::NumericMonthYear, en),
            "03/2021"
        );
        assert_eq!(
            format_date(d(2021, Some(3)), DateStyle::YearOnly, en),
            "2021"
        );
        assert_eq!(
            format_date(d(2021, None), DateStyle::MonthNameYear, en),
            "2021"
        );
        assert_eq!(
            format_date(d(2021, Some(13)), DateStyle::MonthNameYear, en),
            "2021"
        );
    }

    #[test]
    fn periods() {
        let p = |start, end, current| Period {
            start,
            end,
            current,
        };
        let s = DateStyle::MonthNameYear;
        let en = Locale::En;
        assert_eq!(
            format_period(&p(Some(d(2021, Some(3))), None, true), s, en).as_deref(),
            Some("Mar 2021 – Present")
        );
        assert_eq!(
            format_period(&p(Some(d(2019, None)), Some(d(2021, None)), false), s, en).as_deref(),
            Some("2019 – 2021")
        );
        assert_eq!(
            format_period(&p(Some(d(2020, None)), Some(d(2020, None)), false), s, en).as_deref(),
            Some("2020")
        );
        assert_eq!(
            format_period(&p(None, Some(d(2020, Some(1))), false), s, en).as_deref(),
            Some("Jan 2020")
        );
        assert_eq!(
            format_period(&p(None, Some(d(2020, None)), true), s, en).as_deref(),
            Some("Present")
        );
        assert_eq!(format_period(&p(None, None, false), s, en), None);
    }
}
