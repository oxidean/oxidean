//! Shared machinery retained in core after the RPC handlers moved to
//! `oxidean-api-pat` (#162).

pub mod bearer;
pub mod policy;
pub mod rate_limit;

use crate::auth::session::{bytes_to_hex, sha256_hex};
use crate::rpc::RpcCtx;
use oxidean_core::{AppError, ClassicPatScope, FgPerms, PatKind, PatListItem, CLASSIC_PAT_PREFIX};
use uuid::Uuid;

pub const TOKEN_BYTES: usize = 32;

pub fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("pat db error: {e}");
        AppError::new("pat.internal", "personal access token operation failed")
    }
}

/// Apply the instance expiration policy to a create request's `expires_at`
/// (omitted → default TTL; `"never"` gated by policy; explicit → capped).
pub async fn resolve_expires_at_db(
    db: &oxidean_db::Database,
    expires_at: Option<&str>,
) -> Result<Option<String>, AppError> {
    let policy = policy::effective_policy(db).await;
    policy::resolve_expires_at(&policy, expires_at)
}

pub fn require_session_user_id(ctx: &RpcCtx) -> Result<&str, AppError> {
    ctx.session
        .as_ref()
        .map(|s| s.user_id.as_str())
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))
}

pub fn row_to_list_item(row: &oxidean_db::PatRow) -> Result<PatListItem, AppError> {
    let kind = PatKind::parse(&row.kind).map_err(|e| AppError::new("pat.internal", e))?;
    let scopes = match row.scopes_json.as_deref() {
        Some(raw) => {
            let names: Vec<String> = serde_json::from_str(raw).map_err(|e| {
                tracing::error!(error = %e, "invalid scopes_json");
                AppError::new("pat.internal", "corrupt pat scopes")
            })?;
            let mut out = Vec::with_capacity(names.len());
            for n in names {
                out.push(ClassicPatScope::parse(&n).map_err(|e| AppError::new("pat.internal", e))?);
            }
            Some(out)
        }
        None => None,
    };
    let contents = match row.contents_perm.as_deref() {
        Some(s) => Some(
            oxidean_core::ContentsPerm::parse(s).map_err(|e| AppError::new("pat.internal", e))?,
        ),
        None => None,
    };
    let packages = match kind {
        PatKind::FineGrained => scopes.as_ref().and_then(|sc| {
            if sc
                .iter()
                .any(|s| matches!(s, ClassicPatScope::PackageWrite))
            {
                Some(oxidean_core::PackagesPerm::Write)
            } else if sc.iter().any(|s| matches!(s, ClassicPatScope::PackageRead)) {
                Some(oxidean_core::PackagesPerm::Read)
            } else {
                None
            }
        }),
        PatKind::Classic => None,
    };
    let repo_access = match row.repo_access.as_deref() {
        Some(s) => Some(
            oxidean_core::FgRepoAccess::parse(s).map_err(|e| AppError::new("pat.internal", e))?,
        ),
        None => None,
    };
    let perms = match row.perms_json.as_deref() {
        Some(raw) => serde_json::from_str::<FgPerms>(raw).map_err(|e| {
            tracing::error!(error = %e, "invalid perms_json");
            AppError::new("pat.internal", "corrupt pat permissions")
        })?,
        None => FgPerms::default(),
    };
    Ok(PatListItem {
        id: row.id.clone(),
        kind,
        name: row.name.clone(),
        token_prefix: row.token_prefix.clone(),
        scopes: match kind {
            PatKind::Classic => scopes,
            PatKind::FineGrained => None,
        },
        contents,
        packages,
        repo_access,
        perms,
        repository_ids: row.repository_ids.clone(),
        expires_at: row.expires_at.clone(),
        last_used_at: row.last_used_at.clone(),
        last_used_ip: row.last_used_ip.clone(),
        created_at: row.created_at.clone(),
    })
}

/// Insert a classic PAT row for `user_id` and return the plaintext token.
/// Shared by `pat.createClassic` (session-gated RPC) and the OAuth device-grant
/// token exchange, which mints the PAT the CLI stores.
pub async fn mint_classic_pat(
    db: &oxidean_db::Database,
    user_id: &str,
    name: &str,
    scopes: &[ClassicPatScope],
    expires_at: Option<&str>,
) -> Result<(String, String), AppError> {
    let mut secret_bytes = [0u8; TOKEN_BYTES];
    rand::fill(&mut secret_bytes);
    let secret_hex = bytes_to_hex(&secret_bytes);
    let plaintext = format!("{CLASSIC_PAT_PREFIX}{secret_hex}");
    let token_hash = sha256_hex(plaintext.as_bytes());
    // Display fingerprint: brand prefix + first 8 hex of secret (not the hash).
    let token_prefix = format!("{CLASSIC_PAT_PREFIX}{}", &secret_hex[..8]);

    let scopes_json = serde_json::to_string(&scopes.iter().map(|s| s.as_str()).collect::<Vec<_>>())
        .map_err(|e| AppError::new("pat.internal", format!("scopes serialize: {e}")))?;

    let id = Uuid::new_v4().to_string();
    db.create_pat(
        &id,
        user_id,
        PatKind::Classic.as_str(),
        name,
        &token_prefix,
        &token_hash,
        Some(&scopes_json),
        None,
        None,
        None,
        expires_at,
        &[],
    )
    .await
    .map_err(db_err)?;
    Ok((id, plaintext))
}
