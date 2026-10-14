//! Upgrades stored CV and variant JSON to the current schema (spec section 5.3).
//!
//! One function per schema bump. v1 is the first released schema; the v0
//! step exists to prove the migration path with a synthetic older shape in
//! which the name was `basics.name` and `schema_version` was absent.

use cv_core::{CV_SCHEMA_VERSION, Cv, VARIANT_SCHEMA_VERSION, Variant};
use serde_json::Value;

use crate::{Result, StoreError};

/// Upgrades `value` from any older schema version and parses it.
///
/// Fails with [`StoreError::NewerSchema`] for content written by a newer
/// PinaxCV and with [`StoreError::Invalid`] for content that does not parse.
pub fn migrate_cv_json(mut value: Value) -> Result<Cv> {
    let mut version = schema_version(&value)?;
    if version > CV_SCHEMA_VERSION {
        return Err(StoreError::NewerSchema {
            found: version,
            supported: CV_SCHEMA_VERSION,
        });
    }
    while version < CV_SCHEMA_VERSION {
        match version {
            0 => cv_v0_to_v1(&mut value)?,
            _ => return Err(invalid(&format!("no migration from CV schema {version}"))),
        }
        version += 1;
    }
    serde_json::from_value(value).map_err(|e| invalid(&format!("CV does not parse: {e}")))
}

/// Upgrades `value` from any older variant schema version and parses it.
pub fn migrate_variant_json(value: Value) -> Result<Variant> {
    let version = schema_version(&value)?;
    if version > VARIANT_SCHEMA_VERSION {
        return Err(StoreError::NewerSchema {
            found: version,
            supported: VARIANT_SCHEMA_VERSION,
        });
    }
    if version < VARIANT_SCHEMA_VERSION {
        return Err(invalid(&format!(
            "no migration from variant schema {version}"
        )));
    }
    serde_json::from_value(value).map_err(|e| invalid(&format!("variant does not parse: {e}")))
}

/// Reads `schema_version`; a missing field means version 0.
fn schema_version(value: &Value) -> Result<u32> {
    match value.get("schema_version") {
        None => Ok(0),
        Some(v) => v
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| invalid("schema_version is not a number")),
    }
}

/// v0 kept the name in `basics.name` and had no `schema_version`.
fn cv_v0_to_v1(value: &mut Value) -> Result<()> {
    let obj = value
        .as_object_mut()
        .ok_or_else(|| invalid("CV is not a JSON object"))?;
    let basics = obj
        .get_mut("basics")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| invalid("CV has no basics"))?;
    let name = basics
        .remove("name")
        .unwrap_or(Value::String(String::new()));
    basics.insert("full_name".to_owned(), name);
    obj.insert("schema_version".to_owned(), Value::from(1));
    Ok(())
}

fn invalid(message: &str) -> StoreError {
    StoreError::Invalid(message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const V0: &str = include_str!("../tests/fixtures/cv_v0.json");

    fn v1_value() -> Value {
        let mut v: Value = serde_json::from_str(V0).expect("fixture");
        cv_v0_to_v1(&mut v).expect("migrates");
        v
    }

    #[test]
    fn v0_fixture_migrates_to_current_cv() {
        let cv = migrate_cv_json(serde_json::from_str(V0).expect("fixture")).expect("migrate");
        assert_eq!(cv.schema_version, CV_SCHEMA_VERSION);
        assert_eq!(cv.basics.full_name, "Jane Doe");
        assert_eq!(cv.title, "Main CV");
        assert_eq!(cv.sections.len(), 1);
    }

    #[test]
    fn current_cv_parses_without_changes() {
        let value = v1_value();
        let cv = migrate_cv_json(value.clone()).expect("parse");
        let direct: Cv = serde_json::from_value(value).expect("direct parse");
        assert_eq!(cv, direct);
    }

    #[test]
    fn newer_schema_is_refused() {
        let mut value = v1_value();
        value["schema_version"] = Value::from(CV_SCHEMA_VERSION + 1);
        assert!(matches!(
            migrate_cv_json(value),
            Err(StoreError::NewerSchema {
                found: 2,
                supported: 1
            })
        ));
    }

    #[test]
    fn broken_content_is_invalid() {
        assert!(matches!(
            migrate_cv_json(Value::from("not a cv")),
            Err(StoreError::Invalid(_))
        ));
        let mut value = v1_value();
        value["schema_version"] = Value::from("one");
        assert!(matches!(
            migrate_cv_json(value),
            Err(StoreError::Invalid(_))
        ));
        let mut value = v1_value();
        value["sections"] = Value::from(3);
        assert!(matches!(
            migrate_cv_json(value),
            Err(StoreError::Invalid(_))
        ));
    }

    #[test]
    fn variants_parse_and_newer_or_older_are_refused() {
        let v = Variant::empty(
            cv_core::Id::parse("01HZX3M8Q4V7K2N9P5R0000001").expect("id"),
            cv_core::Id::parse("01HZX3M8Q4V7K2N9P5R0000002").expect("id"),
            "Acme",
        );
        let value = serde_json::to_value(&v).expect("serialize");
        assert_eq!(migrate_variant_json(value.clone()).expect("parse"), v);

        let mut newer = value.clone();
        newer["schema_version"] = Value::from(VARIANT_SCHEMA_VERSION + 1);
        assert!(matches!(
            migrate_variant_json(newer),
            Err(StoreError::NewerSchema { .. })
        ));
        let mut older = value;
        older["schema_version"] = Value::from(0);
        assert!(matches!(
            migrate_variant_json(older),
            Err(StoreError::Invalid(_))
        ));
    }
}
