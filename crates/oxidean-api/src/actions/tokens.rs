//! Registration token helpers (D-ACT-08).

use oxidean_db::Database;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub fn hash_token(raw: &str) -> String {
    let mut h = Sha256::new();
    h.update(raw.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Mint instance-scoped registration token (Admin / tests).
pub async fn mint_registration_token(db: &Database) -> Result<String, String> {
    mint_registration_token_for_scope(db, "instance", None).await
}

/// Mint a registration token bound to a scope — `("repo", Some(repo_id))`
/// produces repo-scoped runners whose job claims isolate to that repository.
pub async fn mint_registration_token_for_scope(
    db: &Database,
    scope_type: &str,
    scope_id: Option<&str>,
) -> Result<String, String> {
    let raw = format!("reg_{}", Uuid::new_v4());
    let hash = hash_token(&raw);
    db.insert_action_runner_token(
        &Uuid::new_v4().to_string(),
        &hash,
        scope_type,
        scope_id,
        true,
    )
    .await?;
    Ok(raw)
}

/// Scope carried by a valid registration token — `instance` registers
/// instance-wide runners, `repo` binds the runner to `scope_id`.
#[derive(Debug, Clone)]
pub struct RunnerTokenScope {
    pub scope_type: String,
    pub scope_id: Option<String>,
}

/// Accept either DB registration token or `OXIDEAN_RUNNER_REGISTRATION_TOKEN`
/// bootstrap. Returns the token's scope so registration binds the runner.
pub async fn accept_registration_token(
    db: &Database,
    raw: &str,
) -> Result<Option<RunnerTokenScope>, String> {
    if let Ok(env_tok) = std::env::var("OXIDEAN_RUNNER_REGISTRATION_TOKEN") {
        if !env_tok.is_empty() && env_tok == raw {
            return Ok(Some(RunnerTokenScope {
                scope_type: "instance".into(),
                scope_id: None,
            }));
        }
    }
    let hash = hash_token(raw);
    Ok(db
        .consume_action_runner_registration_token(&hash)
        .await?
        .map(|(scope_type, scope_id)| RunnerTokenScope {
            scope_type,
            scope_id,
        }))
}
