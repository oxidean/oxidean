//! U5 — the hand-authored JSON-Schema walker (NFR9/BR1.4).
//!
//! Deliberately NOT `jsonschema`: the catalog's schema vocabulary is a
//! closed subset — `type`, `required`, `properties` (closed under
//! `additionalProperties: false`), `enum`, `minLength`/`maxLength`,
//! `minimum`/`maximum`, `items`, `maxItems`, and `pattern` resolved
//! through the closed matcher table below (no `regex` dep — BR5.4's
//! supply-chain pin). Every violation names the offending field path
//! so `invalid_args` carries something actionable (BR1.4).

use serde_json::Value;

/// One schema violation — `field` is the dotted/bracketed path the
/// error names (`"title"`, `"files[2].new_path"`); empty string is the
/// args root.
fn err(field: &str, what: impl Into<String>) -> String {
    if field.is_empty() {
        what.into()
    } else {
        format!("{field}: {}", what.into())
    }
}

/// The closed `pattern` matcher table — every `pattern` value in the
/// registry must resolve here; an unknown pattern name fails CLOSED at
/// validation time (the schema author made an error, not the caller,
/// but there is no safe lenient interpretation).
///
/// `repo_path` — BR3.3 + the `DiffFileOp.path` rule: repo-relative,
/// no absolute/`..`/`.git` escape — the field-level floor the shipped
/// `normalize_edit_path` enforcement re-checks inside each `repo.file.*`
/// proc at apply time.
fn match_pattern(name: &str, value: &str) -> Result<(), String> {
    match name {
        "repo_path" => {
            if value.is_empty() {
                return Err("must be a non-empty repo-relative path".to_string());
            }
            if value.starts_with('/') || value.starts_with('~') {
                return Err("must be repo-relative, not absolute".to_string());
            }
            if value.contains('\\') || value.contains('\0') {
                return Err("contains an illegal path character".to_string());
            }
            if value == ".git" || value.starts_with(".git/") {
                return Err("must not enter .git".to_string());
            }
            if value.split('/').any(|seg| seg == "..") {
                return Err("must not contain '..' segments".to_string());
            }
            Ok(())
        }
        other => Err(format!("unsupported pattern {other}")),
    }
}

/// Validate `instance` against `schema` under the catalog's closed
/// vocabulary. `field` is the path prefix recursion builds.
fn check(schema: &Value, instance: &Value, field: &str) -> Result<(), String> {
    // ---- enum ----
    if let Some(variants) = schema.get("enum").and_then(|v| v.as_array()) {
        if !variants.contains(instance) {
            return Err(err(field, "not one of the allowed values"));
        }
    }

    let ty = schema.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match ty {
        "object" => {
            let obj = instance
                .as_object()
                .ok_or_else(|| err(field, "must be an object"))?;
            if let Some(req) = schema.get("required").and_then(|r| r.as_array()) {
                for k in req.iter().filter_map(|k| k.as_str()) {
                    if !obj.contains_key(k) {
                        let child = if field.is_empty() {
                            k.to_string()
                        } else {
                            format!("{field}.{k}")
                        };
                        return Err(err(&child, "required"));
                    }
                }
            }
            let props = schema
                .get("properties")
                .and_then(|p| p.as_object())
                .cloned()
                .unwrap_or_default();
            let closed = schema.get("additionalProperties") == Some(&Value::Bool(false));
            for (key, val) in obj {
                match props.get(key) {
                    Some(sub) => {
                        let child = if field.is_empty() {
                            key.clone()
                        } else {
                            format!("{field}.{key}")
                        };
                        check(sub, val, &child)?;
                    }
                    None if closed => {
                        let child = if field.is_empty() {
                            key.clone()
                        } else {
                            format!("{field}.{key}")
                        };
                        return Err(err(&child, "additional property not allowed"));
                    }
                    None => {}
                }
            }
        }
        "array" => {
            let arr = instance
                .as_array()
                .ok_or_else(|| err(field, "must be an array"))?;
            if let Some(max) = schema.get("maxItems").and_then(|m| m.as_u64()) {
                if arr.len() as u64 > max {
                    return Err(err(field, format!("must have at most {max} items")));
                }
            }
            if let Some(min) = schema.get("minItems").and_then(|m| m.as_u64()) {
                if (arr.len() as u64) < min {
                    return Err(err(field, format!("must have at least {min} items")));
                }
            }
            if let Some(item_schema) = schema.get("items") {
                for (i, item) in arr.iter().enumerate() {
                    check(item_schema, item, &format!("{field}[{i}]"))?;
                }
            }
        }
        "string" => {
            let s = instance
                .as_str()
                .ok_or_else(|| err(field, "must be a string"))?;
            if let Some(min) = schema.get("minLength").and_then(|m| m.as_u64()) {
                if (s.chars().count() as u64) < min {
                    return Err(err(field, format!("must be at least {min} chars")));
                }
            }
            if let Some(max) = schema.get("maxLength").and_then(|m| m.as_u64()) {
                if s.chars().count() as u64 > max {
                    return Err(err(field, format!("must be at most {max} chars")));
                }
            }
            if let Some(pat) = schema.get("pattern").and_then(|p| p.as_str()) {
                match_pattern(pat, s).map_err(|e| err(field, e))?;
            }
        }
        "integer" => {
            if !instance.is_i64() && !instance.is_u64() {
                return Err(err(field, "must be an integer"));
            }
            check_numeric_bounds(schema, instance, field)?;
        }
        "number" => {
            if !instance.is_number() {
                return Err(err(field, "must be a number"));
            }
            check_numeric_bounds(schema, instance, field)?;
        }
        "boolean" => {
            if !instance.is_boolean() {
                return Err(err(field, "must be a boolean"));
            }
        }
        "null" => {
            if !instance.is_null() {
                return Err(err(field, "must be null"));
            }
        }
        // No `type` — keyword-only schema (enum/pattern handled above).
        _ => {}
    }
    Ok(())
}

fn check_numeric_bounds(schema: &Value, instance: &Value, field: &str) -> Result<(), String> {
    if let Some(min) = schema.get("minimum").and_then(|m| m.as_f64()) {
        if instance.as_f64().unwrap_or(f64::INFINITY) < min {
            return Err(err(field, format!("must be >= {min}")));
        }
    }
    if let Some(max) = schema.get("maximum").and_then(|m| m.as_f64()) {
        if instance.as_f64().unwrap_or(f64::NEG_INFINITY) > max {
            return Err(err(field, format!("must be <= {max}")));
        }
    }
    Ok(())
}

/// Validate `args` (already parsed) against a catalog entry's
/// `parameters_json`. Violations return the offending-field message;
/// `invoke` maps it to `invalid_args`.
pub fn validate(schema: &Value, args: &Value) -> Result<(), String> {
    check(schema, args, "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj_schema() -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": {"type": "string", "minLength": 1, "maxLength": 64, "pattern": "repo_path"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                "state": {"enum": ["open", "closed"]},
            },
            "additionalProperties": false
        })
    }

    #[test]
    fn valid_args_pass() {
        assert!(validate(
            &obj_schema(),
            &json!({"path": "src/main.rs", "limit": 5, "state": "open"})
        )
        .is_ok());
    }

    #[test]
    fn missing_required_names_field() {
        let e = validate(&obj_schema(), &json!({})).unwrap_err();
        assert_eq!(e, "path: required");
    }

    #[test]
    fn additional_property_rejected_and_named() {
        let e = validate(&obj_schema(), &json!({"path": "a", "session_id": "x"})).unwrap_err();
        assert_eq!(e, "session_id: additional property not allowed");
    }

    #[test]
    fn bounds_and_enum_violations_name_field() {
        let s = obj_schema();
        assert!(validate(&s, &json!({"path": "a", "limit": 0}))
            .unwrap_err()
            .starts_with("limit:"));
        assert!(validate(&s, &json!({"path": "a", "limit": 101}))
            .unwrap_err()
            .starts_with("limit:"));
        assert!(validate(&s, &json!({"path": "a", "state": "merged"}))
            .unwrap_err()
            .starts_with("state:"));
        assert!(validate(&s, &json!({"path": "a", "limit": 1.5}))
            .unwrap_err()
            .starts_with("limit:"));
        assert!(validate(&s, &json!({"path": "a", "limit": "5"}))
            .unwrap_err()
            .starts_with("limit:"));
        assert!(validate(&s, &json!({"path": "a".repeat(65)}))
            .unwrap_err()
            .starts_with("path:"));
    }

    #[test]
    fn repo_path_pattern() {
        let s = obj_schema();
        for bad in ["/abs/x", "..\\x", "a/../b", ".git/config", "~/x", ""] {
            assert!(
                validate(&s, &json!({"path": bad})).is_err(),
                "{bad} must reject"
            );
        }
        for good in [
            "src/main.rs",
            "a",
            ".github/workflows/ci.yml",
            "a/.gitignore",
        ] {
            assert!(
                validate(&s, &json!({"path": good})).is_ok(),
                "{good} must pass"
            );
        }
    }

    #[test]
    fn array_items_recurse_with_index_paths() {
        let s = json!({
            "type": "object",
            "properties": {
                "files": {
                    "type": "array",
                    "maxItems": 2,
                    "items": {
                        "type": "object",
                        "required": ["path", "op"],
                        "properties": {
                            "path": {"type": "string"},
                            "op": {"enum": ["create", "delete"]},
                        },
                        "additionalProperties": false
                    }
                }
            },
            "additionalProperties": false
        });
        assert!(validate(&s, &json!({"files": [{"path": "a", "op": "create"}]})).is_ok());
        let e = validate(
            &s,
            &json!({"files": [{"path": "a", "op": "create"}, {"path": "b"}]}),
        )
        .unwrap_err();
        assert_eq!(e, "files[1].op: required");
        let e = validate(&s, &json!({"files": [{"path": "a", "op": "rename"}]})).unwrap_err();
        assert!(e.starts_with("files[0].op:"), "{e}");
        let e = validate(
            &s,
            &json!({"files": [{"path": "a", "op": "create", "x": 1}]}),
        )
        .unwrap_err();
        assert!(e.starts_with("files[0].x:"), "{e}");
        assert!(validate(&s, &json!({"files": [{"path":"a","op":"create"},{"path":"b","op":"create"},{"path":"c","op":"create"}]})).unwrap_err().starts_with("files:"));
    }
}
