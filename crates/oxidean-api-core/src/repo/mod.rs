//! Repo domain shared layer — ACL + units + signatures + search helpers +
//! cross-module repo infrastructure. `repo.*` RPC handlers live in
//! `oxidean-api-repo` (issue #162).

pub mod acl;
mod activity;
pub mod author_resolve;
mod collaborators;
pub mod files;
mod fork_network;
pub mod rename_transfer;
mod search;
pub mod search_query;
pub mod signatures;
pub mod units;

pub use acl::{
    archived_error, can_read_as_owner, capability_env_label, coalesce, effective_capability,
    ensure_not_archived, fg_all_covers_repo, is_private_visibility, login_slug_taken,
    lookup_repo_row_or_redirect, meets, not_found, org_member_capability_sources,
    owner_ref_for_repo, resolve_owner_slug, resolve_repo_for_read, AccessibleRepo, Capability,
    MemberBasePermission, OrgRole, OwnerRef,
};
pub use activity::{
    record_branch_creation, record_branch_deletion, record_branch_rename, record_pr_merge,
    record_ref_updates, short_ref, VALID_PUSH_TYPES,
};
pub use collaborators::resolve_repo_for_admin;
pub use files::create as file_create;
pub(crate) use files::{
    assert_path_creatable as file_assert_path_creatable, commit_changes as file_commit_changes,
    commit_message as file_commit_message, resolve_base_branch as file_resolve_base_branch,
};
pub use fork_network::head_valid_for_base;
pub use rename_transfer::{
    redirect_retention_days, resolve_repo_or_redirect, supersede_redirect_on_create,
    DEFAULT_REPO_REDIRECT_RETENTION_DAYS,
};
pub use search::code_search_pathspecs;
pub use units::{require_unit_enabled, RepoUnit};

/// Soft cap for repo.blob response bytes (aligned with git blob streaming).
pub const BLOB_SOFT_MAX_BYTES: usize = 1_048_576;

/// Soft cap for unified patch bytes in commit/compare (aligned with git backend).
pub const DIFF_SOFT_MAX_BYTES: usize = oxidean_git::DIFF_SOFT_MAX_BYTES;

use oxidean_core::{AppError, OwnerType, RepoPublic, RepoVisibility};

use crate::auth::gate::require_verified;
use crate::rpc::RpcCtx;

pub fn db_err(e: String) -> AppError {
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

pub fn require_session_user(
    ctx: &RpcCtx,
) -> Result<&crate::auth::session::ResolvedSession, AppError> {
    ctx.session
        .as_ref()
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))
}

pub fn map_git_err(e: oxidean_git::GitError) -> AppError {
    match e {
        oxidean_git::GitError::NotFound(msg) => AppError::new("repo.path_not_found", msg),
        oxidean_git::GitError::InvalidArg(msg) => AppError::new("repo.invalid_ref", msg),
        other => {
            tracing::error!(error = %other, "git backend error");
            AppError::new("repo.git_failed", "git operation failed")
        }
    }
}

pub fn to_public(repo: &AccessibleRepo) -> RepoPublic {
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
        archived: repo.row.archived,
        homepage: String::new(),
        topics: Vec::new(),
        fork_count: 0,
        watch_count: 0,
        viewer_is_watching: false,
        viewer_watch_level: None,
        // Unit flags default enabled here; `enrich_social` fills real values
        // on single-repo responses (COL-13).
        issues_enabled: true,
        pulls_enabled: true,
        wiki_enabled: true,
        projects_enabled: true,
        discussions_enabled: true,
        fork_network_id: None,
        forked_from: None,
    }
}

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
        public.viewer_watch_level = ctx
            .db
            .get_repo_watch_level(uid, &public.id)
            .await
            .map_err(db_err)?
            .and_then(|l| oxidean_core::WatchLevel::parse(&l).ok());
        public.viewer_is_watching = public
            .viewer_watch_level
            .is_some_and(|l| l != oxidean_core::WatchLevel::Ignore);
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
    public.homepage = ctx.db.get_repo_homepage(&public.id).await.map_err(db_err)?;
    public.topics = ctx.db.list_repo_topics(&public.id).await.map_err(db_err)?;
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
        if let Some(parent) = ctx.db.find_repository_by_id(&pid).await.map_err(db_err)? {
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
    // Per-repo unit toggles (COL-13) drive Issues/Pulls tab visibility.
    let unit_flags = ctx
        .db
        .get_repo_unit_flags(&public.id)
        .await
        .map_err(db_err)?;
    public.issues_enabled = unit_flags.issues_enabled;
    public.pulls_enabled = unit_flags.pulls_enabled;
    public.wiki_enabled = unit_flags.wiki_enabled;
    public.projects_enabled = unit_flags.projects_enabled;
    public.discussions_enabled = unit_flags.discussions_enabled;
    Ok(public)
}

/// Resolve repo for Write mutate (D-27 branch CRUD / T-10-14).
/// Insufficient capability → identical [`acl::not_found`].
/// Visibility / soft-delete use [`resolve_repo_for_admin`] (Admin) instead.
pub async fn resolve_repo_for_owner_mutate(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
) -> Result<AccessibleRepo, AppError> {
    let _ = require_verified(ctx).await?;
    let accessible = resolve_repo_for_read(ctx, owner, name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(acl::not_found());
    }
    // GIT-20: archived repos freeze branch create/rename/delete (D-27/D-28 surface).
    acl::ensure_not_archived(&accessible)?;
    Ok(accessible)
}

pub fn create_forbidden() -> AppError {
    AppError::new(
        "repo.create_forbidden",
        "You do not have permission to create a repository under this owner.",
    )
}
