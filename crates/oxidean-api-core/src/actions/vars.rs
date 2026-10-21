//! Actions configuration variables (`vars` context) — plaintext siblings of
//! the secrets store (D-ACT-17 shape, no ciphertext). Same repo < org <
//! environment precedence; unlike secrets the values are listable — GitHub
//! returns variable values on GET.

use std::collections::HashMap;

use oxidean_db::Database;

/// GitHub's variable value cap — same 48 KiB as secrets.
pub const MAX_VARIABLE_VALUE_BYTES: usize = 48 * 1024;

/// Validate a variable value — GitHub allows empty variables; the cap is
/// the same 48 KiB secrets get.
pub fn validate_variable_value(value: &str) -> Result<(), String> {
    if value.len() > MAX_VARIABLE_VALUE_BYTES {
        return Err(format!(
            "variable value exceeds the {} KiB limit",
            MAX_VARIABLE_VALUE_BYTES / 1024
        ));
    }
    Ok(())
}

/// Validate a variable name — GitHub rules: alnum + `_`, ≤255 chars, no
/// leading digit, no `GITHUB_` prefix (reserved for the runner's own env).
pub fn validate_variable_name(name: &str) -> Result<(), String> {
    let n = name.trim();
    if n.is_empty() {
        return Err("variable name is required".into());
    }
    if n.len() > 255 {
        return Err("variable name too long".into());
    }
    if n.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return Err("variable name must not start with a digit".into());
    }
    if n.to_ascii_uppercase().starts_with("GITHUB_") {
        return Err("variable name must not start with GITHUB_".into());
    }
    if !n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("variable name must be alphanumeric or underscore".into());
    }
    Ok(())
}

/// Repo-level variables as a `vars`-context map (name → value).
pub async fn variables_for_repo(
    db: &Database,
    repository_id: &str,
) -> Result<HashMap<String, String>, String> {
    let rows = db.list_action_variable_values(repository_id).await?;
    Ok(rows.into_iter().map(|r| (r.name, r.value)).collect())
}

/// Run-level `vars` context — org < repo merge: the narrowest scope wins,
/// so repo values overwrite org values (environment-scoped variables join
/// per job at claim, like environment secrets do).
pub async fn variables_for_context(
    db: &Database,
    repository_id: &str,
) -> Result<serde_json::Value, String> {
    let mut vars: HashMap<String, String> = db
        .list_action_org_variables_for_repo(repository_id)
        .await?
        .into_iter()
        .map(|r| (r.name, r.value))
        .collect();
    for (k, v) in variables_for_repo(db, repository_id).await? {
        vars.insert(k, v);
    }
    let m: serde_json::Map<String, serde_json::Value> = vars
        .into_iter()
        .map(|(k, v)| (k, serde_json::Value::String(v)))
        .collect();
    Ok(serde_json::Value::Object(m))
}

/// Claim-time merged variables for one job — repo < org < environment
/// precedence (GitHub: the narrowest scope wins). Variables are plaintext;
/// the trusted/untrusted-PR gate does not apply — they are never secret —
/// but callers still gate on `pr_ctx.trusted` for parity with secrets? No:
/// GitHub exposes `vars.*` to untrusted PRs too (variables are readable in
/// `github.event` workflows regardless). Unlike secrets, variables DO flow
/// to fork-PR runs.
pub async fn variables_for_job(
    db: &Database,
    repository_id: &str,
    environment_name: Option<&str>,
) -> Result<HashMap<String, String>, String> {
    // Broadest scope first so the narrowest overwrites: org < repo < env.
    let mut out: HashMap<String, String> = db
        .list_action_org_variables_for_repo(repository_id)
        .await?
        .into_iter()
        .map(|r| (r.name, r.value))
        .collect();
    for (k, v) in variables_for_repo(db, repository_id).await? {
        out.insert(k, v);
    }
    if let Some(env_name) = environment_name {
        if let Some(env) = db.find_action_environment(repository_id, env_name).await? {
            for row in db.list_action_environment_variable_values(&env.id).await? {
                out.insert(row.name, row.value);
            }
        }
    }
    Ok(out)
}

/// "Re-run with debug logging" stamps `action_jobs.debug_logging`; claim-time
/// vars force the debug flags on for that attempt — GitHub overrides any
/// contrary `ACTIONS_*_DEBUG` configuration on a debug re-run, so this
/// inserts rather than defaults.
pub fn inject_debug_logging(vars: &mut HashMap<String, String>, debug_logging: bool) {
    if debug_logging {
        vars.insert("ACTIONS_STEP_DEBUG".into(), "true".into());
        vars.insert("ACTIONS_RUNNER_DEBUG".into(), "true".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_name_rules_match_github() {
        assert!(validate_variable_name("DEPLOY_TARGET").is_ok());
        assert!(validate_variable_name("my_var2").is_ok());
        assert!(validate_variable_name("").is_err());
        assert!(validate_variable_name("9LIVES").is_err());
        assert!(validate_variable_name("GITHUB_TOKEN").is_err());
        assert!(validate_variable_name("has-dash").is_err());
        assert!(validate_variable_name(&"x".repeat(300)).is_err());
    }

    #[test]
    fn variable_value_allows_empty_caps_at_48k() {
        assert!(validate_variable_value("").is_ok());
        assert!(validate_variable_value("v").is_ok());
        assert!(validate_variable_value(&"x".repeat(48 * 1024)).is_ok());
        assert!(validate_variable_value(&"x".repeat(48 * 1024 + 1)).is_err());
    }
}
