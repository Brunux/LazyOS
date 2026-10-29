//! Argument validation against a tool's input schema.
//!
//! The server checks every `tools/call` before the handler sees it, so a
//! handler can trust the shape of what it gets: the types, the enum values, the
//! lengths, the required names and - the one that matters for a security
//! boundary - **no argument the schema does not declare**. It implements the
//! subset of JSON Schema the table in [`crate::manifest`] uses and refuses a
//! schema keyword it does not know, so a schema can never ask for a check that
//! is silently skipped.

use serde_json::{Map, Value};

/// Why an argument object was refused, in a sentence the agent can act on.
pub type Invalid = String;

const KNOWN: &[&str] = &[
    "type",
    "properties",
    "required",
    "additionalProperties",
    "enum",
    "minLength",
    "maxLength",
    "minimum",
    "maximum",
    "description",
];

/// Check `args` against `schema` (an object schema from the manifest).
pub fn validate(schema: &Value, args: &Value) -> Result<(), Invalid> {
    check_keywords(schema, "the schema")?;
    let Some(object) = args.as_object() else {
        return Err("the arguments must be a JSON object".to_string());
    };
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for required in schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = required.as_str().unwrap_or_default();
        if !object.contains_key(name) {
            return Err(format!("missing required argument `{name}`"));
        }
    }
    let closed = schema.get("additionalProperties") == Some(&Value::Bool(false));
    for (name, value) in object {
        match properties.get(name) {
            Some(property) => check_value(name, property, value)?,
            None if closed => {
                let known: Vec<&str> = properties.keys().map(String::as_str).collect();
                return Err(if known.is_empty() {
                    format!("unknown argument `{name}`: this tool takes none")
                } else {
                    format!(
                        "unknown argument `{name}`: this tool takes {}",
                        known.join(", ")
                    )
                });
            }
            None => {}
        }
    }
    Ok(())
}

fn check_keywords(schema: &Value, what: &str) -> Result<(), Invalid> {
    let Some(object) = schema.as_object() else {
        return Err(format!("{what} is not an object"));
    };
    for key in object.keys() {
        if !KNOWN.contains(&key.as_str()) {
            return Err(format!(
                "{what} uses `{key}`, which this validator does not check"
            ));
        }
    }
    if let Some(properties) = object.get("properties").and_then(Value::as_object) {
        for (name, property) in properties {
            check_keywords(property, &format!("property `{name}`"))?;
        }
    }
    Ok(())
}

fn check_value(name: &str, property: &Value, value: &Value) -> Result<(), Invalid> {
    let wanted = property.get("type").and_then(Value::as_str).unwrap_or("");
    match wanted {
        "string" => {
            let Some(text) = value.as_str() else {
                return Err(format!("`{name}` must be a string"));
            };
            // Characters, not bytes: a limit of 80 is 80 letters of any script.
            let length = text.chars().count() as u64;
            if let Some(min) = property.get("minLength").and_then(Value::as_u64)
                && length < min
            {
                return Err(format!("`{name}` must be at least {min} characters"));
            }
            if let Some(max) = property.get("maxLength").and_then(Value::as_u64)
                && length > max
            {
                return Err(format!("`{name}` must be at most {max} characters"));
            }
            if text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
            {
                return Err(format!("`{name}` contains a control character"));
            }
            if let Some(allowed) = property.get("enum").and_then(Value::as_array)
                && !allowed.iter().any(|a| a.as_str() == Some(text))
            {
                let choices: Vec<&str> = allowed.iter().filter_map(Value::as_str).collect();
                return Err(format!("`{name}` must be one of {}", choices.join(", ")));
            }
        }
        "integer" => {
            let Some(number) = value.as_i64() else {
                return Err(format!("`{name}` must be an integer"));
            };
            if let Some(min) = property.get("minimum").and_then(Value::as_i64)
                && number < min
            {
                return Err(format!("`{name}` must be at least {min}"));
            }
            if let Some(max) = property.get("maximum").and_then(Value::as_i64)
                && number > max
            {
                return Err(format!("`{name}` must be at most {max}"));
            }
        }
        "boolean" => {
            if !value.is_boolean() {
                return Err(format!("`{name}` must be true or false"));
            }
        }
        other => {
            return Err(format!(
                "`{name}` has a schema type `{other}` this validator does not check"
            ));
        }
    }
    Ok(())
}

/// The arguments as an object, with an absent `arguments` meaning none.
pub fn arguments(params: &Map<String, Value>) -> Value {
    match params.get("arguments") {
        None | Some(Value::Null) => Value::Object(Map::new()),
        Some(value) => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{TOOLS, canonical_schema, find};
    use serde_json::json;

    fn schema(name: &str) -> Value {
        canonical_schema(find(name).unwrap())
    }

    #[test]
    fn every_schema_in_the_table_uses_only_what_is_checked() {
        for tool in TOOLS {
            check_keywords(&canonical_schema(tool), tool.name).unwrap();
        }
    }

    #[test]
    fn a_good_call_passes() {
        validate(&schema("power_propose"), &json!({"action": "reboot"})).unwrap();
        validate(&schema("desktop_status"), &json!({})).unwrap();
        validate(
            &schema("packages_installed"),
            &json!({"filter": "sway", "limit": 20}),
        )
        .unwrap();
        validate(
            &schema("notify_send"),
            &json!({"title": "Año nuevo", "body": "two\nlines"}),
        )
        .unwrap();
    }

    #[test]
    fn schema_validation_of_a_bad_call() {
        let bad = [
            (
                "power_propose",
                json!({}),
                "missing required argument `action`",
            ),
            (
                "power_propose",
                json!({"action": "format"}),
                "must be one of logout, reboot, poweroff",
            ),
            (
                "power_propose",
                json!({"action": "reboot", "now": true}),
                "unknown argument `now`",
            ),
            ("desktop_status", json!({"x": 1}), "this tool takes none"),
            (
                "packages_installed",
                json!({"limit": "ten"}),
                "must be an integer",
            ),
            ("packages_installed", json!({"limit": 0}), "at least 1"),
            (
                "packages_installed",
                json!({"limit": 99999}),
                "at most 2000",
            ),
            ("theme_set", json!({"name": 7}), "must be a string"),
            ("theme_set", json!({"name": ""}), "at least 1 characters"),
            (
                "theme_set",
                json!({"name": "x".repeat(41)}),
                "at most 40 characters",
            ),
            (
                "theme_set",
                json!({"name": "a\u{1b}[2J"}),
                "control character",
            ),
            ("view_open", json!(["/etc/passwd"]), "must be a JSON object"),
        ];
        for (tool, args, wanted) in bad {
            let error = validate(&schema(tool), &args).unwrap_err();
            assert!(
                error.contains(wanted),
                "{tool} {args}: {error:?} should say {wanted:?}"
            );
        }
    }

    #[test]
    fn lengths_count_characters_not_bytes() {
        // 40 accented letters are 80 bytes and still within maxLength 40.
        validate(&schema("theme_set"), &json!({"name": "é".repeat(40)})).unwrap();
    }

    #[test]
    fn an_unknown_keyword_is_refused_rather_than_skipped() {
        let schema = json!({"type":"object","properties":{"p":{"type":"string","pattern":"^a"}}});
        let error = validate(&schema, &json!({"p": "b"})).unwrap_err();
        assert!(error.contains("pattern"), "{error}");
    }

    #[test]
    fn absent_arguments_are_an_empty_object() {
        let params: Map<String, Value> = serde_json::from_value(json!({"name": "x"})).unwrap();
        assert_eq!(arguments(&params), json!({}));
    }
}
