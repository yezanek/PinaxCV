//! Stable entity identifiers (spec section 4.1).

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

/// Length of a ULID in its canonical Crockford base32 text form.
const ULID_LEN: usize = 26;

/// A stable entity identifier holding a ULID in its canonical text form.
///
/// IDs are assigned on creation and import and never derived from content.
/// `cv-core` does not generate IDs itself (it has no randomness or clock);
/// callers pass them in through an [`IdSource`]. Deserialization validates the
/// ULID, so an `Id` value is always well-formed.
#[derive(Serialize, JsonSchema, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct Id(String);

/// Error returned when a string is not a valid ULID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidId(pub String);

impl fmt::Display for InvalidId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a valid ULID: {:?}", self.0)
    }
}

impl std::error::Error for InvalidId {}

impl Id {
    /// Parses a ULID string, accepting lower-case input and normalizing it to upper case.
    pub fn parse(value: &str) -> Result<Self, InvalidId> {
        let upper = value.to_ascii_uppercase();
        if is_ulid(&upper) {
            Ok(Self(upper))
        } else {
            Err(InvalidId(value.to_owned()))
        }
    }

    /// Returns the ID's text form.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Id::parse(&raw).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Supplies fresh IDs to `cv-core` operations that create entities.
///
/// The desktop backend implements this with the `ulid` crate; tests use a
/// deterministic sequence.
pub trait IdSource {
    /// Returns a new, never-before-returned ID.
    fn next_id(&mut self) -> Id;
}

/// Canonical ULID: 26 Crockford base32 characters, first character at most `7`.
fn is_ulid(value: &str) -> bool {
    const CROCKFORD: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    value.len() == ULID_LEN
        && value.as_bytes()[0] <= b'7'
        && value.bytes().all(|b| CROCKFORD.contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = "01HZX3M8Q4V7K2N9P5R6T0W1YB";

    #[test]
    fn parse_accepts_canonical_ulid() {
        let id = Id::parse(VALID).expect("valid ULID");
        assert_eq!(id.as_str(), VALID);
    }

    #[test]
    fn parse_normalizes_lower_case() {
        let id = Id::parse(&VALID.to_ascii_lowercase()).expect("valid ULID");
        assert_eq!(id.as_str(), VALID);
    }

    #[test]
    fn parse_rejects_wrong_length_and_alphabet() {
        assert!(Id::parse("").is_err());
        assert!(Id::parse(&VALID[..25]).is_err());
        // I, L, O and U are not in the Crockford alphabet.
        assert!(Id::parse("01HZX3M8Q4V7K2N9P5R6T0W1YI").is_err());
        // The first character encodes the top bits of the timestamp and cannot exceed 7.
        assert!(Id::parse("81HZX3M8Q4V7K2N9P5R6T0W1YB").is_err());
    }

    #[test]
    fn display_and_serde_use_the_plain_string() {
        let id = Id::parse(VALID).expect("valid ULID");
        assert_eq!(id.to_string(), VALID);
        let json = serde_json::to_string(&id).expect("serialize");
        assert_eq!(json, format!("\"{VALID}\""));
        let back: Id = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, id);
    }

    #[test]
    fn deserialize_rejects_invalid_ulid() {
        assert!(serde_json::from_str::<Id>("\"not-a-ulid\"").is_err());
    }

    #[test]
    fn invalid_id_display_names_the_input() {
        let err = Id::parse("nope").expect_err("invalid");
        assert_eq!(err.to_string(), "not a valid ULID: \"nope\"");
    }
}
