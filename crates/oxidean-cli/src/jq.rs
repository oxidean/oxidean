//! `--jq <filter>` — run a jq expression over the RPC `data` payload.
//!
//! `jaq-core` (pure-Rust jq) — same filter language as `gh --jq`. Applies to
//! the `data` field of a `{ok:true}` envelope so `.[]`, `.items[]`, `.title`
//! compose naturally; scalars print raw like jq itself.

use serde_json::Value;

/// Convert a jaq `Val` back into `serde_json::Value` via the JSON writer —
/// keeps object key order and avoids hand-walking `Num`/`BStr` variants.
fn val_to_json(v: &jaq_json::Val) -> Result<Value, String> {
    let pp = jaq_json::write::Pp::default();
    let mut buf = Vec::new();
    jaq_json::write::write(&mut buf, &pp, 0, v).map_err(|e| format!("--jq render: {e}"))?;
    serde_json::from_slice(&buf).map_err(|e| format!("--jq render: {e}"))
}

/// Apply `filter` to `input`. Returns the produced values — a jq filter can
/// emit zero or more results (`.[]`, `select`, `empty`).
pub fn apply(filter: &str, input: &Value) -> Result<Vec<Value>, String> {
    use jaq_core::load::{Arena, File, Loader};

    let arena = Arena::default();
    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let loader = Loader::new(defs);
    let modules = loader
        .load(
            &arena,
            File {
                path: (),
                code: filter,
            },
        )
        .map_err(|errs| format!("invalid --jq filter: {}", fmt_errs(&errs)))?;
    let funs = jaq_core::funs::<jaq_core::data::JustLut<jaq_json::Val>>()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs());
    let compiled = jaq_core::Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|errs| format!("invalid --jq filter: {}", fmt_errs(&errs)))?;

    let x: jaq_json::Val =
        serde_json::from_value(input.clone()).map_err(|e| format!("--jq input conversion: {e}"))?;
    let ctx: jaq_core::Ctx<'_, jaq_core::data::JustLut<jaq_json::Val>> =
        jaq_core::Ctx::new(&compiled.lut, jaq_core::Vars::new([]));
    let mut out = Vec::new();
    for v in compiled.id.run((ctx, x)) {
        match v {
            Ok(v) => out.push(val_to_json(&v)?),
            Err(e) => match e.get_err() {
                Ok(e) => {
                    return Err(format!(
                        "--jq failed: {}",
                        crate::output::truncate(&e.to_string(), 200)
                    ));
                }
                Err(e) => return Err(format!("--jq failed: {e:?}")),
            },
        }
    }
    Ok(out)
}

/// Load/compile errors arrive as `(File, Error)` pairs — the error half is
/// Debug-formatted (jaq errors don't implement Display).
fn fmt_errs<E: std::fmt::Debug>(errs: &[(jaq_core::load::File<&str, ()>, E)]) -> String {
    errs.iter()
        .map(|(_, e)| format!("{e:?}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Print filter results like jq: strings bare, other values pretty JSON.
pub fn render(results: &[Value]) -> String {
    results
        .iter()
        .map(|v| match v {
            Value::String(s) => s.clone(),
            other => serde_json::to_string_pretty(other).unwrap_or_default(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn field_pick() {
        let v = json!({"title": "hi", "n": 2});
        assert_eq!(apply(".title", &v).unwrap(), vec![json!("hi")]);
    }

    #[test]
    fn iterate_and_transform() {
        let v = json!({"items": [{"n": 1}, {"n": 2}]});
        assert_eq!(
            apply(".items[] | .n", &v).unwrap(),
            vec![json!(1), json!(2)]
        );
    }

    #[test]
    fn scalar_render_bare() {
        assert_eq!(render(&[json!("hello")]), "hello");
        assert_eq!(render(&[json!(1), json!(2)]), "1\n2");
    }

    #[test]
    fn bad_filter_errors() {
        assert!(apply("|[", &json!({})).is_err());
    }
}
