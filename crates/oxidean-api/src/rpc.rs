use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use cookie::Cookie;
use oxidean_core::{
    AppError, EchoRequest, EchoResponse, HealthResponse, RpcRequest, RpcResponse, ECHO_MAX_BYTES,
    RPC_PROTOCOL_VERSION,
};
use oxidean_db::Database;
use oxidean_git::GitBackend;

use crate::admin;
use crate::auth::admin as auth_admin;
use crate::auth::bootstrap;
use crate::auth::local;
use crate::auth::profile;
use crate::auth::session::{ResolvedSession, SessionService};
use crate::auth::verify_reset;
use crate::email::EmailSender;
use crate::invites;
use crate::issue;
use crate::label;
use crate::notification;
use crate::org;
use crate::pat;
use crate::pull;
use crate::release;
use crate::ssh_keys;
use crate::gpg_keys;
use crate::emails;
use crate::repo;
use crate::user;
use crate::user::rate_limit::LookupLimiter;
use crate::webhook;

pub const VERSION_HEADER: &str = "Oxidean-RPC-Version";

/// Cookie mutation requested by an RPC handler (attached as Set-Cookie on HTTP).
#[derive(Debug)]
pub enum CookieChange {
    Set(Cookie<'static>),
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
            .and_then(|s| s.split(',').map(str::trim).filter(|p| !p.is_empty()).next_back())
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
    pub git: Arc<dyn GitBackend>,
    pub env_name: String,
    pub session: Option<ResolvedSession>,
    /// Request client metadata (IP / user-agent) — sessions + audit events.
    pub client: ClientMeta,
    pub set_cookie: Option<CookieChange>,
    /// Per-session `user.lookup` rate limiter (T-10-03).
    pub lookup_limiter: Arc<Mutex<LookupLimiter>>,
    pub search_timeout_ms: u64,
    pub search_max_matches: u32,
    pub search_max_files: u32,
}

pub fn check_version_header(value: Option<&str>) -> Result<(), AppError> {
    match value {
        Some(v) if v.trim() == RPC_PROTOCOL_VERSION.to_string() => Ok(()),
        Some(v) => Err(AppError::new(
            "rpc.version_mismatch",
            format!("expected Oxidean-RPC-Version {RPC_PROTOCOL_VERSION}, got {v}"),
        )),
        None => Err(AppError::new(
            "rpc.version_mismatch",
            format!("missing Oxidean-RPC-Version header (expected {RPC_PROTOCOL_VERSION})"),
        )),
    }
}

pub async fn dispatch(ctx: &mut RpcCtx, req: RpcRequest) -> RpcResponse {
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

    match req.procedure.as_str() {
        "system.health" => {
            let database = ctx.db.ping().await.to_string();
            RpcResponse::ok(HealthResponse {
                status: "ok".into(),
                version: env!("CARGO_PKG_VERSION").into(),
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
        "auth.signup" => match local::signup(ctx, req.input).await {
            Ok(user) => RpcResponse::ok(user),
            Err(e) => RpcResponse::err(e),
        },
        "auth.login" => match local::login(ctx, req.input).await {
            Ok(user) => RpcResponse::ok(user),
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
        },
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
        "repo.updateVisibility" => match repo::update_visibility(ctx, req.input).await {
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
        "repo.actions.getRun" => match crate::actions::rpc::get_run(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.actions.getJobLog" => match crate::actions::rpc::get_job_log(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.actions.listWorkflows" => {
            match crate::actions::rpc::list_workflows(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.dispatchWorkflow" => {
            match crate::actions::rpc::dispatch_workflow(ctx, req.input).await {
                Ok(v) => RpcResponse::ok(v),
                Err(e) => RpcResponse::err(e),
            }
        }
        "repo.actions.rerunRun" => match crate::actions::rpc::rerun_run(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.actions.cancelRun" => match crate::actions::rpc::cancel_run(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
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
        "repo.actions.getEnabled" => match crate::actions::rpc::get_enabled(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "repo.actions.setEnabled" => match crate::actions::rpc::set_enabled(ctx, req.input).await {
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
        "packages.deleteVersion" => match crate::packages::rpc::delete_version(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "packages.adminUsage" => match crate::packages::rpc::admin_usage(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
        "packages.adminSetQuota" => match crate::packages::rpc::admin_set_quota(ctx, req.input).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        },
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
        other => RpcResponse::err(AppError::new(
            "rpc.unknown_procedure",
            format!("unknown procedure: {other}"),
        )),
    }
}
