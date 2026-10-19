use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::DefaultBodyLimit;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use oxidean_core::{AppError, RpcRequest, RpcResponse};
use oxidean_db::Database;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use oxidean_git::{CliGitBackend, GitBackend};

use crate::auth::pending::PendingAuthStore;
use crate::auth::session::{
    build_session_presence_cookie, clear_session_cookie, clear_session_presence_cookie,
    ResolvedSession, SessionService, SESSION_COOKIE_NAME, SESSION_IDLE,
};
use crate::email::{self, EmailSender};
use crate::pat::bearer::{self, BearerRejection};
use crate::pat::rate_limit::FailedAuthLimiter;
use crate::routes::{
    action_badge, action_downloads, auth_callbacks, avatar, cli_dist, feeds, git_lfs,
    git_smart_http, pages_serve, release_assets, repo_raw, template_packs,
};
use crate::rpc::{self, CookieChange, RpcCtx, VERSION_HEADER};
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
        let mcp_enabled = crate::mcp::env_mcp_enabled();
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

/// Build router with default email sender from ENV and `OXIDEAN_ENV`.
pub fn router(db: Database, cors: CorsLayer) -> Router {
    let env_name = std::env::var("OXIDEAN_ENV").unwrap_or_else(|_| "development".into());
    let email = email::build_email_sender_from_env();
    router_with_state(AppState::new(db, email, env_name), cors)
}

pub fn router_with_state(state: AppState, cors: CorsLayer) -> Router {
    let asset_body_limit = state.release_asset_max_bytes.max(1024);
    Router::new()
        .route("/health", get(health))
        .route("/api/rpc", post(rpc_http))
        .route("/api/rpc/ws", get(rpc_ws))
        // REST facade over the RPC domain (API-01).
        .nest("/api/v1", crate::rest::router())
        .route(
            "/api/mcp",
            post(crate::mcp::handle_post).get(crate::mcp::handle_get),
        )
        .nest("/api/actions", crate::actions::runner_proto::router())
        // OIDC issuer for `id-token: write` jobs — the request token itself
        // is the credential, so these paths carry no session middleware.
        .nest("/api/actions/oidc", crate::actions::oidc::router())
        .route("/api/auth/workos/start", get(auth_callbacks::workos_start))
        .route(
            "/api/auth/workos/callback",
            get(auth_callbacks::workos_callback),
        )
        .route("/api/auth/oidc/start", get(auth_callbacks::oidc_start))
        .route(
            "/api/auth/oidc/callback",
            get(auth_callbacks::oidc_callback),
        )
        // Web-tier session probe (`oxidean-web` stamps `data-oxidean-session`
        // and gates protected shells on the result): read-only, one SELECT,
        // no session touch on the document path.
        .route("/api/auth/session-check", get(session_check))
        // Web-tier setup probe (`oxidean-web` stamps `data-oxidean-setup`):
        // read-only COUNT, deployment-global so no cookie is needed — the SPA
        // drops its per-page auth.bootstrapStatus fetch in favor of the stamp.
        .route("/api/auth/setup-check", get(setup_check))
        // OAuth2 provider surface (API-03). The consent screen itself is the
        // SPA route /oauth/consent; these three paths are API-owned and must be
        // routed to the API at the edge (Caddyfile / Traefik / vite proxy).
        .route("/oauth/authorize", get(crate::oauth::authorize))
        .route("/oauth/token", post(crate::oauth::token))
        .route("/oauth/userinfo", get(crate::oauth::userinfo))
        .route("/oauth/device/code", post(crate::oauth::device_code))
        // RFC 8414 + RFC 9728 discovery — OAuth/MCP clients resolve these via
        // the `resource_metadata` pointer on /api/mcp 401s. Edge proxies must
        // forward `/.well-known/oauth*` alongside the OAuth routes.
        .route(
            "/.well-known/oauth-authorization-server",
            get(crate::oauth::authorization_server_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource",
            get(crate::oauth::protected_resource_metadata),
        )
        .route(
            "/api/user/avatar",
            post(avatar::upload_avatar)
                .layer(DefaultBodyLimit::max(avatar::AVATAR_MAX_BYTES))
                .delete(avatar::delete_avatar),
        )
        .route("/uploads/avatars/{file}", get(avatar::serve_avatar))
        .route(
            "/api/repos/{owner}/{repo}/releases/{release_id}/assets",
            post(release_assets::upload_asset).layer(DefaultBodyLimit::max(asset_body_limit)),
        )
        .route(
            "/api/releases/assets/{asset_id}",
            get(release_assets::download_asset),
        )
        // Browser-facing Actions downloads — session/PAT-authed, unlike the
        // runner-token /api/actions/* surface.
        .route(
            "/api/repos/{owner}/{repo}/actions/artifacts/{artifact_id}",
            get(action_downloads::download_artifact),
        )
        .route(
            "/api/repos/{owner}/{repo}/actions/jobs/{job_id}/logs",
            get(action_downloads::download_job_log),
        )
        .route(
            "/api/repos/{owner}/{repo}/actions/jobs/{job_id}/logs/stream",
            get(action_downloads::stream_job_log),
        )
        .route(
            "/api/repos/{owner}/{repo}/actions/runs/{run_id}/logs",
            get(action_downloads::download_run_logs),
        )
        .route(
            "/api/admin/templates",
            post(template_packs::upload_pack).layer(DefaultBodyLimit::max(
                crate::templates::store::DEFAULT_MAX_PACK_BYTES as usize,
            )),
        )
        .route(
            "/api/repos/{owner}/{repo}/raw/{ref}/{*path}",
            get(repo_raw::serve_raw),
        )
        .route(
            "/api/repos/{owner}/{repo}/archive/{*archive_file}",
            get(repo_raw::serve_archive),
        )
        .route(
            "/api/repos/{owner}/{repo}/mirror/hook",
            axum::routing::post(crate::mirror::mirror_hook),
        )
        // Atom feeds (API-05) — /api prefix keeps them on this service at the edge.
        .route(
            "/api/repos/{owner}/{repo}/activity.atom",
            get(feeds::repo_activity_feed),
        )
        .route(
            "/api/repos/{owner}/{repo}/releases.atom",
            get(feeds::repo_releases_feed),
        )
        .route(
            "/api/users/{username}/activity.atom",
            get(feeds::user_activity_feed),
        )
        // GitHub-shaped workflow status badge — /{owner}/{repo}/actions/
        // workflows/{file}/badge.svg (web tier proxies this path here); also
        // mounted under /api/repos for direct API consumers.
        .route(
            "/{owner}/{repo}/actions/workflows/{workflow}/badge.svg",
            get(action_badge::workflow_badge),
        )
        .route(
            "/api/repos/{owner}/{repo}/actions/workflows/{workflow}/badge.svg",
            get(action_badge::workflow_badge),
        )
        // Pages sites — /pages/{owner}/{repo}/{*path} serves the materialized
        // tree (edge routes the reserved `/pages` prefix here; the web tier
        // proxies it too). Same-origin user HTML is sandboxed via CSP inside
        // the handler.
        .route("/pages/{owner}/{repo}", get(pages_serve::redirect_root))
        .route("/pages/{owner}/{repo}/", get(pages_serve::serve_index))
        .route("/pages/{owner}/{repo}/{*path}", get(pages_serve::serve))
        // Smart HTTP — D-18/D-22: only on /{owner}/{repo}.git (segment includes .git suffix)
        .route(
            "/{owner}/{repo_git}/info/refs",
            get(git_smart_http::info_refs),
        )
        .route(
            "/{owner}/{repo_git}/git-upload-pack",
            axum::routing::post(git_smart_http::upload_pack),
        )
        .route(
            "/{owner}/{repo_git}/git-receive-pack",
            axum::routing::post(git_smart_http::receive_pack),
        )
        // Git LFS — Batch + basic transfer (D-LFS-06 / D-LFS-07)
        .route(
            "/{owner}/{repo_git}/info/lfs/objects/batch",
            axum::routing::post(git_lfs::batch),
        )
        .route(
            "/{owner}/{repo_git}/info/lfs/objects/verify",
            axum::routing::post(git_lfs::verify_object),
        )
        .route(
            "/{owner}/{repo_git}/info/lfs/objects/{oid}",
            axum::routing::get(git_lfs::get_object)
                .put(git_lfs::put_object)
                .layer(DefaultBodyLimit::max(2 * 1024 * 1024 * 1024)),
        )
        // Agentic VCS object transport (contract C2) — Bearer/PAT only, never
        // the session cookie. The 512 MiB body limit sits above the 256 MiB
        // per-object cap so the typed `vcs.quota_exceeded` answers before
        // Axum's untyped 413.
        .route(
            "/api/vcs/{owner}/{name}/objects/{hash}",
            get(crate::vcs_routes::object_get),
        )
        .route(
            "/api/vcs/{owner}/{name}/objects",
            axum::routing::post(crate::vcs_routes::object_put)
                .layer(DefaultBodyLimit::max(512 * 1024 * 1024)),
        )
        // CLI distribution — install script + ox binaries; must route to the
        // API at the edge (unauthenticated; ox self-update consumes it).
        .route("/cli/install.sh", get(cli_dist::install_script))
        .route("/cli/install.ps1", get(cli_dist::install_ps1))
        .route("/cli/skill.md", get(cli_dist::skill_md))
        .route("/cli/latest", get(cli_dist::latest))
        .route("/cli/bin/{*file}", get(cli_dist::binary))
        // Package registry (D-PKG-01) — path prefixes must outrank SPA at the edge.
        .route("/v2", get(crate::packages::oci::discovery))
        .route("/v2/", get(crate::packages::oci::discovery))
        .nest("/v2", crate::packages::oci::router())
        .nest("/npm", crate::packages::npm::router())
        .nest("/generic", crate::packages::generic::router())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true }))
}

/// `GET /api/auth/session-check` — reports whether the presented
/// `oxidean_session` cookie maps to a live session. Store failures surface as
/// 500 (never `valid:false`) so the web tier can tell "definitively signed
/// out" from "couldn't ask" and keep its presence fallback on errors.
async fn session_check(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    // no-store — a cached probe result would pin a stale validity verdict.
    let no_store = [(header::CACHE_CONTROL, "no-store")];
    let Some(token) = session_token_from_headers(&headers) else {
        return (no_store, Json(serde_json::json!({ "valid": false }))).into_response();
    };
    match state.sessions.peek(&state.db, &token).await {
        Ok(valid) => (no_store, Json(serde_json::json!({ "valid": valid }))).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "session-check probe failed");
            (no_store, StatusCode::INTERNAL_SERVER_ERROR).into_response()
        }
    }
}

/// `GET /api/auth/setup-check` — reports whether the instance still needs
/// first-run setup. Store failures surface as 500 so the web tier stamps
/// nothing (fail-open) rather than pinning a dead-API instance into /setup.
async fn setup_check(State(state): State<AppState>) -> impl IntoResponse {
    let no_store = [(header::CACHE_CONTROL, "no-store")];
    match crate::auth::bootstrap::needs_setup(&state.db).await {
        Ok(needs_setup) => (
            no_store,
            Json(serde_json::json!({ "needs_setup": needs_setup })),
        )
            .into_response(),
        Err(e) => {
            tracing::warn!(error = ?e, "setup-check probe failed");
            (no_store, StatusCode::INTERNAL_SERVER_ERROR).into_response()
        }
    }
}

pub(crate) fn session_token_from_headers(headers: &HeaderMap) -> Option<String> {
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
pub(crate) fn two_factor_id_from_headers(headers: &HeaderMap) -> Option<String> {
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
pub(crate) async fn resolve_session_token(
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
/// Used by the MCP endpoint; PAT Bearer identity is applied separately there.
pub(crate) fn build_rpc_ctx_with_session(
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
    }
}

/// Edge credential for `/api/rpc` (API-02): the session cookie always wins;
/// `Authorization: Bearer <pat>` is consulted only when no cookie is present.
#[derive(Debug, Clone)]
pub(crate) enum RpcCredential {
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

pub(crate) fn edge_credential(headers: &HeaderMap) -> RpcCredential {
    if let Some(token) = session_token_from_headers(headers) {
        return RpcCredential::Cookie(token);
    }
    if let Some(token) = bearer_token_from_headers(headers) {
        return RpcCredential::Bearer(token);
    }
    RpcCredential::Anonymous
}

#[allow(clippy::result_large_err)]
pub(crate) async fn build_rpc_ctx(
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
    })
}

/// Bearer auth failures are HTTP errors (401 + `WWW-Authenticate`, or 429 +
/// `Retry-After`) with a standard RPC error body — never a silent downgrade.
fn bearer_rejection_response(rejection: BearerRejection) -> axum::response::Response {
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

fn attach_set_cookie(
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

fn rpc_status(resp: &RpcResponse) -> StatusCode {
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

async fn rpc_http(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RpcRequest>,
) -> impl IntoResponse {
    let version = headers.get(VERSION_HEADER).and_then(|v| v.to_str().ok());
    if let Err(err) = rpc::check_version_header(version) {
        return (StatusCode::BAD_REQUEST, Json(RpcResponse::err(err))).into_response();
    }

    let credential = edge_credential(&headers);
    let bearer_auth = matches!(credential, RpcCredential::Bearer(_));
    let challenge_id = two_factor_id_from_headers(&headers);
    let mut ctx = match build_rpc_ctx(
        &state,
        credential,
        rpc::ClientMeta::from_headers(&headers),
        challenge_id,
    )
    .await
    {
        Ok(ctx) => ctx,
        Err(rejection) => return bearer_rejection_response(rejection),
    };
    let resp = rpc::dispatch(&mut ctx, body).await;
    let status = rpc_status(&resp);
    // PAT Bearer calls never mint or mutate cookies (API-02).
    let set_cookie = if bearer_auth {
        None
    } else {
        ctx.set_cookie.take()
    };
    let env_name = ctx.env_name.clone();
    let response = (status, Json(resp)).into_response();
    attach_set_cookie(response, set_cookie, &env_name)
}

async fn rpc_ws(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let version = headers.get(VERSION_HEADER).and_then(|v| v.to_str().ok());
    if let Err(err) = rpc::check_version_header(version) {
        return (StatusCode::BAD_REQUEST, Json(RpcResponse::err(err))).into_response();
    }
    let credential = edge_credential(&headers);
    let client = rpc::ClientMeta::from_headers(&headers);
    let challenge_id = two_factor_id_from_headers(&headers);
    ws.on_upgrade(move |socket| handle_socket(socket, state, credential, client, challenge_id))
}

async fn handle_socket(
    socket: WebSocket,
    state: AppState,
    credential: RpcCredential,
    client: rpc::ClientMeta,
    challenge_id: Option<String>,
) {
    let (mut sender, mut receiver) = socket.split();
    while let Some(Ok(msg)) = receiver.next().await {
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => break,
            _ => continue,
        };
        let resp = match serde_json::from_str::<RpcRequest>(&text) {
            Ok(req) => {
                // Bearer tokens re-resolve per frame, matching session-cookie
                // semantics (revocation/expiry takes effect on the next frame).
                match build_rpc_ctx(
                    &state,
                    credential.clone(),
                    client.clone(),
                    challenge_id.clone(),
                )
                .await
                {
                    Ok(mut ctx) => rpc::dispatch(&mut ctx, req).await,
                    Err(rejection) => RpcResponse::err(rejection.error),
                }
            }
            Err(e) => RpcResponse::err(AppError::new(
                "rpc.bad_input",
                format!("invalid rpc frame: {e}"),
            )),
        };
        let payload = serde_json::to_string(&resp).unwrap_or_else(|_| {
            r#"{"ok":false,"error":{"code":"rpc.internal","message":"serialize failed"}}"#.into()
        });
        if sender.send(Message::Text(payload.into())).await.is_err() {
            break;
        }
    }
}
