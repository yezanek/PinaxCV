//! Output schemas for providers' structured output, from the Rust types.
//!
//! OpenAI's strict mode wants every object closed (`additionalProperties:
//! false`) with every property required (optional ones are nullable
//! instead), and rejects `format` values such as `uint8`. The same schema
//! is fine for Anthropic and Gemini.

use schemars::JsonSchema;
use serde_json::Value;

/// The JSON Schema of `T`, adjusted for strict structured output.
pub fn output_schema<T: JsonSchema>() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(T)).unwrap_or(Value::Null);
    if let Value::Object(map) = &mut schema {
        map.remove("$schema");
    }
    strict(&mut schema);
    schema
}

/// Adjusts one schema and every schema inside it. `properties` and `$defs`
/// map names to schemas, so their own keys are left alone.
fn strict(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("format");
            // A `default` is meaningless for output and strict mode rejects it.
            map.remove("default");
            if map.get("type").and_then(Value::as_str) == Some("object")
                && let Some(Value::Object(props)) = map.get("properties")
            {
                let required: Vec<Value> = props.keys().cloned().map(Value::String).collect();
                map.insert("required".into(), Value::Array(required));
                map.insert("additionalProperties".into(), Value::Bool(false));
            }
            for (key, child) in map.iter_mut() {
                match (key.as_str(), child) {
                    ("properties" | "$defs", Value::Object(named)) => {
                        named.values_mut().for_each(strict);
                    }
                    (_, child) => strict(child),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(strict),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cv_core::JobRequirements;

    #[test]
    fn objects_are_closed_and_fully_required() {
        let s = output_schema::<JobRequirements>();
        assert_eq!(s["additionalProperties"], false);
        let required: Vec<&str> = s["required"]
            .as_array()
            .expect("required")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(required.contains(&"title") && required.contains(&"must_have"));
        let text = s.to_string();
        assert!(!text.contains("\"format\"") && !text.contains("$schema"));
        // Nested definitions are closed too.
        let req = &s["$defs"]["Requirement"];
        assert_eq!(req["additionalProperties"], false);
    }
}
