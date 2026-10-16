//! Per-job Actions bearer tokens (`oxidean_job_…`) — the `OXIDEAN_TOKEN`
//! credential injected into the job environment (GITHUB_TOKEN parity).
//!
//! Minted at runner claim time ([`crate::actions::runner_proto::fetch_task`]),
//! hashed like PATs ([`sha256_hex`] at rest, never stored in plaintext), and
//! resolved through the shared Bearer router [`crate::pat::bearer::resolve_bearer`]
//! where the row becomes a repo-locked fine-grained [`PatIdentity`].
//! Revoked when the job reaches a terminal state or the run is cancelled;
//! `expires_at` is the hard backstop (24h).
//!
//! [`PatIdentity`]: crate::pat::bearer::PatIdentity

use oxidean_db::{ActionRunRow, Database, RepositoryRow};
use uuid::Uuid;

use crate::actions::parse::ContentsPermission;
use crate::auth::session::{bytes_to_hex, sha256_hex};
use crate::repo::{effective_capability, meets, Capability, OwnerRef};

/// Bearer token prefix routed by `pat::bearer::resolve_bearer`.
pub const JOB_TOKEN_PREFIX: &str = "oxidean_job_";

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
