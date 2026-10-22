//! Repo Actions secrets crypto (D-ACT-17) — AES-256-GCM at rest; never echo plaintext on GET.

use std::collections::HashMap;

use oxidean_db::Database;

/// Actions secret key chain — dedicated var first, session secret fallback.
pub const KEY_ENVS: &[&str] = &["OXIDEAN_ACTIONS_SECRETS_KEY", "OXIDEAN_SESSION_SECRET"];

/// GitHub's secret value cap — 48 KiB per secret.
pub const MAX_SECRET_VALUE_BYTES: usize = 48 * 1024;

/// Validate a secret value: non-empty and within GitHub's 48 KiB cap.
pub fn validate_secret_value(value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err("secret value is required".into());
    }
    if value.len() > MAX_SECRET_VALUE_BYTES {
        return Err(format!(
            "secret value exceeds the {} KiB limit",
            MAX_SECRET_VALUE_BYTES / 1024
        ));
    }
    Ok(())
}

/// Validate Actions secret name — alphanumeric + `_` + `-`, ≤128 chars.
/// More permissive than GitHub (no `-`, no leading digit, no `GITHUB_`
/// prefix) — tightening is a compat-doc divergence, not a silent one.
pub fn validate_secret_name(name: &str) -> Result<(), String> {
    let n = name.trim();
    if n.is_empty() {
        return Err("secret name is required".into());
    }
    if n.len() > 128 {
        return Err("secret name too long".into());
    }
    if !n
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("secret name must be alphanumeric, underscore, or hyphen".into());
    }
    Ok(())
}

pub fn encrypt_secret(plaintext: &str) -> Result<String, String> {
    crate::crypto::encrypt(KEY_ENVS, plaintext)
}

pub fn decrypt_secret(blob: &str) -> Result<String, String> {
    crate::crypto::decrypt(KEY_ENVS, blob)
}

/// Decrypt all secrets for a repo (FetchTask injection).
pub async fn decrypted_secrets_for_repo(
    db: &Database,
    repository_id: &str,
) -> Result<HashMap<String, String>, String> {
    let rows = db.list_action_secret_ciphertexts(repository_id).await?;
    let mut out = HashMap::with_capacity(rows.len());
    for row in rows {
        out.insert(row.name, decrypt_secret(&row.ciphertext)?);
    }
    Ok(out)
}

/// What the caller knows for the decrypt-path audit record — the event lands
/// under `event_type = "actions.secrets.decrypt"` with name/count detail only
/// (never values).
pub struct SecretAudit<'a> {
    pub actor_username: &'a str,
    pub actor_id: Option<&'a str>,
    pub repository_id: &'a str,
    pub run_id: &'a str,
    pub job_id: &'a str,
}

/// Claim-time merged secrets for one job — repo < org < environment
/// precedence (GitHub: the narrowest scope wins). `environment_name` is the
/// job's resolved environment name; org secrets only exist for org-owned
/// repos (`owner_type = 'org'`) whose `visibility` covers this repo — both
/// resolved inside the queries.
///
/// `pull_request_target` / fork-PR callers must NOT pass this path: keep the
/// existing `pr_ctx.trusted` gate outside — untrusted runs skip the function
/// entirely.
pub async fn decrypted_secrets_for_job(
    db: &Database,
    repository_id: &str,
    environment_name: Option<&str>,
    audit: Option<SecretAudit<'_>>,
) -> Result<HashMap<String, String>, String> {
    // Broadest scope first so the narrowest wins on name collisions —
    // GitHub: environment > repository > organization.
    let mut out: HashMap<String, String> = HashMap::new();
    let mut org_count = 0usize;
    for row in db
        .list_action_org_secret_ciphertexts_for_repo(repository_id)
        .await?
    {
        out.insert(row.name, decrypt_secret(&row.ciphertext)?);
        org_count += 1;
    }
    let repo_rows = decrypted_secrets_for_repo(db, repository_id).await?;
    let repo_count = repo_rows.len();
    for (k, v) in repo_rows {
        out.insert(k, v);
    }
    let mut env_count = 0usize;

    if let Some(env_name) = environment_name {
        if let Some(env) = db.find_action_environment(repository_id, env_name).await? {
            for row in db
                .list_action_environment_secret_ciphertexts(&env.id)
                .await?
            {
                out.insert(row.name, decrypt_secret(&row.ciphertext)?);
                env_count += 1;
            }
        }
    }

    // Decrypt-path audit: one event per claim batch recording scopes +
    // counts — names are logged (they're metadata, listed by the UI) but
    // never values.
    if let Some(a) = audit {
        if repo_count + org_count + env_count > 0 {
            let detail = serde_json::json!({
                "repository_id": a.repository_id,
                "run_id": a.run_id,
                "job_id": a.job_id,
                "repo_secrets": repo_count,
                "org_secrets": org_count,
                "environment": environment_name,
                "environment_secrets": env_count,
            })
            .to_string();
            if let Err(e) = db
                .insert_audit_event(
                    &uuid::Uuid::new_v4().to_string(),
                    a.actor_id,
                    a.actor_username,
                    "actions.secrets.decrypt",
                    Some("repository"),
                    Some(a.repository_id),
                    Some(&detail),
                    None,
                    None,
                )
                .await
            {
                tracing::warn!(error = %e, "secret decrypt audit write failed");
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Env-var-mutating tests serialize on this — `set_var`/`remove_var` are
    /// process-global and parallel test threads otherwise race each other.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn encrypt_roundtrip() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("OXIDEAN_ACTIONS_SECRETS_KEY", "unit-test-key");
        let ct = encrypt_secret("super-secret").unwrap();
        assert!(!ct.contains("super-secret"));
        assert_eq!(decrypt_secret(&ct).unwrap(), "super-secret");
    }

    #[test]
    fn encrypt_fails_closed_without_key() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var("OXIDEAN_ACTIONS_SECRETS_KEY");
        std::env::remove_var("OXIDEAN_SESSION_SECRET");
        let err = encrypt_secret("x").unwrap_err();
        assert!(err.contains("OXIDEAN_ACTIONS_SECRETS_KEY"), "{err}");
    }

    #[test]
    fn validate_name_rejects_empty() {
        assert!(validate_secret_name("").is_err());
        assert!(validate_secret_name("API_TOKEN").is_ok());
    }
}
