use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use oxidean_core::RpcResponse;
use oxidean_db::Database;

use oxidean_git::{CliGitBackend, GitBackend};

use crate::auth::pending::PendingAuthStore;
use crate::auth::session::{
    build_session_presence_cookie, clear_session_cookie, clear_session_presence_cookie,
    ResolvedSession, SessionService, SESSION_COOKIE_NAME, SESSION_IDLE,
};
use crate::email::{self, EmailSender};
use crate::pat::bearer::{self, BearerRejection};
use crate::pat::rate_limit::FailedAuthLimiter;
use crate::rpc::{self, CookieChange, RpcCtx};
use crate::user::rate_limit::LookupLimiter;

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    /// Swappable email sender (rebuilt on admin.auth.update_settings).
    pub email: Arc<RwLock<Arc<dyn EmailSender>>>,
    pub uploads_dir: PathBuf,
    /// Bare repos root (`OXIDEAN_REPOS_DIR`, default `var/repos`) — D-30 / D-31.
    pub repos_dir: PathBuf,
    /// Instance LFS object store (`OXIDEAN_LFS_DIR`, default `var/lfs`) — D-LFS-01.
    pub lfs_dir: PathBuf,
    /// Release asset binaries (`OXIDEAN_RELEASE_ASSETS_DIR`, default `var/release-assets`) — D-REL-04.
    pub release_assets_dir: PathBuf,
    /// Instance template pack zips (`OXIDEAN_TEMPLATE_PACKS_DIR`, default `var/template-packs`).
    pub template_packs_dir: PathBuf,
    /// Max upload bytes for a single release asset (default 512 MiB) — D-REL-05.
    pub release_asset_max_bytes: usize,
    /// Days to retain repository redirects after rename/transfer (default 90) — D-REL-08.
    pub repo_redirect_retention_days: u32,
    /// Package blob store root (`OXIDEAN_PACKAGES_DIR`, default `var/packages`) — D-PKG-07.
    pub packages_dir: PathBuf,
    /// Actions job log store (`OXIDEAN_ACTIONS_LOG_DIR`, default `var/actions-logs`) — D-ACT-13.
    pub actions_log_dir: PathBuf,
    /// Actions artifact blob store (`OXIDEAN_ACTIONS_ARTIFACTS_DIR`,
    /// default `var/actions-artifacts`) — upload/download-artifact payloads.
    pub actions_artifacts_dir: PathBuf,
    /// Actions cache blob store (`OXIDEAN_ACTIONS_CACHE_DIR`, default
    /// `var/actions-cache`) — `actions/cache` service payloads.
    pub actions_cache_dir: PathBuf,
    /// Per-repo cache ceiling (`OXIDEAN_ACTIONS_CACHE_MAX_BYTES`, default
    /// 10 GiB) — LRU eviction past this point.
    pub actions_cache_max_bytes: u64,
    /// Pages materialized site trees (`OXIDEAN_PAGES_DIR`, default
    /// `var/pages`) — `pages.*` builds publish here, `pages_serve` reads.
    pub pages_dir: PathBuf,
    /// Max bytes for a single artifact upload (default 2 GiB).
    pub actions_artifact_max_bytes: u64,
    /// Days an artifact is retained (default 90 — GitHub's own default).
    pub actions_artifact_retention_days: u32,
    /// Instance Actions gate (`OXIDEAN_ACTIONS_ENABLED`, default true) — D-ACT-06.
    pub actions_enabled: bool,
    /// Env default for the MCP endpoint (`OXIDEAN_MCP_ENABLED`, default true) —
    /// AGT-03. `instance_mcp_settings.enabled` (admin override) wins when set.
    pub mcp_enabled: bool,
    /// Git forge backend — Phase 7 registers [`CliGitBackend`] only (D-32).
    pub git: Arc<dyn GitBackend>,
    pub sessions: SessionService,
    pub pending: PendingAuthStore,
    /// Pending second-factor login challenges (issue #130) — per process.
    pub two_factor_store: crate::auth::two_factor::TwoFactorStore,
    /// `auth.login_begin` counters (issue #130) — per process.
    pub login_begin_limiter: Arc<Mutex<crate::auth::login_rate::LoginBeginLimiter>>,
    /// Smart HTTP failed Basic/PAT auth counters (D-26) — per process.
    pub git_auth_limiter: Arc<Mutex<FailedAuthLimiter>>,
    /// `user.lookup` per-session counters (T-10-03) — per process.
    pub lookup_limiter: Arc<Mutex<LookupLimiter>>,
    pub env_name: String,
    /// In-repo search soft caps (D-SRCH-08 / Phase 16).
    pub search_timeout_ms: u64,
    pub search_max_matches: u32,
    pub search_max_files: u32,
    /// Agentic VCS slot — `None` when the `OXIDEAN_VCS_*` config is invalid
    /// or the objects dir refuses (`warn!` at boot); procedures then answer
    /// `vcs.disabled`, the same wire shape an unenabled repo produces.
    pub vcs: Option<crate::vcs::SharedVcs>,
    /// The procedure dispatcher edge code resolves through — REST (`/api/v1`),
    /// MCP, and in-process callers run `ctx.rpc_dispatch(ctx, req)`. The
    /// shell installs its aggregated multi-crate dispatch in
    /// `router_with_state`; the default is [`rpc::core_dispatch`] (issue #162).
    pub rpc_dispatch: crate::rpc::RpcDispatch,
}

/// `OXIDEAN_ACTIONS_ENABLED`: unset or anything but `0`/`false`/`no`/`off`/`""`
/// means on. Default **on** so self-host/local dev needs zero configuration.
fn mcp_env_truthy(raw: &str) -> bool {
    let t = raw.trim().to_ascii_lowercase();
    !(t.is_empty() || t == "0" || t == "false" || t == "no" || t == "off")
}

/// Env default for the MCP endpoint (`OXIDEAN_MCP_ENABLED`, default true) —
/// read once into [`AppState::mcp_enabled`]; the admin override wins when set.
pub fn env_mcp_enabled() -> bool {
    std::env::var("OXIDEAN_MCP_ENABLED")
        .map(|v| mcp_env_truthy(&v))
        .unwrap_or(true)
}

impl AppState {
    pub fn new(db: Database, email: Arc<dyn EmailSender>, env_name: impl Into<String>) -> Self {
        let env_name = env_name.into();
        let repos_dir = std::env::var("OXIDEAN_REPOS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("var/repos"));
        // Absolutize so git ops that run in a temp `-C` worktree (seed push) still
        // resolve the bare remote correctly when OXIDEAN_REPOS_DIR is relative.
        let repos_dir = if repos_dir.is_absolute() {
            repos_dir
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("/"))
                .join(repos_dir)
        };
        let lfs_dir = std::env::var("OXIDEAN_LFS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("var/lfs"));
        let lfs_dir = if lfs_dir.is_absolute() {
            lfs_dir
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("/"))
                .join(lfs_dir)
        };
        let release_assets_dir = std::env::var("OXIDEAN_RELEASE_ASSETS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("var/release-assets"));
        let release_assets_dir = if release_assets_dir.is_absolute() {
            release_assets_dir
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("/"))
                .join(release_assets_dir)
        };
        let template_packs_dir = std::env::var("OXIDEAN_TEMPLATE_PACKS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("var/template-packs"));
        let template_packs_dir = if template_packs_dir.is_absolute() {
            template_packs_dir
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("/"))
                .join(template_packs_dir)
        };
        let release_asset_max_bytes = std::env::var("OXIDEAN_RELEASE_ASSET_MAX_BYTES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(536_870_912usize);
        let repo_redirect_retention_days = std::env::var("OXIDEAN_REPO_REDIRECT_RETENTION_DAYS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(90u32);
        let packages_dir = std::env::var("OXIDEAN_PACKAGES_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("var/packages"));
        let packages_dir = if packages_dir.is_absolute() {
            packages_dir
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("/"))
                .join(packages_dir)
        };
        let actions_log_dir = crate::actions::logs::resolve_log_dir();
        let actions_artifacts_dir = crate::actions::artifacts::resolve_artifact_dir();
        let actions_artifact_max_bytes = std::env::var("OXIDEAN_ACTIONS_ARTIFACT_MAX_BYTES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(2 * 1024 * 1024 * 1024u64);
        let actions_artifact_retention_days =
            std::env::var("OXIDEAN_ACTIONS_ARTIFACT_RETENTION_DAYS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(90u32);
        let actions_enabled = std::env::var("OXIDEAN_ACTIONS_ENABLED")
            .map(|v| {
                let t = v.trim().to_ascii_lowercase();
                !(t.is_empty() || t == "0" || t == "false" || t == "no" || t == "off")
            })
            .unwrap_or(true);
        let mcp_enabled = env_mcp_enabled();
        let search_timeout_ms = std::env::var("OXIDEAN_SEARCH_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8000u64);
        let search_max_matches = std::env::var("OXIDEAN_SEARCH_MAX_MATCHES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100u32);
        let search_max_files = std::env::var("OXIDEAN_SEARCH_MAX_FILES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(50u32);
        let git = Arc::new(CliGitBackend::new()) as Arc<dyn GitBackend>;
        let vcs = build_vcs_slot(&db, &git, &repos_dir);
        Self {
            db,
            email: Arc::new(RwLock::new(email)),
            uploads_dir: PathBuf::from("var/uploads"),
            repos_dir,
            lfs_dir,
            release_assets_dir,
            template_packs_dir,
            release_asset_max_bytes,
            repo_redirect_retention_days,
            packages_dir,
            actions_log_dir,
            actions_artifacts_dir,
            actions_cache_dir: crate::actions::cache::resolve_cache_dir(),
            actions_cache_max_bytes: crate::actions::cache::default_cache_max_bytes(),
            pages_dir: crate::pages::resolve_pages_dir(),
            actions_artifact_max_bytes,
            actions_artifact_retention_days,
            actions_enabled,
            mcp_enabled,
            git,
            sessions: SessionService::new(env_name.clone()),
            pending: PendingAuthStore::new(),
            two_factor_store: crate::auth::two_factor::TwoFactorStore::new(),
            login_begin_limiter: Arc::new(Mutex::new(
                crate::auth::login_rate::LoginBeginLimiter::new(),
            )),
            git_auth_limiter: Arc::new(Mutex::new(FailedAuthLimiter::new())),
            lookup_limiter: Arc::new(Mutex::new(LookupLimiter::new())),
            env_name,
            search_timeout_ms,
            search_max_matches,
            search_max_files,
            vcs,
            rpc_dispatch: crate::rpc::core_dispatch(),
        }
    }

    pub fn with_uploads_dir(mut self, dir: PathBuf) -> Self {
        self.uploads_dir = dir;
        self
    }

    pub fn with_repos_dir(mut self, dir: PathBuf) -> Self {
        self.repos_dir = dir;
        self
    }

    pub fn with_lfs_dir(mut self, dir: PathBuf) -> Self {
        self.lfs_dir = dir;
        self
    }

    pub fn with_release_assets_dir(mut self, dir: PathBuf) -> Self {
        self.release_assets_dir = dir;
        self
    }

    pub fn with_release_asset_max_bytes(mut self, max: usize) -> Self {
        self.release_asset_max_bytes = max;
        self
    }

    pub fn with_packages_dir(mut self, dir: PathBuf) -> Self {
        self.packages_dir = dir;
        self
    }

    pub fn with_actions_log_dir(mut self, dir: PathBuf) -> Self {
        self.actions_log_dir = dir;
        self
    }

    pub fn with_actions_artifacts_dir(mut self, dir: PathBuf) -> Self {
        self.actions_artifacts_dir = dir;
        self
    }

    pub fn with_actions_cache_dir(mut self, dir: PathBuf) -> Self {
        self.actions_cache_dir = dir;
        self
    }

    pub fn with_actions_cache_max_bytes(mut self, max: u64) -> Self {
        self.actions_cache_max_bytes = max;
        self
    }

    pub fn with_pages_dir(mut self, dir: PathBuf) -> Self {
        self.pages_dir = dir;
        self
    }

    pub fn with_actions_enabled(mut self, enabled: bool) -> Self {
        self.actions_enabled = enabled;
        self
    }

    /// Test hook — simulates `OXIDEAN_MCP_ENABLED=false` without process env.
    pub fn with_mcp_enabled(mut self, enabled: bool) -> Self {
        self.mcp_enabled = enabled;
        self
    }

    pub fn with_git(mut self, git: Arc<dyn GitBackend>) -> Self {
        self.git = git;
        // Rebuild the VCS slot so interop rides the injected backend too.
        self.vcs = build_vcs_slot(&self.db, &self.git, &self.repos_dir);
        self
    }

    pub fn current_email(&self) -> Arc<dyn EmailSender> {
        self.email
            .read()
            .map(|g| g.clone())
            .unwrap_or_else(|_| email::build_email_sender_from_env())
    }
}

pub fn session_token_from_headers(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie_header.split(';') {
        let part = part.trim();
        let prefix = format!("{SESSION_COOKIE_NAME}=");
        if let Some(value) = part.strip_prefix(prefix.as_str()) {
            return Some(value.to_string());
        }
    }
    None
}

/// Parse the `oxidean_2fa` pending-challenge cookie (issue #130).
pub fn two_factor_id_from_headers(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie_header.split(';') {
        let part = part.trim();
        let prefix = format!("{}=", crate::auth::two_factor::TWO_FACTOR_COOKIE_NAME);
        if let Some(value) = part.strip_prefix(prefix.as_str()) {
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Resolve a raw `oxidean_session` cookie token to a session (shared by RPC + MCP).
pub async fn resolve_session_token(
    state: &AppState,
    raw_token: Option<&str>,
    client: &rpc::ClientMeta,
) -> Option<ResolvedSession> {
    match raw_token {
        Some(token) => match state
            .sessions
            .resolve(
                &state.db,
                token,
                client.ip_address.as_deref(),
                client.user_agent.as_deref(),
            )
            .await
        {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "session resolve failed");
                None
            }
        },
        None => None,
    }
}

/// Build the VCS slot from the `OXIDEAN_VCS_*` env (frozen defaults — the
/// config's own `from_env` pins them). A malformed env, a refused objects dir
/// (foreign content, dir-is-file), or a caps validation failure degrades to
/// `None` + `warn!` — VCS never takes the instance down at boot; every
/// procedure then answers `vcs.disabled`, identical wire shape to an
/// unenabled repo.
fn build_vcs_slot(
    db: &Database,
    git: &Arc<dyn GitBackend>,
    repos_dir: &Path,
) -> Option<crate::vcs::SharedVcs> {
    use oxidean_vcs::{
        api::VcsApi,
        changeset_store::ChangesetStore,
        git_interop::{GitInterop, InteropCaps},
        merge_engine::{MergeCaps, MergeEngine},
        object_store::{ObjectStore, ObjectStoreConfig},
        read_index::{ReadIndex, ReadIndexCaps},
    };

    let mut config = match ObjectStoreConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "vcs disabled: invalid OXIDEAN_VCS_* config");
            return None;
        }
    };
    // Same absolutize rule as `repos_dir` — relative `./var/…` anchors at the
    // process cwd so the dir is stable no matter where ops `-C` into.
    if !config.objects_dir.is_absolute() {
        config.objects_dir = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(&config.objects_dir);
    }

    /// Each engine component owns an `ObjectStore` handle over the SAME
    /// directory — the store carries no mutable state, this is the u4/u5
    /// construction pattern.
    fn objects(db: &Database, cfg: &ObjectStoreConfig) -> Option<ObjectStore> {
        match ObjectStore::new(db.clone(), cfg.clone()) {
            Ok(o) => Some(o),
            Err(e) => {
                tracing::warn!(error = %e, "vcs disabled: object store refused");
                None
            }
        }
    }

    let api_objects = objects(db, &config)?;
    let index = ReadIndex::new(db.clone(), objects(db, &config)?, ReadIndexCaps::default());
    let merge = match MergeEngine::new(
        db.clone(),
        ReadIndex::new(db.clone(), objects(db, &config)?, ReadIndexCaps::default()),
        ChangesetStore::new(db.clone()),
        objects(db, &config)?,
        MergeCaps::default(),
    ) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(error = %e, "vcs disabled: merge caps refused");
            return None;
        }
    };
    let checker =
        crate::vcs_protection::HostProtectionChecker::new(db.clone(), repos_dir.to_path_buf());
    let interop = match GitInterop::new(
        db.clone(),
        objects(db, &config)?,
        ChangesetStore::new(db.clone()),
        ReadIndex::new(db.clone(), objects(db, &config)?, ReadIndexCaps::default()),
        git.clone(),
        checker,
        InteropCaps::new(config.max_object_bytes),
    ) {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!(error = %e, "vcs disabled: interop caps refused");
            return None;
        }
    };
    tracing::info!(objects_dir = %config.objects_dir.display(), "vcs enabled");
    Some(Arc::new(crate::vcs::VcsSlot {
        api: VcsApi::new(db.clone(), api_objects, index, merge, Some(interop)),
        objects_dir: config.objects_dir.clone(),
        gc_interval_secs: config.gc_interval_secs,
    }))
}

/// Build an [`RpcCtx`] from an already-resolved session (cookie or token-derived).
/// Resolve the request's session cookie or PAT Bearer to `(user_id,
/// pat_identity)`. Cookie session or `Authorization: Bearer <pat>` — asset
/// up/download is the CLI's path, so PATs (which never mint sessions) are
/// honored alongside browser cookies. Returns the user id plus the
/// `PatIdentity` (when bearer-authed) so callers can scope-gate the
/// operation. Shared by REST-adjacent edge routes that key work to a caller
/// without building a full `RpcCtx` (release-asset upload, actions-download
/// auth). Moved to core with the REST extraction (#162).
pub async fn resolve_session_user_id(
    state: &AppState,
    headers: &HeaderMap,
) -> Option<(String, Option<bearer::PatIdentity>)> {
    let client = rpc::ClientMeta::from_headers(headers);
    if let Some(token) = session_token_from_headers(headers) {
        return match state
            .sessions
            .resolve(
                &state.db,
                &token,
                client.ip_address.as_deref(),
                client.user_agent.as_deref(),
            )
            .await
        {
            Ok(Some(s)) => Some((s.user_id, None)),
            _ => None,
        };
    }
    if let Some(token) = bearer_token_from_headers(headers) {
        return match bearer::resolve_bearer(
            &state.db,
            &state.git_auth_limiter,
            &token,
            client.ip_address.as_deref(),
        )
        .await
        {
            Ok((session, identity)) => Some((session.user_id, Some(identity))),
            Err(_) => None,
        };
    }
    None
}

/// Used by the MCP endpoint; PAT Bearer identity is applied separately there.
pub fn build_rpc_ctx_with_session(
    state: &AppState,
    session: Option<ResolvedSession>,
    client: rpc::ClientMeta,
) -> RpcCtx {
    let email = state.current_email();
    RpcCtx {
        db: state.db.clone(),
        email,
        email_slot: state.email.clone(),
        sessions: state.sessions.clone(),
        uploads_dir: state.uploads_dir.clone(),
        repos_dir: state.repos_dir.clone(),
        lfs_dir: state.lfs_dir.clone(),
        release_assets_dir: state.release_assets_dir.clone(),
        template_packs_dir: state.template_packs_dir.clone(),
        actions_log_dir: state.actions_log_dir.clone(),
        actions_artifacts_dir: state.actions_artifacts_dir.clone(),
        actions_cache_dir: state.actions_cache_dir.clone(),
        pages_dir: state.pages_dir.clone(),
        git: state.git.clone(),
        env_name: state.env_name.clone(),
        session,
        pat: None,
        client,
        set_cookie: None,
        two_factor: crate::auth::two_factor::TwoFactorCtx {
            store: state.two_factor_store.clone(),
            challenge_id: None,
        },
        login_begin_limiter: state.login_begin_limiter.clone(),
        lookup_limiter: state.lookup_limiter.clone(),
        search_timeout_ms: state.search_timeout_ms,
        search_max_matches: state.search_max_matches,
        search_max_files: state.search_max_files,
        vcs: state.vcs.clone(),
        rpc_dispatch: state.rpc_dispatch.clone(),
    }
}

/// Edge credential for `/api/rpc` (API-02): the session cookie always wins;
/// `Authorization: Bearer <pat>` is consulted only when no cookie is present.
#[derive(Debug, Clone)]
pub enum RpcCredential {
    Cookie(String),
    Bearer(String),
    Anonymous,
}

fn bearer_token_from_headers(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?.trim();
    let (scheme, token) = raw.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_string())
}

/// `Authorization: Basic <b64 user:token>` → the token when its password
/// slot carries a PAT / job token (same pairs git Smart HTTP accepts —
/// raw account passwords are never honored). Lets Gitea-style Basic
/// clients authenticate the whole REST/RPC edge, e.g. `tea` CLI.
fn basic_pat_from_headers(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let encoded = raw
        .strip_prefix("Basic ")
        .or_else(|| raw.strip_prefix("basic "))?;
    let decoded = String::from_utf8(base64_decode(encoded.trim())?).ok()?;
    let (_user, pass) = decoded.split_once(':')?;
    let pass = pass.trim();
    (pass.starts_with(oxidean_core::CLASSIC_PAT_PREFIX)
        || pass.starts_with(oxidean_core::FINE_GRAINED_PAT_PREFIX)
        || pass.starts_with(crate::actions::job_tokens::JOB_TOKEN_PREFIX))
    .then(|| pass.to_string())
}

/// Small base64 decoder (same shape as `git_smart_http`/`lfs::auth` — no
/// new crate dep for one header).
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let bytes: Vec<u8> = input
        .bytes()
        .filter(|&b| !b.is_ascii_whitespace())
        .collect();
    if bytes.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: i32 = 0;
    for &b in &bytes {
        if b == b'=' {
            break;
        }
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

pub fn edge_credential(headers: &HeaderMap) -> RpcCredential {
    if let Some(token) = session_token_from_headers(headers) {
        return RpcCredential::Cookie(token);
    }
    if let Some(token) = bearer_token_from_headers(headers) {
        return RpcCredential::Bearer(token);
    }
    if let Some(token) = basic_pat_from_headers(headers) {
        return RpcCredential::Bearer(token);
    }
    RpcCredential::Anonymous
}

#[allow(clippy::result_large_err)]
pub async fn build_rpc_ctx(
    state: &AppState,
    credential: RpcCredential,
    client: rpc::ClientMeta,
    two_factor_challenge: Option<String>,
) -> Result<RpcCtx, BearerRejection> {
    let (session, pat) = match &credential {
        RpcCredential::Cookie(token) => {
            let session = match state
                .sessions
                .resolve(
                    &state.db,
                    token,
                    client.ip_address.as_deref(),
                    client.user_agent.as_deref(),
                )
                .await
            {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(error = %e, "session resolve failed");
                    None
                }
            };
            (session, None)
        }
        RpcCredential::Bearer(token) => {
            // Failed-auth limiter is shared with Smart HTTP PAT auth (D-26).
            let (session, identity) = bearer::resolve_bearer(
                &state.db,
                &state.git_auth_limiter,
                token,
                client.ip_address.as_deref(),
            )
            .await?;
            // Job tokens live in `action_job_tokens`, not `personal_access_tokens`
            // — skip the PAT `last_used` write for them.
            if !identity
                .token_prefix
                .starts_with(crate::actions::job_tokens::JOB_TOKEN_PREFIX)
            {
                bearer::touch_last_used(
                    &state.db,
                    &identity.token_id,
                    client.ip_address.as_deref(),
                )
                .await;
            }
            (Some(session), Some(identity))
        }
        RpcCredential::Anonymous => (None, None),
    };
    let email = state.current_email();
    Ok(RpcCtx {
        db: state.db.clone(),
        email,
        email_slot: state.email.clone(),
        sessions: state.sessions.clone(),
        uploads_dir: state.uploads_dir.clone(),
        repos_dir: state.repos_dir.clone(),
        lfs_dir: state.lfs_dir.clone(),
        release_assets_dir: state.release_assets_dir.clone(),
        template_packs_dir: state.template_packs_dir.clone(),
        actions_log_dir: state.actions_log_dir.clone(),
        actions_artifacts_dir: state.actions_artifacts_dir.clone(),
        actions_cache_dir: state.actions_cache_dir.clone(),
        pages_dir: state.pages_dir.clone(),
        git: state.git.clone(),
        env_name: state.env_name.clone(),
        session,
        pat,
        client,
        set_cookie: None,
        two_factor: crate::auth::two_factor::TwoFactorCtx {
            store: state.two_factor_store.clone(),
            challenge_id: two_factor_challenge,
        },
        login_begin_limiter: state.login_begin_limiter.clone(),
        lookup_limiter: state.lookup_limiter.clone(),
        search_timeout_ms: state.search_timeout_ms,
        search_max_matches: state.search_max_matches,
        search_max_files: state.search_max_files,
        vcs: state.vcs.clone(),
        rpc_dispatch: state.rpc_dispatch.clone(),
    })
}

/// Bearer auth failures are HTTP errors (401 + `WWW-Authenticate`, or 429 +
/// `Retry-After`) with a standard RPC error body — never a silent downgrade.
pub fn bearer_rejection_response(rejection: BearerRejection) -> axum::response::Response {
    let status = rejection.status();
    let mut res = (status, Json(RpcResponse::err(rejection.error))).into_response();
    if status == StatusCode::UNAUTHORIZED {
        if let Ok(v) = HeaderValue::from_str(r#"Bearer realm="Oxidean RPC""#) {
            res.headers_mut().insert(header::WWW_AUTHENTICATE, v);
        }
    }
    if let Some(secs) = rejection.retry_after {
        if let Ok(v) = HeaderValue::from_str(&secs.to_string()) {
            res.headers_mut().insert(header::RETRY_AFTER, v);
        }
    }
    res
}

fn append_set_cookie(response: &mut axum::response::Response, cookie: &cookie::Cookie<'_>) {
    let value = cookie.to_string();
    if let Ok(hv) = HeaderValue::from_str(&value) {
        response.headers_mut().append(header::SET_COOKIE, hv);
    }
}

pub fn attach_set_cookie(
    mut response: axum::response::Response,
    change: Option<CookieChange>,
    env_name: &str,
) -> axum::response::Response {
    let Some(change) = change else {
        return response;
    };
    match change {
        CookieChange::Set(c) => {
            let ttl = c
                .max_age()
                .map(|d| std::time::Duration::from_secs(d.whole_seconds().max(0) as u64))
                .unwrap_or(SESSION_IDLE);
            append_set_cookie(&mut response, &c);
            append_set_cookie(&mut response, &build_session_presence_cookie(ttl, env_name));
            // A real session satisfies any pending 2FA challenge (#130).
            append_set_cookie(
                &mut response,
                &crate::auth::two_factor::cleared_challenge_cookie(env_name),
            );
        }
        CookieChange::SetTwoFactor(c) => {
            append_set_cookie(&mut response, &c);
        }
        CookieChange::Clear => {
            append_set_cookie(&mut response, &clear_session_cookie(env_name));
            append_set_cookie(&mut response, &clear_session_presence_cookie(env_name));
            append_set_cookie(
                &mut response,
                &crate::auth::two_factor::cleared_challenge_cookie(env_name),
            );
        }
    }
    response
}

pub fn rpc_status(resp: &RpcResponse) -> StatusCode {
    match resp {
        RpcResponse::Ok { .. } => StatusCode::OK,
        RpcResponse::Err { error, .. } if error.code == "rpc.unknown_procedure" => {
            StatusCode::NOT_FOUND
        }
        RpcResponse::Err { error, .. }
            if matches!(
                error.code.as_str(),
                "auth.unauthenticated" | "auth.token_expired" | "auth.token_revoked"
            ) =>
        {
            StatusCode::UNAUTHORIZED
        }
        RpcResponse::Err { error, .. } if error.code == "admin.forbidden" => StatusCode::FORBIDDEN,
        RpcResponse::Err { error, .. } if error.code == "auth.banned" => StatusCode::FORBIDDEN,
        RpcResponse::Err { error, .. } if error.code == "auth.email_unverified" => {
            StatusCode::FORBIDDEN
        }
        RpcResponse::Err { error, .. } if error.code == "auth.pat_scope" => StatusCode::FORBIDDEN,
        RpcResponse::Err { error, .. } if error.code == "repo.create_forbidden" => {
            StatusCode::FORBIDDEN
        }
        RpcResponse::Err { error, .. } if error.code == "repo.not_found" => StatusCode::NOT_FOUND,
        RpcResponse::Err { error, .. } if error.code == "issue.not_found" => StatusCode::NOT_FOUND,
        RpcResponse::Err { error, .. } if error.code == "issue.comment_not_found" => {
            StatusCode::NOT_FOUND
        }
        RpcResponse::Err { error, .. } if error.code == "repo.path_not_found" => {
            StatusCode::NOT_FOUND
        }
        RpcResponse::Err { .. } => StatusCode::BAD_REQUEST,
    }
}
