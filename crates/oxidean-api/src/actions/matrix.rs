//! `strategy.matrix` expansion — evaluated at enqueue into one job row per
//! leg (all sharing `job_key`, distinguished by `matrix_key`).
//!
//! GitHub semantics implemented:
//! - `matrix: {k: [v, …]}` → the cartesian product of every key's values.
//! - `include: [{k: v, …}]` — merges into legs it doesn't contradict; an
//!   include that merges nowhere appends a new leg.
//! - `exclude: [{k: v, …}]` — drops legs matching every listed key.
//! - Embedded `${{ }}` is evaluated first (github/env/vars/inputs contexts);
//!   anything referencing `needs.*` fails loudly — dynamic matrices aren't
//!   supported yet (they'd need mid-run re-expansion).
//!
//! Determinism: keys iterate in sorted order and legs sort by `matrix_key`
//! so a given matrix always fans out identically.

use serde_json::{Map, Value as JsonValue};

use oxidean_actions_expr::Context;

/// One expanded matrix leg.
#[derive(Debug, Clone)]
pub struct MatrixLeg {
    /// Canonical leg key — `k=v` pairs joined by `,` in sorted order;
    /// `''` for the single non-matrix leg.
    pub matrix_key: String,
    /// The assignment as a JSON object → `action_jobs.matrix_json` and the
    /// runner-side `matrix` context.
    pub assignment: JsonValue,
}

/// Expand `strategy` into legs. A job without `strategy` (or without
/// `matrix:`) yields a single empty leg.
pub fn expand_legs(
    strategy: Option<&crate::actions::parse::StrategySpec>,
    ctx: &Context,
) -> Result<Vec<MatrixLeg>, String> {
    let Some(strategy) = strategy else {
        return Ok(vec![single_leg()]);
    };
    let Some(matrix) = &strategy.matrix else {
        return Ok(vec![single_leg()]);
    };
    // `${{ }}` inside the matrix evaluates against the enqueue ctx; a
    // needs.*-dependent matrix resolves to null and trips the shape check
    // below — loud failure, never a silent empty fan-out.
    let evaluated = oxidean_actions_expr::eval_json(matrix, ctx)
        .map_err(|e| format!("matrix expression failed: {e}"))?;
    let JsonValue::Object(map) = &evaluated else {
        return Err("strategy.matrix must expand to a map of key → values".into());
    };
    let mut keys: Vec<(&String, Vec<JsonValue>)> = Vec::new();
    let mut includes: Vec<Map<String, JsonValue>> = Vec::new();
    let mut excludes: Vec<Map<String, JsonValue>> = Vec::new();
    for (k, v) in map {
        match k.as_str() {
            "include" => includes = partial_maps(v, "include")?,
            "exclude" => excludes = partial_maps(v, "exclude")?,
            _ => {
                let arr = match v {
                    JsonValue::Array(a) => a.clone(),
                    JsonValue::Null => Vec::new(),
                    // GitHub allows scalars as single-element lists.
                    other => vec![other.clone()],
                };
                keys.push((k, arr));
            }
        }
    }
    keys.sort_by(|a, b| a.0.cmp(b.0));
    if keys.is_empty() {
        return Ok(vec![single_leg()]);
    }
    if keys.iter().any(|(_, vals)| vals.is_empty()) {
        return Err("strategy.matrix has an empty value list — no legs to run".into());
    }
    // Cartesian product over sorted keys.
    let mut legs: Vec<Map<String, JsonValue>> = vec![Map::new()];
    for (k, vals) in &keys {
        let mut next = Vec::with_capacity(legs.len() * vals.len());
        for leg in &legs {
            for v in vals {
                let mut l = leg.clone();
                l.insert((*k).clone(), v.clone());
                next.push(l);
            }
        }
        legs = next;
    }
    // include — merge where compatible, append otherwise.
    for inc in includes {
        let mut merged_any = false;
        for leg in legs.iter_mut() {
            if inc.iter().all(|(k, v)| leg.get(k).is_none_or(|lv| lv == v)) {
                for (k, v) in &inc {
                    leg.insert(k.clone(), v.clone());
                }
                merged_any = true;
            }
        }
        if !merged_any {
            legs.push(inc);
        }
    }
    // exclude — drop legs matching every listed key.
    legs.retain(|leg| {
        !excludes
            .iter()
            .any(|exc| exc.iter().all(|(k, v)| leg.get(k) == Some(v)))
    });
    if legs.is_empty() {
        return Err("strategy.matrix + include/exclude produced no legs".into());
    }
    let mut out: Vec<MatrixLeg> = legs
        .into_iter()
        .map(|leg| {
            let mut pairs: Vec<String> = leg
                .iter()
                .map(|(k, v)| format!("{k}={}", oxidean_actions_expr::stringify(v)))
                .collect();
            pairs.sort();
            MatrixLeg {
                matrix_key: pairs.join(","),
                assignment: JsonValue::Object(leg),
            }
        })
        .collect();
    out.sort_by(|a, b| a.matrix_key.cmp(&b.matrix_key));
    Ok(out)
}

/// `strategy.fail-fast` — bool literal or `${{ }}`; defaults true.
pub fn fail_fast(strategy: Option<&crate::actions::parse::StrategySpec>, ctx: &Context) -> bool {
    strategy
        .and_then(|s| s.fail_fast.as_ref())
        .and_then(|v| eval_flag(v, ctx))
        .unwrap_or(true)
}

/// `strategy.max-parallel` — int literal or `${{ }}`; `None` = unbounded.
pub fn max_parallel(
    strategy: Option<&crate::actions::parse::StrategySpec>,
    ctx: &Context,
) -> Result<Option<usize>, String> {
    let Some(v) = strategy.and_then(|s| s.max_parallel.as_ref()) else {
        return Ok(None);
    };
    let evaluated = oxidean_actions_expr::eval_json(v, ctx)
        .map_err(|e| format!("max-parallel expression failed: {e}"))?;
    match evaluated.as_u64() {
        Some(n) if n >= 1 => Ok(Some(n as usize)),
        _ => Err(format!(
            "max-parallel must be a positive integer, got {evaluated}"
        )),
    }
}

/// Display suffix — GitHub names legs `job-name (k1=v1, k2=v2)`.
pub fn leg_display_name(base: &str, leg: &MatrixLeg) -> String {
    if leg.matrix_key.is_empty() {
        base.to_string()
    } else {
        format!("{base} ({})", leg.matrix_key)
    }
}

fn single_leg() -> MatrixLeg {
    MatrixLeg {
        matrix_key: String::new(),
        assignment: JsonValue::Object(Map::new()),
    }
}

fn partial_maps(v: &JsonValue, key: &str) -> Result<Vec<Map<String, JsonValue>>, String> {
    let JsonValue::Array(items) = v else {
        return Err(format!("strategy.matrix.{key} must be a list of maps"));
    };
    items
        .iter()
        .map(|item| match item {
            JsonValue::Object(m) => Ok(m.clone()),
            other => Err(format!(
                "strategy.matrix.{key} entries must be maps, got {other}"
            )),
        })
        .collect()
}

/// Evaluate a flag-ish value: bool literal, string `${{ }}`, or bare
/// `true`/`false` text.
fn eval_flag(v: &JsonValue, ctx: &Context) -> Option<bool> {
    match v {
        JsonValue::Bool(b) => Some(*b),
        JsonValue::String(s) => oxidean_actions_expr::eval_bool(s, ctx).ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx() -> Context {
        Context::new().with_context("github", json!({"ref_name": "main"}))
    }

    #[test]
    fn cartesian_product_sorted() {
        let s = crate::actions::parse::StrategySpec {
            matrix: Some(json!({"os": ["linux", "win"], "v": [1, 2]})),
            fail_fast: None,
            max_parallel: None,
        };
        let legs = expand_legs(Some(&s), &ctx()).unwrap();
        assert_eq!(legs.len(), 4);
        // Deterministic sorted order.
        assert_eq!(legs[0].matrix_key, "os=linux,v=1");
        assert_eq!(legs[3].matrix_key, "os=win,v=2");
        assert_eq!(legs[0].assignment, json!({"os": "linux", "v": 1}));
    }

    #[test]
    fn include_merges_and_appends() {
        let s = crate::actions::parse::StrategySpec {
            matrix: Some(json!({
                "os": ["linux", "win"],
                "include": [{"os": "win", "extra": "yes"}, {"os": "mac"}],
            })),
            fail_fast: None,
            max_parallel: None,
        };
        let legs = expand_legs(Some(&s), &ctx()).unwrap();
        assert_eq!(legs.len(), 3);
        let mac = legs.iter().find(|l| l.matrix_key == "os=mac").unwrap();
        assert_eq!(mac.assignment, json!({"os": "mac"}));
        let win = legs
            .iter()
            .find(|l| l.matrix_key.contains("os=win"))
            .unwrap();
        assert_eq!(win.assignment["extra"], json!("yes"));
    }

    #[test]
    fn exclude_drops_legs() {
        let s = crate::actions::parse::StrategySpec {
            matrix: Some(json!({
                "os": ["linux", "win"],
                "v": ["a", "b"],
                "exclude": [{"os": "win", "v": "b"}],
            })),
            fail_fast: None,
            max_parallel: None,
        };
        let legs = expand_legs(Some(&s), &ctx()).unwrap();
        assert_eq!(legs.len(), 3);
        assert!(!legs.iter().any(|l| l.matrix_key == "os=win,v=b"));
    }

    #[test]
    fn expressions_evaluate() {
        let s = crate::actions::parse::StrategySpec {
            matrix: Some(json!({"branch": ["${{ github.ref_name }}"]})),
            fail_fast: None,
            max_parallel: None,
        };
        let legs = expand_legs(Some(&s), &ctx()).unwrap();
        assert_eq!(legs[0].matrix_key, "branch=main");
    }

    #[test]
    fn empty_axis_fails_loud() {
        let s = crate::actions::parse::StrategySpec {
            matrix: Some(json!({"os": []})),
            fail_fast: None,
            max_parallel: None,
        };
        assert!(expand_legs(Some(&s), &ctx()).is_err());
    }

    #[test]
    fn no_strategy_single_leg() {
        let legs = expand_legs(None, &ctx()).unwrap();
        assert_eq!(legs.len(), 1);
        assert_eq!(legs[0].matrix_key, "");
    }
}
