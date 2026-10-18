//! `oxidean-actions-expr` — GitHub Actions `${{ }}` expression engine.
//!
//! Deps are deliberately minimal (`serde_json` only) so `oxidean-runner`
//! stays dep-light. Covers: literals, `! && || == != < <= > >=`, member /
//! index / `.*` access, calls — `format contains startsWith endsWith join
//! toJSON fromJSON hashFiles success failure cancelled always` — and the
//! standard contexts (`github env runner matrix needs steps inputs vars
//! secrets job strategy`), supplied by the caller via [`Context`].
//!
//! Two entry shapes:
//! - [`eval()`] / [`eval_bool`] — a bare expression (or `${{ }}`-delimited) as
//!   found in `if:` / `timeout-minutes:` / `runs-on:` / `concurrency:`.
//! - [`interpolate`] / [`eval_embedded`] / [`eval_json`] — strings that may
//!   embed `${{ expr }}` inside literal text (`run:`, `name:`, `with:`,
//!   `env:` values, `working-directory:`).

pub mod ast;
pub mod context;
pub mod eval;
pub mod lexer;

pub use context::{Context, HashFilesFn, JobStatus};
pub use eval::{
    check_embedded, eval, eval_ast, eval_bool, eval_embedded, eval_json, interpolate, loose_eq,
    stringify, truthy, uses_status_fn,
};

/// Expression error — parse or evaluation failure. `message` is
/// log/display safe (no workflow secrets can appear — evaluation never
/// formats values into errors).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprError {
    pub message: String,
}

impl ExprError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ExprError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ExprError {}

/// Parse-time syntax check — used by the workflow parser for the
/// fail-loud contract. Returns one error per malformed `${{ }}`.
pub fn check(src: &str) -> Vec<ExprError> {
    check_embedded(src)
}

/// Whole-expression syntax check for `if:`-style fields where the source is
/// the expression itself (bare or `${{ }}`-wrapped) — unlike [`check`] this
/// validates strings with no delimiters too.
pub fn check_expr(src: &str) -> Vec<ExprError> {
    let trimmed = src.trim();
    let inner = if trimmed.starts_with("${{") && trimmed.ends_with("}}") {
        &trimmed[3..trimmed.len() - 2]
    } else {
        trimmed
    };
    ast::parse(inner).err().into_iter().collect()
}
