//! Uniform database adapter boundary — the only place dialect branching is allowed (D-08).

pub mod actions;
pub mod audit_events;
pub mod auth_identities;
pub mod auth_settings;
pub mod branch_protection;
pub mod dialect;
pub mod email_tokens;
pub mod issue_labels;
pub mod issues;
pub mod lfs;
pub mod migrate;
pub mod mirrors;
pub mod notifications;
pub mod instance_invites;
pub mod org_invites;
pub mod org_members;
pub mod repo_invites;
pub mod organizations;
pub mod packages;
pub mod pats;
pub mod pool;
pub mod probe;
pub mod pulls;
pub mod redirects;
pub mod releases;
pub mod webhooks;
pub mod repo_activity;
pub mod repo_collaborators;
pub mod repositories;
pub mod sessions;
pub mod ssh_keys;
pub mod gpg_keys;
pub mod user_emails;
pub mod stars;
pub mod tag_protection;
pub mod templates;
pub mod topics;
pub mod users;
pub mod watches;

pub use actions::{
    ActionJobRow, ActionRunRow, ActionRunnerRow, ActionSecretCipherRow, ActionSecretMetaRow,
};
pub use audit_events::AuditEventRow;
pub use branch_protection::{BranchProtectionRuleRow, CommitStatusRow};
pub use tag_protection::TagProtectionRuleRow;
pub use dialect::{redact_url, resolve_dialect, resolve_dialect_from_env, Dialect};
pub use issue_labels::{IssueAssigneeRow, LabelRow};
pub use issues::{
    CommentRevisionRow, IssueCommentRow, IssueLinkRow, IssueListFilters, IssueRevisionRow, IssueRow,
};
pub use lfs::LfsObjectRow;
pub use mirrors::{RepositoryMirrorRefResultRow, RepositoryMirrorRow};
pub use notifications::NotificationRow;
pub use oxidean_core::DbProbeResponse;
pub use pool::DbPool;
pub use instance_invites::InstanceInviteRow;
pub use org_invites::OrgInviteRow;
pub use repo_invites::RepoInviteRow;
pub use org_members::{OrgMemberListRow, OrgMemberRow, OrgMineRow};
pub use organizations::OrganizationRow;
pub use packages::{PackageRow, PackageVersionRow, PackageUsageBreakdownRow};
pub use pats::PatRow;
pub use pulls::{PullCommentRow, PullReviewRow, PullRow, PullSearchFilters, RepoMergeSettingsRow};
pub use redirects::RedirectRow;
pub use releases::{ReleaseAssetRow, ReleaseRow};
pub use repo_activity::RepoActivityRow;
pub use repo_collaborators::{
    RepoCollaboratorGrantRow, RepoCollaboratorListRow, RepoCollaboratorRow,
};
pub use stars::{ForkListSort, RepoForkListRow, RepoStargazerListRow};
pub use watches::RepoWatcherListRow;
pub use repositories::{RepoDiskRef, RepositoryRow};
pub use ssh_keys::SshKeyRow;
pub use gpg_keys::GpgKeyRow;
pub use user_emails::UserEmailRow;
pub use templates::{InstanceTemplatePackRow, TemplateRepoListRow};
pub use users::UserRow;
pub use auth_settings::AuthSettingsRow;
pub use webhooks::{WebhookDeliveryAttemptRow, WebhookDeliveryRow, WebhookRow};
use dialect::resolve_dialect_from_env as resolve_from_env;
use pool::DbPool as Pool;

#[derive(Clone)]
pub struct Database {
    pool: Option<Pool>,
    dialect: Option<Dialect>,
}

impl Database {
    pub fn skipped() -> Self {
        Self {
            pool: None,
            dialect: None,
        }
    }

    /// True when no database pool is configured.
    pub fn is_skipped(&self) -> bool {
        self.pool.is_none()
    }

    /// Connect using `DATABASE_URL` when set; otherwise run without a pool (`skipped` ping).
    pub async fn from_env() -> Result<Self, String> {
        match std::env::var("DATABASE_URL") {
            Ok(url) if !url.is_empty() => Self::connect(&url).await,
            _ => Ok(Self::skipped()),
        }
    }

    pub async fn connect(url: &str) -> Result<Self, String> {
        let dialect = resolve_from_env(url)?;
        let pool = Pool::connect(url, dialect).await?;
        Ok(Self {
            pool: Some(pool),
            dialect: Some(dialect),
        })
    }

    pub fn dialect(&self) -> Option<Dialect> {
        self.dialect
    }

    fn require_pool(&self) -> Result<&Pool, String> {
        self.pool
            .as_ref()
            .ok_or_else(|| "database not configured".into())
    }

    pub async fn ping(&self) -> &'static str {
        let Some(pool) = &self.pool else {
            return "skipped";
        };
        let ok = match pool {
            Pool::Postgres(p) => sqlx::query("SELECT 1").execute(p).await.is_ok(),
            Pool::MySql(p) => sqlx::query("SELECT 1").execute(p).await.is_ok(),
            Pool::Sqlite(p) => sqlx::query("SELECT 1").execute(p).await.is_ok(),
        };
        if ok { "ok" } else { "error" }
    }

    pub async fn migrate(&self) -> Result<(), String> {
        let Some(pool) = &self.pool else {
            return Err("database not configured".into());
        };
        migrate::run_migrations(pool).await
    }

    pub async fn is_empty(&self) -> Result<bool, String> {
        let Some(pool) = &self.pool else {
            return Err("database not configured".into());
        };
        migrate::is_empty(pool).await
    }

    pub async fn probe(&self) -> Result<DbProbeResponse, String> {
        let (Some(pool), Some(dialect)) = (&self.pool, self.dialect) else {
            return Err("database not configured".into());
        };
        probe::probe(pool, dialect).await
    }

    // --- organizations ---

    pub async fn insert_organization(
        &self,
        id: &str,
        slug: &str,
        display_name: &str,
        member_base_permission: &str,
    ) -> Result<OrganizationRow, String> {
        organizations::insert_organization(
            self.require_pool()?,
            id,
            slug,
            display_name,
            member_base_permission,
        )
        .await
    }

    pub async fn find_organization_by_id(
        &self,
        id: &str,
    ) -> Result<Option<OrganizationRow>, String> {
        organizations::find_by_id(self.require_pool()?, id).await
    }

    pub async fn delete_organization(&self, id: &str) -> Result<(), String> {
        organizations::delete_organization(self.require_pool()?, id).await
    }

    /// Batch variant — one `IN (...)` round trip.
    pub async fn find_organizations_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<OrganizationRow>, String> {
        organizations::find_many_by_id(self.require_pool()?, ids).await
    }

    pub async fn find_organization_by_slug(
        &self,
        slug: &str,
    ) -> Result<Option<OrganizationRow>, String> {
        organizations::find_by_slug(self.require_pool()?, slug).await
    }

    pub async fn insert_org_owner_membership(
        &self,
        org_id: &str,
        user_id: &str,
    ) -> Result<OrgMemberRow, String> {
        org_members::insert_owner_membership(self.require_pool()?, org_id, user_id).await
    }

    pub async fn insert_org_member(
        &self,
        org_id: &str,
        user_id: &str,
        role: &str,
    ) -> Result<OrgMemberRow, String> {
        org_members::insert_member(self.require_pool()?, org_id, user_id, role).await
    }

    pub async fn find_org_member(
        &self,
        org_id: &str,
        user_id: &str,
    ) -> Result<Option<OrgMemberRow>, String> {
        org_members::find_member(self.require_pool()?, org_id, user_id).await
    }

    pub async fn find_org_member_role(
        &self,
        org_id: &str,
        user_id: &str,
    ) -> Result<Option<String>, String> {
        org_members::find_member_role(self.require_pool()?, org_id, user_id).await
    }

    pub async fn find_org_member_base_permission(
        &self,
        org_id: &str,
    ) -> Result<Option<String>, String> {
        organizations::find_member_base_permission(self.require_pool()?, org_id).await
    }

    pub async fn update_organization_settings(
        &self,
        id: &str,
        member_base_permission: Option<&str>,
        display_name: Option<&str>,
    ) -> Result<OrganizationRow, String> {
        organizations::update_settings(
            self.require_pool()?,
            id,
            member_base_permission,
            display_name,
        )
        .await
    }

    pub async fn count_org_owners(&self, org_id: &str) -> Result<i64, String> {
        org_members::count_owners(self.require_pool()?, org_id).await
    }

    pub async fn update_org_member_role(
        &self,
        org_id: &str,
        user_id: &str,
        role: &str,
    ) -> Result<OrgMemberRow, String> {
        org_members::update_member_role(self.require_pool()?, org_id, user_id, role).await
    }

    pub async fn remove_org_member(&self, org_id: &str, user_id: &str) -> Result<(), String> {
        org_members::remove_member(self.require_pool()?, org_id, user_id).await
    }

    pub async fn list_org_members(&self, org_id: &str) -> Result<Vec<OrgMemberListRow>, String> {
        org_members::list_members(self.require_pool()?, org_id).await
    }

    pub async fn list_orgs_for_user(&self, user_id: &str) -> Result<Vec<OrgMineRow>, String> {
        org_members::list_orgs_for_user(self.require_pool()?, user_id).await
    }

    // --- instance invites ---

    pub async fn insert_instance_invite(
        &self,
        id: &str,
        email: Option<&str>,
        token_hash: &str,
        expires_at: Option<&str>,
        invited_by: &str,
        max_uses: Option<i64>,
    ) -> Result<InstanceInviteRow, String> {
        instance_invites::insert_invite(
            self.require_pool()?,
            id,
            email,
            token_hash,
            expires_at,
            invited_by,
            max_uses,
        )
        .await
    }

    pub async fn find_instance_invite_by_id(
        &self,
        id: &str,
    ) -> Result<Option<InstanceInviteRow>, String> {
        instance_invites::find_by_id(self.require_pool()?, id).await
    }

    pub async fn find_instance_invite_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<InstanceInviteRow>, String> {
        instance_invites::find_by_token_hash(self.require_pool()?, token_hash).await
    }

    pub async fn find_pending_instance_invite_by_email(
        &self,
        email: &str,
    ) -> Result<Option<InstanceInviteRow>, String> {
        instance_invites::find_pending_by_email(self.require_pool()?, email).await
    }

    pub async fn list_pending_instance_invites(&self) -> Result<Vec<InstanceInviteRow>, String> {
        instance_invites::list_pending(self.require_pool()?).await
    }

    pub async fn revoke_instance_invite(&self, id: &str, revoked_at: &str) -> Result<(), String> {
        instance_invites::revoke(self.require_pool()?, id, revoked_at).await
    }

    /// Atomic seat consumption — errors when the invite is revoked or out of seats.
    pub async fn consume_instance_invite(&self, id: &str, accepted_at: &str) -> Result<(), String> {
        instance_invites::consume(self.require_pool()?, id, accepted_at).await
    }

    pub async fn count_instance_invites_created_by_since(
        &self,
        invited_by: &str,
        since: &str,
    ) -> Result<i64, String> {
        instance_invites::count_created_by_since(self.require_pool()?, invited_by, since).await
    }

    pub async fn set_instance_invite_expires_at(
        &self,
        id: &str,
        expires_at: &str,
    ) -> Result<(), String> {
        instance_invites::set_expires_at(self.require_pool()?, id, expires_at).await
    }

    // --- organization invites ---

    pub async fn insert_org_invite(
        &self,
        id: &str,
        org_id: &str,
        email: Option<&str>,
        role: &str,
        token_hash: &str,
        expires_at: Option<&str>,
        invited_by: &str,
        max_uses: Option<i64>,
    ) -> Result<OrgInviteRow, String> {
        org_invites::insert_invite(
            self.require_pool()?,
            id,
            org_id,
            email,
            role,
            token_hash,
            expires_at,
            invited_by,
            max_uses,
        )
        .await
    }

    pub async fn find_org_invite_by_id(&self, id: &str) -> Result<Option<OrgInviteRow>, String> {
        org_invites::find_by_id(self.require_pool()?, id).await
    }

    pub async fn find_org_invite_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<OrgInviteRow>, String> {
        org_invites::find_by_token_hash(self.require_pool()?, token_hash).await
    }

    pub async fn find_pending_org_invite_by_org_email(
        &self,
        org_id: &str,
        email: &str,
    ) -> Result<Option<OrgInviteRow>, String> {
        org_invites::find_pending_by_org_email(self.require_pool()?, org_id, email).await
    }

    pub async fn list_pending_org_invites(
        &self,
        org_id: &str,
    ) -> Result<Vec<OrgInviteRow>, String> {
        org_invites::list_pending(self.require_pool()?, org_id).await
    }

    pub async fn revoke_org_invite(&self, id: &str, revoked_at: &str) -> Result<(), String> {
        org_invites::revoke(self.require_pool()?, id, revoked_at).await
    }

    /// Atomic seat consumption — errors when the invite is revoked or out of seats.
    pub async fn consume_org_invite(&self, id: &str, accepted_at: &str) -> Result<(), String> {
        org_invites::consume(self.require_pool()?, id, accepted_at).await
    }

    pub async fn count_org_invites_created_by_since(
        &self,
        invited_by: &str,
        since: &str,
    ) -> Result<i64, String> {
        org_invites::count_created_by_since(self.require_pool()?, invited_by, since).await
    }

    pub async fn set_org_invite_expires_at(
        &self,
        id: &str,
        expires_at: &str,
    ) -> Result<(), String> {
        org_invites::set_expires_at(self.require_pool()?, id, expires_at).await
    }

    // --- repository invites ---

    pub async fn insert_repo_invite(
        &self,
        id: &str,
        repository_id: &str,
        email: Option<&str>,
        permission: &str,
        token_hash: &str,
        expires_at: Option<&str>,
        invited_by: &str,
        max_uses: Option<i64>,
    ) -> Result<RepoInviteRow, String> {
        repo_invites::insert_invite(
            self.require_pool()?,
            id,
            repository_id,
            email,
            permission,
            token_hash,
            expires_at,
            invited_by,
            max_uses,
        )
        .await
    }

    pub async fn find_repo_invite_by_id(
        &self,
        id: &str,
    ) -> Result<Option<RepoInviteRow>, String> {
        repo_invites::find_by_id(self.require_pool()?, id).await
    }

    pub async fn find_repo_invite_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<RepoInviteRow>, String> {
        repo_invites::find_by_token_hash(self.require_pool()?, token_hash).await
    }

    pub async fn find_pending_repo_invite_by_repo_email(
        &self,
        repository_id: &str,
        email: &str,
    ) -> Result<Option<RepoInviteRow>, String> {
        repo_invites::find_pending_by_repo_email(self.require_pool()?, repository_id, email).await
    }

    pub async fn list_pending_repo_invites(
        &self,
        repository_id: &str,
    ) -> Result<Vec<RepoInviteRow>, String> {
        repo_invites::list_pending(self.require_pool()?, repository_id).await
    }

    pub async fn revoke_repo_invite(&self, id: &str, revoked_at: &str) -> Result<(), String> {
        repo_invites::revoke(self.require_pool()?, id, revoked_at).await
    }

    /// Atomic seat consumption — errors when the invite is revoked or out of seats.
    pub async fn consume_repo_invite(&self, id: &str, accepted_at: &str) -> Result<(), String> {
        repo_invites::consume(self.require_pool()?, id, accepted_at).await
    }

    pub async fn count_repo_invites_created_by_since(
        &self,
        invited_by: &str,
        since: &str,
    ) -> Result<i64, String> {
        repo_invites::count_created_by_since(self.require_pool()?, invited_by, since).await
    }

    pub async fn set_repo_invite_expires_at(
        &self,
        id: &str,
        expires_at: &str,
    ) -> Result<(), String> {
        repo_invites::set_expires_at(self.require_pool()?, id, expires_at).await
    }

    pub async fn find_repo_collaborator(
        &self,
        repo_id: &str,
        user_id: &str,
    ) -> Result<Option<RepoCollaboratorRow>, String> {
        repo_collaborators::find_collaborator(self.require_pool()?, repo_id, user_id).await
    }

    pub async fn list_repo_collaborators(
        &self,
        repo_id: &str,
    ) -> Result<Vec<RepoCollaboratorListRow>, String> {
        repo_collaborators::list_collaborators(self.require_pool()?, repo_id).await
    }

    pub async fn list_repo_collaborator_grants_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<RepoCollaboratorGrantRow>, String> {
        repo_collaborators::list_grants_for_user(self.require_pool()?, user_id).await
    }

    pub async fn insert_repo_collaborator(
        &self,
        repo_id: &str,
        user_id: &str,
        permission: &str,
    ) -> Result<RepoCollaboratorRow, String> {
        repo_collaborators::insert_collaborator(
            self.require_pool()?,
            repo_id,
            user_id,
            permission,
        )
        .await
    }

    pub async fn update_repo_collaborator_permission(
        &self,
        repo_id: &str,
        user_id: &str,
        permission: &str,
    ) -> Result<RepoCollaboratorRow, String> {
        repo_collaborators::update_collaborator_permission(
            self.require_pool()?,
            repo_id,
            user_id,
            permission,
        )
        .await
    }

    pub async fn remove_repo_collaborator(
        &self,
        repo_id: &str,
        user_id: &str,
    ) -> Result<(), String> {
        repo_collaborators::remove_collaborator(self.require_pool()?, repo_id, user_id).await
    }

    // --- repositories ---

    pub async fn insert_repository(
        &self,
        id: &str,
        owner_id: &str,
        owner_type: &str,
        name: &str,
        visibility: &str,
        description: &str,
        default_branch: &str,
    ) -> Result<RepositoryRow, String> {
        let row = repositories::insert_repository(
            self.require_pool()?,
            id,
            owner_id,
            owner_type,
            name,
            visibility,
            description,
            default_branch,
        )
        .await?;
        // D-SOC-14: roots get fork_network_id = id (column from 0020_social).
        let _ = stars::set_fork_network_id(self.require_pool()?, &row.id, &row.id).await;
        Ok(row)
    }

    pub async fn star_repository(&self, user_id: &str, repository_id: &str) -> Result<i64, String> {
        stars::star_repository(self.require_pool()?, user_id, repository_id).await
    }

    pub async fn unstar_repository(
        &self,
        user_id: &str,
        repository_id: &str,
    ) -> Result<i64, String> {
        stars::unstar_repository(self.require_pool()?, user_id, repository_id).await
    }

    pub async fn get_repo_star_count(&self, repository_id: &str) -> Result<i64, String> {
        stars::get_star_count(self.require_pool()?, repository_id).await
    }

    pub async fn has_starred_repo(
        &self,
        user_id: &str,
        repository_id: &str,
    ) -> Result<bool, String> {
        stars::has_starred(self.require_pool()?, user_id, repository_id).await
    }

    pub async fn watch_repository(
        &self,
        user_id: &str,
        repository_id: &str,
    ) -> Result<i64, String> {
        watches::watch_repository(self.require_pool()?, user_id, repository_id).await
    }

    pub async fn unwatch_repository(
        &self,
        user_id: &str,
        repository_id: &str,
    ) -> Result<i64, String> {
        watches::unwatch_repository(self.require_pool()?, user_id, repository_id).await
    }

    pub async fn get_repo_watch_count(&self, repository_id: &str) -> Result<i64, String> {
        watches::get_watch_count(self.require_pool()?, repository_id).await
    }

    /// Open-issue count for repo chrome tab badges.
    pub async fn count_open_issues_for_repo(&self, repository_id: &str) -> Result<i64, String> {
        issues::count_open_issues_for_repo(self.require_pool()?, repository_id).await
    }

    /// Open-PR count for repo chrome tab badges.
    pub async fn count_open_pulls_for_repo(&self, repository_id: &str) -> Result<i64, String> {
        pulls::count_open_pulls_for_repo(self.require_pool()?, repository_id).await
    }

    pub async fn has_watched_repo(
        &self,
        user_id: &str,
        repository_id: &str,
    ) -> Result<bool, String> {
        watches::has_watched(self.require_pool()?, user_id, repository_id).await
    }

    pub async fn list_repo_watchers(
        &self,
        repository_id: &str,
        q: Option<&str>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<watches::RepoWatcherListRow>, String> {
        watches::list_repo_watchers(self.require_pool()?, repository_id, q, offset, limit).await
    }

    pub async fn count_repo_watchers(
        &self,
        repository_id: &str,
        q: Option<&str>,
    ) -> Result<i64, String> {
        watches::count_repo_watchers(self.require_pool()?, repository_id, q).await
    }

    pub async fn list_repo_stargazers(
        &self,
        repository_id: &str,
        q: Option<&str>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<stars::RepoStargazerListRow>, String> {
        stars::list_repo_stargazers(self.require_pool()?, repository_id, q, offset, limit).await
    }

    pub async fn count_repo_stargazers(
        &self,
        repository_id: &str,
        q: Option<&str>,
    ) -> Result<i64, String> {
        stars::count_repo_stargazers(self.require_pool()?, repository_id, q).await
    }

    pub async fn list_network_forks(
        &self,
        fork_network_id: &str,
        q: Option<&str>,
        sort: stars::ForkListSort,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<stars::RepoForkListRow>, String> {
        stars::list_network_forks(self.require_pool()?, fork_network_id, q, sort, offset, limit)
            .await
    }

    pub async fn count_network_forks(
        &self,
        fork_network_id: &str,
        q: Option<&str>,
    ) -> Result<i64, String> {
        stars::count_network_forks(self.require_pool()?, fork_network_id, q).await
    }

    pub async fn get_repo_homepage(&self, repository_id: &str) -> Result<String, String> {
        repositories::get_homepage(self.require_pool()?, repository_id).await
    }

    pub async fn update_repository_metadata(
        &self,
        id: &str,
        description: &str,
        homepage: &str,
    ) -> Result<RepositoryRow, String> {
        repositories::update_metadata(self.require_pool()?, id, description, homepage).await
    }

    pub async fn list_repo_topics(&self, repository_id: &str) -> Result<Vec<String>, String> {
        topics::list_repo_topics(self.require_pool()?, repository_id).await
    }

    pub async fn set_repo_topics(
        &self,
        repository_id: &str,
        topic_names: &[String],
    ) -> Result<Vec<String>, String> {
        topics::set_repo_topics(self.require_pool()?, repository_id, topic_names).await
    }

    /// Topic autocomplete: `(name, linked_repo_count)` most-used first.
    pub async fn suggest_topics(
        &self,
        prefix: &str,
        limit: i64,
    ) -> Result<Vec<(String, i64)>, String> {
        topics::suggest_topics(self.require_pool()?, prefix, limit).await
    }

    pub async fn get_repo_fork_count(&self, repository_id: &str) -> Result<i64, String> {
        repositories::get_fork_count(self.require_pool()?, repository_id).await
    }

    pub async fn recount_fork_count_for_network(
        &self,
        network_id: &str,
    ) -> Result<i64, String> {
        repositories::recount_fork_count_for_network(self.require_pool()?, network_id).await
    }

    /// Recount and return the network fork_count (alias for fork bump after create).
    pub async fn bump_fork_count_for_network(&self, network_id: &str) -> Result<i64, String> {
        self.recount_fork_count_for_network(network_id).await
    }

    pub async fn get_repo_fork_network_id(
        &self,
        repository_id: &str,
    ) -> Result<Option<String>, String> {
        stars::get_fork_network_id(self.require_pool()?, repository_id).await
    }

    pub async fn set_repo_fork_network_id(
        &self,
        repository_id: &str,
        network_id: &str,
    ) -> Result<(), String> {
        stars::set_fork_network_id(self.require_pool()?, repository_id, network_id).await
    }

    pub async fn find_active_fork_in_network(
        &self,
        owner_id: &str,
        fork_network_id: &str,
    ) -> Result<Option<RepositoryRow>, String> {
        stars::find_active_fork_in_network(self.require_pool()?, owner_id, fork_network_id).await
    }

    pub async fn list_starred_repo_ids(
        &self,
        user_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<String>, String> {
        stars::list_starred_repo_ids(self.require_pool()?, user_id, offset, limit).await
    }

    pub async fn list_explore_repositories(
        &self,
        q: Option<&str>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<RepositoryRow>, String> {
        stars::list_explore(self.require_pool()?, q, offset, limit).await
    }

    pub async fn find_repository_by_owner_name(
        &self,
        owner_id: &str,
        name: &str,
    ) -> Result<Option<RepositoryRow>, String> {
        repositories::find_by_owner_and_name(self.require_pool()?, owner_id, name).await
    }

    pub async fn find_repository_by_id(&self, id: &str) -> Result<Option<RepositoryRow>, String> {
        repositories::find_by_id(self.require_pool()?, id).await
    }

    /// Batch variant — one `IN (...)` round trip.
    pub async fn find_repositories_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<RepositoryRow>, String> {
        repositories::find_many_by_id(self.require_pool()?, ids).await
    }

    pub async fn list_repositories_by_owner(
        &self,
        owner_id: &str,
    ) -> Result<Vec<RepositoryRow>, String> {
        repositories::list_by_owner(self.require_pool()?, owner_id).await
    }

    pub async fn update_repository_visibility(
        &self,
        id: &str,
        visibility: &str,
    ) -> Result<RepositoryRow, String> {
        repositories::update_visibility(self.require_pool()?, id, visibility).await
    }

    pub async fn update_repository_name(
        &self,
        id: &str,
        name: &str,
    ) -> Result<RepositoryRow, String> {
        repositories::update_name(self.require_pool()?, id, name).await
    }

    pub async fn update_repository_owner(
        &self,
        id: &str,
        owner_id: &str,
        owner_type: &str,
    ) -> Result<RepositoryRow, String> {
        repositories::update_owner(self.require_pool()?, id, owner_id, owner_type).await
    }

    pub async fn soft_delete_repository(&self, id: &str) -> Result<(), String> {
        repositories::soft_delete(self.require_pool()?, id).await
    }

    pub async fn list_repo_disk_refs(&self) -> Result<Vec<repositories::RepoDiskRef>, String> {
        repositories::list_repo_disk_refs(self.require_pool()?).await
    }

    pub async fn hard_delete_repository(&self, id: &str) -> Result<(), String> {
        repositories::hard_delete(self.require_pool()?, id).await
    }

    pub async fn hard_delete_repositories_by_owner(
        &self,
        owner_id: &str,
        owner_type: &str,
    ) -> Result<u64, String> {
        repositories::hard_delete_by_owner(self.require_pool()?, owner_id, owner_type).await
    }

    // --- pulls ---

    pub async fn insert_pull(
        &self,
        id: &str,
        repo_id: &str,
        number: i64,
        title: &str,
        body: &str,
        author_id: &str,
        base_ref: &str,
        base_sha: &str,
        head_repo_id: &str,
        head_ref: &str,
        head_sha: &str,
        draft: bool,
    ) -> Result<PullRow, String> {
        pulls::insert_pull(
            self.require_pool()?,
            id,
            repo_id,
            number,
            title,
            body,
            author_id,
            base_ref,
            base_sha,
            head_repo_id,
            head_ref,
            head_sha,
            draft,
        )
        .await
    }

    pub async fn find_pull_by_repo_number(
        &self,
        repo_id: &str,
        number: i64,
    ) -> Result<Option<PullRow>, String> {
        pulls::find_by_repo_and_number(self.require_pool()?, repo_id, number).await
    }

    pub async fn list_pulls_for_repo(
        &self,
        repo_id: &str,
        state: Option<&str>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<PullRow>, i64), String> {
        pulls::list_by_repo(self.require_pool()?, repo_id, state, offset, limit).await
    }

    pub async fn search_pulls_for_repo(
        &self,
        repo_id: &str,
        filters: pulls::PullSearchFilters<'_>,
    ) -> Result<(Vec<PullRow>, i64), String> {
        pulls::search_by_repo(self.require_pool()?, repo_id, filters).await
    }

    pub async fn set_pull_state(
        &self,
        id: &str,
        state: &str,
        closed_at: Option<&str>,
        closed_by: Option<&str>,
    ) -> Result<(), String> {
        pulls::set_state(self.require_pool()?, id, state, closed_at, closed_by).await
    }

    pub async fn get_repo_merge_settings(
        &self,
        repo_id: &str,
    ) -> Result<RepoMergeSettingsRow, String> {
        pulls::get_merge_settings(self.require_pool()?, repo_id).await
    }

    pub async fn set_repo_merge_settings(
        &self,
        repo_id: &str,
        allow_merge_commit: bool,
        allow_squash_merge: bool,
        allow_rebase_merge: bool,
    ) -> Result<(), String> {
        pulls::set_merge_settings(
            self.require_pool()?,
            repo_id,
            allow_merge_commit,
            allow_squash_merge,
            allow_rebase_merge,
        )
        .await
    }

    pub async fn set_repo_forked_from(
        &self,
        repo_id: &str,
        forked_from: Option<&str>,
    ) -> Result<(), String> {
        pulls::set_forked_from(self.require_pool()?, repo_id, forked_from).await
    }

    pub async fn get_repo_forked_from(&self, repo_id: &str) -> Result<Option<String>, String> {
        pulls::get_forked_from(self.require_pool()?, repo_id).await
    }

    pub async fn update_pull_fields(
        &self,
        id: &str,
        title: &str,
        body: &str,
        draft: bool,
        base_ref: &str,
        base_sha: &str,
    ) -> Result<(), String> {
        pulls::update_fields(
            self.require_pool()?,
            id,
            title,
            body,
            draft,
            base_ref,
            base_sha,
        )
        .await
    }

    pub async fn mark_pull_merged(
        &self,
        id: &str,
        merged_by: &str,
        merge_commit_sha: &str,
        merge_method: &str,
        merged_at: &str,
    ) -> Result<(), String> {
        pulls::mark_merged(
            self.require_pool()?,
            id,
            merged_by,
            merge_commit_sha,
            merge_method,
            merged_at,
        )
        .await
    }

    pub async fn insert_pull_comment(
        &self,
        id: &str,
        pull_id: &str,
        author_id: &str,
        body: &str,
        path: Option<&str>,
        side: Option<&str>,
        line: Option<i64>,
        start_line: Option<i64>,
        commit_sha: Option<&str>,
    ) -> Result<PullCommentRow, String> {
        pulls::insert_pull_comment(
            self.require_pool()?,
            id,
            pull_id,
            author_id,
            body,
            path,
            side,
            line,
            start_line,
            commit_sha,
        )
        .await
    }

    pub async fn find_pull_comment_by_id(
        &self,
        id: &str,
    ) -> Result<Option<PullCommentRow>, String> {
        pulls::find_pull_comment_by_id(self.require_pool()?, id).await
    }

    pub async fn list_pull_comments(
        &self,
        pull_id: &str,
    ) -> Result<Vec<PullCommentRow>, String> {
        pulls::list_pull_comments(self.require_pool()?, pull_id).await
    }

    pub async fn set_pull_comment_resolved(
        &self,
        id: &str,
        resolved: bool,
    ) -> Result<PullCommentRow, String> {
        pulls::set_pull_comment_resolved(self.require_pool()?, id, resolved).await
    }

    pub async fn mark_pull_line_comments_outdated(&self, pull_id: &str) -> Result<(), String> {
        pulls::mark_pull_line_comments_outdated(self.require_pool()?, pull_id).await
    }

    pub async fn update_pull_head_sha(&self, id: &str, head_sha: &str) -> Result<(), String> {
        pulls::update_pull_head_sha(self.require_pool()?, id, head_sha).await
    }

    pub async fn pull_has_label(&self, pull_id: &str, label: &str) -> Result<bool, String> {
        pulls::pull_has_label(self.require_pool()?, pull_id, label).await
    }

    pub async fn pull_has_assignee(&self, pull_id: &str, user_id: &str) -> Result<bool, String> {
        pulls::pull_has_assignee(self.require_pool()?, pull_id, user_id).await
    }

    pub async fn list_pull_assignees(
        &self,
        pull_id: &str,
    ) -> Result<Vec<pulls::PullAssigneeRow>, String> {
        pulls::list_pull_assignees(self.require_pool()?, pull_id).await
    }

    /// Batch variant — `(pull_id, assignee)` pairs in one `IN (...)` round trip.
    pub async fn list_pull_assignees_for_pulls(
        &self,
        pull_ids: &[String],
    ) -> Result<Vec<(String, pulls::PullAssigneeRow)>, String> {
        pulls::list_pull_assignees_for_pulls(self.require_pool()?, pull_ids).await
    }

    /// Subset of `pull_ids` carrying a label matching `label` (id or name).
    pub async fn pull_ids_with_label(
        &self,
        pull_ids: &[String],
        label: &str,
    ) -> Result<Vec<String>, String> {
        pulls::pull_ids_with_label(self.require_pool()?, pull_ids, label).await
    }

    /// Batch `list_pull_reviews` — one `IN (...)` round trip for many pulls.
    pub async fn list_reviews_for_pulls(
        &self,
        pull_ids: &[String],
    ) -> Result<Vec<PullReviewRow>, String> {
        pulls::list_reviews_for_pulls(self.require_pool()?, pull_ids).await
    }

    pub async fn insert_pull_review(
        &self,
        id: &str,
        pull_id: &str,
        author_id: &str,
        state: &str,
        body: &str,
        commit_sha: Option<&str>,
    ) -> Result<PullReviewRow, String> {
        pulls::insert_pull_review(
            self.require_pool()?,
            id,
            pull_id,
            author_id,
            state,
            body,
            commit_sha,
        )
        .await
    }

    pub async fn find_pull_review_by_id(
        &self,
        id: &str,
    ) -> Result<Option<PullReviewRow>, String> {
        pulls::find_pull_review_by_id(self.require_pool()?, id).await
    }

    pub async fn list_pull_reviews(&self, pull_id: &str) -> Result<Vec<PullReviewRow>, String> {
        pulls::list_pull_reviews(self.require_pool()?, pull_id).await
    }

    pub async fn dismiss_pull_review(
        &self,
        id: &str,
        reason: Option<&str>,
        dismissed_at: &str,
    ) -> Result<PullReviewRow, String> {
        pulls::dismiss_pull_review(self.require_pool()?, id, reason, dismissed_at).await
    }

    pub async fn upsert_pull_review_request(
        &self,
        pull_id: &str,
        user_id: &str,
        requested_by: &str,
    ) -> Result<(), String> {
        pulls::upsert_review_request(self.require_pool()?, pull_id, user_id, requested_by).await
    }

    pub async fn delete_pull_review_request(
        &self,
        pull_id: &str,
        user_id: &str,
    ) -> Result<(), String> {
        pulls::delete_review_request(self.require_pool()?, pull_id, user_id).await
    }

    pub async fn list_pull_review_request_user_ids(
        &self,
        pull_id: &str,
    ) -> Result<Vec<String>, String> {
        pulls::list_review_request_user_ids(self.require_pool()?, pull_id).await
    }

    // --- branch protection + commit statuses (Phase 13) ---

    pub async fn list_branch_protection_rules(
        &self,
        repo_id: &str,
    ) -> Result<Vec<BranchProtectionRuleRow>, String> {
        branch_protection::list_rules(self.require_pool()?, repo_id).await
    }

    pub async fn find_branch_protection_rule(
        &self,
        repo_id: &str,
        rule_id: &str,
    ) -> Result<Option<BranchProtectionRuleRow>, String> {
        branch_protection::find_rule(self.require_pool()?, repo_id, rule_id).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_branch_protection_rule(
        &self,
        id: &str,
        repo_id: &str,
        pattern: &str,
        require_reviews: bool,
        required_approving_review_count: i32,
        dismiss_stale_reviews: bool,
        require_conversation_resolution: bool,
        require_last_push_approval: bool,
        required_status_contexts: &str,
        strict_status_checks: bool,
        allow_force_pushes: bool,
        allow_deletions: bool,
        enforce_admins: bool,
        required_linear_history: bool,
        lock_branch: bool,
    ) -> Result<BranchProtectionRuleRow, String> {
        branch_protection::insert_rule(
            self.require_pool()?,
            id,
            repo_id,
            pattern,
            require_reviews,
            required_approving_review_count,
            dismiss_stale_reviews,
            require_conversation_resolution,
            require_last_push_approval,
            required_status_contexts,
            strict_status_checks,
            allow_force_pushes,
            allow_deletions,
            enforce_admins,
            required_linear_history,
            lock_branch,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_branch_protection_rule(
        &self,
        repo_id: &str,
        rule_id: &str,
        pattern: &str,
        require_reviews: bool,
        required_approving_review_count: i32,
        dismiss_stale_reviews: bool,
        require_conversation_resolution: bool,
        require_last_push_approval: bool,
        required_status_contexts: &str,
        strict_status_checks: bool,
        allow_force_pushes: bool,
        allow_deletions: bool,
        enforce_admins: bool,
        required_linear_history: bool,
        lock_branch: bool,
    ) -> Result<BranchProtectionRuleRow, String> {
        branch_protection::update_rule(
            self.require_pool()?,
            repo_id,
            rule_id,
            pattern,
            require_reviews,
            required_approving_review_count,
            dismiss_stale_reviews,
            require_conversation_resolution,
            require_last_push_approval,
            required_status_contexts,
            strict_status_checks,
            allow_force_pushes,
            allow_deletions,
            enforce_admins,
            required_linear_history,
            lock_branch,
        )
        .await
    }

    pub async fn delete_branch_protection_rule(
        &self,
        repo_id: &str,
        rule_id: &str,
    ) -> Result<(), String> {
        branch_protection::delete_rule(self.require_pool()?, repo_id, rule_id).await
    }

    // --- tag protection rulesets (GIT-21) ---

    pub async fn list_tag_protection_rules(
        &self,
        repo_id: &str,
    ) -> Result<Vec<TagProtectionRuleRow>, String> {
        tag_protection::list_rules(self.require_pool()?, repo_id).await
    }

    pub async fn find_tag_protection_rule(
        &self,
        repo_id: &str,
        rule_id: &str,
    ) -> Result<Option<TagProtectionRuleRow>, String> {
        tag_protection::find_rule(self.require_pool()?, repo_id, rule_id).await
    }

    pub async fn insert_tag_protection_rule(
        &self,
        id: &str,
        repo_id: &str,
        pattern: &str,
        allow_create: bool,
        allow_update: bool,
        allow_delete: bool,
        enforce_admins: bool,
    ) -> Result<TagProtectionRuleRow, String> {
        tag_protection::insert_rule(
            self.require_pool()?,
            id,
            repo_id,
            pattern,
            allow_create,
            allow_update,
            allow_delete,
            enforce_admins,
        )
        .await
    }

    pub async fn update_tag_protection_rule(
        &self,
        repo_id: &str,
        rule_id: &str,
        pattern: &str,
        allow_create: bool,
        allow_update: bool,
        allow_delete: bool,
        enforce_admins: bool,
    ) -> Result<TagProtectionRuleRow, String> {
        tag_protection::update_rule(
            self.require_pool()?,
            repo_id,
            rule_id,
            pattern,
            allow_create,
            allow_update,
            allow_delete,
            enforce_admins,
        )
        .await
    }

    pub async fn delete_tag_protection_rule(
        &self,
        repo_id: &str,
        rule_id: &str,
    ) -> Result<(), String> {
        tag_protection::delete_rule(self.require_pool()?, repo_id, rule_id).await
    }

    pub async fn list_commit_statuses(
        &self,
        repo_id: &str,
        sha: &str,
    ) -> Result<Vec<CommitStatusRow>, String> {
        branch_protection::list_statuses_for_sha(self.require_pool()?, repo_id, sha).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_commit_status(
        &self,
        id: &str,
        repo_id: &str,
        sha: &str,
        context: &str,
        state: &str,
        description: &str,
        target_url: Option<&str>,
        creator_id: Option<&str>,
    ) -> Result<CommitStatusRow, String> {
        branch_protection::upsert_status(
            self.require_pool()?,
            id,
            repo_id,
            sha,
            context,
            state,
            description,
            target_url,
            creator_id,
        )
        .await
    }

    // --- issues ---

    pub async fn allocate_next_issue_number(&self, repo_id: &str) -> Result<i64, String> {
        issues::allocate_next_number(self.require_pool()?, repo_id).await
    }

    pub async fn insert_issue(
        &self,
        id: &str,
        repo_id: &str,
        author_id: &str,
        title: &str,
        body: &str,
    ) -> Result<IssueRow, String> {
        issues::insert_issue(
            self.require_pool()?,
            id,
            repo_id,
            author_id,
            title,
            body,
        )
        .await
    }

    pub async fn find_issue_by_id(&self, id: &str) -> Result<Option<IssueRow>, String> {
        issues::find_by_id(self.require_pool()?, id).await
    }

    pub async fn find_issue_by_repo_number(
        &self,
        repo_id: &str,
        number: i64,
    ) -> Result<Option<IssueRow>, String> {
        issues::find_by_repo_number(self.require_pool()?, repo_id, number).await
    }

    /// List issues with state/author/label/assignee/text filters (D-ISS-16..18).
    /// Returns `(rows, total)`.
    pub async fn list_issues_for_repo(
        &self,
        repo_id: &str,
        filters: issues::IssueListFilters<'_>,
    ) -> Result<(Vec<IssueRow>, i64), String> {
        issues::list_for_repo(self.require_pool()?, repo_id, filters).await
    }

    /// Hard-delete; does not reclaim `#N` (D-ISS-01).
    pub async fn delete_issue(&self, id: &str) -> Result<(), String> {
        issues::delete_issue(self.require_pool()?, id).await
    }

    pub async fn insert_issue_revision(
        &self,
        id: &str,
        issue_id: &str,
        editor_id: &str,
        title: &str,
        body: &str,
    ) -> Result<IssueRevisionRow, String> {
        issues::insert_issue_revision(
            self.require_pool()?,
            id,
            issue_id,
            editor_id,
            title,
            body,
        )
        .await
    }

    pub async fn list_issue_revisions(
        &self,
        issue_id: &str,
    ) -> Result<Vec<IssueRevisionRow>, String> {
        issues::list_issue_revisions(self.require_pool()?, issue_id).await
    }

    pub async fn update_issue_content(
        &self,
        id: &str,
        title: &str,
        body: &str,
    ) -> Result<IssueRow, String> {
        issues::update_issue_content(self.require_pool()?, id, title, body).await
    }

    pub async fn close_issue(&self, id: &str, closed_by: &str) -> Result<IssueRow, String> {
        issues::close_issue(self.require_pool()?, id, closed_by).await
    }

    pub async fn reopen_issue(&self, id: &str) -> Result<IssueRow, String> {
        issues::reopen_issue(self.require_pool()?, id).await
    }

    pub async fn insert_issue_comment(
        &self,
        id: &str,
        issue_id: &str,
        author_id: &str,
        body: &str,
    ) -> Result<IssueCommentRow, String> {
        issues::insert_issue_comment(self.require_pool()?, id, issue_id, author_id, body).await
    }

    pub async fn insert_notification(
        &self,
        id: &str,
        recipient_id: &str,
        actor_id: &str,
        reason: &str,
        subject_kind: &str,
        subject_repo_id: &str,
        subject_number: i64,
        subject_title: &str,
    ) -> Result<NotificationRow, String> {
        notifications::insert_notification(
            self.require_pool()?,
            id,
            recipient_id,
            actor_id,
            reason,
            subject_kind,
            subject_repo_id,
            subject_number,
            subject_title,
        )
        .await
    }

    pub async fn find_notification_by_id(
        &self,
        id: &str,
    ) -> Result<Option<NotificationRow>, String> {
        notifications::find_notification_by_id(self.require_pool()?, id).await
    }

    pub async fn list_notifications(
        &self,
        recipient_id: &str,
        unread_only: bool,
        offset: i64,
        limit: i64,
    ) -> Result<(Vec<NotificationRow>, i64), String> {
        notifications::list_notifications(
            self.require_pool()?,
            recipient_id,
            unread_only,
            offset,
            limit,
        )
        .await
    }

    pub async fn notification_unread_count(&self, recipient_id: &str) -> Result<i64, String> {
        notifications::unread_count(self.require_pool()?, recipient_id).await
    }

    pub async fn mark_notifications_read(
        &self,
        recipient_id: &str,
        ids: &[String],
        read_at: &str,
    ) -> Result<i64, String> {
        notifications::mark_read(self.require_pool()?, recipient_id, ids, read_at).await
    }

    pub async fn mark_all_notifications_read(
        &self,
        recipient_id: &str,
        read_at: &str,
    ) -> Result<i64, String> {
        notifications::mark_all_read(self.require_pool()?, recipient_id, read_at).await
    }

    pub async fn find_issue_comment_by_id(
        &self,
        id: &str,
    ) -> Result<Option<IssueCommentRow>, String> {
        issues::find_issue_comment_by_id(self.require_pool()?, id).await
    }

    pub async fn list_issue_comments(
        &self,
        issue_id: &str,
    ) -> Result<Vec<IssueCommentRow>, String> {
        issues::list_issue_comments(self.require_pool()?, issue_id).await
    }

    pub async fn update_issue_comment_body(
        &self,
        id: &str,
        body: &str,
    ) -> Result<IssueCommentRow, String> {
        issues::update_issue_comment_body(self.require_pool()?, id, body).await
    }

    pub async fn delete_issue_comment(&self, id: &str) -> Result<(), String> {
        issues::delete_issue_comment(self.require_pool()?, id).await
    }

    pub async fn insert_comment_revision(
        &self,
        id: &str,
        comment_id: &str,
        editor_id: &str,
        body: &str,
    ) -> Result<CommentRevisionRow, String> {
        issues::insert_comment_revision(self.require_pool()?, id, comment_id, editor_id, body)
            .await
    }

    pub async fn list_comment_revisions(
        &self,
        comment_id: &str,
    ) -> Result<Vec<CommentRevisionRow>, String> {
        issues::list_comment_revisions(self.require_pool()?, comment_id).await
    }

    pub async fn insert_label(
        &self,
        id: &str,
        name: &str,
        color: &str,
        description: &str,
        org_id: Option<&str>,
        repo_id: Option<&str>,
    ) -> Result<LabelRow, String> {
        issue_labels::insert_label(
            self.require_pool()?,
            id,
            name,
            color,
            description,
            org_id,
            repo_id,
        )
        .await
    }

    pub async fn find_label_by_id(&self, id: &str) -> Result<Option<LabelRow>, String> {
        issue_labels::find_label_by_id(self.require_pool()?, id).await
    }

    pub async fn update_label(
        &self,
        id: &str,
        name: &str,
        color: &str,
        description: &str,
    ) -> Result<LabelRow, String> {
        issue_labels::update_label(self.require_pool()?, id, name, color, description).await
    }

    pub async fn delete_label(&self, id: &str) -> Result<(), String> {
        issue_labels::delete_label(self.require_pool()?, id).await
    }

    pub async fn list_labels_for_org(&self, org_id: &str) -> Result<Vec<LabelRow>, String> {
        issue_labels::list_labels_for_org(self.require_pool()?, org_id).await
    }

    pub async fn list_labels_for_repo(&self, repo_id: &str) -> Result<Vec<LabelRow>, String> {
        issue_labels::list_labels_for_repo(self.require_pool()?, repo_id).await
    }

    pub async fn list_hidden_label_ids(&self, repo_id: &str) -> Result<Vec<String>, String> {
        issue_labels::list_hidden_label_ids(self.require_pool()?, repo_id).await
    }

    pub async fn set_repo_label_hidden(
        &self,
        repo_id: &str,
        label_id: &str,
        hidden: bool,
    ) -> Result<(), String> {
        issue_labels::set_repo_label_hidden(self.require_pool()?, repo_id, label_id, hidden).await
    }

    pub async fn list_labels_for_issue(&self, issue_id: &str) -> Result<Vec<LabelRow>, String> {
        issue_labels::list_labels_for_issue(self.require_pool()?, issue_id).await
    }

    /// Batch variant — `(issue_id, label)` pairs in one `IN (...)` round trip.
    pub async fn list_labels_for_issues(
        &self,
        issue_ids: &[String],
    ) -> Result<Vec<(String, LabelRow)>, String> {
        issue_labels::list_labels_for_issues(self.require_pool()?, issue_ids).await
    }

    pub async fn list_issue_assignees(
        &self,
        issue_id: &str,
    ) -> Result<Vec<issue_labels::IssueAssigneeRow>, String> {
        issue_labels::list_issue_assignees(self.require_pool()?, issue_id).await
    }

    /// Batch variant — `(issue_id, assignee)` pairs in one `IN (...)` round trip.
    pub async fn list_assignees_for_issues(
        &self,
        issue_ids: &[String],
    ) -> Result<Vec<(String, issue_labels::IssueAssigneeRow)>, String> {
        issue_labels::list_assignees_for_issues(self.require_pool()?, issue_ids).await
    }

    pub async fn set_issue_labels(
        &self,
        issue_id: &str,
        label_ids: &[String],
    ) -> Result<(), String> {
        issue_labels::set_issue_labels(self.require_pool()?, issue_id, label_ids).await
    }

    pub async fn set_issue_assignees(
        &self,
        issue_id: &str,
        user_ids: &[String],
    ) -> Result<(), String> {
        issue_labels::set_issue_assignees(self.require_pool()?, issue_id, user_ids).await
    }

    pub async fn list_issue_reaction_groups(
        &self,
        issue_id: &str,
        viewer_user_id: Option<&str>,
    ) -> Result<Vec<issues::ReactionGroupRow>, String> {
        issues::list_issue_reaction_groups(self.require_pool()?, issue_id, viewer_user_id).await
    }

    /// Batch variant — `(issue_id, group)` pairs in one `IN (...)` round trip.
    pub async fn list_issue_reaction_groups_for_issues(
        &self,
        issue_ids: &[String],
        viewer_user_id: Option<&str>,
    ) -> Result<Vec<(String, issues::ReactionGroupRow)>, String> {
        issues::list_issue_reaction_groups_for_issues(
            self.require_pool()?,
            issue_ids,
            viewer_user_id,
        )
        .await
    }

    pub async fn list_comment_reaction_groups(
        &self,
        comment_id: &str,
        viewer_user_id: Option<&str>,
    ) -> Result<Vec<issues::ReactionGroupRow>, String> {
        issues::list_comment_reaction_groups(self.require_pool()?, comment_id, viewer_user_id)
            .await
    }

    /// Batch variant — `(comment_id, group)` pairs in one `IN (...)` round trip.
    pub async fn list_comment_reaction_groups_for_comments(
        &self,
        comment_ids: &[String],
        viewer_user_id: Option<&str>,
    ) -> Result<Vec<(String, issues::ReactionGroupRow)>, String> {
        issues::list_comment_reaction_groups_for_comments(
            self.require_pool()?,
            comment_ids,
            viewer_user_id,
        )
        .await
    }

    /// Returns `true` if the reaction is now present (inserted), `false` if removed.
    pub async fn toggle_issue_reaction(
        &self,
        issue_id: &str,
        user_id: &str,
        content: &str,
    ) -> Result<bool, String> {
        issues::toggle_issue_reaction(self.require_pool()?, issue_id, user_id, content).await
    }

    /// Returns `true` if the reaction is now present (inserted), `false` if removed.
    pub async fn toggle_comment_reaction(
        &self,
        comment_id: &str,
        user_id: &str,
        content: &str,
    ) -> Result<bool, String> {
        issues::toggle_comment_reaction(self.require_pool()?, comment_id, user_id, content).await
    }

    pub async fn insert_issue_link(
        &self,
        id: &str,
        issue_id: &str,
        kind: &str,
        target_repo_id: Option<&str>,
        target_number: Option<i64>,
        target_opaque_id: Option<&str>,
        title: Option<&str>,
        created_by: &str,
    ) -> Result<issues::IssueLinkRow, String> {
        issues::insert_issue_link(
            self.require_pool()?,
            id,
            issue_id,
            kind,
            target_repo_id,
            target_number,
            target_opaque_id,
            title,
            created_by,
        )
        .await
    }

    pub async fn find_issue_link_by_id(
        &self,
        id: &str,
    ) -> Result<Option<issues::IssueLinkRow>, String> {
        issues::find_issue_link_by_id(self.require_pool()?, id).await
    }

    pub async fn list_issue_links(
        &self,
        issue_id: &str,
    ) -> Result<Vec<issues::IssueLinkRow>, String> {
        issues::list_issue_links(self.require_pool()?, issue_id).await
    }

    pub async fn delete_issue_link(
        &self,
        issue_id: &str,
        link_id: &str,
    ) -> Result<bool, String> {
        issues::delete_issue_link(self.require_pool()?, issue_id, link_id).await
    }

    // --- users ---

    pub async fn create_user(
        &self,
        id: &str,
        email: &str,
        username: &str,
        password_hash: Option<&str>,
        display_name: &str,
        bio: &str,
        avatar_path: Option<&str>,
        role: oxidean_core::Role,
    ) -> Result<UserRow, String> {
        let pool = self.require_pool()?;
        let user = users::insert_user(
            pool,
            id,
            email,
            username,
            password_hash,
            display_name,
            bio,
            avatar_path,
            role,
        )
        .await?;
        // Mirror primary into user_emails (idempotent if backfill already ran).
        if user_emails::find_primary_for_user(pool, id).await?.is_none() {
            let email_id = format!("{id}-primary-email");
            let _ = user_emails::create(pool, &email_id, id, email, true, None).await;
        }
        Ok(user)
    }

    pub async fn find_user_by_email(&self, email: &str) -> Result<Option<UserRow>, String> {
        users::find_by_email(self.require_pool()?, email).await
    }

    pub async fn find_user_by_username(&self, username: &str) -> Result<Option<UserRow>, String> {
        users::find_by_username(self.require_pool()?, username).await
    }

    /// Username prefix autocomplete rows (no email) — ORG-01 / T-10-03.
    pub async fn list_users_by_username_prefix(
        &self,
        prefix: &str,
        limit: i64,
    ) -> Result<Vec<users::UserLookupRow>, String> {
        users::list_by_username_prefix(self.require_pool()?, prefix, limit).await
    }

    pub async fn find_user_by_id(&self, id: &str) -> Result<Option<UserRow>, String> {
        users::find_by_id(self.require_pool()?, id).await
    }

    /// Batch variant — one `IN (...)` round trip (list enrichment).
    pub async fn find_users_by_ids(&self, ids: &[String]) -> Result<Vec<UserRow>, String> {
        users::find_many_by_id(self.require_pool()?, ids).await
    }

    pub async fn update_user_profile(
        &self,
        id: &str,
        display_name: &str,
        username: &str,
        bio: &str,
        avatar_path: Option<&str>,
    ) -> Result<UserRow, String> {
        users::update_profile(
            self.require_pool()?,
            id,
            display_name,
            username,
            bio,
            avatar_path,
        )
        .await
    }

    pub async fn set_user_default_branch(
        &self,
        id: &str,
        default_branch: &str,
    ) -> Result<UserRow, String> {
        users::set_default_branch(self.require_pool()?, id, default_branch).await
    }

    pub async fn count_users(&self) -> Result<i64, String> {
        users::count_users(self.require_pool()?).await
    }

    pub async fn count_sys_admins(&self) -> Result<i64, String> {
        users::count_sys_admins(self.require_pool()?).await
    }

    pub async fn list_users_page(
        &self,
        query: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<UserRow>, i64), String> {
        users::list_page(self.require_pool()?, query, limit, offset).await
    }

    pub async fn set_user_banned_at(&self, id: &str, at: &str) -> Result<UserRow, String> {
        users::set_banned_at(self.require_pool()?, id, at).await
    }

    pub async fn clear_user_banned_at(&self, id: &str) -> Result<UserRow, String> {
        users::clear_banned_at(self.require_pool()?, id).await
    }

    pub async fn set_user_role(&self, id: &str, role: &str) -> Result<UserRow, String> {
        users::set_role(self.require_pool()?, id, role).await
    }

    pub async fn delete_user(&self, id: &str) -> Result<(), String> {
        users::delete_user(self.require_pool()?, id).await
    }

    pub async fn set_email_verified_at(
        &self,
        id: &str,
        at: &str,
    ) -> Result<UserRow, String> {
        let pool = self.require_pool()?;
        let user = users::set_email_verified_at(pool, id, at).await?;
        if let Some(primary) = user_emails::find_primary_for_user(pool, id).await? {
            let _ = user_emails::set_verified_at(pool, &primary.id, Some(at)).await;
        } else {
            let email_id = format!("{id}-primary-email");
            let _ = user_emails::create(pool, &email_id, id, &user.email, true, Some(at)).await;
        }
        Ok(user)
    }

    pub async fn clear_email_verified_at(&self, id: &str) -> Result<UserRow, String> {
        let pool = self.require_pool()?;
        let user = users::clear_email_verified_at(pool, id).await?;
        if let Some(primary) = user_emails::find_primary_for_user(pool, id).await? {
            let _ = user_emails::set_verified_at(pool, &primary.id, None).await;
        }
        Ok(user)
    }

    pub async fn set_password_hash(
        &self,
        id: &str,
        password_hash: &str,
    ) -> Result<UserRow, String> {
        users::set_password_hash(self.require_pool()?, id, password_hash).await
    }

    pub async fn update_user_email(&self, id: &str, email: &str) -> Result<UserRow, String> {
        let pool = self.require_pool()?;
        let email = email.trim().to_ascii_lowercase();
        let user = users::update_user_email(pool, id, &email).await?;
        if let Some(primary) = user_emails::find_primary_for_user(pool, id).await? {
            let _ = user_emails::update_email_address(pool, &primary.id, &email).await;
        } else {
            let email_id = format!("{id}-primary-email");
            let _ = user_emails::create(pool, &email_id, id, &email, true, None).await;
        }
        Ok(user)
    }

    pub async fn set_must_change_credentials(
        &self,
        id: &str,
        must_change: bool,
    ) -> Result<UserRow, String> {
        users::set_must_change_credentials(self.require_pool()?, id, must_change).await
    }

    pub async fn clear_must_change_credentials(&self, id: &str) -> Result<UserRow, String> {
        users::clear_must_change_credentials(self.require_pool()?, id).await
    }

    // --- email tokens ---

    pub async fn upsert_email_token(
        &self,
        id: &str,
        user_id: &str,
        purpose: &str,
        token_hash: &str,
        otp_hash: &str,
        expires_at: &str,
        issue_count: i32,
    ) -> Result<email_tokens::EmailTokenRow, String> {
        self.upsert_email_token_for_target(
            id,
            user_id,
            purpose,
            "",
            token_hash,
            otp_hash,
            expires_at,
            issue_count,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_email_token_for_target(
        &self,
        id: &str,
        user_id: &str,
        purpose: &str,
        target_email: &str,
        token_hash: &str,
        otp_hash: &str,
        expires_at: &str,
        issue_count: i32,
    ) -> Result<email_tokens::EmailTokenRow, String> {
        email_tokens::upsert_by_user_purpose(
            self.require_pool()?,
            id,
            user_id,
            purpose,
            target_email,
            token_hash,
            otp_hash,
            expires_at,
            issue_count,
        )
        .await
    }

    pub async fn find_email_token_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<email_tokens::EmailTokenRow>, String> {
        email_tokens::find_by_token_hash(self.require_pool()?, token_hash).await
    }

    pub async fn find_email_token_by_otp_hash(
        &self,
        otp_hash: &str,
    ) -> Result<Option<email_tokens::EmailTokenRow>, String> {
        email_tokens::find_by_otp_hash(self.require_pool()?, otp_hash).await
    }

    pub async fn find_email_token_by_user_purpose(
        &self,
        user_id: &str,
        purpose: &str,
    ) -> Result<Option<email_tokens::EmailTokenRow>, String> {
        self.find_email_token_by_user_purpose_target(user_id, purpose, "")
            .await
    }

    pub async fn find_email_token_by_user_purpose_target(
        &self,
        user_id: &str,
        purpose: &str,
        target_email: &str,
    ) -> Result<Option<email_tokens::EmailTokenRow>, String> {
        email_tokens::find_by_user_purpose(self.require_pool()?, user_id, purpose, target_email)
            .await
    }

    pub async fn increment_email_token_attempts(&self, id: &str) -> Result<i32, String> {
        email_tokens::increment_attempts(self.require_pool()?, id).await
    }

    pub async fn delete_email_token(&self, id: &str) -> Result<(), String> {
        email_tokens::delete(self.require_pool()?, id).await
    }

    /// Test helper: rewrite token `created_at` for rate-limit simulations.
    ///
    /// Updates every token for `(user_id, purpose)` (any `target_email`).
    pub async fn set_email_token_created_at(
        &self,
        user_id: &str,
        purpose: &str,
        created_at: &str,
    ) -> Result<(), String> {
        email_tokens::set_created_at(self.require_pool()?, user_id, purpose, created_at).await
    }

    // --- user emails ---

    pub async fn create_user_email(
        &self,
        id: &str,
        user_id: &str,
        email: &str,
        is_primary: bool,
        verified_at: Option<&str>,
    ) -> Result<UserEmailRow, String> {
        user_emails::create(
            self.require_pool()?,
            id,
            user_id,
            email,
            is_primary,
            verified_at,
        )
        .await
    }

    pub async fn find_user_email_by_id(
        &self,
        id: &str,
    ) -> Result<Option<UserEmailRow>, String> {
        user_emails::find_by_id(self.require_pool()?, id).await
    }

    pub async fn find_user_email_by_address(
        &self,
        email: &str,
    ) -> Result<Option<UserEmailRow>, String> {
        user_emails::find_by_email(self.require_pool()?, email).await
    }

    pub async fn list_user_emails(
        &self,
        user_id: &str,
    ) -> Result<Vec<UserEmailRow>, String> {
        user_emails::list_for_user(self.require_pool()?, user_id).await
    }

    pub async fn count_user_emails(&self, user_id: &str) -> Result<i64, String> {
        user_emails::count_for_user(self.require_pool()?, user_id).await
    }

    pub async fn set_user_email_verified_at(
        &self,
        id: &str,
        verified_at: Option<&str>,
    ) -> Result<UserEmailRow, String> {
        user_emails::set_verified_at(self.require_pool()?, id, verified_at).await
    }

    pub async fn set_user_email_primary(&self, id: &str) -> Result<UserEmailRow, String> {
        let pool = self.require_pool()?;
        let primary = user_emails::set_primary(pool, id).await?;
        // Mirror onto users.email / email_verified_at.
        users::update_user_email(pool, &primary.user_id, &primary.email).await?;
        if let Some(at) = primary.verified_at.as_deref() {
            users::set_email_verified_at(pool, &primary.user_id, at).await?;
        } else {
            users::clear_email_verified_at(pool, &primary.user_id).await?;
        }
        Ok(primary)
    }

    pub async fn delete_user_email(&self, id: &str) -> Result<(), String> {
        user_emails::delete(self.require_pool()?, id).await
    }

    pub async fn list_verified_emails_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<String>, String> {
        user_emails::list_verified_emails_for_user(self.require_pool()?, user_id).await
    }

    // --- sessions ---

    pub async fn create_session(
        &self,
        id: &str,
        user_id: &str,
        token_hash: &str,
        expires_at: &str,
        remember_me: bool,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<(), String> {
        sessions::create(
            self.require_pool()?,
            id,
            user_id,
            token_hash,
            expires_at,
            remember_me,
            ip_address,
            user_agent,
        )
        .await
    }

    pub async fn find_session_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<sessions::SessionRow>, String> {
        sessions::find_by_token_hash(self.require_pool()?, token_hash).await
    }

    pub async fn touch_session(
        &self,
        id: &str,
        expires_at: &str,
        last_seen_at: &str,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<(), String> {
        sessions::touch(
            self.require_pool()?,
            id,
            expires_at,
            last_seen_at,
            ip_address,
            user_agent,
        )
        .await
    }

    /// All sessions for a user (admin view — `token_hash` must never be exposed).
    pub async fn list_sessions_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<sessions::SessionRow>, String> {
        sessions::list_for_user(self.require_pool()?, user_id).await
    }

    pub async fn delete_session(&self, id: &str) -> Result<(), String> {
        sessions::delete(self.require_pool()?, id).await
    }

    pub async fn delete_sessions_for_user(&self, user_id: &str) -> Result<u64, String> {
        sessions::delete_all_for_user(self.require_pool()?, user_id).await
    }

    // --- personal access tokens ---

    #[allow(clippy::too_many_arguments)]
    pub async fn create_pat(
        &self,
        id: &str,
        user_id: &str,
        kind: &str,
        name: &str,
        token_prefix: &str,
        token_hash: &str,
        scopes_json: Option<&str>,
        contents_perm: Option<&str>,
        repo_access: Option<&str>,
        expires_at: Option<&str>,
        repository_ids: &[String],
    ) -> Result<(), String> {
        pats::create(
            self.require_pool()?,
            id,
            user_id,
            kind,
            name,
            token_prefix,
            token_hash,
            scopes_json,
            contents_perm,
            repo_access,
            expires_at,
            repository_ids,
        )
        .await
    }

    pub async fn find_pat_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<pats::PatRow>, String> {
        pats::find_by_token_hash(self.require_pool()?, token_hash).await
    }

    pub async fn list_pats_for_user(&self, user_id: &str) -> Result<Vec<pats::PatRow>, String> {
        pats::list_for_user(self.require_pool()?, user_id).await
    }

    pub async fn revoke_pat(&self, id: &str, revoked_at: &str) -> Result<(), String> {
        pats::revoke(self.require_pool()?, id, revoked_at).await
    }

    pub async fn touch_pat_last_used(
        &self,
        id: &str,
        last_used_at: &str,
        last_used_ip: Option<&str>,
    ) -> Result<(), String> {
        pats::touch_last_used(self.require_pool()?, id, last_used_at, last_used_ip).await
    }

    // --- packages registry ---

    pub async fn upsert_package_blob(&self, digest: &str, size_bytes: i64) -> Result<(), String> {
        packages::upsert_blob(self.require_pool()?, digest, size_bytes).await
    }

    pub async fn adjust_package_blob_refcount(
        &self,
        digest: &str,
        delta: i64,
    ) -> Result<i64, String> {
        packages::adjust_blob_refcount(self.require_pool()?, digest, delta).await
    }

    pub async fn find_package(
        &self,
        owner_type: &str,
        owner_id: &str,
        name: &str,
        format: &str,
    ) -> Result<Option<packages::PackageRow>, String> {
        packages::find_package(self.require_pool()?, owner_type, owner_id, name, format).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_package(
        &self,
        id: &str,
        owner_type: &str,
        owner_id: &str,
        name: &str,
        format: &str,
        visibility: &str,
        repository_id: Option<&str>,
        description: &str,
    ) -> Result<(), String> {
        packages::insert_package(
            self.require_pool()?,
            id,
            owner_type,
            owner_id,
            name,
            format,
            visibility,
            repository_id,
            description,
        )
        .await
    }

    pub async fn find_package_version(
        &self,
        package_id: &str,
        version: &str,
    ) -> Result<Option<packages::PackageVersionRow>, String> {
        packages::find_version(self.require_pool()?, package_id, version).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_package_version(
        &self,
        id: &str,
        package_id: &str,
        version: &str,
        digest: Option<&str>,
        metadata_json: &str,
        published_by: Option<&str>,
    ) -> Result<(), String> {
        packages::insert_version(
            self.require_pool()?,
            id,
            package_id,
            version,
            digest,
            metadata_json,
            published_by,
        )
        .await
    }

    pub async fn add_package_blob_ref(
        &self,
        version_id: &str,
        digest: &str,
        role: &str,
    ) -> Result<(), String> {
        packages::add_blob_ref(self.require_pool()?, version_id, digest, role).await
    }

    pub async fn list_package_version_blob_digests(
        &self,
        version_id: &str,
    ) -> Result<Vec<String>, String> {
        packages::list_version_blob_digests(self.require_pool()?, version_id).await
    }

    pub async fn update_package_version_metadata(
        &self,
        version_id: &str,
        metadata_json: &str,
    ) -> Result<(), String> {
        packages::update_version_metadata(self.require_pool()?, version_id, metadata_json).await
    }

    pub async fn delete_package_version(&self, version_id: &str) -> Result<(), String> {
        packages::delete_version(self.require_pool()?, version_id).await
    }

    pub async fn delete_package(&self, package_id: &str) -> Result<(), String> {
        packages::delete_package(self.require_pool()?, package_id).await
    }




    pub async fn find_package_quota_override(
        &self,
        owner_type: &str,
        owner_id: &str,
    ) -> Result<Option<i64>, String> {
        packages::find_package_quota_override(self.require_pool()?, owner_type, owner_id).await
    }

    pub async fn upsert_package_quota_override(
        &self,
        owner_type: &str,
        owner_id: &str,
        max_bytes: i64,
    ) -> Result<(), String> {
        packages::upsert_package_quota_override(self.require_pool()?, owner_type, owner_id, max_bytes).await
    }

    pub async fn sum_package_blob_bytes_for_owner(
        &self,
        owner_type: &str,
        owner_id: &str,
    ) -> Result<i64, String> {
        packages::sum_package_blob_bytes_for_owner(self.require_pool()?, owner_type, owner_id).await
    }

    pub async fn list_package_usage_for_owner(
        &self,
        owner_type: &str,
        owner_id: &str,
    ) -> Result<Vec<packages::PackageUsageBreakdownRow>, String> {
        packages::list_package_usage_for_owner(self.require_pool()?, owner_type, owner_id).await
    }

    pub async fn list_unref_package_blobs(&self, grace_secs: i64) -> Result<Vec<String>, String> {
        packages::list_unref_package_blobs(self.require_pool()?, grace_secs).await
    }

    pub async fn delete_package_blob(&self, digest: &str) -> Result<(), String> {
        packages::delete_package_blob(self.require_pool()?, digest).await
    }

    // --- actions (Phase 19) ---

    pub async fn insert_action_runner(
        &self,
        id: &str,
        name: &str,
        token_hash: &str,
        labels_json: &str,
        repository_id: Option<&str>,
        ephemeral: bool,
    ) -> Result<actions::ActionRunnerRow, String> {
        actions::insert_runner(
            self.require_pool()?,
            id,
            name,
            token_hash,
            labels_json,
            repository_id,
            ephemeral,
        )
        .await
    }

    pub async fn find_action_runner_by_id(
        &self,
        id: &str,
    ) -> Result<Option<actions::ActionRunnerRow>, String> {
        actions::find_runner_by_id(self.require_pool()?, id).await
    }

    pub async fn insert_action_run(
        &self,
        id: &str,
        repository_id: &str,
        workflow_path: &str,
        workflow_name: &str,
        event: &str,
        head_sha: &str,
        head_ref: &str,
        title: &str,
        triggered_by: Option<&str>,
    ) -> Result<actions::ActionRunRow, String> {
        actions::insert_run(
            self.require_pool()?,
            id,
            repository_id,
            workflow_path,
            workflow_name,
            event,
            head_sha,
            head_ref,
            title,
            triggered_by,
        )
        .await
    }

    pub async fn find_action_run_by_id(
        &self,
        id: &str,
    ) -> Result<Option<actions::ActionRunRow>, String> {
        actions::find_run_by_id(self.require_pool()?, id).await
    }

    pub async fn insert_action_job(
        &self,
        id: &str,
        run_id: &str,
        job_key: &str,
        name: &str,
        runs_on_json: &str,
    ) -> Result<actions::ActionJobRow, String> {
        actions::insert_job(
            self.require_pool()?,
            id,
            run_id,
            job_key,
            name,
            runs_on_json,
        )
        .await
    }

    pub async fn find_action_job_by_id(
        &self,
        id: &str,
    ) -> Result<Option<actions::ActionJobRow>, String> {
        actions::find_job_by_id(self.require_pool()?, id).await
    }

    pub async fn insert_action_secret(
        &self,
        id: &str,
        repository_id: &str,
        name: &str,
        ciphertext: &str,
    ) -> Result<(), String> {
        actions::insert_secret(self.require_pool()?, id, repository_id, name, ciphertext).await
    }

    pub async fn list_action_secret_names(
        &self,
        repository_id: &str,
    ) -> Result<Vec<actions::ActionSecretMetaRow>, String> {
        actions::list_secret_names(self.require_pool()?, repository_id).await
    }

    pub async fn list_action_secret_ciphertexts(
        &self,
        repository_id: &str,
    ) -> Result<Vec<actions::ActionSecretCipherRow>, String> {
        actions::list_secret_ciphertexts(self.require_pool()?, repository_id).await
    }

    pub async fn delete_action_secret_by_name(
        &self,
        repository_id: &str,
        name: &str,
    ) -> Result<bool, String> {
        actions::delete_secret_by_name(self.require_pool()?, repository_id, name).await
    }

    pub async fn list_action_runners(&self) -> Result<Vec<actions::ActionRunnerRow>, String> {
        actions::list_runners(self.require_pool()?).await
    }

    pub async fn insert_action_runner_token(
        &self,
        id: &str,
        token_hash: &str,
        scope_type: &str,
        scope_id: Option<&str>,
        active: bool,
    ) -> Result<(), String> {
        actions::insert_runner_token(
            self.require_pool()?,
            id,
            token_hash,
            scope_type,
            scope_id,
            active,
        )
        .await
    }

    pub async fn get_repo_actions_enabled(&self, repo_id: &str) -> Result<bool, String> {
        actions::get_actions_enabled(self.require_pool()?, repo_id).await
    }

    pub async fn set_repo_actions_enabled(&self, repo_id: &str, enabled: bool) -> Result<(), String> {
        actions::set_actions_enabled(self.require_pool()?, repo_id, enabled).await
    }

    pub async fn consume_action_runner_registration_token(
        &self,
        token_hash: &str,
    ) -> Result<bool, String> {
        actions::consume_registration_token(self.require_pool()?, token_hash).await
    }

    pub async fn find_action_runner_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<actions::ActionRunnerRow>, String> {
        actions::find_runner_by_token_hash(self.require_pool()?, token_hash).await
    }

    pub async fn claim_queued_action_job_for_labels(
        &self,
        runner_id: &str,
        labels: &[String],
    ) -> Result<Option<actions::ActionJobRow>, String> {
        actions::claim_queued_job_for_labels(self.require_pool()?, runner_id, labels).await
    }

    pub async fn list_action_runs_for_repo(
        &self,
        repository_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<actions::ActionRunRow>, String> {
        actions::list_runs_for_repo(self.require_pool()?, repository_id, limit, offset).await
    }

    pub async fn count_action_runs_for_repo(&self, repository_id: &str) -> Result<i64, String> {
        actions::count_runs_for_repo(self.require_pool()?, repository_id).await
    }

    pub async fn requeue_action_run(&self, run_id: &str) -> Result<(), String> {
        actions::requeue_run(self.require_pool()?, run_id).await
    }

    pub async fn cancel_action_run(&self, run_id: &str) -> Result<(), String> {
        actions::cancel_run(self.require_pool()?, run_id).await
    }

    pub async fn list_action_jobs_for_run(
        &self,
        run_id: &str,
    ) -> Result<Vec<actions::ActionJobRow>, String> {
        actions::list_jobs_for_run(self.require_pool()?, run_id).await
    }

    pub async fn update_action_job_status(&self, job_id: &str, status: &str) -> Result<(), String> {
        actions::update_job_status(self.require_pool()?, job_id, status).await
    }

    pub async fn recompute_action_run_status(&self, run_id: &str) -> Result<(), String> {
        actions::recompute_run_status(self.require_pool()?, run_id).await
    }

    pub async fn touch_action_runner_online(&self, runner_id: &str) -> Result<(), String> {
        actions::touch_runner_online(self.require_pool()?, runner_id).await
    }

    pub async fn update_action_runner_labels(
        &self,
        runner_id: &str,
        labels_json: &str,
    ) -> Result<(), String> {
        actions::update_runner_labels(self.require_pool()?, runner_id, labels_json).await
    }

    pub async fn wipe_actions_domain(&self) -> Result<(), String> {
        actions::wipe_actions_domain(self.require_pool()?).await
    }

    pub async fn find_package_by_id(&self, id: &str) -> Result<Option<packages::PackageRow>, String> {
        packages::find_package_by_id(self.require_pool()?, id).await
    }

    pub async fn list_packages_by_repository(
        &self,
        repository_id: &str,
    ) -> Result<Vec<packages::PackageRow>, String> {
        packages::list_packages_by_repository(self.require_pool()?, repository_id).await
    }

    pub async fn list_packages_by_owner(
        &self,
        owner_type: &str,
        owner_id: &str,
    ) -> Result<Vec<packages::PackageRow>, String> {
        packages::list_packages_by_owner(self.require_pool()?, owner_type, owner_id).await
    }

    pub async fn update_package_description(
        &self,
        id: &str,
        description: &str,
    ) -> Result<(), String> {
        packages::update_package_description(self.require_pool()?, id, description).await
    }

    pub async fn list_package_versions(
        &self,
        package_id: &str,
    ) -> Result<Vec<packages::PackageVersionRow>, String> {
        packages::list_versions_for_package(self.require_pool()?, package_id).await
    }

    // --- ssh public keys ---

    #[allow(clippy::too_many_arguments)]
    pub async fn create_ssh_key(
        &self,
        id: &str,
        user_id: &str,
        title: &str,
        public_key: &str,
        fingerprint: &str,
        key_type: &str,
        can_authenticate: bool,
        can_sign: bool,
    ) -> Result<(), String> {
        ssh_keys::create(
            self.require_pool()?,
            id,
            user_id,
            title,
            public_key,
            fingerprint,
            key_type,
            can_authenticate,
            can_sign,
        )
        .await
    }

    pub async fn find_ssh_key_by_fingerprint(
        &self,
        fingerprint: &str,
    ) -> Result<Option<ssh_keys::SshKeyRow>, String> {
        ssh_keys::find_by_fingerprint(self.require_pool()?, fingerprint).await
    }

    pub async fn list_ssh_keys_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<ssh_keys::SshKeyRow>, String> {
        ssh_keys::list_for_user(self.require_pool()?, user_id).await
    }

    pub async fn revoke_ssh_key(&self, id: &str) -> Result<(), String> {
        ssh_keys::revoke(self.require_pool()?, id).await
    }

    pub async fn touch_ssh_key_last_used(
        &self,
        id: &str,
        last_used_at: &str,
        last_used_ip: Option<&str>,
    ) -> Result<(), String> {
        ssh_keys::touch_last_used(self.require_pool()?, id, last_used_at, last_used_ip).await
    }

    // --- gpg public keys ---

    #[allow(clippy::too_many_arguments)]
    pub async fn create_gpg_key(
        &self,
        id: &str,
        user_id: &str,
        title: &str,
        armored_public_key: &str,
        fingerprint: &str,
        key_id: &str,
        uid_emails_json: &str,
    ) -> Result<(), String> {
        gpg_keys::create(
            self.require_pool()?,
            id,
            user_id,
            title,
            armored_public_key,
            fingerprint,
            key_id,
            uid_emails_json,
        )
        .await
    }

    pub async fn find_gpg_key_by_fingerprint(
        &self,
        fingerprint: &str,
    ) -> Result<Option<gpg_keys::GpgKeyRow>, String> {
        gpg_keys::find_by_fingerprint(self.require_pool()?, fingerprint).await
    }

    pub async fn list_gpg_keys_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<gpg_keys::GpgKeyRow>, String> {
        gpg_keys::list_for_user(self.require_pool()?, user_id).await
    }

    pub async fn revoke_gpg_key(&self, id: &str) -> Result<(), String> {
        gpg_keys::revoke(self.require_pool()?, id).await
    }

    // --- auth identities ---

    pub async fn upsert_auth_identity(
        &self,
        id: &str,
        user_id: &str,
        provider: &str,
        provider_subject: &str,
        provider_email: Option<&str>,
    ) -> Result<auth_identities::AuthIdentityRow, String> {
        auth_identities::upsert(
            self.require_pool()?,
            id,
            user_id,
            provider,
            provider_subject,
            provider_email,
        )
        .await
    }

    pub async fn find_auth_identity(
        &self,
        provider: &str,
        provider_subject: &str,
    ) -> Result<Option<auth_identities::AuthIdentityRow>, String> {
        auth_identities::find(self.require_pool()?, provider, provider_subject).await
    }

    // --- auth settings ---

    pub async fn get_auth_settings(&self) -> Result<auth_settings::AuthSettingsRow, String> {
        auth_settings::get(self.require_pool()?).await
    }

    pub async fn update_auth_settings(
        &self,
        provider_mode: &str,
        email_provider: &str,
        from_address: Option<&str>,
        oidc_issuer: Option<&str>,
        oidc_client_id: Option<&str>,
        workos_client_id: Option<&str>,
        allow_signup: bool,
        default_visibility: &str,
    ) -> Result<auth_settings::AuthSettingsRow, String> {
        auth_settings::update(
            self.require_pool()?,
            provider_mode,
            email_provider,
            from_address,
            oidc_issuer,
            oidc_client_id,
            workos_client_id,
            allow_signup,
            default_visibility,
        )
        .await
    }

    // --- Releases / redirects (Phase 15) ---

    pub async fn insert_release(
        &self, id: &str, repo_id: &str, tag_name: &str, title: &str, body: &str,
        draft: bool, prerelease: bool, author_id: &str,
    ) -> Result<ReleaseRow, String> {
        releases::insert_release(self.require_pool()?, id, repo_id, tag_name, title, body, draft, prerelease, author_id).await
    }
    pub async fn find_release_by_id(&self, id: &str) -> Result<Option<ReleaseRow>, String> {
        releases::find_release_by_id(self.require_pool()?, id).await
    }
    pub async fn find_release_by_repo_tag(&self, repo_id: &str, tag_name: &str) -> Result<Option<ReleaseRow>, String> {
        releases::find_release_by_repo_tag(self.require_pool()?, repo_id, tag_name).await
    }
    pub async fn list_releases_for_repo(&self, repo_id: &str, include_drafts: bool) -> Result<Vec<ReleaseRow>, String> {
        releases::list_releases_for_repo(self.require_pool()?, repo_id, include_drafts).await
    }
    pub async fn update_release(&self, id: &str, title: &str, body: &str, draft: bool, prerelease: bool) -> Result<ReleaseRow, String> {
        releases::update_release(self.require_pool()?, id, title, body, draft, prerelease).await
    }
    pub async fn delete_release(&self, id: &str) -> Result<(), String> {
        releases::delete_release(self.require_pool()?, id).await
    }
    pub async fn list_assets_for_release(&self, release_id: &str) -> Result<Vec<ReleaseAssetRow>, String> {
        releases::list_assets_for_release(self.require_pool()?, release_id).await
    }
    pub async fn find_release_asset_by_id(&self, id: &str) -> Result<Option<ReleaseAssetRow>, String> {
        releases::find_asset_by_id(self.require_pool()?, id).await
    }
    pub async fn find_release_asset_by_filename(
        &self,
        release_id: &str,
        filename: &str,
    ) -> Result<Option<ReleaseAssetRow>, String> {
        releases::find_asset_by_release_filename(self.require_pool()?, release_id, filename).await
    }
    pub async fn insert_release_asset(&self, id: &str, release_id: &str, filename: &str, content_type: &str, byte_size: i64, uploader_id: &str) -> Result<ReleaseAssetRow, String> {
        releases::insert_asset(self.require_pool()?, id, release_id, filename, content_type, byte_size, uploader_id).await
    }
    pub async fn update_release_asset_bytes(&self, id: &str, content_type: &str, byte_size: i64) -> Result<ReleaseAssetRow, String> {
        releases::update_asset_bytes(self.require_pool()?, id, content_type, byte_size).await
    }
    pub async fn delete_release_asset(&self, id: &str) -> Result<(), String> {
        releases::delete_asset(self.require_pool()?, id).await
    }
    pub async fn insert_repository_redirect(&self, id: &str, old_owner_slug: &str, old_name: &str, repo_id: &str, expires_at: &str) -> Result<RedirectRow, String> {
        redirects::insert_redirect(self.require_pool()?, id, old_owner_slug, old_name, repo_id, expires_at).await
    }
    pub async fn find_repository_redirect(&self, old_owner_slug: &str, old_name: &str) -> Result<Option<RedirectRow>, String> {
        redirects::find_redirect(self.require_pool()?, old_owner_slug, old_name).await
    }
    pub async fn delete_repository_redirect(&self, old_owner_slug: &str, old_name: &str) -> Result<(), String> {
        redirects::delete_redirect(self.require_pool()?, old_owner_slug, old_name).await
    }
    pub async fn purge_expired_repository_redirects(&self, now_rfc3339: &str) -> Result<u64, String> {
        redirects::purge_expired_redirects(self.require_pool()?, now_rfc3339).await
    }

    /// Wipe tenant + auth data so the instance returns to empty-setup (`needs_setup`).
    /// Deletes repositories (cascades collaborators / PAT-repo links / issue domain
    /// tables: issues, counters, comments, revisions, labels, assignees, reactions,
    /// links), organizations (cascades members / invites / org-scoped labels), then
    /// sessions, identities, email tokens, and users; resets auth settings to
    /// local/log defaults with signup closed.
    // --- git LFS (D-LFS-02 / D-LFS-10) ---

    pub async fn get_repo_lfs_enabled(&self, repo_id: &str) -> Result<bool, String> {
        lfs::get_lfs_enabled(self.require_pool()?, repo_id).await
    }

    pub async fn set_repo_lfs_enabled(&self, repo_id: &str, enabled: bool) -> Result<(), String> {
        lfs::set_lfs_enabled(self.require_pool()?, repo_id, enabled).await
    }

    pub async fn find_lfs_object(&self, oid: &str) -> Result<Option<LfsObjectRow>, String> {
        lfs::find_lfs_object(self.require_pool()?, oid).await
    }

    pub async fn upsert_lfs_object(&self, oid: &str, size: i64) -> Result<(), String> {
        lfs::upsert_lfs_object(self.require_pool()?, oid, size).await
    }

    pub async fn link_lfs_object(&self, repository_id: &str, oid: &str) -> Result<(), String> {
        lfs::link_lfs_object(self.require_pool()?, repository_id, oid).await
    }

    pub async fn link_lfs_object_as(
        &self,
        repository_id: &str,
        oid: &str,
        uploaded_by: Option<&str>,
    ) -> Result<(), String> {
        lfs::link_lfs_object_as(self.require_pool()?, repository_id, oid, uploaded_by).await
    }

    pub async fn has_lfs_link(&self, repository_id: &str, oid: &str) -> Result<bool, String> {
        lfs::has_lfs_link(self.require_pool()?, repository_id, oid).await
    }

    pub async fn get_lfs_settings(&self) -> Result<lfs::LfsSettingsRow, String> {
        lfs::get_lfs_settings(self.require_pool()?).await
    }

    pub async fn update_lfs_settings(
        &self,
        max_object_bytes: Option<i64>,
        quota_repo_bytes: Option<i64>,
        quota_user_bytes: Option<i64>,
    ) -> Result<lfs::LfsSettingsRow, String> {
        lfs::update_lfs_settings(
            self.require_pool()?,
            max_object_bytes,
            quota_repo_bytes,
            quota_user_bytes,
        )
        .await
    }

    pub async fn lfs_repo_logical_bytes(&self, repository_id: &str) -> Result<i64, String> {
        lfs::repo_logical_bytes(self.require_pool()?, repository_id).await
    }

    pub async fn lfs_owner_logical_bytes(&self, owner_id: &str) -> Result<i64, String> {
        lfs::owner_logical_bytes(self.require_pool()?, owner_id).await
    }

    pub async fn lfs_physical_bytes(&self) -> Result<i64, String> {
        lfs::physical_bytes(self.require_pool()?).await
    }

    pub async fn list_unreferenced_lfs_oids(
        &self,
        created_before: &str,
    ) -> Result<Vec<String>, String> {
        lfs::list_unreferenced_lfs_oids(self.require_pool()?, created_before).await
    }

    pub async fn list_repo_lfs_objects(
        &self,
        repository_id: &str,
        limit: i64,
    ) -> Result<Vec<lfs::LfsLinkedObjectRow>, String> {
        lfs::list_repo_lfs_objects(self.require_pool()?, repository_id, limit).await
    }

    pub async fn repo_lfs_object_count(&self, repository_id: &str) -> Result<i64, String> {
        lfs::repo_lfs_object_count(self.require_pool()?, repository_id).await
    }

    pub async fn instance_lfs_object_count(&self) -> Result<i64, String> {
        lfs::instance_lfs_object_count(self.require_pool()?).await
    }

    pub async fn lfs_instance_logical_bytes(&self) -> Result<i64, String> {
        lfs::instance_logical_bytes(self.require_pool()?).await
    }

    pub async fn lfs_usage_by_repo(&self, limit: i64) -> Result<Vec<lfs::LfsRepoUsageRow>, String> {
        lfs::usage_by_repo(self.require_pool()?, limit).await
    }

    pub async fn lfs_usage_by_owner(
        &self,
        limit: i64,
    ) -> Result<Vec<lfs::LfsOwnerUsageRow>, String> {
        lfs::usage_by_owner(self.require_pool()?, limit).await
    }

    pub async fn delete_lfs_object(&self, oid: &str) -> Result<(), String> {
        lfs::delete_lfs_object(self.require_pool()?, oid).await
    }

    pub async fn factory_reset_instance(&self) -> Result<(), String> {
        let pool = self.require_pool()?;
        // D-ACT-19: wipe Actions domain before cascading repo deletes (instance runners/tokens).
        actions::wipe_actions_domain(pool).await?;
        match pool {
            Pool::Postgres(p) => {
                // Polymorphic repos no longer cascade from users — wipe explicitly (T-10-15).
                sqlx::query("DELETE FROM repositories")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query("DELETE FROM organizations")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                // audit_events survive user deletion via SET NULL — a factory
                // reset must wipe them explicitly.
                sqlx::query("DELETE FROM audit_events")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query("DELETE FROM auth_email_tokens")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query("DELETE FROM sessions")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query("DELETE FROM auth_identities")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query("DELETE FROM users")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query(
                    "UPDATE instance_auth_settings SET
                      provider_mode = 'local',
                      email_provider = 'log',
                      from_address = NULL,
                      oidc_issuer = NULL,
                      oidc_client_id = NULL,
                      workos_client_id = NULL,
                      allow_signup = false,
                      updated_at = now()
                     WHERE id = 1",
                )
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            }
            Pool::MySql(p) => {
                sqlx::query("DELETE FROM repositories")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query("DELETE FROM organizations")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query("DELETE FROM audit_events")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query("DELETE FROM auth_email_tokens")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query("DELETE FROM sessions")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query("DELETE FROM auth_identities")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query("DELETE FROM users")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query(
                    "UPDATE instance_auth_settings SET
                      provider_mode = 'local',
                      email_provider = 'log',
                      from_address = NULL,
                      oidc_issuer = NULL,
                      oidc_client_id = NULL,
                      workos_client_id = NULL,
                      allow_signup = 0,
                      updated_at = NOW()
                     WHERE id = 1",
                )
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            }
            Pool::Sqlite(p) => {
                // Ensure FK cascades / order are honored.
                sqlx::query("PRAGMA foreign_keys = ON")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query("DELETE FROM repositories")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query("DELETE FROM organizations")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query("DELETE FROM audit_events")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query("DELETE FROM auth_email_tokens")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query("DELETE FROM sessions")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query("DELETE FROM auth_identities")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query("DELETE FROM users")
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query(
                    "UPDATE instance_auth_settings SET
                      provider_mode = 'local',
                      email_provider = 'log',
                      from_address = NULL,
                      oidc_issuer = NULL,
                      oidc_client_id = NULL,
                      workos_client_id = NULL,
                      allow_signup = 0,
                      updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
                     WHERE id = 1",
                )
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    // --- webhooks (Phase 18) ---

    pub async fn insert_webhook(
        &self,
        id: &str,
        repository_id: &str,
        url: &str,
        secret: &str,
        active: bool,
        events_json: &str,
        name: &str,
        created_by: &str,
    ) -> Result<WebhookRow, String> {
        webhooks::insert_webhook(
            self.require_pool()?,
            id,
            repository_id,
            url,
            secret,
            active,
            events_json,
            name,
            created_by,
        )
        .await
    }

    pub async fn get_webhook(&self, id: &str) -> Result<WebhookRow, String> {
        webhooks::get_webhook(self.require_pool()?, id).await
    }

    pub async fn list_webhooks_for_repo(
        &self,
        repository_id: &str,
    ) -> Result<Vec<WebhookRow>, String> {
        webhooks::list_webhooks_for_repo(self.require_pool()?, repository_id).await
    }

    pub async fn get_mirror_by_repo(
        &self,
        repository_id: &str,
    ) -> Result<Option<RepositoryMirrorRow>, String> {
        mirrors::get_mirror_by_repo(self.require_pool()?, repository_id).await
    }

    pub async fn get_mirror_by_id(
        &self,
        id: &str,
    ) -> Result<Option<RepositoryMirrorRow>, String> {
        mirrors::get_mirror_by_id(self.require_pool()?, id).await
    }

    pub async fn list_enabled_mirrors(&self) -> Result<Vec<RepositoryMirrorRow>, String> {
        mirrors::list_enabled_mirrors(self.require_pool()?).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_mirror(
        &self,
        id: &str,
        repository_id: &str,
        remote_url: &str,
        auth_kind: &str,
        username: &str,
        secret_ciphertext: &str,
        ssh_public_key: &str,
        known_hosts: &str,
        webhook_secret_ciphertext: &str,
        poll_interval_secs: i64,
        enabled: bool,
        sync_mode: &str,
        clear_ref_snapshot: bool,
    ) -> Result<RepositoryMirrorRow, String> {
        mirrors::upsert_mirror(
            self.require_pool()?,
            id,
            repository_id,
            remote_url,
            auth_kind,
            username,
            secret_ciphertext,
            ssh_public_key,
            known_hosts,
            webhook_secret_ciphertext,
            poll_interval_secs,
            enabled,
            sync_mode,
            clear_ref_snapshot,
        )
        .await
    }

    pub async fn update_mirror_ref_snapshot(
        &self,
        mirror_id: &str,
        snapshot_json: &str,
    ) -> Result<(), String> {
        mirrors::update_mirror_ref_snapshot(self.require_pool()?, mirror_id, snapshot_json).await
    }

    pub async fn delete_mirror_by_repo(&self, repository_id: &str) -> Result<bool, String> {
        mirrors::delete_mirror_by_repo(self.require_pool()?, repository_id).await
    }

    pub async fn update_mirror_status(
        &self,
        mirror_id: &str,
        status: &str,
        last_error: &str,
        synced: bool,
    ) -> Result<(), String> {
        mirrors::update_mirror_status(self.require_pool()?, mirror_id, status, last_error, synced)
            .await
    }

    pub async fn set_mirror_webhook_secret(
        &self,
        mirror_id: &str,
        ciphertext: &str,
    ) -> Result<(), String> {
        mirrors::set_webhook_secret(self.require_pool()?, mirror_id, ciphertext).await
    }

    pub async fn upsert_mirror_ref_result(
        &self,
        id: &str,
        mirror_id: &str,
        refname: &str,
        outcome: &str,
        local_oid: &str,
        remote_oid: &str,
        detail: &str,
    ) -> Result<(), String> {
        mirrors::upsert_ref_result(
            self.require_pool()?,
            id,
            mirror_id,
            refname,
            outcome,
            local_oid,
            remote_oid,
            detail,
        )
        .await
    }

    pub async fn list_mirror_ref_results(
        &self,
        mirror_id: &str,
    ) -> Result<Vec<RepositoryMirrorRefResultRow>, String> {
        mirrors::list_ref_results(self.require_pool()?, mirror_id).await
    }

    pub async fn list_active_webhooks_for_event(
        &self,
        repository_id: &str,
        event: &str,
    ) -> Result<Vec<WebhookRow>, String> {
        webhooks::list_active_webhooks_for_event(self.require_pool()?, repository_id, event).await
    }

    pub async fn update_webhook(
        &self,
        id: &str,
        url: Option<&str>,
        secret: Option<&str>,
        active: Option<bool>,
        events_json: Option<&str>,
        name: Option<&str>,
    ) -> Result<WebhookRow, String> {
        webhooks::update_webhook(
            self.require_pool()?,
            id,
            url,
            secret,
            active,
            events_json,
            name,
        )
        .await
    }

    pub async fn delete_webhook(&self, id: &str) -> Result<(), String> {
        webhooks::delete_webhook(self.require_pool()?, id).await
    }

    pub async fn insert_repo_activity(
        &self,
        id: &str,
        repository_id: &str,
        actor_id: &str,
        push_type: &str,
        ref_name: &str,
        before_oid: &str,
        after_oid: &str,
        commits_count: i64,
        commit_message: Option<&str>,
        pr_number: Option<i64>,
    ) -> Result<(), String> {
        repo_activity::insert_activity(
            self.require_pool()?,
            repo_activity::InsertRepoActivity {
                id,
                repository_id,
                actor_id,
                push_type,
                ref_name,
                before_oid,
                after_oid,
                commits_count,
                commit_message,
                pr_number,
            },
        )
        .await
    }

    pub async fn list_repo_activity(
        &self,
        repository_id: &str,
        push_type: Option<&str>,
        since: Option<&str>,
        offset: i64,
        limit: i64,
    ) -> Result<(Vec<RepoActivityRow>, i64), String> {
        repo_activity::list_activity(
            self.require_pool()?,
            repository_id,
            push_type,
            since,
            offset,
            limit,
        )
        .await
    }

    /// Repo activity authored by a user, newest first (admin user-activity view).
    pub async fn list_repo_activity_by_actor(
        &self,
        actor_id: &str,
        limit: i64,
    ) -> Result<Vec<RepoActivityRow>, String> {
        repo_activity::list_by_actor(self.require_pool()?, actor_id, limit).await
    }

    // --- audit events ---

    pub async fn insert_audit_event(
        &self,
        id: &str,
        actor_id: Option<&str>,
        actor_username: &str,
        event_type: &str,
        target_type: Option<&str>,
        target_id: Option<&str>,
        detail: Option<&str>,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<(), String> {
        audit_events::insert(
            self.require_pool()?,
            audit_events::InsertAuditEvent {
                id,
                actor_id,
                actor_username,
                event_type,
                target_type,
                target_id,
                detail,
                ip_address,
                user_agent,
            },
        )
        .await
    }

    pub async fn list_audit_events_for_actor(
        &self,
        actor_id: &str,
        event_type: Option<&str>,
        limit: i64,
    ) -> Result<Vec<AuditEventRow>, String> {
        audit_events::list_for_actor(self.require_pool()?, actor_id, event_type, limit).await
    }

    pub async fn list_audit_event_types_for_actor(
        &self,
        actor_id: &str,
    ) -> Result<Vec<String>, String> {
        audit_events::list_event_types_for_actor(self.require_pool()?, actor_id).await
    }

    pub async fn insert_webhook_delivery(
        &self,
        id: &str,
        webhook_id: &str,
        delivery_guid: &str,
        event: &str,
        action: &str,
        payload_json: &str,
    ) -> Result<WebhookDeliveryRow, String> {
        webhooks::insert_delivery(
            self.require_pool()?,
            id,
            webhook_id,
            delivery_guid,
            event,
            action,
            payload_json,
        )
        .await
    }

    pub async fn get_webhook_delivery(&self, id: &str) -> Result<WebhookDeliveryRow, String> {
        webhooks::get_delivery(self.require_pool()?, id).await
    }

    pub async fn list_webhook_deliveries(
        &self,
        webhook_id: &str,
        limit: i64,
    ) -> Result<Vec<WebhookDeliveryRow>, String> {
        webhooks::list_deliveries_for_webhook(self.require_pool()?, webhook_id, limit).await
    }

    pub async fn insert_webhook_delivery_attempt(
        &self,
        id: &str,
        delivery_id: &str,
        attempt_number: i64,
        http_status: Option<i32>,
        error_message: Option<&str>,
        duration_ms: Option<i64>,
        response_snippet: Option<&str>,
    ) -> Result<WebhookDeliveryAttemptRow, String> {
        webhooks::insert_delivery_attempt(
            self.require_pool()?,
            id,
            delivery_id,
            attempt_number,
            http_status,
            error_message,
            duration_ms,
            response_snippet,
        )
        .await
    }

    pub async fn latest_webhook_delivery_attempt(
        &self,
        delivery_id: &str,
    ) -> Result<Option<WebhookDeliveryAttemptRow>, String> {
        webhooks::latest_attempt_for_delivery(self.require_pool()?, delivery_id).await
    }

    pub async fn mark_webhook_delivery_result(
        &self,
        delivery_id: &str,
        status: &str,
        attempt_count: i64,
        next_attempt_at: Option<&str>,
    ) -> Result<(), String> {
        webhooks::mark_delivery_result(
            self.require_pool()?,
            delivery_id,
            status,
            attempt_count,
            next_attempt_at,
        )
        .await
    }

    pub async fn list_pending_webhook_deliveries(
        &self,
        limit: i64,
    ) -> Result<Vec<WebhookDeliveryRow>, String> {
        webhooks::list_pending_deliveries(self.require_pool()?, limit).await
    }

    // --- templates (issue #18) ---

    pub async fn list_instance_template_packs(
        &self,
        enabled_only: bool,
    ) -> Result<Vec<InstanceTemplatePackRow>, String> {
        templates::list_instance_template_packs(self.require_pool()?, enabled_only).await
    }

    pub async fn get_instance_template_pack(
        &self,
        id: &str,
    ) -> Result<Option<InstanceTemplatePackRow>, String> {
        templates::get_instance_template_pack(self.require_pool()?, id).await
    }

    pub async fn get_instance_template_pack_by_slug(
        &self,
        slug: &str,
    ) -> Result<Option<InstanceTemplatePackRow>, String> {
        templates::get_instance_template_pack_by_slug(self.require_pool()?, slug).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_instance_template_pack(
        &self,
        id: &str,
        slug: &str,
        label: &str,
        group: &str,
        description: &str,
        default_gitignore: Option<&str>,
        enabled: bool,
        byte_size: i64,
        content_digest: &str,
        uploaded_by_user_id: &str,
    ) -> Result<InstanceTemplatePackRow, String> {
        templates::insert_instance_template_pack(
            self.require_pool()?,
            id,
            slug,
            label,
            group,
            description,
            default_gitignore,
            enabled,
            byte_size,
            content_digest,
            uploaded_by_user_id,
        )
        .await
    }

    pub async fn update_instance_template_pack(
        &self,
        id: &str,
        label: Option<&str>,
        group: Option<&str>,
        description: Option<&str>,
        default_gitignore: Option<Option<&str>>,
    ) -> Result<(), String> {
        templates::update_instance_template_pack(
            self.require_pool()?,
            id,
            label,
            group,
            description,
            default_gitignore,
        )
        .await
    }

    pub async fn set_instance_template_pack_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<(), String> {
        templates::set_instance_template_pack_enabled(self.require_pool()?, id, enabled).await
    }

    pub async fn delete_instance_template_pack(&self, id: &str) -> Result<(), String> {
        templates::delete_instance_template_pack(self.require_pool()?, id).await
    }

    pub async fn count_instance_template_packs_by_digest(
        &self,
        digest: &str,
    ) -> Result<i64, String> {
        templates::count_instance_template_packs_by_digest(self.require_pool()?, digest).await
    }

    pub async fn get_repo_is_template(&self, repo_id: &str) -> Result<bool, String> {
        templates::get_repo_is_template(self.require_pool()?, repo_id).await
    }

    pub async fn set_repo_is_template(&self, repo_id: &str, enabled: bool) -> Result<(), String> {
        templates::set_repo_is_template(self.require_pool()?, repo_id, enabled).await
    }

    pub async fn set_created_from_template_repo(
        &self,
        repo_id: &str,
        template_repo_id: Option<&str>,
    ) -> Result<(), String> {
        templates::set_created_from_template_repo(self.require_pool()?, repo_id, template_repo_id)
            .await
    }

    pub async fn list_template_repositories(
        &self,
        viewer_user_id: Option<&str>,
    ) -> Result<Vec<TemplateRepoListRow>, String> {
        templates::list_template_repositories(self.require_pool()?, viewer_user_id).await
    }
}
