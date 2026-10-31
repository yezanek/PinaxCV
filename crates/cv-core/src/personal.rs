//! Optional personal details that CVs carry in much of Europe (ADR-088,
//! issue #142): nationality, date and place of birth, gender, marital status,
//! postal address, driving licence and right to work.
//!
//! Every field is empty by default and prints only when it is switched on:
//! [`Basics::show_personal`](crate::Basics::show_personal) for the base CV and
//! [`Variant::show_personal`](crate::Variant::show_personal) for each variant,
//! the same pattern as the profile photo (ADR-083).

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{DateStyle, Locale, locale};

/// A calendar date with an optional month and day, as in ISO 8601
/// (`YYYY`, `YYYY-MM` or `YYYY-MM-DD`). A day needs a month.
#[derive(
    Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord,
)]
pub struct CalendarDate {
    pub year: u16,
    pub month: Option<u8>,
    pub day: Option<u8>,
}

impl CalendarDate {
    /// Parses `YYYY`, `YYYY-MM` or `YYYY-MM-DD` (surrounding spaces allowed).
    /// Returns `None` for anything else, including impossible dates such as
    /// 2023-02-29 and years before 1900.
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.trim().split('-');
        let year = digits(parts.next()?, 4)?;
        let month = match parts.next() {
            Some(m) => Some(digits(m, 2)?),
            None => None,
        };
        let day = match parts.next() {
            Some(d) => Some(digits(d, 2)?),
            None => None,
        };
        if parts.next().is_some() {
            return None;
        }
        let date = Self {
            year: u16::try_from(year).ok()?,
            month: month.map(u8::try_from).transpose().ok()?,
            day: day.map(u8::try_from).transpose().ok()?,
        };
        (date.year >= 1900 && date.is_valid()).then_some(date)
    }

    /// Whether the month is 1..=12 and the day exists in that month. A day
    /// without a month is invalid.
    pub fn is_valid(&self) -> bool {
        match (self.month, self.day) {
            (None, None) => true,
            (None, Some(_)) => false,
            (Some(m), None) => (1..=12).contains(&m),
            (Some(m), Some(d)) => {
                (1..=12).contains(&m) && d >= 1 && d <= days_in_month(self.year, m)
            }
        }
    }

    /// The ISO 8601 form: `1990`, `1990-03` or `1990-03-12`.
    pub fn iso(&self) -> String {
        match (self.month, self.day) {
            (Some(m), Some(d)) => format!("{:04}-{m:02}-{d:02}", self.year),
            (Some(m), None) => format!("{:04}-{m:02}", self.year),
            _ => format!("{:04}", self.year),
        }
    }
}

fn digits(s: &str, len: usize) -> Option<u32> {
    (s.len() == len && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// The optional personal details. All empty by default.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct PersonalDetails {
    /// One or more, for example "Italian" and "British".
    pub nationality: Vec<String>,
    pub birth_date: Option<CalendarDate>,
    pub birth_place: Option<String>,
    pub gender: Option<String>,
    pub marital_status: Option<String>,
    /// Postal address, one line per address line. Separate from
    /// `Basics::location` (city and country).
    pub address: Option<String>,
    /// Licence categories as in EU Directive 2006/126/EC, for example "B" and "C1".
    pub driving_licence: Vec<String>,
    /// Right to work or work permit, for example "EU citizen" or "Swiss C permit".
    pub work_permit: Option<String>,
}

/// One personal detail. Serialized in `snake_case`; [`PersonalField::ALL`]
/// is the order they print in.
#[derive(
    Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[serde(rename_all = "snake_case")]
pub enum PersonalField {
    Nationality,
    BirthDate,
    BirthPlace,
    Gender,
    MaritalStatus,
    Address,
    DrivingLicence,
    WorkPermit,
}

impl PersonalField {
    /// Every field, in print order.
    pub const ALL: [PersonalField; 8] = [
        PersonalField::Nationality,
        PersonalField::BirthDate,
        PersonalField::BirthPlace,
        PersonalField::Gender,
        PersonalField::MaritalStatus,
        PersonalField::Address,
        PersonalField::DrivingLicence,
        PersonalField::WorkPermit,
    ];
}

impl PersonalDetails {
    /// Whether `field` holds anything other than blank text.
    pub fn is_filled(&self, field: PersonalField) -> bool {
        match field {
            PersonalField::Nationality => filled_list(&self.nationality),
            PersonalField::BirthDate => self.birth_date.is_some(),
            PersonalField::BirthPlace => filled(&self.birth_place),
            PersonalField::Gender => filled(&self.gender),
            PersonalField::MaritalStatus => filled(&self.marital_status),
            PersonalField::Address => filled(&self.address),
            PersonalField::DrivingLicence => filled_list(&self.driving_licence),
            PersonalField::WorkPermit => filled(&self.work_permit),
        }
    }

    /// Empties `field`.
    pub fn clear(&mut self, field: PersonalField) {
        match field {
            PersonalField::Nationality => self.nationality.clear(),
            PersonalField::BirthDate => self.birth_date = None,
            PersonalField::BirthPlace => self.birth_place = None,
            PersonalField::Gender => self.gender = None,
            PersonalField::MaritalStatus => self.marital_status = None,
            PersonalField::Address => self.address = None,
            PersonalField::DrivingLicence => self.driving_licence.clear(),
            PersonalField::WorkPermit => self.work_permit = None,
        }
    }

    /// A copy that keeps only the fields in `shown`.
    pub fn only(&self, shown: &BTreeSet<PersonalField>) -> PersonalDetails {
        let mut out = self.clone();
        for field in PersonalField::ALL {
            if !shown.contains(&field) {
                out.clear(field);
            }
        }
        out
    }

    /// The filled fields in print order, each as `(field, value)` with the
    /// value as printed: lists joined with ", ", the address on one line
    /// (its lines joined with ", ") and the birth date in `style`.
    pub fn lines(&self, style: DateStyle, locale: Locale) -> Vec<(PersonalField, String)> {
        PersonalField::ALL
            .into_iter()
            .filter_map(|field| self.value(field, style, locale).map(|v| (field, v)))
            .collect()
    }

    /// `field`'s printed value, or `None` when it is empty.
    pub fn value(&self, field: PersonalField, style: DateStyle, locale: Locale) -> Option<String> {
        let text = |s: &Option<String>| {
            s.as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
        };
        match field {
            PersonalField::Nationality => list(&self.nationality),
            PersonalField::BirthDate => self
                .birth_date
                .filter(CalendarDate::is_valid)
                .map(|d| locale::format_day(d, style, locale)),
            PersonalField::BirthPlace => text(&self.birth_place),
            PersonalField::Gender => text(&self.gender),
            PersonalField::MaritalStatus => text(&self.marital_status),
            PersonalField::Address => self.address.as_deref().and_then(|a| {
                let joined = a
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                (!joined.is_empty()).then_some(joined)
            }),
            PersonalField::DrivingLicence => list(&self.driving_licence),
            PersonalField::WorkPermit => text(&self.work_permit),
        }
    }
}

fn filled(s: &Option<String>) -> bool {
    s.as_deref().is_some_and(|t| !t.trim().is_empty())
}

fn filled_list(items: &[String]) -> bool {
    items.iter().any(|t| !t.trim().is_empty())
}

fn list(items: &[String]) -> Option<String> {
    let joined = items
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    (!joined.is_empty()).then_some(joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: u16, month: Option<u8>, day: Option<u8>) -> CalendarDate {
        CalendarDate { year, month, day }
    }

    fn sample() -> PersonalDetails {
        PersonalDetails {
            nationality: vec!["Italian".into(), " ".into(), "British".into()],
            birth_date: Some(date(1990, Some(3), Some(12))),
            birth_place: Some("Turin".into()),
            gender: Some("  ".into()),
            marital_status: None,
            address: Some("Via Esempio 1\n\n 10100 Torino \n".into()),
            driving_licence: vec!["B".into(), "C1".into()],
            work_permit: Some("EU citizen".into()),
        }
    }

    #[test]
    fn parse_accepts_iso_forms_and_rejects_the_rest() {
        assert_eq!(CalendarDate::parse("1990"), Some(date(1990, None, None)));
        assert_eq!(
            CalendarDate::parse(" 1990-03 "),
            Some(date(1990, Some(3), None))
        );
        assert_eq!(
            CalendarDate::parse("2000-02-29"),
            Some(date(2000, Some(2), Some(29)))
        );
        for bad in [
            "",
            "90",
            "1990-3",
            "1990-13",
            "1990-00",
            "2023-02-29",
            "1900-02-29",
            "1990-04-31",
            "1990-01-01-01",
            "1899",
            "1990-01-1x",
            "１９９０",
        ] {
            assert_eq!(CalendarDate::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn validity_and_iso() {
        assert!(!date(1990, None, Some(1)).is_valid());
        assert!(!date(1990, Some(0), None).is_valid());
        assert!(date(2024, Some(2), Some(29)).is_valid());
        assert_eq!(date(1990, Some(3), Some(2)).iso(), "1990-03-02");
        assert_eq!(date(1990, Some(3), None).iso(), "1990-03");
        assert_eq!(date(1990, None, None).iso(), "1990");
    }

    #[test]
    fn filled_ignores_blank_text() {
        let p = sample();
        assert!(p.is_filled(PersonalField::Nationality));
        assert!(!p.is_filled(PersonalField::Gender));
        assert!(!p.is_filled(PersonalField::MaritalStatus));
        assert!(!PersonalDetails::default().is_filled(PersonalField::DrivingLicence));
        let blank = PersonalDetails {
            nationality: vec![" ".into()],
            ..PersonalDetails::default()
        };
        assert!(!blank.is_filled(PersonalField::Nationality));
    }

    #[test]
    fn only_keeps_the_shown_fields() {
        let shown = BTreeSet::from([PersonalField::Nationality, PersonalField::WorkPermit]);
        let p = sample().only(&shown);
        assert_eq!(p.nationality.len(), 3);
        assert_eq!(p.work_permit.as_deref(), Some("EU citizen"));
        assert_eq!(p.birth_date, None);
        assert_eq!(p.address, None);
        assert!(p.driving_licence.is_empty());
        assert_eq!(sample().only(&BTreeSet::new()), PersonalDetails::default());
    }

    #[test]
    fn lines_print_filled_fields_in_order() {
        let lines = sample().lines(DateStyle::MonthNameYear, Locale::En);
        assert_eq!(
            lines,
            vec![
                (PersonalField::Nationality, "Italian, British".to_owned()),
                (PersonalField::BirthDate, "12 Mar 1990".to_owned()),
                (PersonalField::BirthPlace, "Turin".to_owned()),
                (
                    PersonalField::Address,
                    "Via Esempio 1, 10100 Torino".to_owned()
                ),
                (PersonalField::DrivingLicence, "B, C1".to_owned()),
                (PersonalField::WorkPermit, "EU citizen".to_owned()),
            ]
        );
        let invalid = PersonalDetails {
            birth_date: Some(date(1990, Some(2), Some(31))),
            ..PersonalDetails::default()
        };
        assert!(
            invalid
                .lines(DateStyle::MonthNameYear, Locale::En)
                .is_empty()
        );
    }

    #[test]
    fn missing_fields_deserialize_as_empty() {
        let p: PersonalDetails = serde_json::from_str("{}").expect("empty object");
        assert_eq!(p, PersonalDetails::default());
        let f: PersonalField = serde_json::from_str("\"driving_licence\"").expect("field");
        assert_eq!(f, PersonalField::DrivingLicence);
    }
}
