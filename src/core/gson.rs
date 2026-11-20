//! Gson-style readers over `serde_json` values. Gson reads a number written as a string and a
//! string written as a number, so the pack, config and progress files are read the same way here.
//! A JSON `null` counts as a missing key everywhere.
use mod_api::serde_json::{Map, Value};

/// A JSON object, as Gson's `JsonObject`.
pub(crate) type JsonObject = Map<String, Value>;

/// The value under `name`, or `None` when the key is missing or `null`.
pub(crate) fn field<'a>(object: &'a JsonObject, name: &str) -> Option<&'a Value> {
    object.get(name).filter(|value| !value.is_null())
}

/// `getAsString()`: strings, numbers and booleans.
pub(crate) fn as_string(value: &Value) -> Result<String, String> {
    match value {
        Value::String(text) => Ok(text.clone()),
        Value::Number(number) => Ok(number.to_string()),
        Value::Bool(flag) => Ok(flag.to_string()),
        other => Err(format!("{other} is not a string")),
    }
}

/// `getAsBoolean()`: a boolean, or `Boolean.parseBoolean` of a string.
pub(crate) fn as_bool(value: &Value) -> Result<bool, String> {
    match value {
        Value::Bool(flag) => Ok(*flag),
        Value::String(text) => Ok(text.eq_ignore_ascii_case("true")),
        other => Err(format!("{other} is not a boolean")),
    }
}

/// `getAsLong()`: a fractional number is truncated, as `BigDecimal.longValue()` does.
pub(crate) fn as_i64(value: &Value) -> Result<i64, String> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|n| n as i64))
            .ok_or_else(|| format!("{number} is not a whole number")),
        Value::String(text) => text
            .trim()
            .parse()
            .map_err(|_| format!("'{text}' is not a whole number")),
        other => Err(format!("{other} is not a whole number")),
    }
}

/// `getAsInt()`.
pub(crate) fn as_i32(value: &Value) -> Result<i32, String> {
    as_i64(value).map(|n| n as i32)
}

/// `getAsFloat()`.
pub(crate) fn as_f32(value: &Value) -> Result<f32, String> {
    match value {
        Value::Number(number) => number
            .as_f64()
            .map(|n| n as f32)
            .ok_or_else(|| format!("{number} is not a number")),
        Value::String(text) => text
            .trim()
            .parse()
            .map_err(|_| format!("'{text}' is not a number")),
        other => Err(format!("{other} is not a number")),
    }
}

/// A float written the way Gson writes it: the shortest decimal that reads back as the same
/// `f32`, so `0.6` stays `0.6` and not `0.6000000238418579`. NaN and infinity become `null`.
pub(crate) fn float_value(number: f32) -> Value {
    number
        .to_string()
        .parse::<f64>()
        .map(Value::from)
        .unwrap_or(Value::Null)
}

/// An object whose values all read with `read`, such as `{"<id>": 3}`.
pub(crate) fn map_of<T>(
    value: &Value,
    read: fn(&Value) -> Result<T, String>,
) -> Result<Vec<(String, T)>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{value} is not an object"))?
        .iter()
        .filter(|(_, item)| !item.is_null())
        .map(|(key, item)| -> Result<(String, T), String> { Ok((key.clone(), read(item)?)) })
        .collect()
}
