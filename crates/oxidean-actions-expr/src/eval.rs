//! Evaluator for the `${{ }}` AST against a [`Context`].
//!
//! Semantics mirror GitHub's expression engine (actions/runner
//! `ExpressionNode`): loose equality with type coercion, `&&`/`||` return
//! operand values (not booleans), truthiness is false/0/""/null, member and
//! index access on absent paths yields `null` (never an error).

use serde_json::{Map, Value};

use crate::ast::{BinOp, Expr};
use crate::context::{Context, JobStatus};
use crate::ExprError;

/// GitHub truthiness: `false`, `0`, `""`, `null` are falsy; everything else
/// (including empty arrays/objects) is truthy.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

fn to_num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::String(s) => s.trim().parse().ok(),
        Value::Null => Some(0.0),
        _ => None,
    }
}

/// Loose equality, GitHub-style type coercion:
/// - `null` equals only `null`.
/// - bool coerces the other side through truthiness.
/// - number vs string coerces the string.
/// - strings compare case-insensitively.
/// - arrays/objects compare structurally.
pub fn loose_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Null, _) | (_, Value::Null) => false,
        (Value::Bool(x), other) | (other, Value::Bool(x)) => *x == truthy(other),
        (Value::Number(x), other) | (other, Value::Number(x)) => {
            to_num(other).is_some_and(|o| x.as_f64() == Some(o))
        }
        (Value::String(x), Value::String(y)) => x.eq_ignore_ascii_case(y),
        _ => a == b,
    }
}

/// Loose ordering — numbers numeric, strings case-insensitive ordinal,
/// other pairs coerce through `to_num` where possible.
fn loose_cmp(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::String(x), Value::String(y)) => {
            let xl = x.to_lowercase();
            let yl = y.to_lowercase();
            Some(xl.cmp(&yl))
        }
        _ => match (to_num(a), to_num(b)) {
            (Some(x), Some(y)) => x.partial_cmp(&y),
            _ => None,
        },
    }
}

/// Value → string for interpolation / `format` / `join`.
/// Objects/arrays serialize compactly; strings stay raw; integral numbers
/// render without a decimal point (`3`, not `3.0`) per GitHub's display.
pub fn stringify(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Number(n) => n
            .as_f64()
            .map(|f| {
                if f.fract() == 0.0 && f.abs() < 1e15 {
                    format!("{}", f as i64)
                } else {
                    f.to_string()
                }
            })
            .unwrap_or_default(),
        other => other.to_string(),
    }
}

pub fn eval_ast(e: &Expr, ctx: &Context) -> Result<Value, ExprError> {
    match e {
        Expr::Null => Ok(Value::Null),
        Expr::Bool(b) => Ok(Value::Bool(*b)),
        Expr::Num(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                Ok(serde_json::json!(*n as i64))
            } else {
                Ok(serde_json::json!(n))
            }
        }
        Expr::Str(s) => Ok(Value::String(s.clone())),
        Expr::Ident(name) => Ok(ctx.get_context(name).cloned().unwrap_or(Value::Null)),
        Expr::Member(obj, name) => {
            let v = eval_ast(obj, ctx)?;
            Ok(member(&v, name))
        }
        Expr::Index(obj, idx) => {
            let v = eval_ast(obj, ctx)?;
            let i = eval_ast(idx, ctx)?;
            Ok(index(&v, &i))
        }
        Expr::Deref(obj) => {
            let v = eval_ast(obj, ctx)?;
            Ok(match v {
                Value::Object(m) => Value::Array(m.values().cloned().collect()),
                Value::Array(a) => Value::Array(a),
                _ => Value::Array(vec![]),
            })
        }
        Expr::Not(inner) => {
            let v = eval_ast(inner, ctx)?;
            Ok(Value::Bool(!truthy(&v)))
        }
        Expr::Neg(inner) => {
            let v = eval_ast(inner, ctx)?;
            match to_num(&v) {
                Some(n) => Ok(serde_json::json!(-n)),
                None => Err(ExprError::new("unary '-' requires a number")),
            }
        }
        Expr::Bin(op, l, r) => {
            let lhs = eval_ast(l, ctx)?;
            match op {
                // Operand-value semantics: `a || b` → a if truthy else b.
                BinOp::Or => {
                    if truthy(&lhs) {
                        Ok(lhs)
                    } else {
                        eval_ast(r, ctx)
                    }
                }
                BinOp::And => {
                    if truthy(&lhs) {
                        eval_ast(r, ctx)
                    } else {
                        Ok(lhs)
                    }
                }
                _ => {
                    let rhs = eval_ast(r, ctx)?;
                    Ok(Value::Bool(match op {
                        BinOp::Eq => loose_eq(&lhs, &rhs),
                        BinOp::Ne => !loose_eq(&lhs, &rhs),
                        BinOp::Lt => loose_cmp(&lhs, &rhs).is_some_and(|o| o.is_lt()),
                        BinOp::Le => loose_cmp(&lhs, &rhs).is_some_and(|o| !o.is_gt()),
                        BinOp::Gt => loose_cmp(&lhs, &rhs).is_some_and(|o| o.is_gt()),
                        BinOp::Ge => loose_cmp(&lhs, &rhs).is_some_and(|o| !o.is_lt()),
                        _ => unreachable!(),
                    }))
                }
            }
        }
        Expr::Call(name, args) => call(name, args, ctx),
    }
}

/// `a.name` — case-insensitive object member; on an array it propagates
/// element-wise (post-`.*` chaining); anything else → null.
fn member(v: &Value, name: &str) -> Value {
    match v {
        Value::Object(m) => m
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
            .unwrap_or(Value::Null),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|i| member(i, name))
                .filter(|v| !v.is_null())
                .collect(),
        ),
        _ => Value::Null,
    }
}

fn index(v: &Value, i: &Value) -> Value {
    match (v, i) {
        (Value::Array(a), Value::Number(n)) => n
            .as_f64()
            .and_then(|f| usize::try_from(f as i64).ok().filter(|_| f.fract() == 0.0))
            .and_then(|k| a.get(k).cloned())
            .unwrap_or(Value::Null),
        (Value::Object(_), Value::String(s)) => member(v, s),
        (Value::Array(a), Value::String(s)) => Value::Array(
            a.iter()
                .map(|i| member(i, s))
                .filter(|v| !v.is_null())
                .collect(),
        ),
        _ => Value::Null,
    }
}

fn call(name: &str, args: &[Expr], ctx: &Context) -> Result<Value, ExprError> {
    let eval_args = |ctx: &Context| -> Result<Vec<Value>, ExprError> {
        args.iter().map(|a| eval_ast(a, ctx)).collect()
    };
    let argc = |got: usize, want: &[usize]| -> Result<(), ExprError> {
        if want.contains(&got) {
            Ok(())
        } else {
            Err(ExprError::new(format!(
                "{name}() expects {} args, got {got}",
                want.iter()
                    .map(|w| w.to_string())
                    .collect::<Vec<_>>()
                    .join("|")
            )))
        }
    };

    match name {
        // Status functions — no arg evaluation needed.
        "always" => Ok(Value::Bool(true)),
        "success" => Ok(Value::Bool(ctx.job_status() == JobStatus::Success)),
        "failure" => Ok(Value::Bool(ctx.job_status() == JobStatus::Failure)),
        "cancelled" => Ok(Value::Bool(ctx.job_status() == JobStatus::Cancelled)),
        "contains" => {
            let a = eval_args(ctx)?;
            argc(a.len(), &[2])?;
            match (&a[0], &a[1]) {
                (Value::String(hay), needle) => Ok(Value::Bool(
                    hay.to_lowercase()
                        .contains(&stringify(needle).to_lowercase()),
                )),
                (Value::Array(items), needle) => {
                    Ok(Value::Bool(items.iter().any(|i| loose_eq(i, needle))))
                }
                (Value::Object(m), needle) => Ok(Value::Bool(match needle {
                    Value::String(k) => m.keys().any(|kk| kk.eq_ignore_ascii_case(k)),
                    _ => m.values().any(|v| loose_eq(v, needle)),
                })),
                _ => Err(ExprError::new(
                    "contains() expects an array/object/string first arg",
                )),
            }
        }
        "startsWith" | "endsWith" => {
            let a = eval_args(ctx)?;
            argc(a.len(), &[2])?;
            let (s, p) = (
                stringify(&a[0]).to_lowercase(),
                stringify(&a[1]).to_lowercase(),
            );
            Ok(Value::Bool(if name == "startsWith" {
                s.starts_with(&p)
            } else {
                s.ends_with(&p)
            }))
        }
        "format" => {
            let a = eval_args(ctx)?;
            if a.is_empty() {
                return Err(ExprError::new("format() needs at least a template"));
            }
            let tpl = stringify(&a[0]);
            Ok(Value::String(format_str(&tpl, &a[1..])))
        }
        "join" => {
            let a = eval_args(ctx)?;
            argc(a.len(), &[1, 2])?;
            let sep = a.get(1).map(stringify).unwrap_or_else(|| ",".into());
            let parts: Vec<String> = match &a[0] {
                Value::Array(items) => items.iter().map(stringify).collect(),
                v => vec![stringify(v)],
            };
            Ok(Value::String(parts.join(&sep)))
        }
        "toJSON" => {
            let a = eval_args(ctx)?;
            argc(a.len(), &[1])?;
            // GitHub emits indented JSON.
            Ok(Value::String(
                serde_json::to_string_pretty(&a[0]).unwrap_or_else(|_| "null".into()),
            ))
        }
        "toJson" => Err(ExprError::new("did you mean toJSON()?")),
        "fromJSON" => {
            let a = eval_args(ctx)?;
            argc(a.len(), &[1])?;
            match &a[0] {
                Value::String(s) => {
                    serde_json::from_str(s).map_err(|e| ExprError::new(format!("fromJSON: {e}")))
                }
                v => Ok(v.clone()),
            }
        }
        "fromJson" => Err(ExprError::new("did you mean fromJSON()?")),
        "hashFiles" => {
            let a = eval_args(ctx)?;
            let pats: Vec<String> = a.iter().map(stringify).collect();
            match ctx.hash_files() {
                Some(f) => f(&pats)
                    .map(Value::String)
                    .map_err(|e| ExprError::new(format!("hashFiles: {e}"))),
                None => Ok(Value::String(String::new())),
            }
        }
        other => Err(ExprError::new(format!("unknown function '{other}'"))),
    }
}

/// `format('{0} {1}', a, b)` — `{N}` placeholders plus `{{`/`}}`/`{`/`}`
/// escapes. Unknown placeholders pass through literally.
fn format_str(tpl: &str, args: &[Value]) -> String {
    let b: Vec<char> = tpl.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == '{' {
            if b.get(i + 1) == Some(&'{') {
                out.push('{');
                i += 2;
                continue;
            }
            // {N} placeholder
            let mut j = i + 1;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1 && b.get(j) == Some(&'}') {
                let n: usize = b[i + 1..j].iter().collect::<String>().parse().unwrap_or(0);
                out.push_str(&args.get(n).map(stringify).unwrap_or_default());
                i = j + 1;
                continue;
            }
            out.push('{');
            i += 1;
        } else if b[i] == '}' && b.get(i + 1) == Some(&'}') {
            out.push('}');
            i += 2;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    out
}

/// Evaluate an expression source string (no `${{ }}` delimiters).
pub fn eval(src: &str, ctx: &Context) -> Result<Value, ExprError> {
    eval_ast(&crate::ast::parse(src)?, ctx)
}

/// `if:` semantics — the value may be a bare expression or `${{ … }}`.
pub fn eval_bool(src: &str, ctx: &Context) -> Result<bool, ExprError> {
    Ok(truthy(&eval(unwrap_expr(src), ctx)?))
}

/// Strip the `${{ … }}` wrapper when the whole string is one expression;
/// otherwise return the source unchanged (also trims).
fn unwrap_expr(src: &str) -> &str {
    let trimmed = src.trim();
    if trimmed.starts_with("${{") && trimmed.ends_with("}}") {
        trimmed[3..trimmed.len() - 2].trim()
    } else {
        trimmed
    }
}

/// True when the `if:` source calls a status-check function
/// (`success`/`failure`/`cancelled`/`always`) anywhere in its AST — GitHub
/// implicitly ANDs `if:` with `success()` unless one is present.
pub fn uses_status_fn(src: &str) -> bool {
    fn walk(e: &Expr) -> bool {
        match e {
            Expr::Call(name, args) => {
                matches!(
                    name.as_str(),
                    "success" | "failure" | "cancelled" | "always"
                ) || args.iter().any(walk)
            }
            Expr::Member(obj, _) | Expr::Deref(obj) | Expr::Not(obj) | Expr::Neg(obj) => walk(obj),
            Expr::Index(obj, idx) => walk(obj) || walk(idx),
            Expr::Bin(_, l, r) => walk(l) || walk(r),
            _ => false,
        }
    }
    crate::ast::parse(unwrap_expr(src))
        .map(|e| walk(&e))
        .unwrap_or(false)
}

/// Replace every `${{ expr }}` in `s` with its stringified value.
/// An `s` that is exactly one `${{ expr }}` keeps the value's native JSON
/// type via [`eval_embedded`] — callers that need it use that directly.
pub fn interpolate(s: &str, ctx: &Context) -> Result<String, ExprError> {
    Ok(stringify(&eval_embedded(s, ctx)?))
}

/// Interpolate but preserve the value type when the whole string is one
/// `${{ expr }}` (`with:`/`env:` values keep `true`/`5`/`{…}` instead of
/// stringifying).
pub fn eval_embedded(s: &str, ctx: &Context) -> Result<Value, ExprError> {
    for (inner, whole) in embedded_exprs(s) {
        if whole == s.trim() {
            return eval(inner, ctx);
        }
    }
    let mut out = String::new();
    let mut cursor = 0;
    for (inner, whole) in embedded_exprs(s) {
        // Search from `cursor` — a repeated `${{ expr }}` must not re-match
        // its first occurrence (that would splice backwards and panic).
        let start = cursor + s[cursor..].find(whole).unwrap();
        out.push_str(&s[cursor..start]);
        out.push_str(&stringify(&eval(inner, ctx)?));
        cursor = start + whole.len();
    }
    out.push_str(&s[cursor..]);
    Ok(Value::String(out))
}

/// Locate `inner`/`whole` spans of every `${{ … }}` in `s` — the whole span
/// includes the delimiters. Unclosed `${{` is left literal (GitHub treats it
/// as text, not an error).
fn embedded_exprs(s: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() {
        if bytes[i] == b'$' && bytes[i + 1] == b'{' && bytes[i + 2] == b'{' {
            if let Some(close) = s[i + 3..].find("}}") {
                let inner_end = i + 3 + close;
                out.push((&s[i + 3..inner_end], &s[i..inner_end + 2]));
                i = inner_end + 2;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Syntax-check every `${{ }}` in a string without evaluating — used at
/// parse time for the fail-loud contract.
pub fn check_embedded(s: &str) -> Vec<ExprError> {
    embedded_exprs(s)
        .into_iter()
        .filter_map(|(inner, _)| crate::ast::parse(inner).err())
        .collect()
}

/// Recursively evaluate all `${{ }}` inside a JSON tree's strings —
/// used for `env:`/`with:` maps. Non-string leaves pass through.
pub fn eval_json(v: &Value, ctx: &Context) -> Result<Value, ExprError> {
    match v {
        Value::String(s) => eval_embedded(s, ctx),
        Value::Array(items) => items
            .iter()
            .map(|i| eval_json(i, ctx))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(m) => {
            let mut out = Map::new();
            for (k, v) in m {
                out.insert(k.clone(), eval_json(v, ctx)?);
            }
            Ok(Value::Object(out))
        }
        other => Ok(other.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx() -> Context {
        Context::new()
            .with_context(
                "github",
                json!({
                    "sha": "abc123",
                    "ref": "refs/heads/main",
                    "ref_name": "main",
                    "event_name": "push",
                    "run_number": 7,
                    "repository": "me/repo",
                    "event": {"commits": [{"id": "c1"}, {"id": "c2"}]},
                }),
            )
            .with_context("env", json!({"MODE": "dev"}))
            .with_context("matrix", json!({"os": "ubuntu", "node": ["18", "20"]}))
            .with_context(
                "needs",
                json!({"build": {"result": "success", "outputs": {"v": "1.2"}}}),
            )
            .with_context(
                "steps",
                json!({"prep": {"outputs": {"val": "x"}, "outcome": "success"}}),
            )
            .with_context("vars", json!({"FLAG": "on"}))
            .with_context("secrets", json!({"TOKEN": "s3cr3t"}))
            .with_context("inputs", json!({"dry_run": true}))
    }

    fn ev(s: &str) -> Value {
        eval(s, &ctx()).unwrap_or_else(|e| panic!("eval {s:?}: {e}"))
    }

    #[test]
    fn literals_and_identity() {
        assert_eq!(ev("null"), Value::Null);
        assert_eq!(ev("true"), json!(true));
        assert_eq!(ev("42"), json!(42));
        assert_eq!(ev("'hi'"), json!("hi"));
    }

    #[test]
    fn truthiness_and_logic_operands() {
        assert_eq!(ev("!true"), json!(false));
        assert_eq!(ev("!0"), json!(true));
        // && / || return operands, not booleans.
        assert_eq!(ev("'a' && 'b'"), json!("b"));
        assert_eq!(ev("'' && 'b'"), json!(""));
        assert_eq!(ev("'' || 'b'"), json!("b"));
        assert_eq!(ev("'a' || 'b'"), json!("a"));
        assert_eq!(ev("null || 42"), json!(42));
    }

    #[test]
    fn equality_is_loose_and_case_insensitive() {
        assert_eq!(ev("'Main' == github.ref_name"), json!(true));
        assert_eq!(ev("1 == '1'"), json!(true));
        assert_eq!(ev("true == 'x'"), json!(true));
        assert_eq!(ev("false == null"), json!(false));
        assert_eq!(ev("false == ''"), json!(true));
        assert_eq!(ev("0 == false"), json!(true));
        assert_eq!(ev("null == null"), json!(true));
        assert_eq!(ev("github.missing == null"), json!(true));
    }

    #[test]
    fn ordering() {
        assert_eq!(ev("2 > 1"), json!(true));
        assert_eq!(ev("'b' > 'a'"), json!(true));
        assert_eq!(ev("'a' < 'B'"), json!(true)); // case-insensitive
        assert_eq!(ev("github.run_number >= 7"), json!(true));
    }

    #[test]
    fn member_index_deref() {
        assert_eq!(ev("github.event.commits[0].id"), json!("c1"));
        assert_eq!(ev("github['ref_name']"), json!("main"));
        assert_eq!(ev("matrix.node[1]"), json!("20"));
        assert_eq!(ev("needs['build'].outputs.v"), json!("1.2"));
        assert_eq!(ev("steps.*.outputs.val"), json!(["x"]));
        assert_eq!(ev("matrix.node.*"), json!(["18", "20"]));
        // Missing paths → null, not errors.
        assert_eq!(ev("github.nope.deeper.still"), Value::Null);
        assert_eq!(ev("nope.deep"), Value::Null);
    }

    #[test]
    fn functions() {
        assert_eq!(ev("contains('Hello World', 'llo')"), json!(true));
        assert_eq!(ev("contains(matrix.node, '20')"), json!(true));
        assert_eq!(ev("startsWith(github.ref, 'refs/heads/')"), json!(true));
        assert_eq!(ev("endsWith(github.sha, '123')"), json!(true));
        assert_eq!(ev("format('{0}-{1}', 'a', 3)"), json!("a-3"));
        assert_eq!(ev("format('{{x}} {0}', 'v')"), json!("{x} v"));
        assert_eq!(ev("join(matrix.node, '/')"), json!("18/20"));
        assert_eq!(ev("join('solo')"), json!("solo"));
        assert_eq!(ev("fromJSON('{\"a\": 1}').a"), json!(1));
        assert_eq!(ev("toJSON(null)"), json!("null"));
    }

    #[test]
    fn status_functions() {
        let c = ctx().with_job_status("success");
        assert_eq!(eval("success()", &c).unwrap(), json!(true));
        assert_eq!(eval("failure()", &c).unwrap(), json!(false));
        assert_eq!(eval("always()", &c).unwrap(), json!(true));
        let c = ctx().with_job_status("failure");
        assert_eq!(eval("failure()", &c).unwrap(), json!(true));
        assert_eq!(eval("success()", &c).unwrap(), json!(false));
    }

    #[test]
    fn hash_files_callback() {
        let c = ctx().with_hash_files(|pats| Ok(format!("H{:?}", pats)));
        assert_eq!(
            eval("hashFiles('**/*.rs')", &c).unwrap(),
            json!("H[\"**/*.rs\"]")
        );
        // No callback → "".
        assert_eq!(eval("hashFiles('x')", &ctx()).unwrap(), json!(""));
    }

    #[test]
    fn eval_bool_accepts_bare_or_delimited() {
        assert!(eval_bool("github.run_number == 7", &ctx()).unwrap());
        assert!(eval_bool("${{ github.run_number == 7 }}", &ctx()).unwrap());
        assert!(!eval_bool("false", &ctx()).unwrap());
    }

    #[test]
    fn interpolation() {
        let c = ctx();
        assert_eq!(
            interpolate("sha=${{ github.sha }} mode=${{ env.MODE }}", &c).unwrap(),
            "sha=abc123 mode=dev"
        );
        // Whole-string expr keeps native type.
        assert_eq!(
            eval_embedded("${{ inputs.dry_run }}", &c).unwrap(),
            json!(true)
        );
        // Object interpolates as compact JSON.
        assert_eq!(
            interpolate("x=${{ github.event.commits }}", &c).unwrap(),
            r#"x=[{"id":"c1"},{"id":"c2"}]"#
        );
        // The same `${{ expr }}` repeated in one string must splice at its
        // own position — searching from 0 re-matches the first occurrence
        // and panics on a backwards range.
        assert_eq!(
            interpolate("a=${{ github.sha }} b=${{ github.sha }}", &c).unwrap(),
            "a=abc123 b=abc123"
        );
        assert_eq!(
            interpolate(
                "echo \"env=${{ github.sha }}\" && echo \"r=ok-${{ github.sha }}\"",
                &c
            )
            .unwrap(),
            "echo \"env=abc123\" && echo \"r=ok-abc123\""
        );
    }

    #[test]
    fn secrets_context_is_plain_lookup() {
        assert_eq!(ev("secrets.TOKEN"), json!("s3cr3t"));
    }

    #[test]
    fn status_fn_detection() {
        assert!(uses_status_fn("always()"));
        assert!(uses_status_fn("${{ failure() || success() }}"));
        assert!(uses_status_fn("github.x == 1 && !cancelled()"));
        assert!(!uses_status_fn("github.ref == 'main'"));
        assert!(!uses_status_fn("contains('success(x', 'x')"));
        // Unparseable → false (the parse error surfaces on eval).
        assert!(!uses_status_fn("always("));
    }

    #[test]
    fn unknown_function_and_syntax_errors() {
        assert!(eval("nosuch(1)", &ctx()).is_err());
        assert!(eval("a ==", &ctx()).is_err());
        assert_eq!(
            check_embedded("run ${{ github.sha && }}").len(),
            1,
            "malformed embedded expr is detected"
        );
        assert!(check_embedded("run ${{ github.sha }}").is_empty());
        assert!(check_embedded("no expr here").is_empty());
    }
}
