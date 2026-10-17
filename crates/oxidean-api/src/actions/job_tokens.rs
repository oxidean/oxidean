//! Per-job Actions bearer tokens (`oxidean_job_…`) — the `OXIDEAN_TOKEN`
//! credential injected into the job environment (GITHUB_TOKEN parity).
//!
//! Minted at runner claim time (`fetch_task` in [`crate::actions::runner_proto`]),
//! hashed like PATs ([`sha256_hex`] at rest, never stored in plaintext), and
//! resolved through the shared Bearer router [`crate::pat::bearer::resolve_bearer`]
//! where the row becomes a repo-locked fine-grained [`PatIdentity`].
//! Revoked when the job reaches a terminal state or the run is cancelled;
//! `expires_at` is the hard backstop (24h).
//!
//! [`PatIdentity`]: crate::pat::bearer::PatIdentity

use oxidean_db::{ActionJobTokenRow, ActionRunRow, Database, PatRow, RepositoryRow, UserRow};
use uuid::Uuid;

use crate::actions::parse::ContentsPermission;
use crate::auth::session::{bytes_to_hex, sha256_hex};
use crate::repo::{effective_capability, meets, Capability, OwnerRef};

/// Bearer token prefix routed by `pat::bearer::resolve_bearer`.
pub const JOB_TOKEN_PREFIX: &str = "oxidean_job_";

/// Resolved `oxidean_job_…` row + owner for the Basic-auth surfaces (git smart
/// HTTP, LFS) — mirrors [`crate::oauth::OAuthTokenIdentity`].
#[derive(Debug, Clone)]
pub struct JobTokenIdentity {
    pub token: ActionJobTokenRow,
    pub user: UserRow,
}

impl JobTokenIdentity {
    /// Synthesize a fine-grained-shaped `PatRow` so the git/LFS ACL helpers
    /// (`pat_allows_operation`) apply unchanged: `selected` repo access pinned
    /// to the job's repository and `contents_perm` from the minted permission.
    /// `raw_token` supplies the display prefix (first 8 chars of the secret).
    pub fn synthetic_pat(&self, raw_token: &str) -> PatRow {
        let tail = raw_token
            .strip_prefix(JOB_TOKEN_PREFIX)
            .unwrap_or(raw_token);
        PatRow {
            id: self.token.id.clone(),
            user_id: self.token.user_id.clone(),
            kind: "fine_grained".into(),
            name: "actions job token".into(),
            token_prefix: format!("{JOB_TOKEN_PREFIX}{}", &tail[..tail.len().min(8)]),
            token_hash: self.token.token_hash.clone(),
            scopes_json: Some("[]".into()),
            contents_perm: Some(self.token.permission.clone()),
            perms_json: None,
            repo_access: Some("selected".into()),
            expires_at: Some(self.token.expires_at.clone()),
            revoked_at: self.token.revoked_at.clone(),
            last_used_at: None,
            last_used_ip: None,
            created_at: self.token.created_at.clone(),
            repository_ids: vec![self.token.repository_id.clone()],
        }
    }
}

/// Why an `oxidean_job_` credential failed — mirrors
/// [`crate::oauth::OAuthAuthStatus`] so git/LFS callers can surface a reason.
pub enum JobTokenAuthStatus {
    Ok(Box<JobTokenIdentity>),
    /// Unknown token or vanished owner — indistinguishable from "never existed".
    Invalid,
    Revoked,
    Expired,
    Banned,
}

/// Status-returning resolver for `oxidean_job_` Basic-auth passwords — the
/// revoked-inclusive lookup matches the PAT/OAuth seams so callers can emit
/// distinct revoked/expired/banned responses.
pub async fn authenticate_job_token_status(
    db: &Database,
    raw_token: &str,
) -> Result<JobTokenAuthStatus, String> {
    if !raw_token.starts_with(JOB_TOKEN_PREFIX) {
        return Ok(JobTokenAuthStatus::Invalid);
    }
    let hash = sha256_hex(raw_token.as_bytes());
    let Some(token) = db.find_action_job_token_by_hash(&hash).await? else {
        return Ok(JobTokenAuthStatus::Invalid);
    };
    if token.revoked_at.is_some() {
        return Ok(JobTokenAuthStatus::Revoked);
    }
    if crate::oauth::rfc3339_expired(&token.expires_at) {
        return Ok(JobTokenAuthStatus::Expired);
    }
    let Some(user) = db.find_user_by_id(&token.user_id).await? else {
        return Ok(JobTokenAuthStatus::Invalid);
    };
    if user.banned_at.is_some() {
        return Ok(JobTokenAuthStatus::Banned);
    }
    Ok(JobTokenAuthStatus::Ok(Box::new(JobTokenIdentity {
        token,
        user,
    })))
}

/// 20 random bytes → 40 lowercase hex chars after the prefix.
const JOB_TOKEN_SECRET_BYTES: usize = 20;

/// Hard backstop — terminal-state revocation usually lands first.
const JOB_TOKEN_TTL_HOURS: i64 = 24;

/// The granted `contents` level for a minted token (see [`mint_job_token`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobTokenPermission {
    Read,
    Write,
}

impl JobTokenPermission {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }
}

/// Mint the per-job `oxidean_job_` token for a claimed job.
///
/// Returns `Ok(None)` when the run has no triggering user (`triggered_by` is
/// `NULL`, e.g. an internal/system dispatch) or the workflow's effective
/// `contents` permission is `none` — no token is injected in either case.
///
/// `requested` comes from [`crate::actions::parse::effective_contents_permission`].
/// The grant is capped by the triggering user's effective capability on the
/// repo: a `write` request from a read-only actor is downgraded to `read`
/// (mirrors GitHub's "permissions can only be lowered" rule).
///
/// Re-minting (a requeued job's next claim) replaces the `job_id`-unique row,
/// so the previous plaintext stops resolving.
pub async fn mint_job_token(
    db: &Database,
    run: &ActionRunRow,
    repo: &RepositoryRow,
    owner: &OwnerRef,
    job_id: &str,
    requested: ContentsPermission,
) -> Result<Option<(String, JobTokenPermission)>, String> {
    if matches!(requested, ContentsPermission::None) {
        return Ok(None);
    }
    let Some(user_id) = run.triggered_by.as_deref() else {
        return Ok(None);
    };

    let capability = effective_capability(db, Some(user_id), repo, owner).await?;
    let granted = match requested {
        ContentsPermission::Write if !meets(capability, Capability::Write) => {
            JobTokenPermission::Read
        }
        ContentsPermission::Write => JobTokenPermission::Write,
        _ => JobTokenPermission::Read,
    };

    let mut secret = [0u8; JOB_TOKEN_SECRET_BYTES];
    rand::fill(&mut secret);
    let secret_hex = bytes_to_hex(&secret);
    let plaintext = format!("{JOB_TOKEN_PREFIX}{secret_hex}");
    let token_hash = sha256_hex(plaintext.as_bytes());
    let expires_at = (chrono::Utc::now() + chrono::Duration::hours(JOB_TOKEN_TTL_HOURS))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    db.insert_action_job_token(
        &Uuid::new_v4().to_string(),
        job_id,
        &run.id,
        &run.repository_id,
        user_id,
        granted.as_str(),
        &token_hash,
        &expires_at,
    )
    .await?;

    Ok(Some((plaintext, granted)))
}
