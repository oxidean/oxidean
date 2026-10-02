//! Admin auth settings RPC (`admin.auth.*`) — D-09, T-04-21/22.

use std::sync::Arc;

use std::path::Path;

use oxidean_core::{
    AdminGitSettingsPublic, AdminGitUpdateSettingsRequest, AdminLfsSettingsPublic,
    AdminLfsUpdateSettingsRequest, AdminLfsUsageResponse,
    AdminLfsOwnerUsageEntry, AdminLfsRepoUsageEntry, AppError, AuthSettingsPublic,
    EmailProviderKind, FactoryResetRequest, FactoryResetResponse, FactoryResetScope, ProviderMode,
    RepoVisibility, UpdateAuthSettingsRequest,
};
use oxidean_db::AuthSettingsRow;

use crate::auth::bootstrap;
use crate::email::{self, EmailSender};
use crate::rpc::{CookieChange, RpcCtx};

fn env_nonempty(key: &str) -> bool {
    std::env::var(key)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

fn parse_provider_mode(s: &str) -> Result<ProviderMode, AppError> {
    match s {
        "local" => Ok(ProviderMode::Local),
        "workos" => Ok(ProviderMode::Workos),
        "oidc" => Ok(ProviderMode::Oidc),
        other => Err(AppError::new(
            "admin.invalid_provider_mode",
            format!("invalid provider_mode: {other}"),
        )),
    }
}

fn parse_email_provider(s: &str) -> Result<EmailProviderKind, AppError> {
    match s {
        "log" => Ok(EmailProviderKind::Log),
        "smtp" => Ok(EmailProviderKind::Smtp),
        "resend" => Ok(EmailProviderKind::Resend),
        other => Err(AppError::new(
            "admin.invalid_email_provider",
            format!("invalid email_provider: {other}"),
        )),
    }
}

fn mode_str(m: ProviderMode) -> &'static str {
    match m {
        ProviderMode::Local => "local",
        ProviderMode::Workos => "workos",
        ProviderMode::Oidc => "oidc",
    }
}

fn email_str(e: EmailProviderKind) -> &'static str {
    match e {
        EmailProviderKind::Log => "log",
        EmailProviderKind::Smtp => "smtp",
        EmailProviderKind::Resend => "resend",
    }
}

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("admin auth settings db error: {e}");
        AppError::new("admin.internal", "auth settings operation failed")
    }
}

pub(crate) async fn require_admin(ctx: &RpcCtx) -> Result<(), AppError> {
    let Some(session) = &ctx.session else {
        return Err(AppError::new(
            "auth.unauthenticated",
            "not authenticated",
        ));
    };
    let user = ctx
        .db
        .find_user_by_id(&session.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))?;
    if !user.role.is_sys_admin() {
        return Err(AppError::new(
            "admin.forbidden",
            "You need system admin access to manage auth settings.",
        ));
    }
    if user.banned_at.is_some() {
        return Err(AppError::new(
            "auth.banned",
            "This account has been suspended.",
        ));
    }
    Ok(())
}

/// Map DB row + ENV presence to public DTO (never includes secret values).
pub fn settings_to_public(row: &AuthSettingsRow) -> Result<AuthSettingsPublic, AppError> {
    let workos_client_id = row
        .workos_client_id
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("WORKOS_CLIENT_ID").ok().filter(|s| !s.is_empty()));

    let oidc_issuer = row
        .oidc_issuer
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::env::var("OXIDEAN_OIDC_ISSUER")
                .ok()
                .filter(|s| !s.is_empty())
        });

    let oidc_client_id = row
        .oidc_client_id
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::env::var("OXIDEAN_OIDC_CLIENT_ID")
                .ok()
                .filter(|s| !s.is_empty())
        });

    Ok(AuthSettingsPublic {
        provider_mode: parse_provider_mode(&row.provider_mode)?,
        email_provider: parse_email_provider(&row.email_provider)?,
        from_address: row.from_address.clone(),
        workos_client_id,
        oidc_issuer,
        oidc_client_id,
        smtp_configured: env_nonempty("OXIDEAN_SMTP_URL"),
        resend_configured: env_nonempty("OXIDEAN_RESEND_API_KEY"),
        workos_api_key_configured: env_nonempty("WORKOS_API_KEY"),
        oidc_client_secret_configured: env_nonempty("OXIDEAN_OIDC_CLIENT_SECRET"),
        allow_signup: row.allow_signup,
        default_visibility: RepoVisibility::parse(&row.default_visibility)
            .unwrap_or(RepoVisibility::Public),
    })
}

pub async fn get_settings(ctx: &RpcCtx) -> Result<AuthSettingsPublic, AppError> {
    require_admin(ctx).await?;
    let row = ctx.db.get_auth_settings().await.map_err(db_err)?;
    settings_to_public(&row)
}

pub async fn update_settings(
    ctx: &mut RpcCtx,
    input: serde_json::Value,
) -> Result<AuthSettingsPublic, AppError> {
    require_admin(ctx).await?;
    let req: UpdateAuthSettingsRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid update_settings input: {e}"),
        )
    })?;

    // Persist allow_signup + default_visibility from admin update request (06-01 / D-08).
    let row = ctx
        .db
        .update_auth_settings(
            mode_str(req.provider_mode),
            email_str(req.email_provider),
            req.from_address.as_deref(),
            req.oidc_issuer.as_deref(),
            req.oidc_client_id.as_deref(),
            req.workos_client_id.as_deref(),
            req.allow_signup,
            req.default_visibility.as_str(),
        )
        .await
        .map_err(db_err)?;

    // Rebuild email sender from new settings + ENV secrets (never stored in DB).
    let rebuilt: Arc<dyn EmailSender> = email::build_email_sender_for_settings(&row);
    if let Ok(mut slot) = ctx.email_slot.write() {
        *slot = rebuilt.clone();
    }
    ctx.email = rebuilt;

    settings_to_public(&row)
}

/// Wipe all users/sessions and restore empty-instance setup (sys-admin only).
///
/// Scope (D-34): `database_only` (default) keeps bare repos on disk;
/// `database_and_repositories` also deletes children under `repos_dir`
/// and `release_assets_dir` (D-REL-04 / Phase 15).
pub async fn factory_reset(
    ctx: &mut RpcCtx,
    input: serde_json::Value,
) -> Result<FactoryResetResponse, AppError> {
    require_admin(ctx).await?;
    let req: FactoryResetRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid factory_reset input: {e}"),
        )
    })?;
    if req.confirmation.trim() != "RESET" {
        return Err(AppError::new(
            "admin.factory_reset_confirm",
            "Type RESET to confirm wiping this instance.",
        ));
    }

    ctx.db.factory_reset_instance().await.map_err(db_err)?;

    // D-ACT-19: always clear Actions logs when run metadata is wiped.
    wipe_dir_contents(
        &ctx.actions_log_dir,
        "admin.factory_reset_actions_logs",
        "Actions log storage",
    )
    .await?;

    if matches!(req.scope, FactoryResetScope::DatabaseAndRepositories) {
        wipe_repos_dir_contents(&ctx.repos_dir).await?;
        crate::jobs::wipe_lfs_dir_contents(&ctx.lfs_dir)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "wipe lfs_dir failed");
                AppError::new(
                    "admin.factory_reset_lfs",
                    "Failed to wipe LFS storage.",
                )
            })?;
        wipe_dir_contents(
            &ctx.release_assets_dir,
            "admin.factory_reset_release_assets",
            "release asset storage",
        )
        .await?;
    }

    ctx.set_cookie = Some(CookieChange::Clear);

    let needs = bootstrap::needs_setup(&ctx.db).await?;
    tracing::warn!(
        needs_setup = needs,
        scope = ?req.scope,
        "instance factory reset completed"
    );
    Ok(FactoryResetResponse {
        ok: true,
        needs_setup: needs,
    })
}

/// Sys-admin manual `git gc` for one repo or all active repos (D-37).
pub async fn repo_gc(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<oxidean_core::RepoGcResponse, AppError> {
    require_admin(ctx).await?;
    let req: oxidean_core::RepoGcRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid repo_gc input: {e}"))
    })?;

    let owner = req.owner.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let name = req.name.as_deref().map(str::trim).filter(|s| !s.is_empty());

    match (owner, name) {
        (Some(owner), Some(name)) => {
            let path = crate::git::bare_repo_path(&ctx.repos_dir, owner, name)?;
            crate::jobs::run_gc_one(ctx.git.as_ref(), &ctx.repos_dir, &path)
                .await
                .map_err(|e| {
                    tracing::warn!(error = %e, "manual gc failed");
                    AppError::new("admin.repo_gc_failed", "git gc failed for repository")
                })?;
            Ok(oxidean_core::RepoGcResponse {
                ok: true,
                gc_count: 1,
                error_count: 0,
            })
        }
        (None, None) => {
            let (ok, err) =
                crate::jobs::run_gc_all(&ctx.db, ctx.git.as_ref(), &ctx.repos_dir)
                    .await
                    .map_err(|e| {
                        tracing::error!(error = %e, "manual gc-all failed");
                        AppError::new("admin.repo_gc_failed", "git gc failed")
                    })?;
            Ok(oxidean_core::RepoGcResponse {
                ok: err == 0,
                gc_count: ok,
                error_count: err,
            })
        }
        _ => Err(AppError::new(
            "rpc.bad_input",
            "Provide both owner and name, or omit both to gc all repositories.",
        )),
    }
}

/// Delete all entries under `repos_dir` after canonicalizing (T-07-26).
/// Refuses paths that escape the repos root. Leaves the root directory itself
/// (volume mount point) in place.
pub async fn wipe_repos_dir_contents(repos_dir: &Path) -> Result<(), AppError> {
    wipe_dir_contents(repos_dir, "admin.factory_reset_repos", "repository storage").await
}

/// Delete all entries under a configured storage root after canonicalizing.
/// Refuses paths that escape the root. Leaves the root directory itself
/// (volume mount point) in place.
pub async fn wipe_dir_contents(dir: &Path, err_code: &str, label: &str) -> Result<(), AppError> {
    if !dir.exists() {
        return Ok(());
    }
    let root = tokio::fs::canonicalize(dir).await.map_err(|e| {
        tracing::error!(error = %e, path = %dir.display(), label, "canonicalize storage dir failed");
        AppError::new(
            err_code,
            format!("Failed to resolve {label} path."),
        )
    })?;

    let mut entries = tokio::fs::read_dir(&root).await.map_err(|e| {
        tracing::error!(error = %e, path = %root.display(), label, "read_dir storage failed");
        AppError::new(err_code, format!("Failed to list {label}."))
    })?;

    while let Some(entry) = entries.next_entry().await.map_err(|e| {
        tracing::error!(error = %e, label, "storage dir entry read failed");
        AppError::new(
            err_code,
            format!("Failed to read {label} entry."),
        )
    })? {
        let path = entry.path();
        let canon = match tokio::fs::canonicalize(&path).await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(error = %e, path = %path.display(), label, "canonicalize child failed");
                return Err(AppError::new(
                    err_code,
                    format!("Failed to resolve a {label} path."),
                ));
            }
        };
        if !path_is_under(&canon, &root) {
            tracing::error!(
                path = %canon.display(),
                root = %root.display(),
                label,
                "refusing to delete path outside storage root"
            );
            return Err(AppError::new(
                err_code,
                format!("Refusing to delete a path outside {label}."),
            ));
        }
        let ft = entry.file_type().await.map_err(|e| {
            tracing::error!(error = %e, path = %canon.display(), label, "file_type failed");
            AppError::new(
                err_code,
                format!("Failed to inspect {label} entry."),
            )
        })?;
        if ft.is_dir() {
            tokio::fs::remove_dir_all(&canon).await.map_err(|e| {
                tracing::error!(error = %e, path = %canon.display(), label, "remove_dir_all failed");
                AppError::new(
                    err_code,
                    format!("Failed to delete {label} directories."),
                )
            })?;
        } else {
            tokio::fs::remove_file(&canon).await.map_err(|e| {
                tracing::error!(error = %e, path = %canon.display(), label, "remove_file failed");
                AppError::new(
                    err_code,
                    format!("Failed to delete a file under {label}."),
                )
            })?;
        }
    }
    Ok(())
}

/// `admin.lfs.getSettings` — effective limits + override flags (D-LFS-13).
pub async fn lfs_get_settings(ctx: &RpcCtx) -> Result<AdminLfsSettingsPublic, AppError> {
    require_admin(ctx).await?;
    let row = ctx.db.get_lfs_settings().await.map_err(db_err)?;
    let max = row.max_object_bytes.unwrap_or_else(|| {
        std::env::var("OXIDEAN_LFS_MAX_OBJECT_BYTES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(crate::lfs::quota::DEFAULT_MAX_OBJECT_BYTES)
    });
    let repo_q = row.quota_repo_bytes.unwrap_or_else(|| {
        std::env::var("OXIDEAN_LFS_QUOTA_REPO_BYTES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(crate::lfs::quota::DEFAULT_QUOTA_REPO_BYTES)
    });
    let user_q = row.quota_user_bytes.unwrap_or_else(|| {
        std::env::var("OXIDEAN_LFS_QUOTA_USER_BYTES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(crate::lfs::quota::DEFAULT_QUOTA_USER_BYTES)
    });
    Ok(AdminLfsSettingsPublic {
        max_object_bytes: max,
        quota_repo_bytes: repo_q,
        quota_user_bytes: user_q,
        max_object_bytes_overridden: row.max_object_bytes.is_some(),
        quota_repo_bytes_overridden: row.quota_repo_bytes.is_some(),
        quota_user_bytes_overridden: row.quota_user_bytes.is_some(),
    })
}

/// `admin.lfs.updateSettings` — persist overrides (null clears when clear_overrides).
pub async fn lfs_update_settings(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<AdminLfsSettingsPublic, AppError> {
    require_admin(ctx).await?;
    let req: AdminLfsUpdateSettingsRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid admin.lfs.updateSettings input: {e}"),
        )
    })?;
    if req.clear_overrides {
        ctx.db
            .update_lfs_settings(None, None, None)
            .await
            .map_err(db_err)?;
    } else {
        let current = ctx.db.get_lfs_settings().await.map_err(db_err)?;
        ctx.db
            .update_lfs_settings(
                req.max_object_bytes.or(current.max_object_bytes),
                req.quota_repo_bytes.or(current.quota_repo_bytes),
                req.quota_user_bytes.or(current.quota_user_bytes),
            )
            .await
            .map_err(db_err)?;
    }
    lfs_get_settings(ctx).await
}

/// `admin.lfs.getUsage` — instance physical + logical breakdown (D-LFS-19).
pub async fn lfs_get_usage(ctx: &RpcCtx) -> Result<AdminLfsUsageResponse, AppError> {
    require_admin(ctx).await?;
    let physical_bytes = ctx.db.lfs_physical_bytes().await.map_err(db_err)?;
    let object_count = ctx.db.instance_lfs_object_count().await.map_err(db_err)?;
    let logical_bytes = ctx.db.lfs_instance_logical_bytes().await.map_err(db_err)?;
    let by_repo = ctx
        .db
        .lfs_usage_by_repo(100)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(|r| AdminLfsRepoUsageEntry {
            repository_id: r.repository_id,
            owner: r.owner_slug,
            name: r.name,
            object_count: r.object_count,
            logical_bytes: r.logical_bytes,
        })
        .collect();
    let by_owner = ctx
        .db
        .lfs_usage_by_owner(100)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(|r| AdminLfsOwnerUsageEntry {
            owner_id: r.owner_id,
            owner_slug: r.owner_slug,
            object_count: r.object_count,
            logical_bytes: r.logical_bytes,
        })
        .collect();
    Ok(AdminLfsUsageResponse {
        physical_bytes,
        object_count,
        logical_bytes,
        by_repo,
        by_owner,
    })
}

/// `admin.git.getSettings` — instance git object size quota default (GIT-25).
pub async fn git_get_settings(ctx: &RpcCtx) -> Result<AdminGitSettingsPublic, AppError> {
    require_admin(ctx).await?;
    let row = ctx.db.get_git_settings().await.map_err(db_err)?;
    let repo_q = row.repo_quota_bytes.unwrap_or_else(|| {
        std::env::var(crate::git::quota::REPO_QUOTA_ENV)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(crate::git::quota::DEFAULT_REPO_QUOTA_BYTES)
    });
    Ok(AdminGitSettingsPublic {
        repo_quota_bytes: repo_q,
        repo_quota_bytes_overridden: row.repo_quota_bytes.is_some(),
    })
}

/// `admin.git.updateSettings` — persist the instance quota override
/// (`clear_overrides` reverts to env/built-in default).
pub async fn git_update_settings(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<AdminGitSettingsPublic, AppError> {
    require_admin(ctx).await?;
    let req: AdminGitUpdateSettingsRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid admin.git.updateSettings input: {e}"),
        )
    })?;
    if req.clear_overrides {
        ctx.db.update_git_settings(None).await.map_err(db_err)?;
    } else {
        let current = ctx.db.get_git_settings().await.map_err(db_err)?;
        ctx.db
            .update_git_settings(req.repo_quota_bytes.or(current.repo_quota_bytes))
            .await
            .map_err(db_err)?;
    }
    git_get_settings(ctx).await
}

fn path_is_under(path: &Path, root: &Path) -> bool {
    path.strip_prefix(root).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_reset_scope_defaults_database_only() {
        let req: FactoryResetRequest =
            serde_json::from_str(r#"{"confirmation":"RESET"}"#).expect("parse");
        assert_eq!(req.scope, FactoryResetScope::DatabaseOnly);
        let both: FactoryResetRequest = serde_json::from_str(
            r#"{"confirmation":"RESET","scope":"database_and_repositories"}"#,
        )
        .expect("parse");
        assert_eq!(both.scope, FactoryResetScope::DatabaseAndRepositories);
    }

    #[tokio::test]
    async fn wipe_repos_dir_removes_children_keeps_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("repos");
        tokio::fs::create_dir_all(root.join("owner").join("a.git"))
            .await
            .unwrap();
        tokio::fs::write(root.join("stray.txt"), b"x").await.unwrap();
        wipe_repos_dir_contents(&root).await.expect("wipe");
        assert!(root.exists(), "root mount dir must remain");
        assert!(
            tokio::fs::read_dir(&root)
                .await
                .unwrap()
                .next_entry()
                .await
                .unwrap()
                .is_none(),
            "children must be gone"
        );
    }

    #[tokio::test]
    async fn factory_reset_wipe_release_assets_removes_children_keeps_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("release-assets");
        tokio::fs::create_dir_all(&root).await.unwrap();
        tokio::fs::write(root.join("asset-aaa"), b"bin").await.unwrap();
        wipe_dir_contents(
            &root,
            "admin.factory_reset_release_assets",
            "release asset storage",
        )
        .await
        .expect("wipe");
        assert!(root.exists(), "release-assets mount dir must remain");
        assert!(
            tokio::fs::read_dir(&root)
                .await
                .unwrap()
                .next_entry()
                .await
                .unwrap()
                .is_none(),
            "release asset children must be gone"
        );
    }

    #[test]
    fn path_is_under_rejects_sibling_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("repos");
        let sibling = tmp.path().join("repos-evil");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&sibling).unwrap();
        let root_c = root.canonicalize().unwrap();
        let sib_c = sibling.canonicalize().unwrap();
        assert!(!path_is_under(&sib_c, &root_c));
        assert!(path_is_under(&root_c.join("owner"), &root_c) || {
            // join may not exist — create then check
            std::fs::create_dir_all(root.join("owner")).unwrap();
            path_is_under(&root.join("owner").canonicalize().unwrap(), &root_c)
        });
    }

    #[test]
    fn public_settings_exposes_client_id_not_secrets() {
        let row = AuthSettingsRow {
            provider_mode: "local".into(),
            email_provider: "log".into(),
            from_address: Some("Oxidean <noreply@test>".into()),
            oidc_issuer: None,
            oidc_client_id: None,
            workos_client_id: Some("client_abc".into()),
            allow_signup: false,
            default_visibility: "public".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        };
        let pub_ = settings_to_public(&row).expect("map");
        assert_eq!(pub_.provider_mode, ProviderMode::Local);
        assert_eq!(pub_.workos_client_id.as_deref(), Some("client_abc"));
        assert!(!pub_.allow_signup, "D-07 fail-closed default on row");
        let json = serde_json::to_string(&pub_).unwrap();
        assert!(json.contains("workos_api_key_configured"));
        assert!(json.contains("smtp_configured"));
        assert!(json.contains("oidc_client_secret_configured"));
        assert!(json.contains("\"allow_signup\":false"));
        // Boolean badge only — no actual secret value fields.
        assert!(!json.contains("\"api_key\":"));
        assert!(!json.contains("\"client_secret\":"));
        assert!(!json.contains("\"password\":"));
    }
}
