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
//! [`authorize_rpc`] enforces at dispatch time (see `proc_classes::classify_pat_proc`).
//!
//! Scope model (docs/API.md):
//! - Classic `repo` → all repo-domain procedures (read, write, administration
//!   short of org/instance admin) plus account-scoped repo reads; narrower
//!   classic scopes gate single families (`workflow`, `repo:status`,
//!   `admin:repo_hook`, `notifications`, `read:org`/`write:org`, the
//!   `*:public_key` / `*:gpg_key` tiers, `user`-family, `delete_repo`,
//!   `package:delete`, `public_repo`, …). `delete_repo` and `package:delete`
//!   are deliberately NOT implied by `repo`/`package:write` (GitHub parity).
//! - Classic `public_repo` → repo-domain collaboration (contents, issues,
//!   pulls, actions, statuses, releases, star/watch) on public repos only.
//! - Fine-grained `contents:read|write` → contents-domain read/write
//!   procedures on repositories covered by `repo_access` (`selected` ids, or
//!   `all` = personal-owned + org Owner/Admin — ASSUME A4); the `perms`
//!   families (`issues`, `pull_requests`, `actions`, `commit_statuses`,
//!   `secrets`, `webhooks`, `administration`, `releases`, …) gate their own
//!   domains the same way. Account-level families (`emails`,
//!   `notifications`, `profile`, `ssh_keys`, `gpg_keys`, `starring`,
//!   `watching`, `followers`) act on the token owner without repo binding.
//!   Public repos stay readable. Org mutations are unreachable for FG
//!   tokens (repo-bound credentials).
//! - Admin (`admin.*`, `packages.admin*`), session lifecycle (`auth.*` other
//!   than `me`/`provider_config`/`bootstrap_status`), PAT minting
//!   (`pat.createClassic`/`createFineGrained` — a token must never mint
//!   tokens), OAuth app/device-grant procedures, and invite accepts are
//!   never callable with a PAT — fail-closed default for new procedures.
//!   The only `pat.*` procedures a PAT may call are introspection
//!   (`pat.self`/`pat.policy`/`pat.catalog`), `pat.list` (its owner's
//!   tokens), and self-revocation (`pat.revoke` on its own id — the
//!   handler enforces that last part).
//! - Bearer responses never carry `Set-Cookie` and never mint sessions.

use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Utc};
use oxidean_core::{
    AppError, ClassicPatScope, ContentsPerm, FgPermLevel, FgPerms, FgRepoAccess, PatKind,
    CLASSIC_PAT_PREFIX, FINE_GRAINED_PAT_PREFIX,
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
    /// `personal_access_tokens.id` — used for `last_used` bookkeeping, as the
    /// synthesized session id, and as the `pat.self` / self-revoke target.
    pub token_id: String,
    /// Token owner's user id (PAT acts on the owner's behalf).
    pub user_id: String,
    pub kind: PatKind,
    /// Token note — surfaced by `pat.self`.
    pub name: String,
    /// Display fingerprint (never the secret) — surfaced by `pat.self`.
    pub token_prefix: String,
    /// RFC3339 expiry on the row (`None` = non-expiring) — surfaced by
    /// `pat.self`.
    pub expires_at_raw: Option<String>,
    pub last_used_at: Option<String>,
    pub created_at: String,
    /// Classic scope names, or FG package perms (`package:read`/`package:write`
    /// are stored in `scopes_json` for both kinds — see `pat::row_to_list_item`).
    pub scopes: Vec<ClassicPatScope>,
    /// FG `contents` permission (classic: `None`).
    pub contents: Option<ContentsPerm>,
    /// FG `repo_access` mode (classic: `None`).
    pub repo_access: Option<FgRepoAccess>,
    /// FG `selected` repository ids (empty for `all` / classic).
    pub repository_ids: Vec<String>,
    /// FG permission families (empty for classic / legacy tokens).
    pub perms: FgPerms,
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

    /// Distinct auth-failure codes so clients (`ox`) can pick a recovery path:
    /// `auth.token_expired`/`auth.token_revoked`/`auth.banned` are all still
    /// HTTP 401 — only the RPC `error.code` differentiates them.
    fn coded(code: &str, message: &str) -> Self {
        Self {
            error: AppError::new(code, message),
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

pub(crate) fn lock(
    limiter: &Mutex<FailedAuthLimiter>,
) -> std::sync::MutexGuard<'_, FailedAuthLimiter> {
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
    // Revoked rows must reach this classification — filtering them at the DB
    // layer makes a revoked token indistinguishable from a nonexistent one.
    let pat = match db.find_pat_by_token_hash_including_revoked(&token_hash).await {
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

    if pat.revoked_at.is_some() {
        fail_all(&pat);
        return Err(BearerRejection::coded(
            "auth.token_revoked",
            "personal access token has been revoked",
        ));
    }
    if pat_expired(&pat.expires_at) {
        fail_all(&pat);
        return Err(BearerRejection::coded(
            "auth.token_expired",
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
        return Err(BearerRejection::coded(
            "auth.banned",
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
        name: pat.name.clone(),
        token_prefix: pat.token_prefix.clone(),
        expires_at_raw: pat.expires_at.clone(),
        last_used_at: pat.last_used_at.clone(),
        created_at: pat.created_at.clone(),
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
        perms: pat
            .perms_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<FgPerms>(raw).ok())
            .unwrap_or_default(),
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

/// Procedure → scope-class mapping lives in `oxidean_core::proc_classes` —
/// shared with first-party clients (`ox` coverage audits, `auth status`).
/// This module keeps only the db/context-dependent half: `repo_target`
/// resolution, FG `repo_access`/perm evaluation, and the authorize fns.
use oxidean_core::proc_classes::{
    classify_pat_proc as classify, classic_scopes_for, fg_family_name, public_repo_covers,
    required_scope_names, PatProc,
};

fn pat_scope_err(procedure: &str, message: &str) -> AppError {
    AppError::new("auth.pat_scope", format!("{procedure}: {message}"))
}

/// Scope implication lives in the shared catalog
/// (`oxidean_core::classic_scope_satisfies`) — `user` ⊃ `read:user`/
/// `user:email`/`user:follow`, `write:org` ⊃ `read:org`, key tiers
/// admin ⊃ write ⊃ read, `package:write` ⊃ `package:read` (GitHub parity).
fn scope_satisfies(held: ClassicPatScope, required: ClassicPatScope) -> bool {
    oxidean_core::classic_scope_satisfies(held, required)
}

fn has_scope(scopes: &[ClassicPatScope], required: ClassicPatScope) -> bool {
    scopes.iter().any(|s| scope_satisfies(*s, required))
}

/// Classic-token scope check: umbrella `repo` covers the repo domain;
/// narrower scopes gate single families; `public_repo` covers public-repo
/// collaboration via a target-visibility check.
async fn authorize_classic(
    ctx: &RpcCtx,
    pat: &PatIdentity,
    class: PatProc,
    procedure: &str,
    input: &serde_json::Value,
) -> Result<(), AppError> {
    let scopes = &pat.scopes;
    if classic_scopes_for(class)
        .iter()
        .any(|s| has_scope(scopes, *s))
    {
        return Ok(());
    }
    // `public_repo` — repo-targeted collaboration classes on public repos.
    // A missing/unresolvable target (e.g. `repo.create` names a repo that does
    // not exist yet) is a plain scope denial; an existing private target gets
    // the anti-enumeration 404.
    if public_repo_covers(class) && has_scope(scopes, ClassicPatScope::PublicRepo) {
        if let Ok(Some((row, _))) = repo_target(ctx, input).await {
            if !is_private_visibility(&row.visibility) {
                return Ok(());
            }
            return Err(not_found());
        }
    }
    let names = required_scope_names(class).join("\", \"");
    Err(pat_scope_err(
        procedure,
        &format!(
            "insufficient scope — this procedure requires one of: \"{names}\""
        ),
    ))
}

/// Repo the procedure targets, parsed from the common `{owner, name}` input
/// convention, `{owner, repo}` (`label.*` mutations), or `repository_id`
/// (`packages.list`). `Ok(None)` = input names no repo target;
/// `Err(not_found)` = named but missing (anti-enumeration).
async fn repo_target(
    ctx: &RpcCtx,
    input: &serde_json::Value,
) -> Result<Option<(RepositoryRow, OwnerRef)>, AppError> {
    let owner = input
        .get("owner")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let name = ["name", "repo"]
        .iter()
        .find_map(|k| input.get(k).and_then(|v| v.as_str()))
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
    db: &Database,
    pat: &PatIdentity,
    repo: &RepositoryRow,
    owner: &OwnerRef,
) -> Result<bool, AppError> {
    match pat.repo_access {
        Some(FgRepoAccess::Selected) => Ok(pat.repository_ids.iter().any(|id| id == &repo.id)),
        Some(FgRepoAccess::All) => fg_all_covers_repo(db, &pat.user_id, owner)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "fg_all_covers_repo failed");
                AppError::new("repo.internal", "repository operation failed")
            }),
        // Corrupt/absent repo_access on an authenticated FG row — fail closed.
        None => Ok(false),
    }
}

/// `contents` column as an `FgPermLevel` (kept on the identity — it predates
/// `perms` and stays the required FG field).
fn contents_level(pat: &PatIdentity) -> Option<FgPermLevel> {
    pat.contents.map(|c| match c {
        ContentsPerm::Read => FgPermLevel::Read,
        ContentsPerm::Write => FgPermLevel::Write,
    })
}

/// FG perm family + level required for a repo-bound class. `metadata` may
/// satisfy `RepoRead` in place of `contents` (checked separately); releases
/// reads also accept `contents:read`.
fn fg_repo_perm(pat: &PatIdentity, class: PatProc) -> Option<(Option<FgPermLevel>, FgPermLevel)> {
    use PatProc::*;
    let p = &pat.perms;
    let (held, need): (Option<FgPermLevel>, FgPermLevel) = match class {
        RepoRead => (
            contents_level(pat).or(p.metadata.map(|_| FgPermLevel::Read)),
            FgPermLevel::Read,
        ),
        RepoWrite => (contents_level(pat), FgPermLevel::Write),
        IssuesRead => (p.issues, FgPermLevel::Read),
        IssuesWrite => (p.issues, FgPermLevel::Write),
        PullsRead => (p.pull_requests, FgPermLevel::Read),
        PullsWrite => (p.pull_requests, FgPermLevel::Write),
        ActionsRead => (p.actions, FgPermLevel::Read),
        ActionsWrite => (p.actions, FgPermLevel::Write),
        StatusesRead => (p.commit_statuses, FgPermLevel::Read),
        StatusesWrite => (p.commit_statuses, FgPermLevel::Write),
        SecretsRead => (p.secrets, FgPermLevel::Read),
        SecretsWrite => (p.secrets, FgPermLevel::Write),
        ReleasesRead => return Some((p.releases.or(contents_level(pat)), FgPermLevel::Read)),
        ReleasesWrite => (p.releases, FgPermLevel::Write),
        WebhooksRead => (p.webhooks, FgPermLevel::Read),
        WebhooksWrite => (p.webhooks, FgPermLevel::Write),
        RepoAdminRead => (p.administration, FgPermLevel::Read),
        RepoAdminWrite | RepoInvite | RepoDelete => (p.administration, FgPermLevel::Write),
        _ => return None,
    };
    Some((held, need))
}

/// FG perm family for account-level classes — no repo binding required.
fn fg_account_perm(p: &FgPerms, class: PatProc) -> Option<(Option<FgPermLevel>, FgPermLevel)> {
    use PatProc::*;
    let (held, need): (Option<FgPermLevel>, FgPermLevel) = match class {
        NotificationsRead => (p.notifications, FgPermLevel::Read),
        NotificationsWrite => (p.notifications, FgPermLevel::Write),
        EmailRead => (p.emails, FgPermLevel::Read),
        EmailWrite => (p.emails, FgPermLevel::Write),
        ProfileWrite => (p.profile, FgPermLevel::Write),
        FollowWrite => (p.followers, FgPermLevel::Write),
        FollowRead => (p.followers, FgPermLevel::Read),
        StarringRead => (p.starring, FgPermLevel::Read),
        WatchingRead => (p.watching, FgPermLevel::Read),
        StarringWrite => (p.starring, FgPermLevel::Write),
        WatchingWrite => (p.watching, FgPermLevel::Write),
        SshKeyRead => (p.ssh_keys, FgPermLevel::Read),
        SshKeyWrite => (p.ssh_keys, FgPermLevel::Write),
        GpgKeyRead => (p.gpg_keys, FgPermLevel::Read),
        GpgKeyWrite => (p.gpg_keys, FgPermLevel::Write),
        _ => return None,
    };
    Some((held, need))
}

/// Fine-grained-token scope check: `contents` maps to repo-domain read/write
/// on covered repositories; the `perms` families gate their own domains;
/// account-level families (emails/notifications/…) act on the token owner
/// without repo binding. Org mutations and PAT management stay unreachable.
async fn authorize_fine_grained(
    ctx: &RpcCtx,
    pat: &PatIdentity,
    class: PatProc,
    procedure: &str,
    input: &serde_json::Value,
) -> Result<(), AppError> {
    // Account-level families first (no repo binding).
    if let Some((held, need)) = fg_account_perm(&pat.perms, class) {
        return match held {
            Some(level) if level.meets(need) => Ok(()),
            _ => Err(pat_scope_err(
                procedure,
                &format!(
                    "insufficient scope — this procedure requires the \"{}\" permission",
                    fg_family_name(class).unwrap_or("unknown")
                ),
            )),
        };
    }

    // Repo-bound domain families.
    if let Some((held, need)) = fg_repo_perm(pat, class) {
        let perm_ok = held.is_some_and(|l| l.meets(need));
        if !perm_ok {
            return Err(pat_scope_err(
                procedure,
                &format!(
                    "insufficient scope — this procedure requires the \"{}\" permission",
                    fg_family_name(class).unwrap_or("unknown")
                ),
            ));
        }
        let write = need == FgPermLevel::Write;
        let Some((row, owner_ref)) = repo_target(ctx, input).await? else {
            return Err(pat_scope_err(
                procedure,
                "fine-grained tokens can only call procedures that name a repository",
            ));
        };
        if fg_covers(&ctx.db, pat, &row, &owner_ref).await? {
            return Ok(());
        }
        // Public repos stay readable with any FG token (GitHub parity);
        // writes and private repos outside the selection → not_found.
        if !write && !is_private_visibility(&row.visibility) {
            return Ok(());
        }
        return Err(not_found());
    }

    match class {
        PatProc::PackageRead | PatProc::PackageWrite | PatProc::PackageDelete => {
            let package_ok = if class == PatProc::PackageRead {
                has_scope(&pat.scopes, ClassicPatScope::PackageRead)
                    || has_scope(&pat.scopes, ClassicPatScope::PackageWrite)
            } else if class == PatProc::PackageWrite {
                has_scope(&pat.scopes, ClassicPatScope::PackageWrite)
            } else {
                has_scope(&pat.scopes, ClassicPatScope::PackageDelete)
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
            if fg_covers(&ctx.db, pat, &row, &owner_ref).await? {
                Ok(())
            } else {
                Err(not_found())
            }
        }
        _ => Err(pat_scope_err(
            procedure,
            "procedure is not callable with a fine-grained token",
        )),
    }
}

/// PAT scope gate for non-RPC routes (release-asset up/download) that resolved
/// `(repo, owner)` themselves — same classification as `authorize_rpc`, minus
/// input parsing. `procedure` is the RPC name the route maps to (e.g. asset
/// upload → `release.update`) and drives both classification and error text.
pub(crate) async fn authorize_pat_on_repo(
    db: &Database,
    pat: &PatIdentity,
    procedure: &str,
    repo: &RepositoryRow,
    owner: &OwnerRef,
) -> Result<(), AppError> {
    let class = classify(procedure);
    match pat.kind {
        PatKind::Classic => {
            if classic_scopes_for(class)
                .iter()
                .any(|s| has_scope(&pat.scopes, *s))
            {
                return Ok(());
            }
            if public_repo_covers(class)
                && has_scope(&pat.scopes, ClassicPatScope::PublicRepo)
            {
                if !is_private_visibility(&repo.visibility) {
                    return Ok(());
                }
                return Err(not_found());
            }
            let names = required_scope_names(class).join("\", \"");
            Err(pat_scope_err(
                procedure,
                &format!(
                    "insufficient scope — this procedure requires one of: \"{names}\""
                ),
            ))
        }
        PatKind::FineGrained => {
            let Some((held, need)) = fg_repo_perm(pat, class) else {
                return Err(pat_scope_err(
                    procedure,
                    "procedure is not callable with a fine-grained token",
                ));
            };
            if !held.is_some_and(|l| l.meets(need)) {
                return Err(pat_scope_err(
                    procedure,
                    "fine-grained token permission does not allow this procedure",
                ));
            }
            if fg_covers(db, pat, repo, owner).await? {
                return Ok(());
            }
            if need == FgPermLevel::Read && !is_private_visibility(&repo.visibility) {
                return Ok(());
            }
            Err(not_found())
        }
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
        PatProc::Open | PatProc::Identity | PatProc::PatSelfManage => Ok(()),
        PatProc::SessionOnly => Err(pat_scope_err(
            procedure,
            "procedure requires a session cookie (not callable with a personal access token)",
        )),
        class => match pat.kind {
            PatKind::Classic => authorize_classic(ctx, pat, class, procedure, input).await,
            PatKind::FineGrained => authorize_fine_grained(ctx, pat, class, procedure, input).await,
        },
    }
}
