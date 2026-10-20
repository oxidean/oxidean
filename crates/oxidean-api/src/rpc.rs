use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use cookie::Cookie;
use futures_util::FutureExt;
use oxidean_core::{
    wire_compatible, AppError, EchoRequest, EchoResponse, HealthResponse, ManifestCapabilities,
    ManifestResponse, RpcRequest, RpcResponse, ECHO_MAX_BYTES, LEGACY_WIRE_VERSION,
    RPC_PROTOCOL_VERSION, WIRE_VERSION,
};
use oxidean_db::Database;
use oxidean_git::GitBackend;

use crate::admin;
use crate::auth::admin as auth_admin;
use crate::auth::bootstrap;
use crate::auth::local;
use crate::auth::profile;
use crate::auth::session::{ResolvedSession, SessionService};
use crate::auth::two_factor;
use crate::auth::verify_reset;
use crate::check;
use crate::deployment;
use crate::discussion;
use crate::email::EmailSender;
use crate::emails;
use crate::gpg_keys;
use crate::invites;
use crate::issue;
use crate::label;
use crate::milestone;
use crate::notification;
use crate::oauth;
use crate::org;
use crate::pat;
use crate::project;
use crate::pull;
use crate::release;
use crate::repo;
use crate::search;
use crate::ssh_keys;
use crate::user;
use crate::user::rate_limit::LookupLimiter;
use crate::vcs;
use crate::webhook;
use crate::wiki;

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

/// Client request metadata captured from the HTTP edge (session details, audit).
#[derive(Debug, Clone, Default)]
pub struct ClientMeta {
    /// Rightmost `X-Forwarded-For` hop (trusted-proxy convention — see
    /// `routes::git_smart_http::client_ip`).
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

impl ClientMeta {
    pub fn from_headers(headers: &axum::http::HeaderMap) -> Self {
        let ip_address = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split(',').map(str::trim).rfind(|p| !p.is_empty()))
            .map(|s| s.to_string());
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
    "auth.signup",
    "auth.login",
    "auth.login_begin",
    "auth.logout",
    "auth.logout_all",
    "auth.me",
    "auth.provider_config",
    "auth.bootstrap_status",
    "auth.bootstrap_setup",
    "auth.confirm_admin_credentials",
    "auth.verify",
    "auth.request_verify",
    "auth.resend_verify",
    "auth.request_password_reset",
    "auth.reset_password",
    "auth.dev.privileged_ping",
    "auth.two_factor.status",
    "auth.two_factor.totp_begin",
    "auth.two_factor.totp_confirm",
    "auth.two_factor.totp_disable",
    "auth.two_factor.recovery_regenerate",
    "auth.two_factor.complete",
    "auth.two_factor.passkey_begin",
    "auth.two_factor.passkey_complete",
    "auth.two_factor.passkey_register_begin",
    "auth.two_factor.passkey_register_complete",
    "auth.two_factor.passkey_update",
    "auth.two_factor.passkey_delete",
    "auth.two_factor.email_send",
    "user.get_profile",
    "user.getPublicProfile",
    "user.update_profile",
    "user.lookup",
    "user.listStarred",
    "user.listWatched",
    "user.follow",
    "user.unfollow",
    "user.followers.list",
    "user.following.list",
    "admin.auth.get_settings",
    "admin.auth.update_settings",
    "admin.lfs.getSettings",
    "admin.lfs.updateSettings",
    "admin.lfs.getUsage",
    "admin.git.getSettings",
    "admin.git.updateSettings",
    "admin.mcp.getSettings",
    "admin.mcp.updateSettings",
    "admin.pat.getSettings",
    "admin.pat.updateSettings",
    "admin.templates.list",
    "admin.templates.update",
    "admin.templates.setEnabled",
    "admin.templates.delete",
    "admin.workflowTemplates.list",
    "admin.workflowTemplates.create",
    "admin.workflowTemplates.update",
    "admin.workflowTemplates.setEnabled",
    "admin.workflowTemplates.delete",
    "admin.instance.factory_reset",
    "admin.repos.gc",
    "admin.users.list",
    "admin.users.updateRole",
    "admin.users.revokeSessions",
    "admin.users.ban",
    "admin.users.unban",
    "admin.users.delete",
    "admin.users.getAccess",
    "admin.users.listSessions",
    "admin.users.getActivity",
    "admin.users.resetTwoFactor",
    "admin.invites.create",
    "admin.invites.createLink",
    "admin.invites.list",
    "admin.invites.revoke",
    "org.create",
    "org.get",
    "org.listMine",
    "org.updateSettings",
    "org.members.list",
    "org.members.add",
    "org.members.updateRole",
    "org.members.remove",
    "org.invites.create",
    "org.invites.list",
    "org.invites.revoke",
    "org.invites.accept",
    "org.invites.createLink",
    "org.actions.secrets.list",
    "org.actions.secrets.publicKey",
    "org.actions.secrets.put",
    "org.actions.secrets.delete",
    "org.actions.secrets.listRepos",
    "org.actions.secrets.setRepos",
    "org.actions.variables.list",
    "org.actions.variables.put",
    "org.actions.variables.delete",
    "org.actions.variables.listRepos",
    "org.actions.variables.setRepos",
    "org.actions.runners.list",
    "org.actions.runners.get",
    "org.actions.runners.delete",
    "org.actions.runners.createRegistrationToken",
    "org.actions.runners.createRemoveToken",
    "invites.accept",
    "invites.get",
    "repo.listMine",
    "repo.listByOwner",
    "repo.createDefaults",
    "repo.create",
    "repo.fork",
    "repo.star",
    "repo.unstar",
    "repo.watch",
    "repo.unwatch",
    "repo.stargazers.list",
    "repo.watchers.list",
    "repo.forks.list",
    "repo.forkStatus",
    "repo.syncFork",
    "repo.updateMetadata",
    "repo.topicsSuggest",
    "repo.explore",
    "repo.get",
    "repo.tree",
    "repo.blob",
    "repo.refs",
    "repo.commits",
    "repo.pathLastCommits",
    "repo.commitCount",
    "repo.contributors.list",
    "repo.languages",
    "repo.activity.list",
    "repo.insights.contributors",
    "repo.insights.commitActivity",
    "repo.insights.forkNetwork",
    "repo.commit",
    "repo.compare",
    "repo.blame",
    "repo.search",
    "search.global",
    "repo.branchCreate",
    "repo.branchRename",
    "repo.branchDelete",
    "repo.file.create",
    "repo.file.update",
    "repo.file.delete",
    "repo.file.rename",
    "repo.file.upload",
    "repo.file.mkdir",
    "repo.file.commitPolicy",
    "repo.updateVisibility",
    "repo.setArchived",
    "repo.lfs.setEnabled",
    "repo.lfs.getEnabled",
    "repo.mirror.get",
    "repo.mirror.upsert",
    "repo.mirror.delete",
    "repo.mirror.syncNow",
    "repo.mirror.generateSshKey",
    "repo.mirror.rotateWebhookSecret",
    "repo.mirror.fetchHostKey",
    "repo.templates.list",
    "repo.templates.getEnabled",
    "repo.templates.setEnabled",
    "repo.lfs.getStatus",
    "repo.lfs.getUsage",
    "repo.quota.get",
    "repo.quota.set",
    "repo.lfs.listObjects",
    "repo.lfs.download",
    "repo.softDelete",
    "repo.rename",
    "repo.transfer",
    "repo.collaborators.list",
    "repo.collaborators.add",
    "repo.collaborators.update",
    "repo.collaborators.remove",
    "repo.invites.create",
    "repo.invites.createLink",
    "repo.invites.list",
    "repo.invites.revoke",
    "repo.branchProtection.list",
    "repo.branchProtection.create",
    "repo.branchProtection.update",
    "repo.branchProtection.delete",
    "repo.tagProtection.list",
    "repo.tagProtection.create",
    "repo.tagProtection.update",
    "repo.tagProtection.delete",
    "repo.deployKey.list",
    "repo.deployKey.create",
    "repo.deployKey.delete",
    "repo.commitStatus.create",
    "repo.commitStatus.list",
    "repo.actions.listRuns",
    "repo.actions.getRun",
    "repo.actions.getJob",
    "repo.actions.getJobLog",
    "repo.actions.jobAnnotations",
    "repo.actions.listWorkflows",
    "repo.actions.dispatchWorkflow",
    "repo.actions.dispatchRepository",
    "repo.actions.setWorkflowEnabled",
    "repo.actions.rerunRun",
    "repo.actions.cancelRun",
    "repo.actions.deleteRun",
    "repo.actions.deleteRunLogs",
    "repo.actions.approveRun",
    "repo.actions.secrets.list",
    "repo.actions.secrets.publicKey",
    "repo.actions.secrets.put",
    "repo.actions.secrets.delete",
    "repo.actions.variables.list",
    "repo.actions.variables.put",
    "repo.actions.variables.delete",
    "repo.actions.getEnabled",
    "repo.actions.setEnabled",
    "repo.actions.getPullRequestTarget",
    "repo.actions.setPullRequestTarget",
    "repo.actions.getForkPrApproval",
    "repo.actions.setForkPrApproval",
    "repo.actions.getDefaultPermissions",
    "repo.actions.setDefaultPermissions",
    "repo.actions.environments.list",
    "repo.actions.environments.get",
    "repo.actions.environments.upsert",
    "repo.actions.environments.delete",
    "repo.actions.environments.secretsList",
    "repo.actions.environments.secretsPublicKey",
    "repo.actions.environments.secretsPut",
    "repo.actions.environments.secretsDelete",
    "repo.actions.environments.variablesList",
    "repo.actions.environments.variablesPut",
    "repo.actions.environments.variablesDelete",
    "repo.actions.listDeployments",
    "repo.actions.approveJob",
    "repo.actions.reviewDeployment",
    "repo.actions.listArtifacts",
    "repo.actions.listAllArtifacts",
    "repo.actions.getArtifact",
    "repo.actions.deleteArtifact",
    "repo.actions.listCaches",
    "repo.actions.deleteCache",
    "repo.actions.listRunners",
    "repo.actions.getRunner",
    "repo.actions.deleteRunner",
    "repo.actions.createRegistrationToken",
    "repo.actions.createRemoveToken",
    "repo.actions.listRunApprovals",
    "repo.actions.organizationSecrets",
    "repo.actions.organizationVariables",
    "repo.actions.workflowTemplates.list",
    "repo.actions.workflowTemplates.get",
    "repo.actions.workflows.create",
    "repo.issues.getEnabled",
    "repo.issues.setEnabled",
    "repo.pulls.getEnabled",
    "repo.pulls.setEnabled",
    "repo.wiki.getEnabled",
    "repo.wiki.setEnabled",
    "repo.projects.getEnabled",
    "repo.projects.setEnabled",
    "admin.actions.createRegistrationToken",
    "admin.actions.listRunners",
    "issue.create",
    "issue.get",
    "issue.list",
    "issue.update",
    "issue.close",
    "issue.reopen",
    "issue.history",
    "issue.delete",
    "issue.comments.list",
    "issue.comments.create",
    "notification.list",
    "notification.unreadCount",
    "notification.markRead",
    "notification.markAllRead",
    "issue.comments.update",
    "issue.comments.delete",
    "issue.comments.history",
    "issue.labels.set",
    "issue.assignees.set",
    "issue.milestone.set",
    "milestone.list",
    "milestone.get",
    "milestone.create",
    "milestone.update",
    "milestone.delete",
    "deployment.list",
    "deployment.get",
    "deployment.create",
    "deployment.delete",
    "deployment.status.list",
    "deployment.status.create",
    "check.suites.list",
    "check.suites.get",
    "check.suites.create",
    "check.suites.rerequest",
    "check.runs.listForSuite",
    "check.runs.listForRef",
    "check.runs.get",
    "check.runs.create",
    "check.runs.update",
    "check.runs.rerequest",
    "check.annotations.list",
    "wiki.pages.list",
    "wiki.pages.get",
    "wiki.pages.create",
    "wiki.pages.update",
    "wiki.pages.delete",
    "wiki.revisions.list",
    "projects.list",
    "projects.get",
    "projects.create",
    "projects.update",
    "projects.delete",
    "projects.columns.list",
    "projects.columns.create",
    "projects.columns.update",
    "projects.columns.move",
    "projects.columns.delete",
    "projects.cards.list",
    "projects.cards.create",
    "projects.cards.update",
    "projects.cards.move",
    "projects.cards.convert",
    "projects.cards.delete",
    "discussions.list",
    "discussions.get",
    "discussions.create",
    "discussions.update",
    "discussions.delete",
    "discussions.pin",
    "discussions.lock",
    "discussions.transfer",
    "discussions.labels.set",
    "discussions.answer.set",
    "discussions.answer.clear",
    "discussions.categories.list",
    "discussions.categories.create",
    "discussions.categories.update",
    "discussions.categories.delete",
    "discussions.comments.list",
    "discussions.comments.create",
    "discussions.comments.update",
    "discussions.comments.delete",
    "pages.get",
    "pages.enable",
    "pages.update",
    "pages.disable",
    "pages.builds.list",
    "pages.builds.get",
    "pages.builds.request",
    "pages.deploys.create",
    "merge_queue.list",
    "merge_queue.get",
    "merge_queue.groups",
    "merge_queue.enqueue",
    "merge_queue.dequeue",
    "repo.discussions.getEnabled",
    "repo.discussions.setEnabled",
    "issue.assigneeCandidates",
    "issue.reactions.toggle",
    "issue.links.list",
    "issue.links.add",
    "issue.links.remove",
    "pull.create",
    "pull.get",
    "pull.list",
    "pull.update",
    "pull.close",
    "pull.reopen",
    "pull.files",
    "pull.commits",
    "pull.comments.list",
    "pull.comments.create",
    "pull.comments.update",
    "pull.comments.delete",
    "pull.comments.history",
    "pull.comments.resolve",
    "pull.reviews.list",
    "pull.reviews.submit",
    "pull.reviews.dismiss",
    "pull.reviewRequests.list",
    "pull.reviewRequests.add",
    "pull.reviewRequests.remove",
    "pull.merge",
    "pull.branchStatus",
    "pull.updateBranch",
    "repo.mergeSettings.get",
    "repo.mergeSettings.update",
    "release.create",
    "release.list",
    "release.get",
    "release.update",
    "release.delete",
    "release.deleteAsset",
    "webhook.create",
    "webhook.list",
    "webhook.get",
    "webhook.update",
    "webhook.delete",
    "webhook.deliveries.list",
    "webhook.deliveries.get",
    "webhook.ping",
    "webhook.redeliver",
    "label.listForRepo",
    "label.listForOrg",
    "label.create",
    "label.update",
    "label.delete",
    "packages.list",
    "packages.deleteVersion",
    "packages.adminUsage",
    "packages.adminSetQuota",
    "pat.createClassic",
    "pat.createFineGrained",
    "pat.list",
    "pat.revoke",
    "pat.self",
    "pat.policy",
    "pat.catalog",
    "oauthApp.create",
    "oauthApp.list",
    "oauthApp.update",
    "oauthApp.delete",
    "oauthApp.regenerateSecret",
    "oauthApp.listGrants",
    "oauthApp.revoke",
    "oauthApp.authorizeInfo",
    "oauthApp.authorize",
    "oauth.deviceInfo",
    "oauth.deviceAuthorize",
    "sshKey.add",
    "sshKey.list",
    "sshKey.revoke",
    "gpgKey.add",
    "gpgKey.list",
    "gpgKey.revoke",
    "email.list",
    "email.add",
    "email.remove",
    "email.setPrimary",
    "email.resendVerify",
    // Agentic VCS protocol (contract C1 — the pinned 17-name set, BR1.2).
    "vcs.repo_enable",
    "vcs.status",
    "vcs.changeset_create",
    "vcs.changeset_get",
    "vcs.log",
    "vcs.diff",
    "vcs.tree",
    "vcs.file_get",
    "vcs.blame",
    "vcs.path_history",
    "vcs.merge_base",
    "vcs.merge",
    "vcs.rebase",
    "vcs.ref_list",
    "vcs.ref_update",
    "vcs.git_import",
    "vcs.git_export",
];

/// `system.manifest` payload — the versioned capability contract (CLI-02).
pub fn manifest_response() -> ManifestResponse {
    ManifestResponse {
        protocol_version: RPC_PROTOCOL_VERSION,
        wire_version: WIRE_VERSION.into(),
        server_version: crate::version::version().into(),
        procedures: PROCEDURES
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

/// The dispatch match's union future is only as small as its largest arm —
/// procedure handlers hold serde trees, expr contexts, and whole workflow
/// enqueue pipelines. Heap-allocate the inner future once here so every
/// entry point (HTTP, WS, MCP, REST) carries a small frame instead of
/// inheriting the union's size.
pub async fn dispatch(ctx: &mut RpcCtx, req: RpcRequest) -> RpcResponse {
    Box::pin(dispatch_inner(ctx, req)).await
}

async fn dispatch_inner(ctx: &mut RpcCtx, req: RpcRequest) -> RpcResponse {
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
                return RpcResponse::err(AppError::new(
                    "auth.setup_required",
                    "Complete instance setup before continuing.",
                ));
            }
        }
        Ok(false) => {}
        Err(e) => return RpcResponse::err(e),
    }

    // API-02: Bearer-authenticated requests are scope-gated at dispatch.
    // Cookie sessions skip this entirely (`ctx.pat` is `None`).
    if let Err(e) = crate::pat::bearer::authorize_rpc(ctx, &req.procedure, &req.input).await {
        return RpcResponse::err(e);
    }

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
        "system.manifest" => RpcResponse::ok(manifest_response()),
        "auth.signup" => match local::signup(ctx, req.input).await {
            Ok(user) => RpcResponse::ok(user),
            Err(e) => RpcResponse::err(e),
        },
        "auth.login" => match local::login(ctx, req.input).await {
            Ok(user) => RpcResponse::ok(user),
            Err(e) => RpcResponse::err(e),
        },
        "auth.login_begin" => match local::login_begin(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "auth.logout" => match local::logout(ctx).await {
            Ok(()) => RpcResponse::ok(serde_json::json!({ "ok": true })),
            Err(e) => RpcResponse::err(e),
        },
        "auth.logout_all" => match local::logout_all(ctx).await {
            Ok(()) => RpcResponse::ok(serde_json::json!({ "ok": true })),
            Err(e) => RpcResponse::err(e),
        },
        "auth.me" => match local::me(ctx).await {
            Ok(user) => RpcResponse::ok(user),
            Err(e) => RpcResponse::err(e),
        },
        "auth.provider_config" => match local::provider_config(ctx).await {
            Ok(cfg) => RpcResponse::ok(cfg),
            Err(e) => RpcResponse::err(e),
        },
        "auth.bootstrap_status" => match bootstrap::bootstrap_status(ctx).await {
            Ok(status) => RpcResponse::ok(status),
            Err(e) => RpcResponse::err(e),
        },
        "auth.bootstrap_setup" => match bootstrap::bootstrap_setup(ctx, req.input).await {
            Ok(user) => RpcResponse::ok(user),
            Err(e) => RpcResponse::err(e),
        },
        "auth.confirm_admin_credentials" => {
            match bootstrap::confirm_admin_credentials(ctx, req.input).await {
                Ok(user) => RpcResponse::ok(user),
                Err(e) => RpcResponse::err(e),
            }
        }
        "auth.verify" => match verify_reset::verify(ctx, req.input).await {
            Ok(user) => RpcResponse::ok(user),
            Err(e) => RpcResponse::err(e),
        },
        "auth.request_verify" => match verify_reset::request_verify(ctx).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "auth.resend_verify" => match verify_reset::resend_verify(ctx).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "auth.request_password_reset" => {
            match verify_reset::request_password_reset(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "auth.reset_password" => match verify_reset::reset_password(ctx, req.input).await {
            Ok(user) => RpcResponse::ok(user),
            Err(e) => RpcResponse::err(e),
        },
        "auth.dev.privileged_ping" => {
            if !verify_reset::privileged_ping_env_allowed(&ctx.env_name) {
                RpcResponse::err(AppError::new(
                    "rpc.unknown_procedure",
                    format!("unknown procedure: {}", req.procedure),
                ))
            } else {
                match verify_reset::privileged_ping(ctx).await {
                    Ok(v) => RpcResponse::ok(v),
                    Err(e) => RpcResponse::err(e),
                }
            }
        }
        "auth.two_factor.status" => match two_factor::status(ctx).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "auth.two_factor.totp_begin" => match two_factor::totp_begin(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "auth.two_factor.totp_confirm" => match two_factor::totp_confirm(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "auth.two_factor.totp_disable" => match two_factor::totp_disable(ctx).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "auth.two_factor.recovery_regenerate" => match two_factor::recovery_regenerate(ctx).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "auth.two_factor.complete" => match two_factor::complete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "auth.two_factor.passkey_begin" => match two_factor::passkey_begin(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "auth.two_factor.passkey_complete" => {
            match two_factor::passkey_complete(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "auth.two_factor.passkey_register_begin" => {
            match two_factor::passkey_register_begin(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "auth.two_factor.passkey_register_complete" => {
            match two_factor::passkey_register_complete(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "auth.two_factor.passkey_update" => {
            match two_factor::passkey_update(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "auth.two_factor.passkey_delete" => {
            match two_factor::passkey_delete(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "auth.two_factor.email_send" => match two_factor::email_send(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "user.get_profile" => match profile::get_profile(ctx).await {
            Ok(user) => RpcResponse::ok(user),
            Err(e) => RpcResponse::err(e),
        },
        "user.getPublicProfile" => match profile::get_public_profile(ctx, req.input).await {
            Ok(profile) => RpcResponse::ok(profile),
            Err(e) => RpcResponse::err(e),
        },
        "user.update_profile" => match profile::update_profile(ctx, req.input).await {
            Ok(user) => RpcResponse::ok(user),
            Err(e) => RpcResponse::err(e),
        },
        "user.lookup" => match user::lookup(ctx, req.input).await {
            Ok(list) => RpcResponse::ok(list),
            Err(e) => RpcResponse::err(e),
        },
        "user.listStarred" => match user::list_starred(ctx, req.input).await {
            Ok(list) => RpcResponse::ok(list),
            Err(e) => RpcResponse::err(e),
        },
        "user.listWatched" => match user::list_watched(ctx, req.input).await {
            Ok(list) => RpcResponse::ok(list),
            Err(e) => RpcResponse::err(e),
        },
        "user.follow" => match user::follow(ctx, req.input).await {
            Ok(profile) => RpcResponse::ok(profile),
            Err(e) => RpcResponse::err(e),
        },
        "user.unfollow" => match user::unfollow(ctx, req.input).await {
            Ok(profile) => RpcResponse::ok(profile),
            Err(e) => RpcResponse::err(e),
        },
        "user.followers.list" => match user::followers_list(ctx, req.input).await {
            Ok(list) => RpcResponse::ok(list),
            Err(e) => RpcResponse::err(e),
        },
        "user.following.list" => match user::following_list(ctx, req.input).await {
            Ok(list) => RpcResponse::ok(list),
            Err(e) => RpcResponse::err(e),
        },
        "admin.auth.get_settings" => match auth_admin::get_settings(ctx).await {
            Ok(settings) => RpcResponse::ok(settings),
            Err(e) => RpcResponse::err(e),
        },
        "admin.auth.update_settings" => match auth_admin::update_settings(ctx, req.input).await {
            Ok(settings) => RpcResponse::ok(settings),
            Err(e) => RpcResponse::err(e),
        },
        "admin.lfs.getSettings" => match auth_admin::lfs_get_settings(ctx).await {
            Ok(s) => RpcResponse::ok(s),
            Err(e) => RpcResponse::err(e),
        },
        "admin.lfs.updateSettings" => match auth_admin::lfs_update_settings(ctx, req.input).await {
            Ok(s) => RpcResponse::ok(s),
            Err(e) => RpcResponse::err(e),
        },
        "admin.lfs.getUsage" => match auth_admin::lfs_get_usage(ctx).await {
            Ok(s) => RpcResponse::ok(s),
            Err(e) => RpcResponse::err(e),
        },
        "admin.git.getSettings" => match auth_admin::git_get_settings(ctx).await {
            Ok(s) => RpcResponse::ok(s),
            Err(e) => RpcResponse::err(e),
        },
        "admin.git.updateSettings" => match auth_admin::git_update_settings(ctx, req.input).await {
            Ok(s) => RpcResponse::ok(s),
            Err(e) => RpcResponse::err(e),
        },
        "admin.mcp.getSettings" => match auth_admin::mcp_get_settings(ctx).await {
            Ok(s) => RpcResponse::ok(s),
            Err(e) => RpcResponse::err(e),
        },
        "admin.mcp.updateSettings" => match auth_admin::mcp_update_settings(ctx, req.input).await {
            Ok(s) => RpcResponse::ok(s),
            Err(e) => RpcResponse::err(e),
        },
        "admin.pat.getSettings" => match auth_admin::pat_get_settings(ctx).await {
            Ok(s) => RpcResponse::ok(s),
            Err(e) => RpcResponse::err(e),
        },
        "admin.pat.updateSettings" => match auth_admin::pat_update_settings(ctx, req.input).await {
            Ok(s) => RpcResponse::ok(s),
            Err(e) => RpcResponse::err(e),
        },
        "admin.templates.list" => match crate::templates::handlers::admin_list(ctx).await {
            Ok(s) => RpcResponse::ok(s),
            Err(e) => RpcResponse::err(e),
        },
        "admin.templates.update" => {
            match crate::templates::handlers::admin_update(ctx, req.input).await {
                Ok(s) => RpcResponse::ok(s),
                Err(e) => RpcResponse::err(e),
            }
        }
        "admin.templates.setEnabled" => {
            match crate::templates::handlers::admin_set_enabled(ctx, req.input).await {
                Ok(s) => RpcResponse::ok(s),
                Err(e) => RpcResponse::err(e),
            }
        }
        "admin.templates.delete" => {
            match crate::templates::handlers::admin_delete(ctx, req.input).await {
                Ok(s) => RpcResponse::ok(s),
                Err(e) => RpcResponse::err(e),
            }
        }
        "admin.workflowTemplates.list" => match crate::actions::templates::admin_list(ctx).await {
            Ok(s) => RpcResponse::ok(s),
            Err(e) => RpcResponse::err(e),
        },
        "admin.workflowTemplates.create" => {
            match crate::actions::templates::admin_create(ctx, req.input).await {
                Ok(s) => RpcResponse::ok(s),
                Err(e) => RpcResponse::err(e),
            }
        }
        "admin.workflowTemplates.update" => {
            match crate::actions::templates::admin_update(ctx, req.input).await {
                Ok(s) => RpcResponse::ok(s),
                Err(e) => RpcResponse::err(e),
            }
        }
        "admin.workflowTemplates.setEnabled" => {
            match crate::actions::templates::admin_set_enabled(ctx, req.input).await {
                Ok(s) => RpcResponse::ok(s),
                Err(e) => RpcResponse::err(e),
            }
        }
        "admin.workflowTemplates.delete" => {
            match crate::actions::templates::admin_delete(ctx, req.input).await {
                Ok(s) => RpcResponse::ok(s),
                Err(e) => RpcResponse::err(e),
            }
        }
        "admin.instance.factory_reset" => match auth_admin::factory_reset(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "admin.repos.gc" => match auth_admin::repo_gc(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "admin.users.list" => match admin::users_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "admin.users.updateRole" => match admin::users_update_role(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "admin.users.revokeSessions" => match admin::users_revoke_sessions(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "admin.users.ban" => match admin::users_ban(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "admin.users.unban" => match admin::users_unban(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "admin.users.delete" => match admin::users_delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "admin.users.getAccess" => match admin::users_get_access(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "admin.users.listSessions" => match admin::users_list_sessions(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "admin.users.getActivity" => match admin::users_get_activity(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "admin.users.resetTwoFactor" => match two_factor::admin_reset(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "admin.invites.create" => match admin::invites_create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "admin.invites.createLink" => match admin::invites_create_link(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "admin.invites.list" => match admin::invites_list(ctx).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "admin.invites.revoke" => match admin::invites_revoke(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "org.create" => match org::create(ctx, req.input).await {
            Ok(org) => RpcResponse::ok(org),
            Err(e) => RpcResponse::err(e),
        },
        "org.get" => match org::get(ctx, req.input).await {
            Ok(org) => RpcResponse::ok(org),
            Err(e) => RpcResponse::err(e),
        },
        "org.listMine" => match org::list_mine(ctx).await {
            Ok(list) => RpcResponse::ok(list),
            Err(e) => RpcResponse::err(e),
        },
        "org.updateSettings" => match org::update_settings(ctx, req.input).await {
            Ok(org) => RpcResponse::ok(org),
            Err(e) => RpcResponse::err(e),
        },
        "org.members.list" => match org::members_list(ctx, req.input).await {
            Ok(list) => RpcResponse::ok(list),
            Err(e) => RpcResponse::err(e),
        },
        "org.members.add" => match org::members_add(ctx, req.input).await {
            Ok(member) => RpcResponse::ok(member),
            Err(e) => RpcResponse::err(e),
        },
        "org.members.updateRole" => match org::members_update_role(ctx, req.input).await {
            Ok(member) => RpcResponse::ok(member),
            Err(e) => RpcResponse::err(e),
        },
        "org.members.remove" => match org::members_remove(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "org.invites.create" => match org::invites_create(ctx, req.input).await {
            Ok(invite) => RpcResponse::ok(invite),
            Err(e) => RpcResponse::err(e),
        },
        "org.invites.list" => match org::invites_list(ctx, req.input).await {
            Ok(list) => RpcResponse::ok(list),
            Err(e) => RpcResponse::err(e),
        },
        "org.invites.revoke" => match org::invites_revoke(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "org.invites.accept" => match org::invites_accept(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "org.invites.createLink" => match org::invites_create_link(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "org.actions.secrets.list" => match org::secrets_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "org.actions.secrets.publicKey" => match org::secrets_public_key(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "org.actions.secrets.put" => match org::secrets_put(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "org.actions.secrets.delete" => match org::secrets_delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "org.actions.secrets.listRepos" => match org::secrets_list_repos(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "org.actions.secrets.setRepos" => match org::secrets_set_repos(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "org.actions.variables.list" => match org::vars::list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "org.actions.variables.put" => match org::vars::put(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "org.actions.variables.delete" => match org::vars::delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "org.actions.variables.listRepos" => match org::vars::list_repos(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "org.actions.runners.list" => match org::runners_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "org.actions.runners.get" => match org::runners_get(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "org.actions.runners.delete" => match org::runners_delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "org.actions.runners.createRegistrationToken" => {
            match org::runners_create_registration_token(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "org.actions.runners.createRemoveToken" => {
            match org::runners_create_remove_token(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "org.actions.variables.setRepos" => match org::vars::set_repos(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "invites.accept" => match invites::accept(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "invites.get" => match invites::get(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.listMine" => match repo::list_mine(ctx).await {
            Ok(list) => RpcResponse::ok(list),
            Err(e) => RpcResponse::err(e),
        },
        "repo.listByOwner" => match repo::list_by_owner(ctx, req.input).await {
            Ok(list) => RpcResponse::ok(list),
            Err(e) => RpcResponse::err(e),
        },
        "repo.createDefaults" => match repo::create_defaults(ctx).await {
            Ok(defaults) => RpcResponse::ok(defaults),
            Err(e) => RpcResponse::err(e),
        },
        "repo.create" => match repo::create(ctx, req.input).await {
            Ok(repo) => RpcResponse::ok(repo),
            Err(e) => RpcResponse::err(e),
        },
        "repo.fork" => match repo::fork(ctx, req.input).await {
            Ok(repo) => RpcResponse::ok(repo),
            Err(e) => RpcResponse::err(e),
        },
        "repo.star" => match repo::star(ctx, req.input).await {
            Ok(repo) => RpcResponse::ok(repo),
            Err(e) => RpcResponse::err(e),
        },
        "repo.unstar" => match repo::unstar(ctx, req.input).await {
            Ok(repo) => RpcResponse::ok(repo),
            Err(e) => RpcResponse::err(e),
        },
        "repo.watch" => match repo::watch(ctx, req.input).await {
            Ok(repo) => RpcResponse::ok(repo),
            Err(e) => RpcResponse::err(e),
        },
        "repo.unwatch" => match repo::unwatch(ctx, req.input).await {
            Ok(repo) => RpcResponse::ok(repo),
            Err(e) => RpcResponse::err(e),
        },
        "repo.stargazers.list" => match repo::stargazers_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.watchers.list" => match repo::watchers_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.forks.list" => match repo::forks_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.forkStatus" => match repo::fork_status(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.syncFork" => match repo::sync_fork(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.updateMetadata" => match repo::update_metadata(ctx, req.input).await {
            Ok(repo) => RpcResponse::ok(repo),
            Err(e) => RpcResponse::err(e),
        },
        "repo.topicsSuggest" => match repo::topics_suggest(ctx, req.input).await {
            Ok(list) => RpcResponse::ok(list),
            Err(e) => RpcResponse::err(e),
        },
        "repo.explore" => match repo::explore(ctx, req.input).await {
            Ok(list) => RpcResponse::ok(list),
            Err(e) => RpcResponse::err(e),
        },
        "repo.get" => match repo::get(ctx, req.input).await {
            Ok(repo) => RpcResponse::ok(repo),
            Err(e) => RpcResponse::err(e),
        },
        "repo.tree" => match repo::tree(ctx, req.input).await {
            Ok(tree) => RpcResponse::ok(tree),
            Err(e) => RpcResponse::err(e),
        },
        "repo.blob" => match repo::blob(ctx, req.input).await {
            Ok(blob) => RpcResponse::ok(blob),
            Err(e) => RpcResponse::err(e),
        },
        "repo.refs" => match repo::refs(ctx, req.input).await {
            Ok(refs) => RpcResponse::ok(refs),
            Err(e) => RpcResponse::err(e),
        },
        "repo.commits" => match repo::commits(ctx, req.input).await {
            Ok(commits) => RpcResponse::ok(commits),
            Err(e) => RpcResponse::err(e),
        },
        "repo.pathLastCommits" => match repo::path_last_commits(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.commitCount" => match repo::commit_count(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.contributors.list" => match repo::contributors_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.languages" => match repo::languages(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.activity.list" => match repo::activity_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.insights.contributors" => match repo::insights_contributors(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.insights.commitActivity" => {
            match repo::insights_commit_activity(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.insights.forkNetwork" => match repo::insights_fork_network(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.commit" => match repo::commit(ctx, req.input).await {
            Ok(commit) => RpcResponse::ok(commit),
            Err(e) => RpcResponse::err(e),
        },
        "repo.compare" => match repo::compare(ctx, req.input).await {
            Ok(compare) => RpcResponse::ok(compare),
            Err(e) => RpcResponse::err(e),
        },
        "repo.blame" => match repo::blame(ctx, req.input).await {
            Ok(blame) => RpcResponse::ok(blame),
            Err(e) => RpcResponse::err(e),
        },
        "repo.search" => match repo::search(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "search.global" => match search::global(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.branchCreate" => match repo::branch_create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.branchRename" => match repo::branch_rename(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.branchDelete" => match repo::branch_delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.file.create" => match repo::file_create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.file.update" => match repo::file_update(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.file.delete" => match repo::file_delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.file.rename" => match repo::file_rename(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.file.upload" => match repo::file_upload(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.file.mkdir" => match repo::file_mkdir(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.file.commitPolicy" => match repo::file_commit_policy(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.updateVisibility" => match repo::update_visibility(ctx, req.input).await {
            Ok(repo) => RpcResponse::ok(repo),
            Err(e) => RpcResponse::err(e),
        },
        "repo.setArchived" => match repo::set_archived(ctx, req.input).await {
            Ok(repo) => RpcResponse::ok(repo),
            Err(e) => RpcResponse::err(e),
        },
        "repo.lfs.setEnabled" => match repo::lfs_set_enabled(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.lfs.getEnabled" => match repo::lfs_get_enabled(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.mirror.get" => match crate::mirror::get(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.mirror.upsert" => match crate::mirror::upsert(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.mirror.delete" => match crate::mirror::delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.mirror.syncNow" => match crate::mirror::sync_now(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.mirror.generateSshKey" => {
            match crate::mirror::generate_ssh_key(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.mirror.rotateWebhookSecret" => {
            match crate::mirror::rotate_webhook_secret(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.mirror.fetchHostKey" => match crate::mirror::fetch_host_key(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.templates.list" => match repo::file_templates(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.templates.getEnabled" => {
            match crate::templates::handlers::repo_get_enabled(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.templates.setEnabled" => {
            match crate::templates::handlers::repo_set_enabled(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.lfs.getStatus" => match repo::lfs_get_status(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.lfs.getUsage" => match repo::lfs_get_usage(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.quota.get" => match repo::quota_get(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.quota.set" => match repo::quota_set(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.lfs.listObjects" => match repo::lfs_list_objects(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.lfs.download" => match repo::lfs_download(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.softDelete" => match repo::soft_delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.rename" => match repo::rename(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.transfer" => match repo::transfer(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.collaborators.list" => match repo::collaborators_list(ctx, req.input).await {
            Ok(list) => RpcResponse::ok(list),
            Err(e) => RpcResponse::err(e),
        },
        "repo.collaborators.add" => match repo::collaborators_add(ctx, req.input).await {
            Ok(c) => RpcResponse::ok(c),
            Err(e) => RpcResponse::err(e),
        },
        "repo.collaborators.update" => match repo::collaborators_update(ctx, req.input).await {
            Ok(c) => RpcResponse::ok(c),
            Err(e) => RpcResponse::err(e),
        },
        "repo.collaborators.remove" => match repo::collaborators_remove(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.invites.create" => match repo::invites_create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.invites.createLink" => match repo::invites_create_link(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.invites.list" => match repo::invites_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.invites.revoke" => match repo::invites_revoke(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.branchProtection.list" => match repo::branch_protection_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.branchProtection.create" => {
            match repo::branch_protection_create(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.branchProtection.update" => {
            match repo::branch_protection_update(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.branchProtection.delete" => {
            match repo::branch_protection_delete(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.tagProtection.list" => match repo::tag_protection_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.tagProtection.create" => match repo::tag_protection_create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.tagProtection.update" => match repo::tag_protection_update(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.tagProtection.delete" => match repo::tag_protection_delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.deployKey.list" => match crate::deploy_keys::list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.deployKey.create" => match crate::deploy_keys::create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.deployKey.delete" => match crate::deploy_keys::delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.commitStatus.create" => match repo::commit_status_create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.commitStatus.list" => match repo::commit_status_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.actions.listRuns" => match crate::actions::rpc::list_runs(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.actions.getJob" => match crate::actions::rpc::get_job(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.actions.getRun" => match crate::actions::rpc::get_run(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.actions.getJobLog" => match crate::actions::rpc::get_job_log(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.actions.jobAnnotations" => {
            match crate::actions::rpc::job_annotations(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.listWorkflows" => {
            match crate::actions::rpc::list_workflows(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        // Boxed: dispatch_workflow → enqueue_run (matrix expansion,
        // payloads, supersede + scheduler passes) — keep it off the
        // monolithic dispatch future's stack.
        "repo.actions.dispatchWorkflow" => {
            match Box::pin(crate::actions::rpc::dispatch_workflow(ctx, req.input)).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        // Boxed: dispatch fans out to discovery + enqueue for every matching
        // `repository_dispatch` workflow — keep the future off the dispatch
        // stack like dispatch_workflow.
        "repo.actions.dispatchRepository" => {
            match Box::pin(crate::actions::rpc::dispatch_repository(ctx, req.input)).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        // Boxed: rerun_run's future grew past the stack budget of the
        // monolithic dispatch future (scheduler re-gating + timeout restore).
        "repo.actions.rerunRun" => {
            match Box::pin(crate::actions::rpc::rerun_run(ctx, req.input)).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.cancelRun" => match crate::actions::rpc::cancel_run(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.actions.deleteRunLogs" => {
            match crate::actions::rpc::delete_run_logs(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.deleteRun" => match crate::actions::rpc::delete_run(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.actions.setWorkflowEnabled" => {
            match crate::actions::rpc::set_workflow_enabled(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.approveRun" => match crate::actions::rpc::approve_run(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.actions.secrets.publicKey" => {
            match crate::actions::rpc::repo_secret_public_key(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.secrets.list" => {
            match crate::actions::rpc::list_secrets(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.secrets.put" => match crate::actions::rpc::put_secret(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.actions.secrets.delete" => {
            match crate::actions::rpc::delete_secret(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.variables.list" => {
            match crate::actions::rpc::list_variables(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.variables.put" => {
            match crate::actions::rpc::put_variable(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.variables.delete" => {
            match crate::actions::rpc::delete_variable(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.getEnabled" => match crate::actions::rpc::get_enabled(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.actions.setEnabled" => match crate::actions::rpc::set_enabled(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.actions.getPullRequestTarget" => {
            match crate::actions::rpc::get_pull_request_target(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.setPullRequestTarget" => {
            match crate::actions::rpc::set_pull_request_target(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.getForkPrApproval" => {
            match crate::actions::rpc::get_fork_pr_approval(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.setForkPrApproval" => {
            match crate::actions::rpc::set_fork_pr_approval(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.getDefaultPermissions" => {
            match crate::actions::rpc::get_default_permissions(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.setDefaultPermissions" => {
            match crate::actions::rpc::set_default_permissions(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.environments.list" => {
            match crate::actions::rpc::list_environments(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.environments.get" => {
            match crate::actions::rpc::get_environment(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.environments.upsert" => {
            match crate::actions::rpc::upsert_environment(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.environments.delete" => {
            match crate::actions::rpc::delete_environment(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.environments.secretsPublicKey" => {
            match crate::actions::rpc::environment_secret_public_key(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.environments.secretsList" => {
            match crate::actions::rpc::list_environment_secrets(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.environments.secretsPut" => {
            match crate::actions::rpc::put_environment_secret(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.environments.secretsDelete" => {
            match crate::actions::rpc::delete_environment_secret(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.environments.variablesList" => {
            match crate::actions::rpc::list_environment_variables(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.environments.variablesPut" => {
            match crate::actions::rpc::put_environment_variable(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.environments.variablesDelete" => {
            match crate::actions::rpc::delete_environment_variable(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.listDeployments" => {
            match crate::actions::rpc::list_deployments(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.approveJob" => match crate::actions::rpc::approve_job(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.actions.reviewDeployment" => {
            match crate::actions::rpc::review_deployment(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.listArtifacts" => {
            match crate::actions::rpc::list_artifacts(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.listAllArtifacts" => {
            match crate::actions::rpc::list_all_artifacts(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.getArtifact" => {
            match crate::actions::rpc::get_artifact(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.deleteArtifact" => {
            match crate::actions::rpc::delete_artifact(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.listCaches" => match crate::actions::rpc::list_caches(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.actions.deleteCache" => {
            match crate::actions::rpc::delete_cache(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.listRunners" => {
            match crate::actions::rpc::list_runners(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.getRunner" => match crate::actions::rpc::get_runner(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.actions.deleteRunner" => {
            match crate::actions::rpc::delete_runner(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.createRegistrationToken" => {
            match crate::actions::rpc::create_registration_token(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.createRemoveToken" => {
            match crate::actions::rpc::create_remove_token(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.listRunApprovals" => {
            match crate::actions::rpc::list_run_approvals(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.organizationSecrets" => {
            match crate::actions::rpc::organization_secrets(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.organizationVariables" => {
            match crate::actions::rpc::organization_variables(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.workflowTemplates.list" => {
            match crate::actions::templates::list_templates(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.workflowTemplates.get" => {
            match crate::actions::templates::get_template(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.workflows.create" => {
            match Box::pin(crate::actions::templates::create_workflow(ctx, req.input)).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.issues.getEnabled" => match repo::issues_get_enabled(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.issues.setEnabled" => match repo::issues_set_enabled(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.pulls.getEnabled" => match repo::pulls_get_enabled(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.pulls.setEnabled" => match repo::pulls_set_enabled(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.wiki.getEnabled" => match repo::wiki_get_enabled(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.wiki.setEnabled" => match repo::wiki_set_enabled(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.projects.getEnabled" => match repo::projects_get_enabled(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.projects.setEnabled" => match repo::projects_set_enabled(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "admin.actions.createRegistrationToken" => {
            match crate::actions::rpc::admin_create_registration_token(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "admin.actions.listRunners" => {
            match crate::actions::rpc::admin_list_runners(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "issue.create" => match issue::create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "issue.get" => match issue::get(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "issue.list" => match issue::list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "issue.update" => match issue::update(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "issue.close" => match issue::close(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "issue.reopen" => match issue::reopen(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "issue.history" => match issue::history(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "issue.delete" => match issue::delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "issue.comments.list" => match issue::comments_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "issue.comments.create" => match issue::comments_create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "notification.list" => match notification::list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "notification.unreadCount" => match notification::unread_count(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "notification.markRead" => match notification::mark_read(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "notification.markAllRead" => match notification::mark_all_read(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "issue.comments.update" => match issue::comments_update(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "issue.comments.delete" => match issue::comments_delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "issue.comments.history" => match issue::comments_history(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "issue.labels.set" => match issue::labels_set(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "issue.assignees.set" => match issue::assignees_set(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "issue.milestone.set" => match milestone::set_issue_milestone(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "milestone.list" => match milestone::list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "milestone.get" => match milestone::get(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "milestone.create" => match milestone::create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "milestone.update" => match milestone::update(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "milestone.delete" => match milestone::delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "deployment.list" => match deployment::list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "deployment.get" => match deployment::get(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "deployment.create" => match deployment::create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "deployment.delete" => match deployment::delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "deployment.status.list" => match deployment::status_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "deployment.status.create" => match deployment::status_create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "check.suites.list" => match check::suites_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "check.suites.get" => match check::suites_get(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "check.suites.create" => match check::suites_create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "check.suites.rerequest" => match check::suites_rerequest(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "check.runs.listForSuite" => match check::runs_list_for_suite(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "check.runs.listForRef" => match check::runs_list_for_ref(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "check.runs.get" => match check::runs_get(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "check.runs.create" => match check::runs_create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "check.runs.update" => match check::runs_update(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "check.runs.rerequest" => match check::runs_rerequest(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "check.annotations.list" => match check::annotations_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "wiki.pages.list" => match wiki::pages_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "wiki.pages.get" => match wiki::page_get(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "wiki.pages.create" => match wiki::page_create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "wiki.pages.update" => match wiki::page_update(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "wiki.pages.delete" => match wiki::page_delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "wiki.revisions.list" => match wiki::revisions_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "projects.list" => match project::list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "projects.get" => match project::get(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "projects.create" => match project::create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "projects.update" => match project::update(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "projects.delete" => match project::delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "projects.columns.list" => match project::columns_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "projects.columns.create" => match project::columns_create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "projects.columns.update" => match project::columns_update(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "projects.columns.move" => match project::columns_move(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "projects.columns.delete" => match project::columns_delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "projects.cards.list" => match project::cards_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "projects.cards.create" => match project::cards_create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "projects.cards.update" => match project::cards_update(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "projects.cards.move" => match project::cards_move(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "projects.cards.convert" => match project::cards_convert(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "projects.cards.delete" => match project::cards_delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "discussions.list" => match discussion::list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "discussions.get" => match discussion::get(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "discussions.create" => match discussion::create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "discussions.update" => match discussion::update(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "discussions.delete" => match discussion::delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "discussions.pin" => match discussion::pin(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "discussions.lock" => match discussion::lock(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "discussions.transfer" => match discussion::transfer(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "discussions.labels.set" => match discussion::labels_set(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "discussions.answer.set" => match discussion::answer_set(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "discussions.answer.clear" => match discussion::answer_clear(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "discussions.categories.list" => match discussion::categories_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "discussions.categories.create" => {
            match discussion::categories_create(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "discussions.categories.update" => {
            match discussion::categories_update(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "discussions.categories.delete" => {
            match discussion::categories_delete(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "discussions.comments.list" => match discussion::comments_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "discussions.comments.create" => match discussion::comments_create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "discussions.comments.update" => match discussion::comments_update(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "discussions.comments.delete" => match discussion::comments_delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.discussions.getEnabled" => {
            match repo::discussions_get_enabled(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.discussions.setEnabled" => {
            match repo::discussions_set_enabled(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "pages.get" => match crate::pages::get(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pages.enable" => match crate::pages::enable(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pages.update" => match crate::pages::update(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pages.disable" => match crate::pages::disable(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pages.builds.list" => match crate::pages::builds_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pages.builds.get" => match crate::pages::builds_get(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pages.builds.request" => match crate::pages::builds_request(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pages.deploys.create" => match crate::pages::deploys_create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "merge_queue.list" => match crate::merge_queue::list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "merge_queue.get" => match crate::merge_queue::get(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "merge_queue.groups" => match crate::merge_queue::groups(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "merge_queue.enqueue" => match crate::merge_queue::enqueue(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "merge_queue.dequeue" => match crate::merge_queue::dequeue(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "issue.assigneeCandidates" => match issue::assignee_candidates(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "issue.reactions.toggle" => match issue::reactions_toggle(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "issue.links.list" => match issue::links_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "issue.links.add" => match issue::links_add(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "issue.links.remove" => match issue::links_remove(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.create" => match pull::create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.get" => match pull::get(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.list" => match pull::list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.update" => match pull::update(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.close" => match pull::close(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.reopen" => match pull::reopen(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.files" => match pull::files(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.commits" => match pull::commits(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.comments.list" => match pull::comments_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.comments.create" => match pull::comments_create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.comments.update" => match pull::comments_update(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.comments.delete" => match pull::comments_delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.comments.history" => match pull::comments_history(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.comments.resolve" => match pull::comments_resolve(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.reviews.list" => match pull::reviews_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.reviews.submit" => match pull::reviews_submit(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.reviews.dismiss" => match pull::reviews_dismiss(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.reviewRequests.list" => match pull::review_requests_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.reviewRequests.add" => match pull::review_requests_add(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.reviewRequests.remove" => match pull::review_requests_remove(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.merge" => match pull::merge(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.branchStatus" => match pull::branch_status(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pull.updateBranch" => match pull::update_branch(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.mergeSettings.get" => match pull::merge_settings_get(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.mergeSettings.update" => match pull::merge_settings_update(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "release.create" => match release::create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "release.list" => match release::list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "release.get" => match release::get(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "release.update" => match release::update(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "release.delete" => match release::delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "release.deleteAsset" => match release::delete_asset(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "webhook.create" => match webhook::create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "webhook.list" => match webhook::list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "webhook.get" => match webhook::get(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "webhook.update" => match webhook::update(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "webhook.delete" => match webhook::delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "webhook.deliveries.list" => match webhook::deliveries_list(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "webhook.deliveries.get" => match webhook::deliveries_get(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "webhook.ping" => match webhook::ping(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "webhook.redeliver" => match webhook::redeliver(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "label.listForRepo" => match label::list_for_repo(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "label.listForOrg" => match label::list_for_org(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "label.create" => match label::create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "label.update" => match label::update(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "label.delete" => match label::delete(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "packages.list" => match crate::packages::rpc::list(ctx, req.input).await {
            Ok(list) => RpcResponse::ok(list),
            Err(e) => RpcResponse::err(e),
        },
        "packages.deleteVersion" => {
            match crate::packages::rpc::delete_version(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "packages.adminUsage" => match crate::packages::rpc::admin_usage(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "packages.adminSetQuota" => {
            match crate::packages::rpc::admin_set_quota(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "pat.createClassic" => match pat::create_classic(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pat.createFineGrained" => match pat::create_fine_grained(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pat.list" => match pat::list(ctx).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pat.revoke" => match pat::revoke(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pat.self" => match pat::self_info(ctx) {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pat.policy" => match pat::effective_policy(ctx).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "pat.catalog" => match pat::catalog(ctx) {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "oauthApp.create" => match oauth::create(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "oauthApp.list" => match oauth::list(ctx).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "oauthApp.update" => match oauth::update(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "oauthApp.delete" => match oauth::delete_app(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "oauthApp.regenerateSecret" => match oauth::regenerate_secret(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "oauthApp.listGrants" => match oauth::list_grants(ctx).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "oauthApp.revoke" => match oauth::revoke_grant(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "oauthApp.authorizeInfo" => match oauth::authorize_info(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "oauthApp.authorize" => match oauth::authorize_rpc(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "oauth.deviceInfo" => match oauth::device_info(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "oauth.deviceAuthorize" => match oauth::device_authorize(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "sshKey.add" => match ssh_keys::add(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "sshKey.list" => match ssh_keys::list(ctx).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "sshKey.revoke" => match ssh_keys::revoke(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "gpgKey.add" => match gpg_keys::add(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "gpgKey.list" => match gpg_keys::list(ctx).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "gpgKey.revoke" => match gpg_keys::revoke(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "email.list" => match emails::list(ctx).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "email.add" => match emails::add(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "email.remove" => match emails::remove(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "email.setPrimary" => match emails::set_primary(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "email.resendVerify" => match emails::resend_verify(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        // Agentic VCS protocol — domain handlers in `crate::vcs`. One
        // single-name arm per procedure: `procedures_list_matches_dispatch`
        // scans `"name" =>` arm literals, so alternation arms are invisible
        // to it. `.boxed()` on every arm: it statically enforces `Send` on
        // each handler future AND keeps the deep VCS await graphs out of
        // `dispatch`'s coroutine witness (rustc's Send-unroll overflows
        // `recursion_depth_exceeding_limit` on the unboxed match).
        "vcs.repo_enable" => match vcs::repo_enable(ctx, req.input).boxed().await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "vcs.status" => match vcs::status(ctx, req.input).boxed().await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "vcs.changeset_create" => match vcs::changeset_create(ctx, req.input).boxed().await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "vcs.changeset_get" => match vcs::changeset_get(ctx, req.input).boxed().await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "vcs.log" => match vcs::log(ctx, req.input).boxed().await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "vcs.diff" => match vcs::diff(ctx, req.input).boxed().await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "vcs.tree" => match vcs::tree(ctx, req.input).boxed().await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "vcs.file_get" => match vcs::file_get(ctx, req.input).boxed().await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "vcs.blame" => match vcs::blame(ctx, req.input).boxed().await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "vcs.path_history" => match vcs::path_history(ctx, req.input).boxed().await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "vcs.merge_base" => match vcs::merge_base(ctx, req.input).boxed().await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "vcs.merge" => match vcs::merge(ctx, req.input).boxed().await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "vcs.rebase" => match vcs::rebase(ctx, req.input).boxed().await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "vcs.ref_list" => match vcs::ref_list(ctx, req.input).boxed().await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "vcs.ref_update" => match vcs::ref_update(ctx, req.input).boxed().await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "vcs.git_import" => match vcs::git_import(ctx, req.input).boxed().await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "vcs.git_export" => match vcs::git_export(ctx, req.input).boxed().await {
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
        let m = manifest_response();
        assert_eq!(m.protocol_version, RPC_PROTOCOL_VERSION);
        assert_eq!(m.wire_version, WIRE_VERSION);
        assert_eq!(m.server_version, crate::version::version());
        assert_eq!(m.min_cli_version, MIN_CLI_VERSION);
        assert_eq!(m.procedures.len(), PROCEDURES.len());
        assert_eq!(m.procedures.get("repo.listMine"), Some(&true));
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

    /// vcs.* registration completeness (BR1.2/BR1.8, SD1): every pinned name
    /// is in `PROCEDURES` with a dispatch arm and a non-`SessionOnly` token
    /// class; the two object-route aliases are classified yet absent from
    /// `PROCEDURES` and dispatch.
    #[test]
    fn vcs_procedures_registered_and_classified() {
        use oxidean_core::{classify_pat_proc, PatProc};

        assert_eq!(oxidean_core::VCS_PROCEDURES.len(), 17);
        let listed: std::collections::BTreeSet<&str> = PROCEDURES.iter().copied().collect();

        let src = include_str!("rpc.rs");
        let arms: std::collections::BTreeSet<&str> = src
            .lines()
            .filter_map(|line| {
                let line = line.trim_start();
                if !line.starts_with('"') {
                    return None;
                }
                let end = line[1..].find('"')?;
                let rest = line[2 + end..].trim_start();
                rest.starts_with("=>").then(|| &line[1..1 + end])
            })
            .collect();

        for name in oxidean_core::VCS_PROCEDURES {
            assert!(listed.contains(name), "{name} missing from PROCEDURES");
            assert!(arms.contains(name), "{name} has no dispatch arm");
            assert_ne!(
                classify_pat_proc(name),
                PatProc::SessionOnly,
                "{name} must classify non-SessionOnly — only unlisted names fail closed"
            );
        }
        for alias in oxidean_core::VCS_OBJECT_ROUTE_ALIASES {
            assert!(
                !listed.contains(alias),
                "route alias {alias} must never appear in PROCEDURES"
            );
            assert!(
                !arms.contains(alias),
                "route alias {alias} must never gain a dispatch arm"
            );
            assert_ne!(
                classify_pat_proc(alias),
                PatProc::SessionOnly,
                "route alias {alias} must classify for route authorization"
            );
        }

        // Reverse closed-set (BR1.2): nothing `vcs.*` may appear in
        // PROCEDURES or gain a dispatch arm without being in the pinned set —
        // an unlisted name would classify SessionOnly yet stay invocable by
        // cookie sessions, silently widening the surface.
        let pinned: std::collections::BTreeSet<&str> =
            oxidean_core::VCS_PROCEDURES.iter().copied().collect();
        for name in listed.iter().chain(arms.iter()) {
            if name.starts_with("vcs.") {
                assert!(
                    pinned.contains(name),
                    "unlisted vcs.* name {name} reached the wire surface"
                );
            }
        }
    }

    /// Each `vcs.*` arm must call its real `crate::vcs::<proc>` handler —
    /// no `vcs.disabled` stub may remain (u7's `dispatch_real_arms` row; a
    /// handler's own `vcs.disabled` errors are returned by the domain layer,
    /// never by the arm itself).
    #[test]
    fn vcs_dispatch_arms_call_real_handlers() {
        let src = include_str!("rpc.rs");
        let lines: Vec<&str> = src.lines().collect();
        for name in oxidean_core::VCS_PROCEDURES {
            let needle = format!("\"{name}\" =>");
            let start = lines
                .iter()
                .position(|l| l.trim_start().starts_with(&needle))
                .unwrap_or_else(|| panic!("no dispatch arm for {name}"));
            let arm: String = lines[start..(start + 6).min(lines.len())].join("\n");
            let handler = format!("vcs::{}(ctx, req.input)", &name["vcs.".len()..]);
            assert!(
                arm.contains(&handler),
                "{name} arm must call the real handler ({handler}):\n{arm}"
            );
            assert!(
                !arm.contains("VcsErrorCode::Disabled"),
                "{name} arm still carries a disabled stub:\n{arm}"
            );
        }
    }
}
