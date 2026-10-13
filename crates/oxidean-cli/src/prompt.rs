//! Interactive prompts — `dialoguer` on a TTY, hard usage errors otherwise.
//!
//! Rule for every promptable value: **flag wins; TTY + missing → prompt;
//! non-TTY + missing → the same usage error as before prompts existed.**
//! Scripts never depend on prompts because every prompted input keeps a flag.

use crate::term;

/// Resolve a missing flag value by prompting. `flag` (already `Some`) returns
/// untouched; `None` prompts when `term::can_prompt()`, else `usage_error`.
pub fn resolve_or_prompt(
    flag: Option<String>,
    message: &str,
    usage_error: impl FnOnce() -> String,
) -> Result<String, String> {
    if let Some(v) = flag {
        return Ok(v);
    }
    if !term::can_prompt() {
        return Err(usage_error());
    }
    input(message).map_err(|e| e.to_string())
}

/// Free-text input with an optional default shown in brackets.
pub fn input(message: &str) -> Result<String, dialoguer::Error> {
    dialoguer::Input::new()
        .with_prompt(message)
        .interact_text()
        .map(|s: String| s.trim().to_string())
}

/// Pick from a list (arrow keys). Returns the chosen index.
pub fn select(message: &str, items: &[String]) -> Result<usize, dialoguer::Error> {
    dialoguer::Select::new()
        .with_prompt(message)
        .items(items)
        .default(0)
        .interact()
}

/// Yes/no confirm — defaults to **no** for destructive verbs.
pub fn confirm(message: &str) -> Result<bool, dialoguer::Error> {
    dialoguer::Confirm::new()
        .with_prompt(message)
        .default(false)
        .interact()
}

/// Masked input — never for scripting paths; callers must keep a
/// non-interactive alternative (flag/stdin) for secrets.
pub fn password(message: &str) -> Result<String, dialoguer::Error> {
    dialoguer::Password::new()
        .with_prompt(message)
        .interact()
}

/// Multi-select checkboxes (e.g. OAuth scope picking on `auth login --web`).
pub fn multi_select(message: &str, items: &[String], defaults: &[bool]) -> Result<Vec<usize>, dialoguer::Error> {
    dialoguer::MultiSelect::new()
        .with_prompt(message)
        .items(items)
        .defaults(defaults)
        .interact()
}
