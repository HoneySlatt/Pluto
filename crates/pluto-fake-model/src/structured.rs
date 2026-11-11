//! JSON that fits a JSON Schema, for the side requests that ask for structured output (a thread's
//! title as JSON).
//!
//! [`example`] walks the schema and builds the plainest value that fits it, every string being
//! the text given:
//! - `const` gives its value, `enum` its first value;
//! - `anyOf` and `oneOf` give their first branch;
//! - an `object` gives every property of `properties`, in order, each built the same way;
//! - a `string` gives the text; a `boolean`, `false`; an `integer` or a `number`, the integer `0`;
//!   an `array`, `[]`; `null`, `null`;
//! - a `type` that is an array gives its first type that is not `"null"`;
//! - a schema with none of these (`{}`, `true`, or one this module does not read, such as `$ref`)
//!   gives the text.
//!
//! Limits: `required`, bounds, patterns and formats are not read, so a schema that demands a
//! longer string or a non-empty array is not met; `allOf` is not merged, and `anyOf` or `oneOf`
//! beside `properties` gives its first branch alone. The title schemas of the bench's subjects
//! (Pluto's server, T3 Code) need none of these.

use serde_json::{Map, Value};

/// The value that fits `schema`, every string being `text`.
pub fn example(schema: &Value, text: &str) -> Value {
    let Some(fields) = schema.as_object() else {
        return Value::String(text.to_owned());
    };
    if let Some(constant) = fields.get("const") {
        return constant.clone();
    }
    if let Some(first) = fields
        .get("enum")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
    {
        return first.clone();
    }
    if let Some(branch) = ["anyOf", "oneOf"]
        .iter()
        .find_map(|key| fields.get(*key)?.as_array()?.first())
    {
        return example(branch, text);
    }
    match type_name(schema) {
        Some("object") => Value::Object(properties(fields, text)),
        Some("string") => Value::String(text.to_owned()),
        Some("boolean") => Value::Bool(false),
        Some("integer" | "number") => Value::from(0),
        Some("array") => Value::Array(Vec::new()),
        Some("null") => Value::Null,
        _ => Value::String(text.to_owned()),
    }
}

/// A schema's type: its `type`, or the first of a `type` array that is not `"null"` (`"null"` if
/// all are); a schema with `properties` and no `type` is an object.
pub(crate) fn type_name(schema: &Value) -> Option<&str> {
    match &schema["type"] {
        Value::String(name) => Some(name),
        Value::Array(names) => {
            let mut names = names.iter().filter_map(Value::as_str);
            names.clone().find(|name| *name != "null").or(names.next())
        }
        _ if schema["properties"].is_object() => Some("object"),
        _ => None,
    }
}

fn properties(fields: &Map<String, Value>, text: &str) -> Map<String, Value> {
    fields
        .get("properties")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(name, schema)| (name.clone(), example(schema, text)))
        .collect()
}
