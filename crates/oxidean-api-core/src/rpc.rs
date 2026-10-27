use crate::auth::bootstrap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use cookie::Cookie;
use oxidean_core::{
    wire_compatible, AppError, EchoRequest, EchoResponse, HealthResponse, ManifestCapabilities,
    ManifestResponse, RpcRequest, RpcResponse, ECHO_MAX_BYTES, LEGACY_WIRE_VERSION,
    RPC_PROTOCOL_VERSION, WIRE_VERSION,
};
use oxidean_db::Database;
use oxidean_git::GitBackend;

use crate::auth::session::{ResolvedSession, SessionService};
use crate::email::EmailSender;
use crate::invites;
use crate::search;
use crate::user::rate_limit::LookupLimiter;

pub const VERSION_HEADER: &str = "Oxidean-RPC-Version";

/// Cookie mutation requested by an RPC handler (attached as Set-Cookie on HTTP).
#[derive(Debug)]
pub enum CookieChange {
    Set(Cookie<'static>),
    /// Set only the `oxidean_2fa` challenge cookie (no session) — opened by a
    /// successful primary-auth step that still needs a second factor (#130).
    SetTwoFactor(Cookie<'static>),
    Clear,
}

/// Future returned by an [`RpcDispatch`] — boxed so the fn-pointer stays
/// `Send + Sync` and object-safe across async handlers.
pub type RpcDispatchFuture<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = RpcResponse> + Send + 'a>>;

/// Procedure dispatcher handle — `AppState` and `RpcCtx` carry one so edge
/// code calls the assembled (core + domain crates) dispatch without
/// depending on the shell crate. The shell installs its aggregated dispatch
/// at `router_with_state`; the default answers core's residual surface only.
pub type RpcDispatch = std::sync::Arc<
    dyn for<'a> Fn(&'a mut RpcCtx, RpcRequest) -> RpcDispatchFuture<'a> + Send + Sync,
>;

/// Core's residual dispatch — `gate` + `dispatch_ungated` over
/// [`PROCEDURES`]. Default for `AppState::new` and test-built `RpcCtx`s; the
/// shell replaces it with the aggregated multi-crate dispatch.
pub fn core_dispatch() -> RpcDispatch {
    std::sync::Arc::new(|ctx, req| {
        Box::pin(async move {
            if let Err(resp) = gate(ctx, &req).await {
                return resp;
            }
            dispatch_ungated(ctx, req, PROCEDURES).await
        })
    })
}

/// Client request metadata captured from the HTTP edge (session details, audit).
#[derive(Debug, Clone, Default)]
pub struct ClientMeta {
    /// Peer socket address, or — when `OXIDEAN_WEB_BEHIND_PROXY` is set — the
    /// rightmost nonempty `X-Forwarded-For` hop. Direct exposure never trusts
    /// client-supplied XFF (audit / rate-limit spoof).
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

/// Internal hop header stamped by the API edge from `ConnectInfo` (never
/// trusted if a client sends it — the edge strips then re-sets).
pub const CONNECT_IP_HEADER: &str = "x-oxidean-connect-ip";

impl ClientMeta {
    /// Build from headers. Prefer the edge-stamped [`CONNECT_IP_HEADER`] peer
    /// when present; otherwise treat peer as unknown.
    pub fn from_headers(headers: &axum::http::HeaderMap) -> Self {
        let peer = headers
            .get(CONNECT_IP_HEADER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse().ok());
        Self::from_headers_and_peer(headers, peer)
    }

    /// Resolve client IP.
    ///
    /// - Private/loopback peer (typical web→api hop): honor the rightmost
    ///   nonempty `X-Forwarded-For` hop the upstream set, falling back to peer.
    /// - Public peer (API directly exposed): ignore client-supplied XFF unless
    ///   `OXIDEAN_WEB_BEHIND_PROXY` opts into trusting a sanitizing edge.
    /// - No peer (legacy callers / oneshot tests): XFF only when the flag is set.
    pub fn from_headers_and_peer(
        headers: &axum::http::HeaderMap,
        peer: Option<std::net::IpAddr>,
    ) -> Self {
        let trust_xff = match peer {
            Some(ip) if !oxidean_core::net::is_public_ip(&ip) => true,
            _ => crate::egress::env_flag("OXIDEAN_WEB_BEHIND_PROXY"),
        };
        let xff = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split(',').map(str::trim).rfind(|p| !p.is_empty()))
            .map(|s| s.to_string());
        let peer_s = peer.map(|ip| ip.to_string());
        let ip_address = if trust_xff { xff.or(peer_s) } else { peer_s };
        let user_agent = headers
            .get(axum::http::header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.chars().take(512).collect());
        Self {
            ip_address,
            user_agent,
        }
    }
}

/// Session-aware RPC context (RESEARCH Pattern 1).
pub struct RpcCtx {
    pub db: Database,
    pub email: Arc<dyn EmailSender>,
    /// Shared slot so `admin.auth.update_settings` can rebuild the sender for the process.
    pub email_slot: Arc<RwLock<Arc<dyn EmailSender>>>,
    pub sessions: SessionService,
    pub uploads_dir: PathBuf,
    pub repos_dir: PathBuf,
    pub lfs_dir: PathBuf,
    pub release_assets_dir: PathBuf,
    pub template_packs_dir: PathBuf,
    pub actions_log_dir: PathBuf,
    pub actions_artifacts_dir: PathBuf,
    /// `actions/cache` payload root — delete handlers unlink blobs here.
    pub actions_cache_dir: PathBuf,
    /// Pages site trees (`OXIDEAN_PAGES_DIR`, default `var/pages`) — the
    /// materialized per-repo sites `/pages/{owner}/{repo}` serves.
    pub pages_dir: PathBuf,
    pub git: Arc<dyn GitBackend>,
    pub env_name: String,
    pub session: Option<ResolvedSession>,
    /// PAT Bearer identity (API-02) — `Some` only when the request authenticated
    /// with `Authorization: Bearer <pat>` instead of the session cookie. When
    /// set, `session` is a synthesized session-equivalent (user_id = token
    /// owner) and `pat::bearer::authorize_rpc` gates the procedure at dispatch.
    pub pat: Option<crate::pat::bearer::PatIdentity>,
    /// Request client metadata (IP / user-agent) — sessions + audit events.
    pub client: ClientMeta,
    pub set_cookie: Option<CookieChange>,
    /// Pending second-factor challenge state (#130): shared store + the
    /// `oxidean_2fa` cookie id parsed at the HTTP edge.
    pub two_factor: crate::auth::two_factor::TwoFactorCtx,
    /// `auth.login_begin` rate limiter — bounds pre-password challenge churn
    /// per client IP and per identifier (#130).
    pub login_begin_limiter: Arc<Mutex<crate::auth::login_rate::LoginBeginLimiter>>,
    /// Per-session `user.lookup` rate limiter (T-10-03).
    pub lookup_limiter: Arc<Mutex<LookupLimiter>>,
    pub search_timeout_ms: u64,
    pub search_max_matches: u32,
    pub search_max_files: u32,
    /// Agentic VCS slot — `None` when the `OXIDEAN_VCS_*` config is invalid
    /// or the objects dir refused at boot; `vcs.*` procedures then answer
    /// `vcs.disabled` (same wire shape as an unenabled repo).
    pub vcs: Option<crate::vcs::SharedVcs>,
    /// The dispatcher this request resolves through — copied from
    /// `AppState.rpc_dispatch` when the edge builds the ctx (issue #162).
    /// Every transport (REST, MCP, feeds, edge routes) calls the assembled
    /// dispatch through this handle so domain-crate fan-out is identical
    /// on every path.
    pub rpc_dispatch: RpcDispatch,
}

/// Wire-compat gate for `Oxidean-RPC-Version` (BR6.2). During the 0.x window
/// the legacy ordinal `"1"` is accepted as the `LEGACY_WIRE_VERSION` contract;
/// otherwise the header must be a SemVer compatible with `WIRE_VERSION` under
/// `wire_compatible`. The `"1"` alias disappears automatically when
/// `WIRE_VERSION` reaches `1.0.0` (a `1.x` wire is not 0.x-compatible).
pub fn check_version_header(value: Option<&str>) -> Result<(), AppError> {
    let compatible = match value.map(str::trim) {
        Some(v) if v == RPC_PROTOCOL_VERSION.to_string() => {
            wire_compatible(LEGACY_WIRE_VERSION, WIRE_VERSION)
        }
        Some(v) => wire_compatible(v, WIRE_VERSION),
        None => false,
    };
    if compatible {
        Ok(())
    } else {
        Err(AppError::new(
            "rpc.version_mismatch",
            match value {
                Some(v) => format!("expected Oxidean-RPC-Version {WIRE_VERSION}, got {v}"),
                None => format!("missing Oxidean-RPC-Version header (expected {WIRE_VERSION})"),
            },
        ))
    }
}

/// Oldest `ox` CLI version this server guarantees to stay wire-compatible
/// with (CLI-02). Bump when a breaking change makes older clients unsafe.
pub const MIN_CLI_VERSION: &str = "0.1.0";

/// Every procedure name the dispatch match below handles — single source of
/// truth for `system.manifest`'s `procedures` map so clients can feature-gate
/// without a hand-maintained second list. The `procedures_list_matches_dispatch`
/// test scans this file's match arms and asserts the two stay in sync.
pub const PROCEDURES: &[&str] = &[
    "system.health",
    "system.echo",
    "system.db_probe",
    "system.manifest",
    "invites.accept",
    "invites.get",
    "search.global",
    // Agentic VCS protocol (contract C1 — the pinned 17-name set, BR1.2).
];

/// `system.manifest` payload — the versioned capability contract (CLI-02).
pub fn manifest_response(procedures: &[&str]) -> ManifestResponse {
    ManifestResponse {
        protocol_version: RPC_PROTOCOL_VERSION,
        wire_version: WIRE_VERSION.into(),
        server_version: crate::version::version().into(),
        procedures: procedures
            .iter()
            .map(|p| ((*p).to_string(), true))
            .collect(),
        capabilities: ManifestCapabilities {
            // All mounted unconditionally in app.rs: /api/mcp, /api/v1,
            // /oauth/*, /cli/*.
            mcp: true,
            rest: true,
            oauth: true,
            cli_dist: true,
        },
        min_cli_version: MIN_CLI_VERSION.into(),
    }
}

/// Dispatch gates — the per-request checks that run once before any
/// procedure-specific work. Extracted for issue #162 so the shell can gate a
/// request, then fan out to per-domain dispatchers before falling back to
/// core's residual match.
#[allow(clippy::result_large_err)]
pub async fn gate(ctx: &mut RpcCtx, req: &RpcRequest) -> Result<(), RpcResponse> {
    // D-11 / T-06-06: empty-instance lock — bootstrap_* + health/db_probe diagnostics
    // until setup completes. confirm_admin_credentials stays off the list (ENV path
    // already has users). db_probe is allowlisted so compose dialect smokes work
    // before bootstrap (read-only probe_count / dialect).
    match bootstrap::needs_setup(&ctx.db).await {
        Ok(true) => {
            let allowed = matches!(
                req.procedure.as_str(),
                "auth.bootstrap_status"
                    | "auth.bootstrap_setup"
                    | "system.health"
                    | "system.db_probe"
                    | "system.manifest"
            );
            if !allowed {
                return Err(RpcResponse::err(AppError::new(
                    "auth.setup_required",
                    "Complete instance setup before continuing.",
                )));
            }
        }
        Ok(false) => {}
        Err(e) => return Err(RpcResponse::err(e)),
    }

    // API-02: Bearer-authenticated requests are scope-gated at dispatch.
    // Cookie sessions skip this entirely (`ctx.pat` is `None`).
    if let Err(e) = crate::pat::bearer::authorize_rpc(ctx, &req.procedure, &req.input).await {
        return Err(RpcResponse::err(e));
    }
    Ok(())
}

/// The dispatch match's union future is only as small as its largest arm —
/// procedure handlers hold serde trees, expr contexts, and whole workflow
/// enqueue pipelines. Heap-allocate the inner future once here so every
/// entry point (HTTP, WS, MCP, REST) carries a small frame instead of
/// inheriting the union's size.
///
/// `procedures` is the *assembled* surface (core residual + every domain
/// crate) so `system.manifest` reports the full catalog; the shell passes
/// its aggregated `PROCEDURES`.
pub async fn dispatch_ungated(
    ctx: &mut RpcCtx,
    req: RpcRequest,
    procedures: &'static [&'static str],
) -> RpcResponse {
    Box::pin(dispatch_ungated_inner(ctx, req, procedures)).await
}

async fn dispatch_ungated_inner(
    ctx: &mut RpcCtx,
    req: RpcRequest,
    procedures: &[&str],
) -> RpcResponse {
    match req.procedure.as_str() {
        "system.health" => {
            let database = ctx.db.ping().await.to_string();
            RpcResponse::ok(HealthResponse {
                status: "ok".into(),
                version: crate::version::version().into(),
                database,
            })
        }
        "system.echo" => {
            let echo: EchoRequest = match serde_json::from_value(req.input) {
                Ok(v) => v,
                Err(e) => {
                    return RpcResponse::err(AppError::new(
                        "rpc.bad_input",
                        format!("invalid echo input: {e}"),
                    ))
                }
            };
            if echo.message.len() > ECHO_MAX_BYTES {
                return RpcResponse::err(AppError::new(
                    "rpc.payload_too_large",
                    format!("echo message exceeds {ECHO_MAX_BYTES} bytes"),
                ));
            }
            RpcResponse::ok(EchoResponse {
                message: echo.message,
            })
        }
        "system.db_probe" => match ctx.db.probe().await {
            Ok(result) => RpcResponse::ok(result),
            Err(e) if e == "database not configured" => RpcResponse::err(AppError::new(
                "db.not_configured",
                "no database configured for this instance",
            )),
            Err(e) => {
                tracing::error!("db probe failed: {e}");
                RpcResponse::err(AppError::new("db.probe_failed", "database probe failed"))
            }
        },
        "system.manifest" => RpcResponse::ok(manifest_response(procedures)),
        "invites.accept" => match invites::accept(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "invites.get" => match invites::get(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "search.global" => match search::global(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        other => RpcResponse::err(AppError::new(
            "rpc.unknown_procedure",
            format!("unknown procedure: {other}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `PROCEDURES` is the manifest's procedure map — it must exactly mirror
    /// the `match req.procedure.as_str()` arms. Instead of trusting a hand
    /// maintained list, scan this source file for `"name" =>` arm literals.
    #[test]
    fn procedures_list_matches_dispatch() {
        let src = include_str!("rpc.rs");
        let mut arms: Vec<&str> = Vec::new();
        for line in src.lines() {
            let line = line.trim_start();
            if !line.starts_with('"') {
                continue;
            }
            if let Some(end) = line[1..].find('"') {
                let name = &line[1..1 + end];
                let rest = line[2 + end..].trim_start();
                if rest.starts_with("=>") {
                    arms.push(name);
                }
            }
        }
        let listed: std::collections::BTreeSet<&str> = PROCEDURES.iter().copied().collect();
        for arm in &arms {
            assert!(
                listed.contains(arm),
                "dispatch arm `{arm}` missing from PROCEDURES — add it so the manifest stays accurate"
            );
        }
        for name in &listed {
            assert!(
                arms.contains(name),
                "PROCEDURES entry `{name}` has no dispatch arm — remove it"
            );
        }
    }

    #[test]
    fn manifest_shape() {
        let m = manifest_response(PROCEDURES);
        assert_eq!(m.protocol_version, RPC_PROTOCOL_VERSION);
        assert_eq!(m.wire_version, WIRE_VERSION);
        assert_eq!(m.server_version, crate::version::version());
        assert_eq!(m.min_cli_version, MIN_CLI_VERSION);
        assert_eq!(m.procedures.len(), PROCEDURES.len());
        // `invites.accept` stays core-owned (cross-domain callers); domain
        // procedures live in the per-domain crates' PROCEDURES consts (#162).
        assert_eq!(m.procedures.get("invites.accept"), Some(&true));
        assert_eq!(m.procedures.get("system.manifest"), Some(&true));
        assert!(
            m.capabilities.mcp
                && m.capabilities.rest
                && m.capabilities.oauth
                && m.capabilities.cli_dist
        );
        let v = serde_json::to_value(&m).unwrap();
        for key in [
            "protocol_version",
            "wire_version",
            "server_version",
            "procedures",
            "capabilities",
            "min_cli_version",
        ] {
            assert!(v.get(key).is_some(), "manifest missing `{key}`");
        }
    }

    /// Dual-accept transition matrix (BR6.2): while `WIRE_VERSION` is `0.1.x`,
    /// both the legacy `"1"` ordinal and any `0.1.*` SemVer are accepted.
    #[test]
    fn version_header_dual_accept() {
        assert!(check_version_header(Some("1")).is_ok());
        assert!(check_version_header(Some(" 1 ")).is_ok());
        assert!(check_version_header(Some("0.1.0")).is_ok());
        assert!(check_version_header(Some("0.1.9")).is_ok());
        assert!(check_version_header(Some("0.1.0-rc.1")).is_ok());
    }

    #[test]
    fn version_header_rejects() {
        for bad in [
            None,
            Some(""),
            Some("0.2.0"),
            Some("1.0.0"),
            Some("2"),
            Some("garbage"),
            Some("0.1"),
        ] {
            let err = check_version_header(bad).unwrap_err();
            assert_eq!(err.code, "rpc.version_mismatch");
        }
    }

    /// The `"1"` alias must die with the 0.x line: `wire_compatible` itself
    /// refuses `LEGACY_WIRE_VERSION` ↔ a `1.x` wire.
    #[test]
    fn legacy_alias_expires_at_1_0() {
        assert!(wire_compatible(LEGACY_WIRE_VERSION, WIRE_VERSION));
        assert!(!wire_compatible(LEGACY_WIRE_VERSION, "1.0.0"));
    }
}
