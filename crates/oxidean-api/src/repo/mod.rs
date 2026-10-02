//! Repository RPC handlers (`repo.create` + browse ACL — GIT-01 / GIT-05 / D-23–D-25).

mod acl;
mod activity;
pub(crate) mod author_resolve;
mod branch_protection;
mod collaborators;
mod commit_status;
mod files;
mod fork_network;
mod invites;
mod language_stats;
mod rename_transfer;
mod search;
mod search_query;
pub(crate) mod signatures;
mod social_lists;
mod templates;

pub use acl::{
    can_read_as_owner, coalesce, effective_capability, fg_all_covers_repo, is_private_visibility,
    login_slug_taken, lookup_repo_row_or_redirect, meets, not_found, owner_ref_for_repo,
    resolve_owner_slug, resolve_repo_for_read, AccessibleRepo, Capability, MemberBasePermission,
    OrgRole, OwnerRef,
};
pub use branch_protection::{
    create as branch_protection_create, delete as branch_protection_delete,
    list as branch_protection_list, update as branch_protection_update,
};
pub use collaborators::{
    add as collaborators_add, list as collaborators_list, remove as collaborators_remove,
    resolve_repo_for_admin, update as collaborators_update,
};
pub use commit_status::{create as commit_status_create, list as commit_status_list};
pub use files::{
    commit_policy as file_commit_policy, create as file_create, delete as file_delete,
    mkdir as file_mkdir, rename as file_rename, update as file_update, upload as file_upload,
};
pub use invites::{
    create as invites_create, create_link as invites_create_link, list as invites_list,
    revoke as invites_revoke,
};
pub use fork_network::head_valid_for_base;
pub use rename_transfer::{
    redirect_retention_days, rename, resolve_repo_or_redirect, supersede_redirect_on_create,
    transfer, DEFAULT_REPO_REDIRECT_RETENTION_DAYS,
};
pub use activity::{
    list as activity_list, record_branch_creation, record_branch_deletion, record_branch_rename,
    record_pr_merge, record_ref_updates,
};
pub use search::search;
pub use social_lists::{forks_list, stargazers_list, watchers_list};

/// Soft size limit for blob preview / raw soft-cap (D-20 / T-07-16).
/// 1 MiB keeps preview responses cheap without clipping most source files.
pub const BLOB_SOFT_MAX_BYTES: usize = 1_048_576;

/// Soft cap for unified patch bytes in commit/compare (aligned with git backend).
pub const DIFF_SOFT_MAX_BYTES: usize = oxidean_git::DIFF_SOFT_MAX_BYTES;

use oxidean_core::{
    validate_repo_name, AppError, CreateRepoRequest, OwnerType, RepoBlameLine, RepoBlameRequest,
    RepoBlameResponse, RepoBranchCreateRequest, RepoBranchDeleteRequest, RepoBranchMutationResponse,
    RepoBranchRenameRequest, RepoCommitRequest, RepoCommitResponse, RepoCommitSummary,
    RepoCommitsRequest, RepoCommitsResponse, RepoCompareRequest, RepoCompareResponse,
    RepoCreateDefaults, RepoDiffFile, RepoBlobRequest, RepoBlobResponse, RepoGetRequest,
    RepoLfsDownloadRequest, RepoLfsDownloadResponse, RepoLfsEnabledResponse,
    RepoLfsGetEnabledRequest, RepoLfsListObjectsRequest, RepoLfsListObjectsResponse,
    RepoLfsObjectEntry, RepoLfsSetEnabledRequest, RepoLfsStatusResponse, RepoLfsUsageResponse,
    RepoListByOwnerRequest, RepoListMineResponse, RepoPublic, RepoRefEntry, RepoRefsResponse,
    RepoSoftDeleteRequest, RepoSoftDeleteResponse, RepoTemplateOption, RepoTreeEntry,
    RepoTreeRequest, RepoTreeResponse, RepoUpdateVisibilityRequest, RepoVisibility,
    TemplateProvenance,
};
use uuid::Uuid;

use crate::auth::gate::require_verified;
use crate::git::bare_repo_path;
use crate::rpc::RpcCtx;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else if e.contains("UNIQUE") || e.contains("unique") || e.contains("Duplicate") {
        AppError::new(
            "repo.name_taken",
            "A repository with this name already exists. Choose a different name.",
        )
    } else {
        tracing::error!("repo db error: {e}");
        AppError::new("repo.internal", "repository operation failed")
    }
}

fn map_visibility(v: RepoVisibility) -> &'static str {
    v.as_str()
}

/// Normalize homepage to empty or an http(s) URL (blocks javascript:/data: stored XSS).
/// Bare hosts (`example.com`) become `https://example.com/`.
fn normalize_homepage(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if trimmed.starts_with("//") {
        return Err(AppError::new(
            "repo.invalid_homepage",
            "homepage must be an http(s) URL",
        ));
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let parsed = url::Url::parse(&candidate).map_err(|_| {
        AppError::new(
            "repo.invalid_homepage",
            "homepage must be a valid http(s) URL",
        )
    })?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => {
            return Err(AppError::new(
                "repo.invalid_homepage",
                "homepage must use http or https",
            ));
        }
    }
    if parsed.host_str().is_none() {
        return Err(AppError::new(
            "repo.invalid_homepage",
            "homepage must include a host",
        ));
    }
    // Keep the trimmed input (or https-prefixed bare host) after validation — avoid
    // Url::to_string() rewriting (trailing slash, etc.).
    Ok(candidate)
}

fn none_like_opt(v: &Option<String>) -> bool {
    match v {
        None => true,
        Some(s) => {
            let t = s.trim();
            t.is_empty() || t.eq_ignore_ascii_case("none")
        }
    }
}

const TEMPLATE_SEED_MAX_FILES: usize = 5_000;
const TEMPLATE_SEED_MAX_BYTES: u64 = 25 * 1024 * 1024;

async fn resolve_create_seed_files(
    ctx: &RpcCtx,
    stack_id: &Option<String>,
    instance_pack_id: &Option<String>,
    template_repo_id: &Option<String>,
    license_id: &Option<String>,
    gitignore_id: &Option<String>,
) -> Result<(Vec<(String, Vec<u8>)>, Option<String>), AppError> {
    if !none_like_opt(template_repo_id) {
        let tid = template_repo_id.as_ref().unwrap().trim();
        let source = ctx
            .db
            .find_repository_by_id(tid)
            .await
            .map_err(db_err)?
            .ok_or_else(|| {
                AppError::new("repo.invalid_template", "Template repository not found.")
            })?;
        let is_template = ctx
            .db
            .get_repo_is_template(&source.id)
            .await
            .map_err(db_err)?;
        if !is_template {
            return Err(AppError::new(
                "repo.invalid_template",
                "Repository is not marked as a template.",
            ));
        }
        let owner_slug = if source.owner_type == "org" {
            ctx.db
                .find_organization_by_id(&source.owner_id)
                .await
                .map_err(db_err)?
                .map(|o| o.slug)
        } else {
            ctx.db
                .find_user_by_id(&source.owner_id)
                .await
                .map_err(db_err)?
                .map(|u| u.username)
        }
        .ok_or_else(|| AppError::new("repo.invalid_template", "Template owner not found."))?;
        // ACL: must be able to read the template repo.
        let _accessible = resolve_repo_for_read(ctx, &owner_slug, &source.name).await?;
        let bare = bare_repo_path(&ctx.repos_dir, &owner_slug, &source.name)?;
        let files = collect_template_tree(ctx, &bare, &source.default_branch).await?;
        let mut map: std::collections::BTreeMap<String, Vec<u8>> = files.into_iter().collect();
        // License / gitignore overlays still apply.
        let overlay =
            templates::assemble_seed_files(&None, license_id, gitignore_id)?;
        for (p, b) in overlay {
            map.insert(p, b);
        }
        return Ok((map.into_iter().collect(), Some(source.id)));
    }

    if !none_like_opt(instance_pack_id) {
        let pid = instance_pack_id.as_ref().unwrap().trim();
        let pack = ctx
            .db
            .get_instance_template_pack(pid)
            .await
            .map_err(db_err)?
            .ok_or_else(|| AppError::new("repo.invalid_template", "Instance template not found."))?;
        if !pack.enabled {
            return Err(AppError::new(
                "repo.invalid_template",
                "Instance template is disabled.",
            ));
        }
        let bytes = crate::templates::store::read_pack(&ctx.template_packs_dir, &pack.content_digest)
            .map_err(|e| {
                AppError::new(
                    "repo.invalid_template",
                    format!("Could not read instance template: {e}"),
                )
            })?;
        let mut map = crate::templates::store::unzip_to_map(&bytes).map_err(|e| {
            AppError::new(
                "repo.invalid_template",
                format!("Invalid instance template pack: {e}"),
            )
        })?;
        // Match built-in stacks: explicit `"none"` / empty skips the pack default;
        // omitted (`None`) falls back to the pack's default_gitignore.
        let gi = match gitignore_id {
            Some(_) => gitignore_id.clone(),
            None => pack.default_gitignore.clone(),
        };
        let overlay = templates::assemble_seed_files(&None, license_id, &gi)?;
        for (p, b) in overlay {
            map.insert(p, b);
        }
        return Ok((map.into_iter().collect(), None));
    }

    let files = templates::assemble_seed_files(stack_id, license_id, gitignore_id)?;
    Ok((files, None))
}

async fn collect_template_tree(
    ctx: &RpcCtx,
    bare: &std::path::Path,
    default_branch: &str,
) -> Result<Vec<(String, Vec<u8>)>, AppError> {
    use oxidean_git::TreeEntryKind;
    let mut out = Vec::new();
    let mut total_bytes: u64 = 0;
    let mut stack = vec![String::new()];
    while let Some(prefix) = stack.pop() {
        let entries = ctx
            .git
            .ls_tree(bare, default_branch, &prefix)
            .await
            .map_err(map_git_err)?;
        for e in entries {
            let path = if prefix.is_empty() {
                e.name.clone()
            } else {
                format!("{prefix}/{}", e.name)
            };
            match e.kind {
                TreeEntryKind::Tree => stack.push(path),
                TreeEntryKind::Blob => {
                    if out.len() >= TEMPLATE_SEED_MAX_FILES {
                        return Err(AppError::new(
                            "repo.invalid_template",
                            "Template repository has too many files.",
                        ));
                    }
                    let bytes = ctx
                        .git
                        .cat_blob(bare, default_branch, &path)
                        .await
                        .map_err(map_git_err)?;
                    total_bytes += bytes.len() as u64;
                    if total_bytes > TEMPLATE_SEED_MAX_BYTES {
                        return Err(AppError::new(
                            "repo.invalid_template",
                            "Template repository exceeds size limit.",
                        ));
                    }
                    out.push((path, bytes));
                }
                TreeEntryKind::Commit => {}
            }
        }
    }
    Ok(out)
}

pub(crate) fn to_public(repo: &AccessibleRepo) -> RepoPublic {
    let visibility = RepoVisibility::parse(&repo.row.visibility).unwrap_or(RepoVisibility::Public);
    let owner_type = OwnerType::parse(&repo.row.owner_type).unwrap_or(OwnerType::User);
    RepoPublic {
        id: repo.row.id.clone(),
        owner_id: repo.row.owner_id.clone(),
        owner_type,
        owner_username: repo.owner_username.clone(),
        name: repo.row.name.clone(),
        description: repo.row.description.clone(),
        visibility,
        default_branch: repo.row.default_branch.clone(),
        updated_at: repo.row.updated_at.clone(),
        can_admin: meets(repo.capability, Capability::Admin),
        can_write: meets(repo.capability, Capability::Write),
        star_count: 0,
        open_issue_count: 0,
        open_pull_count: 0,
        viewer_has_starred: false,
        is_fork: false,
        is_template: false,
        homepage: String::new(),
        topics: Vec::new(),
        fork_count: 0,
        watch_count: 0,
        viewer_is_watching: false,
        fork_network_id: None,
        forked_from: None,
    }
}

/// Fill star/fork/about fields on a `RepoPublic` (D-SOC-03, D-SOC-16, issue #23).
pub async fn enrich_social(
    ctx: &RpcCtx,
    mut public: RepoPublic,
    viewer_user_id: Option<&str>,
) -> Result<RepoPublic, AppError> {
    public.star_count = ctx
        .db
        .get_repo_star_count(&public.id)
        .await
        .map_err(db_err)?;
    public.open_issue_count = ctx
        .db
        .count_open_issues_for_repo(&public.id)
        .await
        .map_err(db_err)?;
    public.open_pull_count = ctx
        .db
        .count_open_pulls_for_repo(&public.id)
        .await
        .map_err(db_err)?;
    if let Some(uid) = viewer_user_id {
        public.viewer_has_starred = ctx
            .db
            .has_starred_repo(uid, &public.id)
            .await
            .map_err(db_err)?;
        public.viewer_is_watching = ctx
            .db
            .has_watched_repo(uid, &public.id)
            .await
            .map_err(db_err)?;
    }
    public.watch_count = ctx
        .db
        .get_repo_watch_count(&public.id)
        .await
        .map_err(db_err)?;
    public.fork_count = ctx
        .db
        .get_repo_fork_count(&public.id)
        .await
        .map_err(db_err)?;
    public.homepage = ctx
        .db
        .get_repo_homepage(&public.id)
        .await
        .map_err(db_err)?;
    public.topics = ctx
        .db
        .list_repo_topics(&public.id)
        .await
        .map_err(db_err)?;
    public.fork_network_id = ctx
        .db
        .get_repo_fork_network_id(&public.id)
        .await
        .map_err(db_err)?;
    let parent_id = ctx
        .db
        .get_repo_forked_from(&public.id)
        .await
        .map_err(db_err)?;
    if let Some(pid) = parent_id {
        public.is_fork = true;
        if let Some(parent) = ctx
            .db
            .find_repository_by_id(&pid)
            .await
            .map_err(db_err)?
        {
            let parent_slug = if parent.owner_type == "org" {
                ctx.db
                    .find_organization_by_id(&parent.owner_id)
                    .await
                    .ok()
                    .flatten()
                    .map(|o| o.slug)
            } else {
                ctx.db
                    .find_user_by_id(&parent.owner_id)
                    .await
                    .ok()
                    .flatten()
                    .map(|u| u.username)
            };
            if let Some(slug) = parent_slug {
                public.forked_from = Some(oxidean_core::ForkParentSummary {
                    id: parent.id,
                    owner: slug,
                    name: parent.name,
                });
            }
        }
    }
    public.is_template = ctx
        .db
        .get_repo_is_template(&public.id)
        .await
        .map_err(db_err)?;
    Ok(public)
}

/// `repo.star` — idempotent star (D-SOC-01…03, D-SOC-19).
pub async fn star(ctx: &RpcCtx, input: serde_json::Value) -> Result<RepoPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: oxidean_core::RepoStarRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid repo.star input: {e}"))
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let _count = ctx
        .db
        .star_repository(&user.id, &accessible.row.id)
        .await
        .map_err(db_err)?;
    enrich_social(ctx, to_public(&accessible), Some(&user.id)).await
}

/// `repo.unstar` — idempotent unstar.
pub async fn unstar(ctx: &RpcCtx, input: serde_json::Value) -> Result<RepoPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: oxidean_core::RepoStarRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid repo.unstar input: {e}"))
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let _count = ctx
        .db
        .unstar_repository(&user.id, &accessible.row.id)
        .await
        .map_err(db_err)?;
    enrich_social(ctx, to_public(&accessible), Some(&user.id)).await
}

/// `repo.watch` — idempotent watch (issue #23).
pub async fn watch(ctx: &RpcCtx, input: serde_json::Value) -> Result<RepoPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: oxidean_core::RepoWatchRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid repo.watch input: {e}"))
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let _count = ctx
        .db
        .watch_repository(&user.id, &accessible.row.id)
        .await
        .map_err(db_err)?;
    enrich_social(ctx, to_public(&accessible), Some(&user.id)).await
}

/// `repo.unwatch` — idempotent unwatch.
pub async fn unwatch(ctx: &RpcCtx, input: serde_json::Value) -> Result<RepoPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: oxidean_core::RepoWatchRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid repo.unwatch input: {e}"))
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let _count = ctx
        .db
        .unwatch_repository(&user.id, &accessible.row.id)
        .await
        .map_err(db_err)?;
    enrich_social(ctx, to_public(&accessible), Some(&user.id)).await
}

/// `repo.updateMetadata` — Admin updates description / homepage / topics (issue #23).
/// Insufficient capability → soft not_found.
pub async fn update_metadata(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: oxidean_core::RepoUpdateMetadataRequest =
        serde_json::from_value(input).map_err(|e| {
            AppError::new(
                "rpc.bad_input",
                format!("invalid repo.updateMetadata input: {e}"),
            )
        })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;

    let description = match &req.description {
        Some(d) => d.clone(),
        None => accessible.row.description.clone(),
    };
    let homepage = match &req.homepage {
        Some(h) => normalize_homepage(h)?,
        None => ctx
            .db
            .get_repo_homepage(&accessible.row.id)
            .await
            .map_err(db_err)?,
    };

    let row = ctx
        .db
        .update_repository_metadata(&accessible.row.id, &description, &homepage)
        .await
        .map_err(db_err)?;

    if let Some(topics) = &req.topics {
        ctx.db
            .set_repo_topics(&accessible.row.id, topics)
            .await
            .map_err(|e| {
                if e.contains("topic") || e.contains("at most") {
                    AppError::new("repo.invalid_topics", e)
                } else {
                    db_err(e)
                }
            })?;
    }

    let accessible = AccessibleRepo {
        row,
        owner_username: accessible.owner_username,
        capability: Some(Capability::Admin),
    };
    enrich_social(ctx, to_public(&accessible), Some(&user.id)).await
}

/// `repo.topicsSuggest` — topic autocomplete for the chips editor.
/// Anonymous OK — topic names are public metadata (like `repo.explore`).
pub async fn topics_suggest(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<oxidean_core::RepoTopicsSuggestResponse, AppError> {
    let req: oxidean_core::RepoTopicsSuggestRequest = serde_json::from_value(input)
        .unwrap_or(oxidean_core::RepoTopicsSuggestRequest {
            q: String::new(),
            limit: None,
        });
    let rows = ctx
        .db
        .suggest_topics(&req.q, req.limit.unwrap_or(10))
        .await
        .map_err(db_err)?;
    Ok(oxidean_core::RepoTopicsSuggestResponse {
        topics: rows
            .into_iter()
            .map(|(name, repo_count)| oxidean_core::RepoTopicSuggestion { name, repo_count })
            .collect(),
    })
}

async fn resolve_visibility(
    ctx: &RpcCtx,
    requested: Option<RepoVisibility>,
) -> Result<RepoVisibility, AppError> {
    if let Some(v) = requested {
        return Ok(v);
    }
    // D-08: instance default_visibility; unset column default is public.
    match ctx.db.get_auth_settings().await {
        Ok(settings) => Ok(RepoVisibility::parse(&settings.default_visibility)
            .unwrap_or(RepoVisibility::Public)),
        Err(e) => {
            tracing::warn!(error = %e, "default_visibility lookup failed; using public");
            Ok(RepoVisibility::Public)
        }
    }
}

fn require_session_user(
    ctx: &RpcCtx,
) -> Result<&crate::auth::session::ResolvedSession, AppError> {
    ctx.session.as_ref().ok_or_else(|| {
        AppError::new("auth.unauthenticated", "not authenticated")
    })
}

fn map_git_err(e: oxidean_git::GitError) -> AppError {
    match e {
        oxidean_git::GitError::NotFound(msg) => AppError::new("repo.path_not_found", msg),
        oxidean_git::GitError::InvalidArg(msg) => AppError::new("repo.invalid_ref", msg),
        other => {
            tracing::error!(error = %other, "git backend error");
            AppError::new("repo.git_failed", "git operation failed")
        }
    }
}

/// WR-01: after insert + git failure, soft-delete the row and best-effort remove partial disk path.
async fn compensate_failed_create(ctx: &RpcCtx, repo_id: &str, path: &std::path::Path) {
    if let Err(e) = ctx.db.soft_delete_repository(repo_id).await {
        tracing::error!(
            error = %e,
            repo_id,
            "soft_delete_repository failed during create compensate"
        );
    }
    match tokio::fs::remove_dir_all(path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            // Blocking file (not a dir) or other leftover — try remove_file.
            if let Err(e2) = tokio::fs::remove_file(path).await {
                if e2.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(
                        error = %e,
                        remove_file = %e2,
                        path = %path.display(),
                        "best-effort remove of partial bare path failed"
                    );
                }
            }
        }
    }
}

fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|&b| b == 0)
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut n = (chunk[0] as u32) << 16;
        if chunk.len() > 1 {
            n |= (chunk[1] as u32) << 8;
        }
        if chunk.len() > 2 {
            n |= chunk[2] as u32;
        }
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// `repo.listMine` — caller's non-deleted repos sorted by updated_at desc (GIT-01 / D-13).
pub async fn list_mine(ctx: &RpcCtx) -> Result<RepoListMineResponse, AppError> {
    let session = require_session_user(ctx)?;
    let user = ctx
        .db
        .find_user_by_id(&session.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))?;

    let rows = ctx
        .db
        .list_repositories_by_owner(&user.id)
        .await
        .map_err(db_err)?;

    let repos = rows
        .into_iter()
        .map(|row| {
            let visibility = RepoVisibility::parse(&row.visibility).unwrap_or(RepoVisibility::Public);
            let owner_type = OwnerType::parse(&row.owner_type).unwrap_or(OwnerType::User);
            RepoPublic {
                id: row.id,
                owner_id: row.owner_id,
                owner_type,
                owner_username: user.username.clone(),
                name: row.name,
                description: row.description,
                visibility,
                default_branch: row.default_branch,
                updated_at: row.updated_at,
                can_admin: true,
                can_write: true,
                star_count: 0,
                open_issue_count: 0,
                open_pull_count: 0,
                viewer_has_starred: false,
                is_fork: false,
                is_template: false,
                homepage: String::new(),
                topics: Vec::new(),
                fork_count: 0,
                watch_count: 0,
                viewer_is_watching: false,
                fork_network_id: None,
                forked_from: None,
            }
        })
        .collect();

    Ok(RepoListMineResponse { repos })
}

/// `repo.listByOwner` — ACL-filtered repos under a user/org slug (D-ORG-06 org overview).
/// Public repos are visible to any caller; private only when coalesce grants Read.
pub async fn list_by_owner(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoListMineResponse, AppError> {
    let req: RepoListByOwnerRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.listByOwner input: {e}"),
        )
    })?;
    let owner_slug = req.owner.trim();
    if owner_slug.is_empty() {
        return Err(AppError::new("rpc.bad_input", "owner is required"));
    }

    let owner_ref = match resolve_owner_slug(&ctx.db, owner_slug).await {
        Ok(Some(r)) => r,
        Ok(None) => return Ok(RepoListMineResponse { repos: vec![] }),
        Err(e) => {
            tracing::error!(error = %e, "resolve_owner_slug failed");
            return Err(AppError::new("repo.internal", "repository operation failed"));
        }
    };

    let rows = ctx
        .db
        .list_repositories_by_owner(owner_ref.id())
        .await
        .map_err(db_err)?;

    let caller_id = ctx.session.as_ref().map(|s| s.user_id.as_str());
    let mut repos = Vec::with_capacity(rows.len());
    for row in rows {
        let capability = match effective_capability(&ctx.db, caller_id, &row, &owner_ref).await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "effective_capability failed");
                return Err(AppError::new("repo.internal", "repository operation failed"));
            }
        };
        if !meets(capability, Capability::Read) {
            continue;
        }
        repos.push(to_public(&AccessibleRepo {
            row,
            owner_username: owner_ref.slug().to_string(),
            capability,
        }));
    }

    Ok(RepoListMineResponse { repos })
}

/// `repo.createDefaults` — visibility default + stack/gitignore catalogs for `/new`.
pub async fn create_defaults(ctx: &RpcCtx) -> Result<RepoCreateDefaults, AppError> {
    let user = require_verified(ctx).await?;
    let default_visibility = resolve_visibility(ctx, None).await?;
    let mut stacks = templates::list_stacks()?;

    let instance_packs = ctx
        .db
        .list_instance_template_packs(true)
        .await
        .map_err(db_err)?;
    for pack in instance_packs {
        stacks.push(RepoTemplateOption {
            id: format!("instance:{}", pack.id),
            label: pack.label,
            group: pack.group,
            description: pack.description,
            default_gitignore: pack.default_gitignore,
            provenance: TemplateProvenance::Instance,
            source_label: Some(pack.slug),
        });
    }

    let template_repos = ctx
        .db
        .list_template_repositories(Some(&user.id))
        .await
        .map_err(db_err)?;
    for tr in template_repos {
        // Defense in depth: SQL candidate list may over-include; only expose
        // templates the viewer can actually read (matches settings copy).
        if tr.visibility != "public" {
            match resolve_repo_for_read(ctx, &tr.owner_slug, &tr.name).await {
                Ok(_) => {}
                Err(_) => continue,
            }
        }
        stacks.push(RepoTemplateOption {
            id: format!("repo:{}", tr.id),
            label: tr.name.clone(),
            group: "From template".into(),
            description: if tr.description.trim().is_empty() {
                format!("Template repository from @{}/{}", tr.owner_slug, tr.name)
            } else {
                tr.description.clone()
            },
            default_gitignore: None,
            provenance: TemplateProvenance::User,
            source_label: Some(format!("{}/{}", tr.owner_slug, tr.name)),
        });
    }

    Ok(RepoCreateDefaults {
        default_visibility,
        stacks,
        gitignores: templates::list_gitignores()?,
    })
}

/// `repo.get` — ACL-safe metadata (D-23–D-25). Anonymous OK for public.
/// Honors unexpired repository redirects (D-REL-08).
pub async fn get(ctx: &RpcCtx, input: serde_json::Value) -> Result<RepoPublic, AppError> {
    let req: RepoGetRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid repo.get input: {e}"))
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let viewer = ctx.session.as_ref().map(|s| s.user_id.as_str());
    enrich_social(ctx, to_public(&accessible), viewer).await
}

/// `repo.tree` — `ls_tree` behind ACL; empty repo → `{ empty: true, entries: [] }`.
pub async fn tree(ctx: &RpcCtx, input: serde_json::Value) -> Result<RepoTreeResponse, AppError> {
    let req: RepoTreeRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid repo.tree input: {e}"))
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let path = bare_repo_path(&ctx.repos_dir, &accessible.owner_username, &accessible.row.name)?;
    let ref_name = if req.ref_name.trim().is_empty() {
        accessible.row.default_branch.clone()
    } else {
        req.ref_name.trim().to_string()
    };
    let rel = req.path.unwrap_or_default();
    let entries = ctx
        .git
        .ls_tree(&path, &ref_name, &rel)
        .await
        .map_err(map_git_err)?;

    let refs = ctx.git.list_refs(&path).await.map_err(map_git_err)?;
    let empty = refs.is_empty() && entries.is_empty();

    Ok(RepoTreeResponse {
        empty,
        ref_name,
        path: rel.trim_start_matches('/').to_string(),
        entries: entries
            .into_iter()
            .map(|e| RepoTreeEntry {
                mode: e.mode,
                kind: e.kind.as_str().to_string(),
                oid: e.oid,
                name: e.name,
            })
            .collect(),
    })
}

/// `repo.blob` — blob metadata + soft-capped content (D-20).
pub async fn blob(ctx: &RpcCtx, input: serde_json::Value) -> Result<RepoBlobResponse, AppError> {
    let req: RepoBlobRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid repo.blob input: {e}"))
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let path = bare_repo_path(&ctx.repos_dir, &accessible.owner_username, &accessible.row.name)?;
    let ref_name = if req.ref_name.trim().is_empty() {
        accessible.row.default_branch.clone()
    } else {
        req.ref_name.trim().to_string()
    };
    let file_path = req.path.trim_start_matches('/').to_string();
    let bytes = ctx
        .git
        .cat_blob(&path, &ref_name, &file_path)
        .await
        .map_err(map_git_err)?;

    let size = bytes.len() as u64;
    let binary = looks_binary(&bytes);
    let truncated = bytes.len() > BLOB_SOFT_MAX_BYTES;
    let preview = if truncated {
        &bytes[..BLOB_SOFT_MAX_BYTES]
    } else {
        &bytes[..]
    };

    let (encoding, content) = if binary {
        ("base64".to_string(), Some(base64_encode(preview)))
    } else {
        match std::str::from_utf8(preview) {
            Ok(s) => ("utf-8".to_string(), Some(s.to_string())),
            Err(_) => ("base64".to_string(), Some(base64_encode(preview))),
        }
    };

    Ok(RepoBlobResponse {
        path: file_path,
        ref_name,
        size,
        truncated,
        is_binary: binary,
        encoding,
        content,
        soft_max_bytes: BLOB_SOFT_MAX_BYTES as u64,
    })
}

/// `repo.refs` — branches + tags (ACL-safe).
pub async fn refs(ctx: &RpcCtx, input: serde_json::Value) -> Result<RepoRefsResponse, AppError> {
    let req: RepoGetRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid repo.refs input: {e}"))
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let path = bare_repo_path(&ctx.repos_dir, &accessible.owner_username, &accessible.row.name)?;
    let list = ctx.git.list_refs(&path).await.map_err(map_git_err)?;
    Ok(RepoRefsResponse {
        refs: list
            .into_iter()
            .map(|r| RepoRefEntry {
                name: r.name,
                oid: r.oid,
                tip_author_name: r.tip_author_name,
                tip_committed_at: r.tip_committed_at,
            })
            .collect(),
    })
}

/// `repo.commits` — paged `git log` behind ACL.
pub async fn commits(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoCommitsResponse, AppError> {
    let req: RepoCommitsRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid repo.commits input: {e}"))
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let path = bare_repo_path(&ctx.repos_dir, &accessible.owner_username, &accessible.row.name)?;
    let ref_name = if req.ref_name.trim().is_empty() {
        accessible.row.default_branch.clone()
    } else {
        req.ref_name.trim().to_string()
    };
    let limit = if req.limit == 0 { 30 } else { req.limit.min(100) };
    let emails_probe = ctx
        .git
        .log(&path, &ref_name, req.skip, limit, None, None)
        .await
        .map_err(map_git_err)?;
    let mut emails: Vec<String> = Vec::new();
    for c in &emails_probe {
        if !c.committer_email.trim().is_empty() {
            emails.push(c.committer_email.clone());
        }
        emails.push(c.author_email.clone());
    }
    let keyring = signatures::keyring_for_emails(&ctx.db, &emails).await;
    let commits = if keyring.has_any() {
        ctx.git
            .log(
                &path,
                &ref_name,
                req.skip,
                limit,
                keyring.allowed_signers.as_deref(),
                keyring.gpg_home.as_deref(),
            )
            .await
            .map_err(map_git_err)?
    } else {
        emails_probe
    };
    let resolved =
        author_resolve::resolve_author_emails(&ctx.db, commits.iter().map(|c| c.author_email.as_str()))
            .await;
    let mut out_commits = Vec::with_capacity(commits.len());
    for c in commits {
        let r = resolved.get(&c.author_email).cloned().unwrap_or_default();
        let signature_status = signatures::apply_verified_policy(
            &ctx.db,
            &c.committer_email,
            &c.author_email,
            &c.signature_status,
            &c.signature_kind,
        )
        .await;
        out_commits.push(RepoCommitSummary {
            sha: c.sha,
            short_sha: c.short_sha,
            subject: c.subject,
            author_name: c.author_name,
            author_email: c.author_email,
            authored_at: c.authored_at,
            author_user_id: r.user_id,
            author_username: r.username,
            author_avatar_url: r.avatar_url,
            signature_status,
            signature_kind: c.signature_kind,
        });
    }
    Ok(RepoCommitsResponse {
        ref_name,
        commits: out_commits,
        skip: req.skip,
        limit,
    })
}

/// `repo.pathLastCommits` — batched last commit per tree entry (issue #23).
pub async fn path_last_commits(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<oxidean_core::RepoPathLastCommitsResponse, AppError> {
    let req: oxidean_core::RepoPathLastCommitsRequest =
        serde_json::from_value(input).map_err(|e| {
            AppError::new(
                "rpc.bad_input",
                format!("invalid repo.pathLastCommits input: {e}"),
            )
        })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let path = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    let ref_name = if req.ref_name.trim().is_empty() {
        accessible.row.default_branch.clone()
    } else {
        req.ref_name.trim().to_string()
    };
    let dir_path = req
        .path
        .as_deref()
        .unwrap_or("")
        .trim()
        .trim_start_matches('/')
        .to_string();
    let entries = ctx
        .git
        .ls_tree(&path, &ref_name, &dir_path)
        .await
        .map_err(map_git_err)?;
    let names: Vec<String> = entries.into_iter().map(|e| e.name).collect();
    let map = ctx
        .git
        .path_last_commits(&path, &ref_name, &dir_path, &names)
        .await
        .map_err(map_git_err)?;
    let mut commits = std::collections::BTreeMap::new();
    let resolved = author_resolve::resolve_author_emails(
        &ctx.db,
        map.values().map(|c| c.author_email.as_str()),
    )
    .await;
    for (name, c) in map {
        let r = resolved.get(&c.author_email).cloned().unwrap_or_default();
        commits.insert(
            name,
            RepoCommitSummary {
                sha: c.sha,
                short_sha: c.short_sha,
                subject: c.subject,
                author_name: c.author_name,
                author_email: c.author_email,
                authored_at: c.authored_at,
                author_user_id: r.user_id,
                author_username: r.username,
                author_avatar_url: r.avatar_url,
                signature_status: c.signature_status,
                signature_kind: c.signature_kind,
            },
        );
    }
    Ok(oxidean_core::RepoPathLastCommitsResponse {
        ref_name,
        path: dir_path,
        commits,
    })
}

/// `repo.commitCount` — rev-list count for Code home header (issue #23).
pub async fn commit_count(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<oxidean_core::RepoCommitCountResponse, AppError> {
    let req: oxidean_core::RepoCommitCountRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.commitCount input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let path = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    let ref_name = if req.ref_name.trim().is_empty() {
        accessible.row.default_branch.clone()
    } else {
        req.ref_name.trim().to_string()
    };
    let count = ctx
        .git
        .rev_list_count(&path, &ref_name)
        .await
        .map_err(map_git_err)?;
    Ok(oxidean_core::RepoCommitCountResponse { ref_name, count })
}

/// `repo.contributors.list` — shortlog authors for About sidebar (issue #23).
pub async fn contributors_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<oxidean_core::RepoContributorsListResponse, AppError> {
    let req: oxidean_core::RepoContributorsListRequest =
        serde_json::from_value(input).map_err(|e| {
            AppError::new(
                "rpc.bad_input",
                format!("invalid repo.contributors.list input: {e}"),
            )
        })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let path = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    let ref_name = accessible.row.default_branch.clone();
    let limit = req.limit.unwrap_or(30).clamp(1, 100) as u32;
    let raw = ctx
        .git
        .shortlog(&path, &ref_name, limit)
        .await
        .map_err(map_git_err)?;
    let mut contributors = Vec::with_capacity(raw.len());
    for c in raw {
        let mut username = None;
        let mut avatar_url = None;
        let mut display_name = c.name.clone();
        if !c.email.is_empty() {
            if let Ok(Some(user)) = ctx.db.find_user_by_email(&c.email).await {
                username = Some(user.username.clone());
                display_name = if user.display_name.trim().is_empty() {
                    user.username.clone()
                } else {
                    user.display_name.clone()
                };
                if user.avatar_path.is_some() {
                    avatar_url = Some(format!("/uploads/avatars/{}.webp", user.id));
                }
            }
        }
        contributors.push(oxidean_core::RepoContributorPublic {
            display_name,
            username,
            avatar_url,
            commit_count: c.commit_count,
        });
    }
    Ok(oxidean_core::RepoContributorsListResponse { contributors })
}

/// `repo.languages` — default-branch language byte breakdown for About (linguist-lite).
pub async fn languages(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<oxidean_core::RepoLanguagesResponse, AppError> {
    let req: oxidean_core::RepoLanguagesRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.languages input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let path = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    let ref_name = accessible.row.default_branch.clone();
    let blobs = ctx
        .git
        .ls_tree_sized_blobs(&path, &ref_name, language_stats::MAX_BLOBS)
        .await
        .map_err(map_git_err)?;
    let languages = language_stats::aggregate_language_stats(
        blobs.iter().map(|b| (b.path.as_str(), b.size)),
    );
    Ok(oxidean_core::RepoLanguagesResponse { languages })
}

/// `repo.commit` — commit detail + unified patches.
pub async fn commit(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoCommitResponse, AppError> {
    let req: RepoCommitRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid repo.commit input: {e}"))
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let path = bare_repo_path(&ctx.repos_dir, &accessible.owner_username, &accessible.row.name)?;
    let detail = ctx
        .git
        .show_commit(&path, req.sha.trim(), None, None)
        .await
        .map_err(map_git_err)?;
    let mut emails = vec![detail.author_email.clone()];
    if !detail.committer_email.trim().is_empty() {
        emails.push(detail.committer_email.clone());
    }
    let keyring = signatures::keyring_for_emails(&ctx.db, &emails).await;
    let detail = if keyring.has_any() {
        ctx.git
            .show_commit(
                &path,
                req.sha.trim(),
                keyring.allowed_signers.as_deref(),
                keyring.gpg_home.as_deref(),
            )
            .await
            .map_err(map_git_err)?
    } else {
        detail
    };
    let r = author_resolve::resolve_author_email(&ctx.db, &detail.author_email).await;
    let signature_status = signatures::apply_verified_policy(
        &ctx.db,
        &detail.committer_email,
        &detail.author_email,
        &detail.signature_status,
        &detail.signature_kind,
    )
    .await;
    Ok(RepoCommitResponse {
        sha: detail.sha,
        short_sha: detail.short_sha,
        subject: detail.subject,
        body: detail.body,
        author_name: detail.author_name,
        author_email: detail.author_email,
        authored_at: detail.authored_at,
        author_user_id: r.user_id,
        author_username: r.username,
        author_avatar_url: r.avatar_url,
        signature_status,
        signature_kind: detail.signature_kind,
        parents: detail.parents,
        files: detail
            .files
            .into_iter()
            .map(|f| RepoDiffFile {
                path: f.path,
                status: f.status,
                patch: f.patch,
            })
            .collect(),
        truncated: detail.truncated,
    })
}

/// `repo.compare` — `base...head` unified diff (empty → empty result).
pub async fn compare(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoCompareResponse, AppError> {
    let req: RepoCompareRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid repo.compare input: {e}"))
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let path = bare_repo_path(&ctx.repos_dir, &accessible.owner_username, &accessible.row.name)?;
    let result = ctx
        .git
        .diff(&path, req.base.trim(), req.head.trim())
        .await
        .map_err(map_git_err)?;
    Ok(RepoCompareResponse {
        base: result.base,
        head: result.head,
        empty: result.empty,
        truncated: result.truncated,
        files: result
            .files
            .into_iter()
            .map(|f| RepoDiffFile {
                path: f.path,
                status: f.status,
                patch: f.patch,
            })
            .collect(),
    })
}

/// `repo.blame` — per-line blame for a text path.
pub async fn blame(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoBlameResponse, AppError> {
    let req: RepoBlameRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid repo.blame input: {e}"))
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let path = bare_repo_path(&ctx.repos_dir, &accessible.owner_username, &accessible.row.name)?;
    let ref_name = if req.ref_name.trim().is_empty() {
        accessible.row.default_branch.clone()
    } else {
        req.ref_name.trim().to_string()
    };
    let file_path = req.path.trim_start_matches('/').to_string();
    let blame = ctx
        .git
        .blame(&path, &ref_name, &file_path)
        .await
        .map_err(map_git_err)?;
    let resolved = author_resolve::resolve_author_emails(
        &ctx.db,
        blame.lines.iter().map(|l| l.author_email.as_str()),
    )
    .await;
    Ok(RepoBlameResponse {
        path: blame.path,
        ref_name: blame.ref_name,
        lines: blame
            .lines
            .into_iter()
            .map(|l| {
                let r = resolved.get(&l.author_email).cloned().unwrap_or_default();
                RepoBlameLine {
                    sha: l.sha,
                    author_name: l.author_name,
                    author_email: l.author_email,
                    authored_at: l.authored_at,
                    line_number: l.line_number,
                    content: l.content,
                    author_user_id: r.user_id,
                    author_username: r.username,
                    author_avatar_url: r.avatar_url,
                }
            })
            .collect(),
        truncated: blame.truncated,
    })
}

fn soft_protect_err() -> AppError {
    AppError::new(
        "repo.default_branch_protected",
        "The default branch can't be renamed or deleted.",
    )
}

/// Resolve repo for Write mutate (D-27 branch CRUD / T-10-14).
/// Insufficient capability → identical [`acl::not_found`].
/// Visibility / soft-delete use [`resolve_repo_for_admin`] (Admin) instead.
async fn resolve_repo_for_owner_mutate(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
) -> Result<AccessibleRepo, AppError> {
    let _ = require_verified(ctx).await?;
    let accessible = resolve_repo_for_read(ctx, owner, name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(acl::not_found());
    }
    Ok(accessible)
}

/// Reject reserved git option tokens used as branch names (CR-02 / D-28).
fn reject_option_like_branch(name: &str) -> Result<(), AppError> {
    let t = name.trim();
    if t.starts_with('-') {
        return Err(AppError::new(
            "repo.invalid_ref",
            format!("invalid branch name: {name}"),
        ));
    }
    Ok(())
}

/// `repo.branchCreate` — owner creates a branch from `start` (GIT-06 / D-27).
pub async fn branch_create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoBranchMutationResponse, AppError> {
    let req: RepoBranchCreateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.branchCreate input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_owner_mutate(ctx, &req.owner, &req.name).await?;
    let actor_id = ctx
        .session
        .as_ref()
        .map(|s| s.user_id.clone())
        .ok_or_else(|| AppError::new("auth.unauthenticated", "sign in required"))?;
    let branch = req.branch.trim();
    if branch.is_empty() {
        return Err(AppError::new("repo.invalid_ref", "branch name required"));
    }
    reject_option_like_branch(branch)?;
    let start = req
        .start
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(accessible.row.default_branch.as_str());
    reject_option_like_branch(start)?;
    let path = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    ctx.git
        .branch_create(&path, branch, start)
        .await
        .map_err(map_git_err)?;
    let after_oid = ctx
        .git
        .list_refs(&path)
        .await
        .ok()
        .and_then(|refs| {
            let want = format!("refs/heads/{branch}");
            refs.into_iter()
                .find(|r| r.name == want)
                .map(|r| r.oid)
        })
        .unwrap_or_default();
    crate::repo::record_branch_creation(
        &ctx.db,
        &accessible.row.id,
        &actor_id,
        branch,
        &after_oid,
    )
    .await;
    crate::mirror::notify_mirror_after_local_mutation(
        ctx.db.clone(),
        ctx.git.clone(),
        ctx.repos_dir.clone(),
        accessible.row.id.clone(),
    );
    Ok(RepoBranchMutationResponse {
        branch: branch.to_string(),
    })
}

/// `repo.branchRename` — owner renames a branch; default soft-protected (D-28).
pub async fn branch_rename(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoBranchMutationResponse, AppError> {
    let req: RepoBranchRenameRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.branchRename input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_owner_mutate(ctx, &req.owner, &req.name).await?;
    let actor_id = ctx
        .session
        .as_ref()
        .map(|s| s.user_id.clone())
        .ok_or_else(|| AppError::new("auth.unauthenticated", "sign in required"))?;
    let from = req.from.trim();
    let to = req.to.trim();
    if from.is_empty() || to.is_empty() {
        return Err(AppError::new(
            "repo.invalid_ref",
            "from and to branch names required",
        ));
    }
    if from == accessible.row.default_branch {
        return Err(soft_protect_err());
    }
    let path = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    let tip_oid = ctx
        .git
        .list_refs(&path)
        .await
        .ok()
        .and_then(|refs| {
            let want = format!("refs/heads/{from}");
            refs.into_iter()
                .find(|r| r.name == want)
                .map(|r| r.oid)
        })
        .unwrap_or_default();
    ctx.git
        .branch_rename(&path, from, to)
        .await
        .map_err(map_git_err)?;
    crate::repo::record_branch_rename(
        &ctx.db,
        &accessible.row.id,
        &actor_id,
        from,
        to,
        &tip_oid,
    )
    .await;
    crate::mirror::notify_mirror_after_local_mutation(
        ctx.db.clone(),
        ctx.git.clone(),
        ctx.repos_dir.clone(),
        accessible.row.id.clone(),
    );
    Ok(RepoBranchMutationResponse {
        branch: to.to_string(),
    })
}

/// `repo.branchDelete` — owner deletes a branch; default soft-protected (D-28).
pub async fn branch_delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoBranchMutationResponse, AppError> {
    let req: RepoBranchDeleteRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.branchDelete input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_owner_mutate(ctx, &req.owner, &req.name).await?;
    let actor_id = ctx
        .session
        .as_ref()
        .map(|s| s.user_id.clone())
        .ok_or_else(|| AppError::new("auth.unauthenticated", "sign in required"))?;
    let branch = req.branch.trim();
    if branch.is_empty() {
        return Err(AppError::new("repo.invalid_ref", "branch name required"));
    }
    if branch == accessible.row.default_branch {
        return Err(soft_protect_err());
    }
    // Phase 13 / D-20: honor allow_deletions on matching protection rules.
    {
        let eff = crate::protection::effective_for_branch(
            &ctx.db,
            &accessible.row.id,
            branch,
        )
        .await?;
        if let Err(e) = crate::protection::evaluate_push(
            &eff,
            crate::protection::ProtectionIntent::Delete,
            accessible.capability,
        ) {
            return Err(e);
        }
    }
    let path = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    let before_oid = ctx
        .git
        .list_refs(&path)
        .await
        .ok()
        .and_then(|refs| {
            let want = format!("refs/heads/{branch}");
            refs.into_iter()
                .find(|r| r.name == want)
                .map(|r| r.oid)
        })
        .unwrap_or_default();
    ctx.git
        .branch_delete(&path, branch)
        .await
        .map_err(map_git_err)?;
    crate::repo::record_branch_deletion(
        &ctx.db,
        &accessible.row.id,
        &actor_id,
        branch,
        &before_oid,
    )
    .await;
    crate::mirror::notify_mirror_after_local_mutation(
        ctx.db.clone(),
        ctx.git.clone(),
        ctx.repos_dir.clone(),
        accessible.row.id.clone(),
    );
    Ok(RepoBranchMutationResponse {
        branch: branch.to_string(),
    })
}

/// `repo.updateVisibility` — Admin capability toggles public/private (D-26 / ORG-03).
/// Insufficient capability → soft not_found.
pub async fn update_visibility(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoPublic, AppError> {
    let req: RepoUpdateVisibilityRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.updateVisibility input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let row = ctx
        .db
        .update_repository_visibility(&accessible.row.id, map_visibility(req.visibility))
        .await
        .map_err(db_err)?;
    Ok(to_public(&AccessibleRepo {
        row,
        owner_username: accessible.owner_username,
        capability: Some(Capability::Admin),
    }))
}

/// Soft cap for session RPC browser Download (D-LFS-18) — larger objects use git-lfs client.
pub const LFS_RPC_DOWNLOAD_MAX_BYTES: usize = 16 * 1024 * 1024;

/// `repo.lfs.setEnabled` — Admin-only per-repo LFS toggle (D-LFS-10).
pub async fn lfs_set_enabled(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoLfsEnabledResponse, AppError> {
    let req: RepoLfsSetEnabledRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.lfs.setEnabled input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    ctx.db
        .set_repo_lfs_enabled(&accessible.row.id, req.enabled)
        .await
        .map_err(db_err)?;
    Ok(RepoLfsEnabledResponse {
        enabled: req.enabled,
    })
}

/// `repo.lfs.getEnabled` — Read capability may inspect LFS enable status.
pub async fn lfs_get_enabled(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoLfsEnabledResponse, AppError> {
    let req: RepoLfsGetEnabledRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.lfs.getEnabled input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let enabled = ctx
        .db
        .get_repo_lfs_enabled(&accessible.row.id)
        .await
        .map_err(db_err)?;
    Ok(RepoLfsEnabledResponse { enabled })
}

/// `repo.lfs.getStatus` — enable + light usage for Settings.
pub async fn lfs_get_status(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoLfsStatusResponse, AppError> {
    let req: RepoLfsGetEnabledRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.lfs.getStatus input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let enabled = ctx
        .db
        .get_repo_lfs_enabled(&accessible.row.id)
        .await
        .map_err(db_err)?;
    let object_count = ctx
        .db
        .repo_lfs_object_count(&accessible.row.id)
        .await
        .map_err(db_err)?;
    let logical_bytes = ctx
        .db
        .lfs_repo_logical_bytes(&accessible.row.id)
        .await
        .map_err(db_err)?;
    Ok(RepoLfsStatusResponse {
        enabled,
        object_count,
        logical_bytes,
    })
}

/// `repo.lfs.getUsage` — this-repo breakdown (D-LFS-19).
pub async fn lfs_get_usage(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoLfsUsageResponse, AppError> {
    let req: RepoLfsGetEnabledRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.lfs.getUsage input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let enabled = ctx
        .db
        .get_repo_lfs_enabled(&accessible.row.id)
        .await
        .map_err(db_err)?;
    let object_count = ctx
        .db
        .repo_lfs_object_count(&accessible.row.id)
        .await
        .map_err(db_err)?;
    let logical_bytes = ctx
        .db
        .lfs_repo_logical_bytes(&accessible.row.id)
        .await
        .map_err(db_err)?;
    let settings = ctx.db.get_lfs_settings().await.map_err(db_err)?;
    let quota_repo_bytes = settings.quota_repo_bytes.unwrap_or_else(|| {
        std::env::var("OXIDEAN_LFS_QUOTA_REPO_BYTES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(crate::lfs::quota::DEFAULT_QUOTA_REPO_BYTES)
    });
    let objects = ctx
        .db
        .list_repo_lfs_objects(&accessible.row.id, 25)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(|r| RepoLfsObjectEntry {
            oid: r.oid,
            size: r.size,
            refcount: r.refcount,
        })
        .collect();
    Ok(RepoLfsUsageResponse {
        enabled,
        object_count,
        logical_bytes,
        quota_repo_bytes,
        objects,
    })
}

/// `repo.lfs.listObjects` — in-app LFS browser listing (D-LFS-16).
pub async fn lfs_list_objects(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoLfsListObjectsResponse, AppError> {
    let req: RepoLfsListObjectsRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.lfs.listObjects input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let enabled = ctx
        .db
        .get_repo_lfs_enabled(&accessible.row.id)
        .await
        .map_err(db_err)?;
    let limit = req.limit.unwrap_or(100);
    let objects = ctx
        .db
        .list_repo_lfs_objects(&accessible.row.id, limit)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(|r| RepoLfsObjectEntry {
            oid: r.oid,
            size: r.size,
            refcount: r.refcount,
        })
        .collect();
    Ok(RepoLfsListObjectsResponse { enabled, objects })
}

/// `repo.lfs.download` — session + Read; base64 soft-capped (never PAT / .git/info/lfs cookie).
pub async fn lfs_download(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoLfsDownloadResponse, AppError> {
    let req: RepoLfsDownloadRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.lfs.download input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    crate::lfs::store::validate_oid(&req.oid).map_err(|e| {
        AppError::new("rpc.bad_input", e)
    })?;
    let linked = ctx
        .db
        .has_lfs_link(&accessible.row.id, &req.oid)
        .await
        .map_err(db_err)?;
    if !linked {
        return Err(AppError::new(
            "lfs.object_not_found",
            "LFS object not found for this repository",
        ));
    }
    let meta = ctx
        .db
        .find_lfs_object(&req.oid)
        .await
        .map_err(db_err)?
        .ok_or_else(|| {
            AppError::new(
                "lfs.object_not_found",
                "LFS object not found for this repository",
            )
        })?;
    if meta.size < 0 || meta.size as usize > LFS_RPC_DOWNLOAD_MAX_BYTES {
        return Err(AppError::new(
            "lfs.too_large_for_rpc",
            format!(
                "object exceeds browser Download limit ({LFS_RPC_DOWNLOAD_MAX_BYTES} bytes); use git lfs"
            ),
        ));
    }
    let bytes = crate::lfs::store::read_object(&ctx.lfs_dir, &req.oid)
        .await
        .map_err(|e| AppError::new("lfs.read_failed", e))?;
    Ok(RepoLfsDownloadResponse {
        oid: req.oid,
        size: meta.size,
        encoding: "base64".into(),
        content: base64_encode(&bytes),
    })
}

/// `repo.softDelete` — Admin soft-deletes after typed name confirm (D-35). Disk purge deferred.
pub async fn soft_delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoSoftDeleteResponse, AppError> {
    let req: RepoSoftDeleteRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.softDelete input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let confirm = req.confirm_name.trim();
    if confirm != accessible.row.name.as_str() {
        return Err(AppError::new(
            "repo.confirm_mismatch",
            "Type the repository name exactly to confirm deletion.",
        ));
    }
    ctx.db
        .soft_delete_repository(&accessible.row.id)
        .await
        .map_err(db_err)?;
    Ok(RepoSoftDeleteResponse {
        name: accessible.row.name,
    })
}

/// Deny create under a foreign owner slug (T-10-06 / A5).
fn create_forbidden() -> AppError {
    AppError::new(
        "repo.create_forbidden",
        "You do not have permission to create a repository under this owner.",
    )
}

/// Resolve create target: omit/self → user; org slug → Owner/Admin only (A5).
async fn resolve_create_owner(
    ctx: &RpcCtx,
    user_id: &str,
    user_username: &str,
    owner_slug: Option<&str>,
) -> Result<(String, OwnerType, String), AppError> {
    let slug = owner_slug.map(str::trim).filter(|s| !s.is_empty());
    let Some(slug) = slug else {
        return Ok((
            user_id.to_string(),
            OwnerType::User,
            user_username.to_string(),
        ));
    };

    if slug.eq_ignore_ascii_case(user_username) {
        return Ok((
            user_id.to_string(),
            OwnerType::User,
            user_username.to_string(),
        ));
    }

    let owner_ref = match resolve_owner_slug(&ctx.db, slug).await {
        Ok(Some(r)) => r,
        Ok(None) => return Err(create_forbidden()),
        Err(e) => {
            tracing::error!(error = %e, "resolve_owner_slug for create failed");
            return Err(AppError::new("repo.internal", "repository operation failed"));
        }
    };

    match owner_ref {
        OwnerRef::User { id, username } => {
            if id == user_id {
                Ok((id, OwnerType::User, username))
            } else {
                Err(create_forbidden())
            }
        }
        OwnerRef::Org { id, slug } => {
            let member = match ctx.db.find_org_member(&id, user_id).await {
                Ok(m) => m,
                Err(e) => {
                    tracing::error!(error = %e, "find_org_member for create failed");
                    return Err(AppError::new("repo.internal", "repository operation failed"));
                }
            };
            let allowed = member
                .as_ref()
                .map(|m| m.role == "owner" || m.role == "admin")
                .unwrap_or(false);
            if !allowed {
                return Err(create_forbidden());
            }
            Ok((id, OwnerType::Org, slug))
        }
    }
}

/// `repo.create` — verified owner creates a public/private repo (DB + bare git + optional seed).
pub async fn create(ctx: &RpcCtx, input: serde_json::Value) -> Result<RepoPublic, AppError> {
    let user = require_verified(ctx).await?;

    let req: CreateRepoRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid repo.create input: {e}"))
    })?;

    validate_repo_name(&req.name).map_err(|msg| AppError::new("repo.invalid_name", msg))?;

    let name = req.name.trim().to_string();
    let description = req
        .description
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_string();
    let visibility = resolve_visibility(ctx, req.visibility).await?;
    let default_branch = if user.default_branch.trim().is_empty() {
        "main".to_string()
    } else {
        user.default_branch.clone()
    };

    let (owner_id, owner_type, owner_slug) = resolve_create_owner(
        ctx,
        &user.id,
        &user.username,
        req.owner.as_deref(),
    )
    .await?;

    let source_count = [
        none_like_opt(&req.stack_id),
        none_like_opt(&req.instance_pack_id),
        none_like_opt(&req.template_repo_id),
    ]
    .iter()
    .filter(|&&is_none| !is_none)
    .count();
    if source_count > 1 {
        return Err(AppError::new(
            "repo.invalid_template",
            "Choose only one of stack_id, instance_pack_id, or template_repo_id.",
        ));
    }

    // Normalize picker ids: UI may send `instance:<uuid>` / `repo:<uuid>` via stack_id.
    let mut stack_id = req.stack_id.clone();
    let mut instance_pack_id = req.instance_pack_id.clone();
    let mut template_repo_id = req.template_repo_id.clone();
    if let Some(raw) = stack_id.clone() {
        let t = raw.trim();
        if let Some(rest) = t.strip_prefix("instance:") {
            instance_pack_id = Some(rest.to_string());
            stack_id = None;
        } else if let Some(rest) = t.strip_prefix("repo:") {
            template_repo_id = Some(rest.to_string());
            stack_id = None;
        }
    }

    let (seed_files, from_template_repo_id) = resolve_create_seed_files(
        ctx,
        &stack_id,
        &instance_pack_id,
        &template_repo_id,
        &req.license_id,
        &req.gitignore_id,
    )
    .await?;

    if ctx
        .db
        .find_repository_by_owner_name(&owner_id, &name)
        .await
        .map_err(db_err)?
        .is_some()
    {
        return Err(AppError::new(
            "repo.name_taken",
            "A repository with this name already exists. Choose a different name.",
        ));
    }

    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_repository(
            &id,
            &owner_id,
            owner_type.as_str(),
            &name,
            map_visibility(visibility),
            &description,
            &default_branch,
        )
        .await
        .map_err(db_err)?;

    if let Some(tid) = &from_template_repo_id {
        let _ = ctx
            .db
            .set_created_from_template_repo(&row.id, Some(tid))
            .await;
    }

    let path = bare_repo_path(&ctx.repos_dir, &owner_slug, &name)?;
    if let Err(e) = ctx.git.init_bare(&path, &default_branch).await {
        tracing::error!(
            error = %e,
            path = %path.display(),
            "init_bare failed after DB insert"
        );
        // WR-01: compensate so the name is not permanently occupied.
        compensate_failed_create(ctx, &row.id, &path).await;
        return Err(AppError::new(
            "repo.git_init_failed",
            "failed to initialize repository storage",
        ));
    }

    if !seed_files.is_empty() {
        let author_name = if user.display_name.trim().is_empty() {
            user.username.clone()
        } else {
            user.display_name.clone()
        };
        let signing_key = match crate::git::web_flow::ensure_web_flow_key().await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(error = %e, "web-flow signing key unavailable for seed");
                compensate_failed_create(ctx, &row.id, &path).await;
                return Err(AppError::new(
                    "repo.git_seed_failed",
                    "failed to prepare web-flow signing key for initial commit",
                ));
            }
        };
        if let Err(e) = ctx
            .git
            .seed_commit_authored(
                &path,
                &default_branch,
                "Initial commit",
                &seed_files,
                &author_name,
                &user.email,
                Some(signing_key.as_path()),
            )
            .await
        {
            tracing::error!(
                error = %e,
                path = %path.display(),
                "seed_commit_authored failed after init_bare"
            );
            compensate_failed_create(ctx, &row.id, &path).await;
            return Err(AppError::new(
                "repo.git_seed_failed",
                "failed to seed initial commit from templates",
            ));
        }
    }

    // Live repo at this slug/name supersedes any redirect (D-REL-08).
    if let Err(e) = rename_transfer::supersede_redirect_on_create(&ctx.db, &owner_slug, &name).await
    {
        tracing::warn!(error = %e, "delete matching redirect after create failed");
    }

    Ok(RepoPublic {
        id: row.id,
        owner_id: row.owner_id,
        owner_type,
        owner_username: owner_slug,
        name: row.name,
        description: row.description,
        visibility,
        default_branch: row.default_branch,
        updated_at: row.updated_at,
        can_admin: true,
        can_write: true,
                star_count: 0,
                open_issue_count: 0,
                open_pull_count: 0,
                viewer_has_starred: false,
                is_fork: false,
                is_template: false,
                homepage: String::new(),
                topics: Vec::new(),
                fork_count: 0,
                watch_count: 0,
                viewer_is_watching: false,
                fork_network_id: None,
                forked_from: None,
    })
}

/// `repo.fork` — Read+ on **public** source; bare copy + fork network (D-SOC-12…18, extends Phase 12).
pub async fn fork(ctx: &RpcCtx, input: serde_json::Value) -> Result<RepoPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: oxidean_core::ForkRepoRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid repo.fork input: {e}"))
    })?;
    let source = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    // SOC-04: public sources only (private → same not_found anti-enumeration when no Read;
    // with Read on private still deny for this phase).
    if is_private_visibility(&source.row.visibility) {
        return Err(not_found());
    }
    let into_owner = req
        .into_owner
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(user.username.as_str());
    if into_owner != user.username.as_str() {
        // Phase 21 still lands under self for now; org owner picker lands in UI later.
        return Err(AppError::new(
            "repo.fork_owner",
            "Forks must land under the signed-in user",
        ));
    }
    let into_name = req
        .into_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(source.row.name.as_str())
        .to_string();
    validate_repo_name(&into_name).map_err(|msg| AppError::new("repo.invalid_name", msg))?;

    if ctx
        .db
        .find_repository_by_owner_name(&user.id, &into_name)
        .await
        .map_err(db_err)?
        .is_some()
    {
        return Err(AppError::new(
            "repo.name_taken",
            "A repository with this name already exists. Choose a different name.",
        ));
    }

    let network_id = ctx
        .db
        .get_repo_fork_network_id(&source.row.id)
        .await
        .map_err(db_err)?
        .unwrap_or_else(|| source.row.id.clone());

    if ctx
        .db
        .find_active_fork_in_network(&user.id, &network_id)
        .await
        .map_err(db_err)?
        .is_some()
    {
        return Err(AppError::new(
            "repo.fork_exists",
            "You already have a fork in this network",
        ));
    }

    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_repository(
            &id,
            &user.id,
            "user",
            &into_name,
            "public",
            &source.row.description,
            &source.row.default_branch,
        )
        .await
        .map_err(db_err)?;
    // insert_repository sets fork_network_id = own id; overwrite with source network root.
    ctx.db
        .set_repo_fork_network_id(&row.id, &network_id)
        .await
        .map_err(db_err)?;
    ctx.db
        .set_repo_forked_from(&row.id, Some(&source.row.id))
        .await
        .map_err(db_err)?;
    ctx.db
        .bump_fork_count_for_network(&network_id)
        .await
        .map_err(db_err)?;

    let source_path = bare_repo_path(
        &ctx.repos_dir,
        &source.owner_username,
        &source.row.name,
    )?;
    let dest_path = bare_repo_path(&ctx.repos_dir, &user.username, &into_name)?;
    if let Err(e) = ctx.git.clone_bare(&source_path, &dest_path).await {
        tracing::error!(error = %e, "clone_bare failed after fork insert");
        compensate_failed_create(ctx, &row.id, &dest_path).await;
        return Err(AppError::new(
            "repo.fork_failed",
            "failed to copy repository storage",
        ));
    }

    let accessible = AccessibleRepo {
        row,
        owner_username: user.username.clone(),
        capability: Some(Capability::Admin),
    };
    enrich_social(ctx, to_public(&accessible), Some(&user.id)).await
}

/// `repo.explore` — public discovery listing (D-SOC-09…11). Anonymous OK.
pub async fn explore(ctx: &RpcCtx, input: serde_json::Value) -> Result<RepoListMineResponse, AppError> {
    let req: oxidean_core::RepoExploreRequest =
        serde_json::from_value(input).unwrap_or(oxidean_core::RepoExploreRequest {
            q: None,
            offset: None,
            limit: None,
        });
    let offset = req.offset.unwrap_or(0).max(0);
    let limit = req.limit.unwrap_or(30).clamp(1, 50);
    let q = req
        .q
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(100).collect::<String>());

    let rows = ctx
        .db
        .list_explore_repositories(q.as_deref(), offset, limit)
        .await
        .map_err(db_err)?;

    let viewer = ctx.session.as_ref().map(|s| s.user_id.as_str());
    let mut repos = Vec::with_capacity(rows.len());
    for row in rows {
        let owner_username = if row.owner_type == "org" {
            ctx.db
                .find_organization_by_id(&row.owner_id)
                .await
                .ok()
                .flatten()
                .map(|o| o.slug)
        } else {
            ctx.db
                .find_user_by_id(&row.owner_id)
                .await
                .ok()
                .flatten()
                .map(|u| u.username)
        };
        let Some(owner_username) = owner_username else {
            continue;
        };
        let accessible = AccessibleRepo {
            row,
            owner_username,
            capability: Some(Capability::Read),
        };
        let enriched = enrich_social(ctx, to_public(&accessible), viewer).await?;
        repos.push(enriched);
    }
    Ok(RepoListMineResponse { repos })
}
