//! Bearer-token (personal access token) auth for the typed RPC edge (API-02).
//!
//! `Authorization: Bearer <pat>` is accepted on `POST /api/rpc` and
//! `GET /api/rpc/ws` when no `oxidean_session` cookie is present — the session
//! cookie always wins when both are sent. Verification mirrors Smart HTTP Basic
//! PAT auth (see `routes::git_smart_http::authenticate_pat`): SHA-256 lookup,
//! expiry + banned-owner checks, and the shared failed-auth limiter (D-26).
//!
//! A resolved PAT produces a *session-equivalent* identity: `RpcCtx.session`
//! carries the token owner's `user_id` so existing capability/ACL gates apply
//! unchanged, while `RpcCtx.pat` carries the parsed token permissions that
//! [`authorize_rpc`] enforces at dispatch time (see [`classify`]).
//!
//! Scope model (docs/API.md):
//! - Classic `repo` → all repo-domain procedures (read, write, administration
//!   short of org/instance admin) plus account-scoped repo reads.
//! - Classic `package:read` / `package:write` → `packages.list` /
//!   `packages.deleteVersion` RPC.
//! - Fine-grained `contents:read|write` → repo-domain read/write procedures on
//!   repositories covered by `repo_access` (`selected` ids, or `all` =
//!   personal-owned + org Owner/Admin — ASSUME A4). Public repos stay readable.
//! - Fine-grained `packages:read|write` → `packages.list` when `repository_id`
//!   names a covered repository.
//! - Admin (`admin.*`, `packages.admin*`), session lifecycle (`auth.*` other
//!   than `me`/`provider_config`/`bootstrap_status`), credential management
//!   (`pat.*`, `sshKey.*`, `gpgKey.*`, `email.*`), and org mutations are never
//!   callable with a PAT — fail-closed default for new procedures.
//! - Bearer responses never carry `Set-Cookie` and never mint sessions.

use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Utc};
use oxidean_core::{
    AppError, ClassicPatScope, ContentsPerm, FgRepoAccess, PatKind, CLASSIC_PAT_PREFIX,
    FINE_GRAINED_PAT_PREFIX,
};
use oxidean_db::{Database, PatRow, RepositoryRow};

use crate::auth::session::{sha256_hex, ResolvedSession, SESSION_IDLE};
use crate::pat::rate_limit::FailedAuthLimiter;
use crate::repo::{
    fg_all_covers_repo, is_private_visibility, lookup_repo_row_or_redirect, not_found,
    owner_ref_for_repo, OwnerRef,
};
use crate::rpc::RpcCtx;

/// Parsed PAT identity carried on `RpcCtx` for dispatch-level scope checks.
#[derive(Debug, Clone)]
pub struct PatIdentity {
    /// `personal_access_tokens.id` — used for `last_used` bookkeeping and as
    /// the synthesized session id (rate-limit key for `user.lookup`).
    pub token_id: String,
    /// Token owner's user id (PAT acts on the owner's behalf).
    pub user_id: String,
    pub kind: PatKind,
    /// Classic scope names, or FG package perms (`package:read`/`package:write`
    /// are stored in `scopes_json` for both kinds — see `pat::row_to_list_item`).
    pub scopes: Vec<ClassicPatScope>,
    /// FG `contents` permission (classic: `None`).
    pub contents: Option<ContentsPerm>,
    /// FG `repo_access` mode (classic: `None`).
    pub repo_access: Option<FgRepoAccess>,
    /// FG `selected` repository ids (empty for `all` / classic).
    pub repository_ids: Vec<String>,
}

/// Failed Bearer resolution — mapped to an HTTP response by the edge, or to an
/// `RpcResponse` frame on the WebSocket path.
#[derive(Debug)]
pub struct BearerRejection {
    pub error: AppError,
    /// Seconds for `Retry-After` when the failed-auth limiter tripped.
    pub retry_after: Option<u64>,
}

impl BearerRejection {
    fn unauthenticated(message: &str) -> Self {
        Self {
            error: AppError::new("auth.unauthenticated", message),
            retry_after: None,
        }
    }

    fn rate_limited(retry_after: Duration) -> Self {
        Self {
            error: AppError::new(
                "auth.rate_limited",
                "Too many failed authentication attempts. Try again later.",
            ),
            retry_after: Some(retry_after.as_secs().max(1)),
        }
    }

    /// HTTP status for the edge response (kept in sync with `rpc_status`).
    pub fn status(&self) -> axum::http::StatusCode {
        if self.retry_after.is_some() {
            axum::http::StatusCode::TOO_MANY_REQUESTS
        } else {
            axum::http::StatusCode::UNAUTHORIZED
        }
    }
}

fn looks_like_pat(token: &str) -> bool {
    token.starts_with(CLASSIC_PAT_PREFIX) || token.starts_with(FINE_GRAINED_PAT_PREFIX)
}

fn lock(limiter: &Mutex<FailedAuthLimiter>) -> std::sync::MutexGuard<'_, FailedAuthLimiter> {
    limiter.lock().unwrap_or_else(|e| e.into_inner())
}

fn pat_expired(expires_at: &Option<String>) -> bool {
    let Some(raw) = expires_at.as_deref() else {
        return false;
    };
    match DateTime::parse_from_rfc3339(raw) {
        Ok(dt) => dt.with_timezone(&Utc) <= Utc::now(),
        Err(_) => true,
    }
}

fn parse_scopes(scopes_json: Option<&str>) -> Vec<ClassicPatScope> {
    let Some(raw) = scopes_json else {
        return Vec::new();
    };
    let Ok(names) = serde_json::from_str::<Vec<String>>(raw) else {
        tracing::error!("corrupt pat scopes_json on authenticated token");
        return Vec::new();
    };
    names
        .iter()
        .filter_map(|n| ClassicPatScope::parse(n).ok())
        .collect()
}

#[allow(clippy::result_large_err)]
/// Resolve `Authorization: Bearer <pat>` to a session-equivalent identity.
///
/// Returns `Err` for any presented-but-invalid token — a Bearer header is an
/// explicit credential, so failures are never silently downgraded to anonymous.
/// The caller must only invoke this when no session cookie is present.
pub async fn resolve_bearer(
    db: &Database,
    limiter: &Mutex<FailedAuthLimiter>,
    token: &str,
    ip: Option<&str>,
) -> Result<(ResolvedSession, PatIdentity), BearerRejection> {
    let ip = ip.unwrap_or("unknown");
    // D-26: IP failed-auth gate before any token work.
    if let Err(retry) = lock(limiter).check_ip(ip) {
        return Err(BearerRejection::rate_limited(retry));
    }

    if !looks_like_pat(token) {
        lock(limiter).record_ip(ip);
        return Err(BearerRejection::unauthenticated("invalid bearer token"));
    }

    let token_hash = sha256_hex(token.as_bytes());
    let pat = match db.find_pat_by_token_hash(&token_hash).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            lock(limiter).record_ip(ip);
            return Err(BearerRejection::unauthenticated("invalid bearer token"));
        }
        Err(e) => {
            tracing::error!(error = %e, "find_pat_by_token_hash failed");
            return Err(BearerRejection::unauthenticated("authentication failed"));
        }
    };

    // Per-user failed-auth gate once the token maps to an account (D-26).
    if let Err(retry) = lock(limiter).check_user(&pat.user_id) {
        return Err(BearerRejection::rate_limited(retry));
    }
    let fail_all = |pat: &PatRow| {
        let mut lim = lock(limiter);
        lim.record_ip(ip);
        lim.record_user(&pat.user_id);
    };

    if pat_expired(&pat.expires_at) {
        fail_all(&pat);
        return Err(BearerRejection::unauthenticated(
            "personal access token expired",
        ));
    }
    let owner = match db.find_user_by_id(&pat.user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            fail_all(&pat);
            return Err(BearerRejection::unauthenticated("invalid bearer token"));
        }
        Err(e) => {
            tracing::error!(error = %e, "find_user_by_id failed");
            return Err(BearerRejection::unauthenticated("authentication failed"));
        }
    };
    if owner.banned_at.is_some() {
        fail_all(&pat);
        return Err(BearerRejection::unauthenticated(
            "This account has been suspended.",
        ));
    }
    // Successful auth clears the user bucket only (D-26).
    lock(limiter).clear_user(&owner.id);

    let kind = PatKind::parse(&pat.kind).unwrap_or(PatKind::Classic);
    let expires_at = pat
        .expires_at
        .as_deref()
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| {
            Utc::now()
                + chrono::Duration::from_std(SESSION_IDLE)
                    .unwrap_or_else(|_| chrono::Duration::hours(24))
        });
    let identity = PatIdentity {
        token_id: pat.id.clone(),
        user_id: owner.id.clone(),
        kind,
        scopes: parse_scopes(pat.scopes_json.as_deref()),
        contents: pat
            .contents_perm
            .as_deref()
            .and_then(|s| ContentsPerm::parse(s).ok()),
        repo_access: pat
            .repo_access
            .as_deref()
            .and_then(|s| FgRepoAccess::parse(s).ok()),
        repository_ids: pat.repository_ids.clone(),
    };
    let session = ResolvedSession {
        session_id: format!("pat:{}", pat.id),
        user_id: owner.id,
        remember_me: false,
        expires_at,
    };
    Ok((session, identity))
}

/// Best-effort `last_used_at`/`last_used_ip` update after a resolved Bearer call.
pub async fn touch_last_used(db: &Database, pat_id: &str, ip: Option<&str>) {
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    if let Err(e) = db.touch_pat_last_used(pat_id, &now, ip).await {
        tracing::warn!(error = %e, pat_id, "touch_pat_last_used failed");
    }
}

/// Procedure classification for PAT callers (fail-closed: unlisted →
/// `SessionOnly`). Applied only when `ctx.pat` is set — cookie sessions are
/// unaffected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatProc {
    /// Anonymous-safe procedures — callable by any valid PAT.
    Open,
    /// Own-identity read (`auth.me`, `user.get_profile`) — any valid PAT.
    Identity,
    /// Repo-domain read. Classic `repo`; FG `contents:read|write` on a covered
    /// repo (public repos readable regardless of selection).
    RepoRead,
    /// Repo-domain write. Classic `repo`; FG `contents:write` on a covered repo.
    RepoWrite,
    /// Repo-domain administration — classic `repo` only (FG carries no
    /// administration permission).
    RepoAdmin,
    /// `packages.list` — `package:read`|`package:write`; FG also requires a
    /// covered `repository_id` in the input.
    PackageRead,
    /// `packages.deleteVersion` — `package:write` (classic only; FG denied
    /// because the input names no repository target).
    PackageWrite,
    /// Session-cookie only — never callable with a PAT.
    SessionOnly,
}

fn classify(procedure: &str) -> PatProc {
    use PatProc::*;
    match procedure {
        // Public / anonymous-safe.
        "system.health"
        | "system.manifest"
        | "system.echo"
        | "system.db_probe"
        | "auth.provider_config"
        | "auth.bootstrap_status"
        | "user.getPublicProfile"
        | "org.get"
        | "repo.explore"
        | "invites.get" => Open,

        // Own-identity reads.
        "auth.me" | "user.get_profile" => Identity,

        // Repo-domain reads on a named repository (`{owner, name}` input), plus
        // account-scoped reads tied to the repo domain (no single repository
        // target — FG tokens are repo-bound and cannot call those).
        "repo.get"
        | "repo.tree"
        | "repo.blob"
        | "repo.refs"
        | "repo.commits"
        | "repo.pathLastCommits"
        | "repo.commitCount"
        | "repo.contributors.list"
        | "repo.languages"
        | "repo.activity.list"
        | "repo.compare"
        | "repo.blame"
        | "repo.search"
        | "repo.stargazers.list"
        | "repo.watchers.list"
        | "repo.forks.list"
        | "repo.lfs.getEnabled"
        | "repo.lfs.getStatus"
        | "repo.lfs.getUsage"
        | "repo.lfs.listObjects"
        | "repo.lfs.download"
        | "repo.templates.getEnabled"
        | "repo.actions.getEnabled"
        | "repo.actions.listRuns"
        | "repo.actions.getRun"
        | "repo.actions.getJobLog"
        | "repo.actions.listWorkflows"
        | "repo.commitStatus.list"
        | "repo.mergeSettings.get"
        | "issue.get"
        | "issue.list"
        | "issue.history"
        | "issue.comments.list"
        | "issue.comments.history"
        | "issue.links.list"
        | "issue.assigneeCandidates"
        | "pull.get"
        | "pull.list"
        | "pull.files"
        | "pull.commits"
        | "pull.comments.list"
        | "pull.comments.history"
        | "pull.reviews.list"
        | "pull.reviewRequests.list"
        | "release.list"
        | "release.get"
        | "label.listForRepo"
        | "label.listForOrg"
        | "repo.listMine"
        | "repo.listByOwner"
        | "repo.createDefaults"
        | "repo.topicsSuggest"
        | "notification.list"
        | "notification.unreadCount"
        | "notification.markRead"
        | "notification.markAllRead"
        | "user.lookup"
        | "user.listStarred"
        | "org.listMine"
        | "org.members.list" => RepoRead,

        // Repo-domain writes.
        "repo.create"
        | "repo.fork"
        | "repo.commit"
        | "repo.branchCreate"
        | "repo.branchRename"
        | "repo.branchDelete"
        | "repo.commitStatus.create"
        | "repo.star"
        | "repo.unstar"
        | "repo.watch"
        | "repo.unwatch"
        | "repo.actions.dispatchWorkflow"
        | "repo.actions.rerunRun"
        | "repo.actions.cancelRun"
        | "issue.create"
        | "issue.update"
        | "issue.close"
        | "issue.reopen"
        | "issue.comments.create"
        | "issue.comments.update"
        | "issue.comments.delete"
        | "issue.labels.set"
        | "issue.assignees.set"
        | "issue.reactions.toggle"
        | "issue.links.add"
        | "issue.links.remove"
        | "pull.create"
        | "pull.update"
        | "pull.close"
        | "pull.reopen"
        | "pull.comments.create"
        | "pull.comments.update"
        | "pull.comments.delete"
        | "pull.comments.resolve"
        | "pull.reviews.submit"
        | "pull.reviews.dismiss"
        | "pull.reviewRequests.add"
        | "pull.reviewRequests.remove"
        | "pull.merge"
        | "release.create"
        | "release.update"
        | "release.delete"
        | "release.deleteAsset" => RepoWrite,

        // Repo-domain administration — classic `repo` only. `label.*` mutation
        // inputs use `repo` (not `name`) for the repo target, so they sit here
        // rather than under RepoWrite's `{owner,name}` FG check.
        "repo.updateMetadata"
        | "repo.updateVisibility"
        | "repo.softDelete"
        | "repo.rename"
        | "repo.transfer"
        | "repo.collaborators.list"
        | "repo.collaborators.add"
        | "repo.collaborators.update"
        | "repo.collaborators.remove"
        | "repo.invites.create"
        | "repo.invites.createLink"
        | "repo.invites.list"
        | "repo.invites.revoke"
        | "repo.branchProtection.list"
        | "repo.branchProtection.create"
        | "repo.branchProtection.update"
        | "repo.branchProtection.delete"
        | "repo.mirror.get"
        | "repo.mirror.upsert"
        | "repo.mirror.delete"
        | "repo.mirror.syncNow"
        | "repo.mirror.generateSshKey"
        | "repo.mirror.rotateWebhookSecret"
        | "repo.mirror.fetchHostKey"
        | "repo.actions.secrets.list"
        | "repo.actions.secrets.put"
        | "repo.actions.secrets.delete"
        | "repo.actions.setEnabled"
        | "repo.lfs.setEnabled"
        | "repo.templates.setEnabled"
        | "repo.mergeSettings.update"
        | "webhook.create"
        | "webhook.list"
        | "webhook.get"
        | "webhook.update"
        | "webhook.delete"
        | "webhook.deliveries.list"
        | "webhook.deliveries.get"
        | "webhook.ping"
        | "webhook.redeliver"
        | "issue.delete"
        | "label.create"
        | "label.update"
        | "label.delete" => RepoAdmin,

        "packages.list" => PackageRead,
        "packages.deleteVersion" => PackageWrite,

        // Everything else is session-only: auth lifecycle (`auth.login`,
        // `auth.logout`, verify/reset/bootstrap…), credential management
        // (`pat.*`, `sshKey.*`, `gpgKey.*`, `email.*`), `admin.*` and
        // `packages.admin*`, org mutations/invites, invite accepts, profile
        // writes. Unknown procedures fall here too — fail closed.
        _ => SessionOnly,
    }
}

fn pat_scope_err(procedure: &str, message: &str) -> AppError {
    AppError::new("auth.pat_scope", format!("{procedure}: {message}"))
}

fn has_scope(scopes: &[ClassicPatScope], scope: ClassicPatScope) -> bool {
    scopes.contains(&scope)
}

/// Classic-token scope check: `repo` covers the repo domain; `package:*` covers
/// `packages.*`; identity/open classes are handled by the caller.
fn authorize_classic(pat: &PatIdentity, class: PatProc, procedure: &str) -> Result<(), AppError> {
    let allowed = match class {
        PatProc::RepoRead | PatProc::RepoWrite | PatProc::RepoAdmin => {
            has_scope(&pat.scopes, ClassicPatScope::Repo)
        }
        PatProc::PackageRead => {
            has_scope(&pat.scopes, ClassicPatScope::PackageRead)
                || has_scope(&pat.scopes, ClassicPatScope::PackageWrite)
        }
        PatProc::PackageWrite => has_scope(&pat.scopes, ClassicPatScope::PackageWrite),
        _ => unreachable!("non-scope classes handled by caller"),
    };
    if allowed {
        Ok(())
    } else {
        Err(pat_scope_err(
            procedure,
            "personal access token scope does not allow this procedure",
        ))
    }
}

/// Repo the procedure targets, parsed from the common `{owner, name}` input
/// convention, or `repository_id` (`packages.list`). `Ok(None)` = input names
/// no repo target; `Err(not_found)` = named but missing (anti-enumeration).
async fn repo_target(
    ctx: &RpcCtx,
    input: &serde_json::Value,
) -> Result<Option<(RepositoryRow, OwnerRef)>, AppError> {
    let owner = input
        .get("owner")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let name = input
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if !owner.is_empty() && !name.is_empty() {
        let pair = lookup_repo_row_or_redirect(&ctx.db, owner, name)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "pat scope repo lookup failed");
                AppError::new("repo.internal", "repository operation failed")
            })?;
        return match pair {
            Some((row, owner_ref)) => Ok(Some((row, owner_ref))),
            None => Err(not_found()),
        };
    }
    if let Some(repo_id) = input
        .get("repository_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let row = ctx
            .db
            .find_repository_by_id(repo_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "pat scope repo lookup failed");
                AppError::new("repo.internal", "repository operation failed")
            })?
            .ok_or_else(not_found)?;
        let owner_ref = owner_ref_for_repo(&ctx.db, &row).await.map_err(|e| {
            tracing::error!(error = %e, "pat scope owner lookup failed");
            AppError::new("repo.internal", "repository operation failed")
        })?;
        return match owner_ref {
            Some(o) => Ok(Some((row, o))),
            None => Err(not_found()),
        };
    }
    Ok(None)
}

/// Whether the FG token's `repo_access` covers `repo` (ASSUME A4 for `all`).
async fn fg_covers(
    ctx: &RpcCtx,
    pat: &PatIdentity,
    repo: &RepositoryRow,
    owner: &OwnerRef,
) -> Result<bool, AppError> {
    match pat.repo_access {
        Some(FgRepoAccess::Selected) => Ok(pat.repository_ids.iter().any(|id| id == &repo.id)),
        Some(FgRepoAccess::All) => fg_all_covers_repo(&ctx.db, &pat.user_id, owner)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "fg_all_covers_repo failed");
                AppError::new("repo.internal", "repository operation failed")
            }),
        // Corrupt/absent repo_access on an authenticated FG row — fail closed.
        None => Ok(false),
    }
}

/// Fine-grained-token scope check: `contents` maps to repo-domain read/write on
/// covered repositories only; package perms map to `packages.list` with a
/// covered `repository_id`. RepoAdmin and non-repo-targeted procedures are
/// denied (FG tokens are repo-bound credentials).
async fn authorize_fine_grained(
    ctx: &RpcCtx,
    pat: &PatIdentity,
    class: PatProc,
    procedure: &str,
    input: &serde_json::Value,
) -> Result<(), AppError> {
    match class {
        PatProc::RepoRead | PatProc::RepoWrite => {
            let write = class == PatProc::RepoWrite;
            let contents_ok = match pat.contents {
                Some(ContentsPerm::Write) => true,
                Some(ContentsPerm::Read) => !write,
                None => false,
            };
            if !contents_ok {
                return Err(pat_scope_err(
                    procedure,
                    "fine-grained token contents permission does not allow this procedure",
                ));
            }
            let Some((row, owner_ref)) = repo_target(ctx, input).await? else {
                return Err(pat_scope_err(
                    procedure,
                    "fine-grained tokens can only call procedures that name a repository",
                ));
            };
            if fg_covers(ctx, pat, &row, &owner_ref).await? {
                return Ok(());
            }
            // Public repos stay readable with any FG token (GitHub parity);
            // writes and private repos outside the selection → not_found.
            if !write && !is_private_visibility(&row.visibility) {
                return Ok(());
            }
            Err(not_found())
        }
        PatProc::RepoAdmin => Err(pat_scope_err(
            procedure,
            "fine-grained tokens carry no repository administration permission",
        )),
        PatProc::PackageRead | PatProc::PackageWrite => {
            let package_ok = match class {
                PatProc::PackageRead => {
                    has_scope(&pat.scopes, ClassicPatScope::PackageRead)
                        || has_scope(&pat.scopes, ClassicPatScope::PackageWrite)
                }
                _ => has_scope(&pat.scopes, ClassicPatScope::PackageWrite),
            };
            if !package_ok {
                return Err(pat_scope_err(
                    procedure,
                    "fine-grained token packages permission does not allow this procedure",
                ));
            }
            let Some((row, owner_ref)) = repo_target(ctx, input).await? else {
                return Err(pat_scope_err(
                    procedure,
                    "fine-grained package calls must name a repository_id",
                ));
            };
            if fg_covers(ctx, pat, &row, &owner_ref).await? {
                Ok(())
            } else {
                Err(not_found())
            }
        }
        _ => unreachable!("non-scope classes handled by caller"),
    }
}

/// Dispatch-time PAT scope gate — `ctx.pat.is_some()` → classify + authorize.
/// Cookie-authenticated contexts return `Ok` immediately.
pub async fn authorize_rpc(
    ctx: &RpcCtx,
    procedure: &str,
    input: &serde_json::Value,
) -> Result<(), AppError> {
    let Some(pat) = ctx.pat.as_ref() else {
        return Ok(());
    };
    match classify(procedure) {
        PatProc::Open | PatProc::Identity => Ok(()),
        PatProc::SessionOnly => Err(pat_scope_err(
            procedure,
            "procedure requires a session cookie (not callable with a personal access token)",
        )),
        class => match pat.kind {
            PatKind::Classic => authorize_classic(pat, class, procedure),
            PatKind::FineGrained => authorize_fine_grained(ctx, pat, class, procedure, input).await,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_only_classification() {
        for p in [
            "admin.users.list",
            "admin.auth.get_settings",
            "admin.actions.listRunners",
            "packages.adminUsage",
            "packages.adminSetQuota",
            "auth.login",
            "auth.logout",
            "auth.signup",
            "auth.verify",
            "pat.list",
            "pat.createClassic",
            "sshKey.add",
            "gpgKey.list",
            "email.add",
            "org.create",
            "org.updateSettings",
            "org.members.add",
            "org.invites.create",
            "invites.accept",
            "org.invites.accept",
            "user.update_profile",
            "does.not.exist",
        ] {
            assert_eq!(classify(p), PatProc::SessionOnly, "{p}");
        }
    }

    #[test]
    fn open_and_identity_classification() {
        for p in [
            "system.health",
            "system.manifest",
            "system.echo",
            "system.db_probe",
            "auth.provider_config",
            "auth.bootstrap_status",
            "user.getPublicProfile",
            "org.get",
            "repo.explore",
            "invites.get",
        ] {
            assert_eq!(classify(p), PatProc::Open, "{p}");
        }
        assert_eq!(classify("auth.me"), PatProc::Identity);
        assert_eq!(classify("user.get_profile"), PatProc::Identity);
    }

    #[test]
    fn repo_domain_classification_samples() {
        assert_eq!(classify("repo.get"), PatProc::RepoRead);
        assert_eq!(classify("issue.assigneeCandidates"), PatProc::RepoRead);
        assert_eq!(classify("issue.create"), PatProc::RepoWrite);
        assert_eq!(classify("pull.merge"), PatProc::RepoWrite);
        assert_eq!(classify("repo.commitStatus.create"), PatProc::RepoWrite);
        assert_eq!(classify("repo.listMine"), PatProc::RepoRead);
        assert_eq!(classify("notification.list"), PatProc::RepoRead);
        assert_eq!(classify("webhook.create"), PatProc::RepoAdmin);
        assert_eq!(classify("repo.collaborators.add"), PatProc::RepoAdmin);
        assert_eq!(classify("packages.list"), PatProc::PackageRead);
        assert_eq!(classify("packages.deleteVersion"), PatProc::PackageWrite);
    }
}
