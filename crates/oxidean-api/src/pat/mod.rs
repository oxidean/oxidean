//! Personal access token RPC (`pat.createClassic` / `createFineGrained` / `list` / `revoke`).

pub mod bearer;
pub mod rate_limit;

use oxidean_core::{
    AppError, ClassicPatScope, ContentsPerm, CreateClassicPatRequest, CreateFineGrainedPatRequest,
    CreatePatResponse, FgRepoAccess, PatKind, PatListItem, CLASSIC_PAT_PREFIX,
    FINE_GRAINED_PAT_PREFIX,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::gate::require_verified;
use crate::auth::session::{bytes_to_hex, sha256_hex};
use crate::repo::{effective_capability, meets, owner_ref_for_repo, Capability};
use crate::rpc::RpcCtx;

const TOKEN_BYTES: usize = 32;

fn db_err(e: String) -> AppError {
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

/// Reject missing/blank as `None`; otherwise require RFC3339 and a future instant.
fn validate_expires_at(expires_at: Option<&str>) -> Result<Option<&str>, AppError> {
    let Some(raw) = expires_at.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let dt = chrono::DateTime::parse_from_rfc3339(raw).map_err(|_| {
        AppError::new(
            "rpc.bad_input",
            "expires_at must be a valid RFC3339 timestamp",
        )
    })?;
    if dt.with_timezone(&chrono::Utc) <= chrono::Utc::now() {
        return Err(AppError::new(
            "rpc.bad_input",
            "expires_at must be in the future",
        ));
    }
    Ok(Some(raw))
}

fn require_session_user_id(ctx: &RpcCtx) -> Result<&str, AppError> {
    ctx.session
        .as_ref()
        .map(|s| s.user_id.as_str())
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))
}

fn row_to_list_item(row: &oxidean_db::PatRow) -> Result<PatListItem, AppError> {
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
        repository_ids: row.repository_ids.clone(),
        expires_at: row.expires_at.clone(),
        last_used_at: row.last_used_at.clone(),
        last_used_ip: row.last_used_ip.clone(),
        created_at: row.created_at.clone(),
    })
}

/// Mint classic PAT: verified session, non-empty note, `repo` scope (D-08 / D-15 / D-16 / D-24).
pub async fn create_classic(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<CreatePatResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: CreateClassicPatRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pat.createClassic input: {e}"),
        )
    })?;

    if req.name.trim().is_empty() {
        return Err(AppError::new(
            "pat.note_required",
            "A note (name) is required for personal access tokens",
        ));
    }
    if !req.scopes.contains(&ClassicPatScope::Repo) {
        return Err(AppError::new(
            "pat.invalid_scope",
            "classic tokens must include the repo scope",
        ));
    }

    let mut secret_bytes = [0u8; TOKEN_BYTES];
    rand::fill(&mut secret_bytes);
    let secret_hex = bytes_to_hex(&secret_bytes);
    let plaintext = format!("{CLASSIC_PAT_PREFIX}{secret_hex}");
    let token_hash = sha256_hex(plaintext.as_bytes());
    // Display fingerprint: brand prefix + first 8 hex of secret (not the hash).
    let token_prefix = format!("{CLASSIC_PAT_PREFIX}{}", &secret_hex[..8]);

    let scopes_json =
        serde_json::to_string(&req.scopes.iter().map(|s| s.as_str()).collect::<Vec<_>>())
            .map_err(|e| AppError::new("pat.internal", format!("scopes serialize: {e}")))?;

    let expires_at = validate_expires_at(req.expires_at.as_deref())?;

    let id = Uuid::new_v4().to_string();
    ctx.db
        .create_pat(
            &id,
            &user.id,
            PatKind::Classic.as_str(),
            req.name.trim(),
            &token_prefix,
            &token_hash,
            Some(&scopes_json),
            None,
            None,
            expires_at,
            &[],
        )
        .await
        .map_err(db_err)?;

    let row = ctx
        .db
        .list_pats_for_user(&user.id)
        .await
        .map_err(db_err)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| AppError::new("pat.internal", "created pat missing from list"))?;

    Ok(CreatePatResponse {
        token: plaintext,
        item: row_to_list_item(&row)?,
    })
}

/// Mint fine-grained PAT: verified session, note, selected|all repos, contents read|write (D-04–D-08 / D-16 / D-24).
///
/// FG All (ASSUME A4): covers personal-owned + org Owner/Admin repos at authorize time
/// (no join rows). FG Selected: each repository_id must be ACL-accessible at the
/// contents permission requested (Read for contents:read, Write for contents:write).
pub async fn create_fine_grained(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<CreatePatResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: CreateFineGrainedPatRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pat.createFineGrained input: {e}"),
        )
    })?;

    if req.name.trim().is_empty() {
        return Err(AppError::new(
            "pat.note_required",
            "A note (name) is required for personal access tokens",
        ));
    }

    let need = match req.contents {
        ContentsPerm::Read => Capability::Read,
        ContentsPerm::Write => Capability::Write,
    };

    let repository_ids = match req.repo_access {
        FgRepoAccess::All => Vec::new(),
        FgRepoAccess::Selected => {
            if req.repository_ids.is_empty() {
                return Err(AppError::new(
                    "pat.repos_required",
                    "selected fine-grained tokens require at least one repository",
                ));
            }
            let mut owned = Vec::with_capacity(req.repository_ids.len());
            for repo_id in &req.repository_ids {
                let id = repo_id.trim();
                if id.is_empty() {
                    return Err(AppError::new(
                        "pat.invalid_scope",
                        "repository id must not be empty",
                    ));
                }
                let row = ctx
                    .db
                    .find_repository_by_id(id)
                    .await
                    .map_err(db_err)?
                    .ok_or_else(|| {
                        AppError::new(
                            "pat.invalid_scope",
                            "one or more repositories are not accessible for this token",
                        )
                    })?;
                let owner = owner_ref_for_repo(&ctx.db, &row)
                    .await
                    .map_err(db_err)?
                    .ok_or_else(|| {
                        AppError::new(
                            "pat.invalid_scope",
                            "one or more repositories are not accessible for this token",
                        )
                    })?;
                let capability = effective_capability(&ctx.db, Some(&user.id), &row, &owner)
                    .await
                    .map_err(db_err)?;
                if !meets(capability, need) {
                    return Err(AppError::new(
                        "pat.invalid_scope",
                        "one or more repositories are not accessible for this token",
                    ));
                }
                // Deduplicate after ACL checks (preserve order) so duplicate
                // ids cannot fail the composite PK on personal_access_token_repos.
                if !owned.iter().any(|id| id == &row.id) {
                    owned.push(row.id);
                }
            }
            owned
        }
    };

    let mut secret_bytes = [0u8; TOKEN_BYTES];
    rand::fill(&mut secret_bytes);
    let secret_hex = bytes_to_hex(&secret_bytes);
    let plaintext = format!("{FINE_GRAINED_PAT_PREFIX}{secret_hex}");
    let token_hash = sha256_hex(plaintext.as_bytes());
    // Display fingerprint: brand prefix + first 8 hex of secret (not the hash).
    let token_prefix = format!("{FINE_GRAINED_PAT_PREFIX}{}", &secret_hex[..8]);

    let expires_at = validate_expires_at(req.expires_at.as_deref())?;

    let scopes_json = match req.packages {
        Some(oxidean_core::PackagesPerm::Read) => Some(
            serde_json::to_string(&["package:read"])
                .map_err(|e| AppError::new("pat.internal", format!("scopes serialize: {e}")))?,
        ),
        Some(oxidean_core::PackagesPerm::Write) => Some(
            serde_json::to_string(&["package:write"])
                .map_err(|e| AppError::new("pat.internal", format!("scopes serialize: {e}")))?,
        ),
        None => None,
    };

    let id = Uuid::new_v4().to_string();
    ctx.db
        .create_pat(
            &id,
            &user.id,
            PatKind::FineGrained.as_str(),
            req.name.trim(),
            &token_prefix,
            &token_hash,
            scopes_json.as_deref(),
            Some(req.contents.as_str()),
            Some(req.repo_access.as_str()),
            expires_at,
            &repository_ids,
        )
        .await
        .map_err(db_err)?;

    let row = ctx
        .db
        .list_pats_for_user(&user.id)
        .await
        .map_err(db_err)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| AppError::new("pat.internal", "created pat missing from list"))?;

    Ok(CreatePatResponse {
        token: plaintext,
        item: row_to_list_item(&row)?,
    })
}

/// List active PATs for the signed-in user (session required; email verify not required).
pub async fn list(ctx: &RpcCtx) -> Result<Vec<PatListItem>, AppError> {
    let user_id = require_session_user_id(ctx)?;
    let rows = ctx.db.list_pats_for_user(user_id).await.map_err(db_err)?;
    let mut items = Vec::with_capacity(rows.len());
    for row in &rows {
        items.push(row_to_list_item(row)?);
    }
    Ok(items)
}

#[derive(Debug, Deserialize)]
struct RevokePatRequest {
    id: String,
}

/// Soft-revoke a PAT owned by the signed-in user.
pub async fn revoke(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let user_id = require_session_user_id(ctx)?;
    let req: RevokePatRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid pat.revoke input: {e}")))?;
    let id = req.id.trim();
    if id.is_empty() {
        return Err(AppError::new(
            "pat.not_found",
            "personal access token not found",
        ));
    }

    let owned = ctx
        .db
        .list_pats_for_user(user_id)
        .await
        .map_err(db_err)?
        .into_iter()
        .any(|p| p.id == id);
    if !owned {
        return Err(AppError::new(
            "pat.not_found",
            "personal access token not found",
        ));
    }

    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    ctx.db.revoke_pat(id, &now).await.map_err(db_err)?;
    Ok(serde_json::json!({ "ok": true }))
}
