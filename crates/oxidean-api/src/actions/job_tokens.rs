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
    /// to the job's repository, `contents_perm` from the minted permission,
    /// and the GitHub scope map translated onto `FgPerms` families.
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
            perms_json: Some(fg_perms_json(&self.token)),
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

/// Translate the stored GitHub scope map (`{"issues":"write",…}`) into the
/// `FgPerms` the shared PAT ACL consumes. Scopes with no FG family
/// (`id-token`, `attestations`, `discussions`, `repository-projects`,
/// `security-events`) carry no proc-class grants; `contents`/`packages` ride
/// their dedicated `PatRow` columns instead.
fn fg_perms(token: &ActionJobTokenRow) -> oxidean_core::FgPerms {
    use oxidean_core::{FgPermLevel, FgPerms};
    let Some(map) = stored_scope_map(token) else {
        return FgPerms::default();
    };
    let level = |scope: &str| -> Option<FgPermLevel> {
        match map.get(scope).map(String::as_str) {
            Some("read") => Some(FgPermLevel::Read),
            Some("write") => Some(FgPermLevel::Write),
            _ => None,
        }
    };
    FgPerms {
        issues: level("issues"),
        pull_requests: level("pull-requests"),
        actions: level("actions"),
        commit_statuses: level("statuses"),
        deployments: level("deployments"),
        checks: level("checks"),
        environments: level("environments"),
        pages: level("pages"),
        ..FgPerms::default()
    }
}

/// `perms_json` → `{scope: "none"|"read"|"write"}` — `None` on absent or
/// malformed storage (pre-Phase-7 rows carry only the contents column).
fn stored_scope_map(
    token: &ActionJobTokenRow,
) -> Option<std::collections::BTreeMap<String, String>> {
    let raw = token.perms_json.as_deref()?;
    serde_json::from_str::<std::collections::BTreeMap<String, String>>(raw).ok()
}

fn fg_perms_json(token: &ActionJobTokenRow) -> String {
    serde_json::to_string(&fg_perms(token)).unwrap_or_else(|_| "{}".into())
}

/// The granted scope set as RPC `PatIdentity` fields — `(contents, perms,
/// scopes)`: `contents` reads from the granted map (falling back to the row's
/// `read`/`write` column on pre-Phase-7 storage), domain scopes land on
/// `perms`, and `packages` becomes the FG-classic `package:*` scope names
/// `authorize_fine_grained` checks on `PatIdentity.scopes`.
pub(crate) fn rpc_identity_grants(
    token: &ActionJobTokenRow,
) -> (
    Option<oxidean_core::ContentsPerm>,
    oxidean_core::FgPerms,
    Vec<oxidean_core::ClassicPatScope>,
) {
    use oxidean_core::{ClassicPatScope, ContentsPerm};
    let map = stored_scope_map(token);
    let contents = match &map {
        Some(m) => m.get("contents").and_then(|s| ContentsPerm::parse(s).ok()),
        // Pre-Phase-7 row: the `permission` column is the whole grant.
        None => ContentsPerm::parse(&token.permission).ok(),
    };
    let scopes = match map
        .as_ref()
        .and_then(|m| m.get("packages"))
        .map(String::as_str)
    {
        Some("write") => vec![ClassicPatScope::PackageWrite],
        Some("read") => vec![ClassicPatScope::PackageRead],
        _ => Vec::new(),
    };
    (contents, fg_perms(token), scopes)
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

/// What a successful mint hands back to the claim path.
#[derive(Debug)]
pub struct MintedJobToken {
    /// `oxidean_job_…` plaintext — injected as `OXIDEAN_TOKEN`/`GITHUB_TOKEN`.
    /// `None` when the map grants `contents: none` but OIDC creds still minted
    /// (GitHub injects `ACTIONS_ID_TOKEN_REQUEST_*` without `GITHUB_TOKEN`).
    pub plaintext: Option<String>,
    /// The granted `contents` level after capability capping.
    pub contents: JobTokenPermission,
    /// `ACTIONS_ID_TOKEN_REQUEST_TOKEN` plaintext — `Some` only when the
    /// granted scope set carries `id-token: write`. The row stores its hash;
    /// the OIDC endpoint mints JWTs against it ([`crate::actions::oidc`]).
    pub oidc_request_token: Option<String>,
}

/// Mint the per-job `oxidean_job_` token for a claimed job.
///
/// Returns `Ok(None)` when the run has no triggering user (`triggered_by` is
/// `NULL`, e.g. an internal/system dispatch) or the scope map grants neither
/// `contents` nor `id-token: write` — no credentials are injected then.
///
/// `requested` is the frozen scope map from the durable payload
/// (`permissions` — see [`crate::actions::effective_permissions`]). Grant
/// levels are capped by the triggering user's effective capability on the
/// repo: `write` scopes for a read-only actor downgrade to `read` (GitHub's
/// "permissions can only be lowered" rule, applied per scope).
///
/// Re-minting (a requeued job's next claim) replaces the `job_id`-unique row,
/// so the previous plaintext stops resolving.
pub async fn mint_job_token(
    db: &Database,
    run: &ActionRunRow,
    repo: &RepositoryRow,
    owner: &OwnerRef,
    job_id: &str,
    requested: &std::collections::BTreeMap<String, ContentsPermission>,
) -> Result<Option<MintedJobToken>, String> {
    let contents_requested = requested
        .get("contents")
        .copied()
        .unwrap_or(ContentsPermission::None);
    let id_token_requested = requested
        .get("id-token")
        .copied()
        .unwrap_or(ContentsPermission::None);
    if matches!(contents_requested, ContentsPermission::None)
        && matches!(id_token_requested, ContentsPermission::None)
    {
        return Ok(None);
    }
    let Some(user_id) = run.triggered_by.as_deref() else {
        return Ok(None);
    };

    let capability = effective_capability(db, Some(user_id), repo, owner).await?;
    let actor_can_write = meets(capability, Capability::Write);
    // Per-scope cap — write grants for a non-write actor drop to read. The
    // granted map (not the requested one) is what lands in perms_json.
    let mut granted_map = std::collections::BTreeMap::new();
    for (scope, level) in requested {
        let granted = match level {
            ContentsPermission::Write if !actor_can_write => ContentsPermission::Read,
            l => *l,
        };
        granted_map.insert(scope.clone(), granted);
    }
    let contents_granted = match granted_map.get("contents") {
        Some(ContentsPermission::Write) => JobTokenPermission::Write,
        Some(ContentsPermission::Read) => JobTokenPermission::Read,
        _ => {
            // `contents: none` — no OXIDEAN_TOKEN plaintext is injected; the
            // row still exists to carry the OIDC request hash.
            JobTokenPermission::Read
        }
    };
    let inject_token = !matches!(
        granted_map.get("contents"),
        None | Some(ContentsPermission::None)
    );
    let perms_json = serde_json::to_string(
        &granted_map
            .iter()
            .map(|(k, v)| (k, v.as_str()))
            .collect::<std::collections::BTreeMap<_, _>>(),
    )
    .map_err(|e| e.to_string())?;

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
        // The truthful granted level — `none` rows exist only to bind the
        // OIDC request hash and resolve to no contents access.
        granted_map
            .get("contents")
            .map(|l| l.as_str())
            .unwrap_or("none"),
        Some(&perms_json),
        &token_hash,
        &expires_at,
    )
    .await?;

    // `id-token: write` → mint the OIDC request credential. The job env gets
    // the plaintext; the row gets its hash so `/.well-known/…/token` can bind
    // a request to exactly this job.
    let oidc_request_token =
        if matches!(granted_map.get("id-token"), Some(ContentsPermission::Write)) {
            let mut oid = [0u8; JOB_TOKEN_SECRET_BYTES];
            rand::fill(&mut oid);
            let plain = format!("{}{}", OIDC_REQUEST_TOKEN_PREFIX, bytes_to_hex(&oid));
            db.set_job_token_oidc_hash(job_id, &sha256_hex(plain.as_bytes()))
                .await?;
            Some(plain)
        } else {
            None
        };

    Ok(Some(MintedJobToken {
        plaintext: inject_token.then_some(plaintext),
        contents: contents_granted,
        oidc_request_token,
    }))
}

/// Prefix for the OIDC request credential (`ACTIONS_ID_TOKEN_REQUEST_TOKEN`).
/// Distinct from [`JOB_TOKEN_PREFIX`] so bearer routing never confuses them —
/// the request token resolves only through `oidc_request_hash`.
pub const OIDC_REQUEST_TOKEN_PREFIX: &str = "oxidean_oidc_";

/// Extract the frozen GitHub scope map from a durable job payload
/// (`permissions` — `{scope: "none"|"read"|"write"}`). Payloads enqueued
/// before Phase 7 carry only `contents_permission`; the fallback synthesizes
/// a `contents`-only map so legacy jobs mint identically to before.
pub fn permissions_from_payload(
    payload_json: &serde_json::Value,
) -> std::collections::BTreeMap<String, ContentsPermission> {
    use std::collections::BTreeMap;
    let parse_level = |s: &str| -> ContentsPermission {
        match s {
            "write" => ContentsPermission::Write,
            "none" => ContentsPermission::None,
            _ => ContentsPermission::Read,
        }
    };
    if let Some(map) = payload_json.get("permissions").and_then(|v| v.as_object()) {
        let mut out = BTreeMap::new();
        for (k, v) in map {
            if let Some(s) = v.as_str() {
                out.insert(k.clone(), parse_level(s));
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    let mut out = BTreeMap::new();
    let legacy = payload_json
        .get("contents_permission")
        .and_then(|v| v.as_str())
        .unwrap_or("read");
    out.insert("contents".to_string(), parse_level(legacy));
    out
}

/// Untrusted `pull_request` runs (forks, untrusted actors): every `write`
/// grant drops to `read` — the same rule GitHub applies to `GITHUB_TOKEN`
/// on fork PRs, applied across the whole scope map (`id-token` included).
pub fn downgrade_write_grants(
    permissions: &mut std::collections::BTreeMap<String, ContentsPermission>,
) {
    for level in permissions.values_mut() {
        if *level == ContentsPermission::Write {
            *level = ContentsPermission::Read;
        }
    }
}
