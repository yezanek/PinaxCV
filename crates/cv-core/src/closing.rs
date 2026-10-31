//! The optional closing block (ADR-089, issue #143): a data-protection
//! consent clause, as Italian and Polish employers expect, and the "place,
//! date" line that ends a German or Austrian Lebenslauf.
//!
//! Each part has its own show switch, off by default:
//! [`Closing::show`] for the base CV and
//! [`Variant::show_closing`](crate::Variant::show_closing) for each variant,
//! the same pattern as the photo (ADR-083).

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The closing block's content and the base CV's switches. Empty by default.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Closing {
    /// Consent clause, plain text, for example the Italian GDPR clause.
    pub consent: Option<String>,
    /// The place in the "place, date" line, for example "Berlin". The date
    /// is the export date.
    pub place: Option<String>,
    /// The parts the base CV prints.
    pub show: BTreeSet<ClosingPart>,
}

/// One part of the closing block. Serialized in `snake_case`.
#[derive(
    Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[serde(rename_all = "snake_case")]
pub enum ClosingPart {
    /// The consent clause.
    Consent,
    /// The "place, date" line.
    PlaceDate,
    /// The signature image, stored beside the CV like the photo (ADR-090).
    Signature,
}

impl Closing {
    /// A copy that keeps only the parts in `shown`, with `show` set to
    /// `shown`. The place is dropped with the place and date line.
    pub fn only(&self, shown: &BTreeSet<ClosingPart>) -> Closing {
        Closing {
            consent: self
                .consent
                .clone()
                .filter(|_| shown.contains(&ClosingPart::Consent)),
            place: self
                .place
                .clone()
                .filter(|_| shown.contains(&ClosingPart::PlaceDate)),
            show: shown.clone(),
        }
    }

    /// The consent clause as printed: trimmed, `None` when blank.
    pub fn consent_text(&self) -> Option<&str> {
        self.consent
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
    }

    /// The place as printed: trimmed, `None` when blank.
    pub fn place_text(&self) -> Option<&str> {
        self.place
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
    }
}

/// A consent clause the editor can insert, in the language of the country
/// that expects it. The CV's own language is English only for now, so the
/// user picks the wording.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsentPreset {
    /// ISO 639-1 code of the wording.
    pub lang: &'static str,
    pub text: &'static str,
}

/// English, Italian and Polish consent clauses under the GDPR (Regulation
/// (EU) 2016/679); the Italian one also names the Italian privacy code.
pub const CONSENT_PRESETS: [ConsentPreset; 3] = [
    ConsentPreset {
        lang: "en",
        text: "I consent to the processing of the personal data in this CV for the purposes \
               of recruitment, in accordance with Regulation (EU) 2016/679 (GDPR).",
    },
    ConsentPreset {
        lang: "it",
        text: "Autorizzo il trattamento dei miei dati personali presenti nel CV ai sensi del \
               Regolamento UE 2016/679 (GDPR) e del D.Lgs. 196/2003.",
    },
    ConsentPreset {
        lang: "pl",
        text: "Wyrażam zgodę na przetwarzanie moich danych osobowych dla potrzeb niezbędnych \
               do realizacji procesu rekrutacji, zgodnie z Rozporządzeniem Parlamentu \
               Europejskiego i Rady (UE) 2016/679 z dnia 27 kwietnia 2016 r. (RODO).",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Closing {
        Closing {
            consent: Some(" I consent. ".into()),
            place: Some("Berlin".into()),
            show: BTreeSet::new(),
        }
    }

    #[test]
    fn only_keeps_the_shown_parts() {
        let none = sample().only(&BTreeSet::new());
        assert_eq!(none, Closing::default());
        let consent = sample().only(&[ClosingPart::Consent].into());
        assert_eq!(consent.consent_text(), Some("I consent."));
        assert_eq!(consent.place, None);
        let line = sample().only(&[ClosingPart::PlaceDate].into());
        assert_eq!(line.consent, None);
        assert_eq!(line.place_text(), Some("Berlin"));
        assert!(line.show.contains(&ClosingPart::PlaceDate));
    }

    #[test]
    fn blank_text_prints_nothing() {
        let c = Closing {
            consent: Some("  ".into()),
            place: Some(String::new()),
            show: BTreeSet::new(),
        };
        assert_eq!(c.consent_text(), None);
        assert_eq!(c.place_text(), None);
    }

    #[test]
    fn presets_cover_english_italian_and_polish() {
        let langs: Vec<&str> = CONSENT_PRESETS.iter().map(|p| p.lang).collect();
        assert_eq!(langs, ["en", "it", "pl"]);
        assert!(CONSENT_PRESETS.iter().all(|p| p.text.contains("2016/679")));
        assert!(CONSENT_PRESETS.iter().all(|p| !p.text.contains("  ")));
    }

    #[test]
    fn missing_fields_deserialize_as_empty() {
        let c: Closing = serde_json::from_str("{}").expect("empty object");
        assert_eq!(c, Closing::default());
        let p: ClosingPart = serde_json::from_str("\"place_date\"").expect("part");
        assert_eq!(p, ClosingPart::PlaceDate);
    }
}
