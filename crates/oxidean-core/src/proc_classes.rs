//! Procedure classification for PAT/OAuth bearer callers — shared between the
//! API edge (`oxidean-api::pat::bearer`, which owns the db/context-dependent
//! authorization) and first-party clients (`ox` CLI coverage audits,
//! `auth status` scope-gap reporting).
//!
//! Fail-closed: unlisted procedures classify `SessionOnly` — never callable
//! with a token. Each class maps to a classic-scope requirement
//! (`classic_scopes_for`), a `public_repo` alternate (`public_repo_covers`),
//! and a fine-grained permission family (`fg_family_name`).

use crate::pat_types::ClassicPatScope;

/// Procedure class for a token caller. Variants map to scope requirements;
/// the doc comments name the classic scopes + FG family each accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatProc {
    /// Anonymous-safe procedures — callable by any valid token.
    Open,
    /// Own-identity read (`auth.me`, `user.get_profile`) — any valid token.
    Identity,

    // ---- repo-targeted domain classes ({owner,name}|repository_id input) ----
    /// Repo contents/metadata reads. Classic `repo` | `public_repo` (public
    /// targets only); FG `contents:read` or `metadata` on a covered repo.
    RepoRead,
    /// Repo contents writes + create/fork. Classic `repo` | `public_repo`;
    /// FG `contents:write` on a covered repo.
    RepoWrite,
    /// Issues read/write. Classic `repo`|`public_repo`; FG `issues` perm.
    IssuesRead,
    IssuesWrite,
    /// Pull requests read/write. Classic `repo`|`public_repo`; FG
    /// `pull_requests` perm.
    PullsRead,
    PullsWrite,
    /// Actions run/workflow reads. Classic `repo`|`public_repo`; FG
    /// `actions:read`.
    ActionsRead,
    /// Workflow dispatch/rerun/cancel/approve. Classic `workflow`|`repo`|
    /// `public_repo`; FG `actions:write`.
    ActionsWrite,
    /// Actions environments config (protection rules). Classic `repo`; FG
    /// `environments` perm.
    EnvironmentsRead,
    EnvironmentsWrite,
    /// Environment deployment records. Classic `repo`; FG `deployments`
    /// perm.
    DeploymentsRead,
    /// Creating deployments and deployment statuses (GitHub Deployments
    /// API). Classic `repo`; FG `deployments:write`.
    DeploymentsWrite,
    /// Check runs/suites and annotations. Classic `repo`; FG `checks` perm.
    ChecksRead,
    /// Creating/updating/rerequesting check runs and suites (GitHub Checks
    /// API). Classic `repo`; FG `checks:write`.
    ChecksWrite,
    /// Project boards/columns/cards. Classic `repo`; FG
    /// `repository_projects` perm.
    ProjectsRead,
    /// Board/column/card mutations (GitHub Projects API). Classic `repo`;
    /// FG `repository_projects:write`.
    ProjectsWrite,
    /// Discussion categories/threads/comments. Classic `repo`; FG
    /// `discussions` perm.
    DiscussionsRead,
    /// Discussion/category/comment mutations (GitHub Discussions surface).
    /// Classic `repo`; FG `discussions:write`.
    DiscussionsWrite,
    /// Pages site config + build history reads. Classic `repo`; FG `pages`.
    PagesRead,
    /// Pages config mutations, manual builds, and the `deploys.create`
    /// job-token publish seam (GitHub `pages` perm). Classic `repo`; FG
    /// `pages:write`.
    PagesWrite,
    /// Commit statuses. Classic `repo:status`|`repo`|`public_repo`; FG
    /// `commit_statuses` perm.
    StatusesRead,
    StatusesWrite,
    /// Actions secrets. Classic `repo`; FG `secrets` perm.
    SecretsRead,
    SecretsWrite,
    /// Releases. Classic `repo`|`public_repo`; FG `releases` perm (reads also
    /// satisfied by `contents:read`).
    ReleasesRead,
    ReleasesWrite,
    /// Repository webhooks. Classic `admin:repo_hook`|`repo`; FG `webhooks`.
    WebhooksRead,
    WebhooksWrite,
    /// Repo administration reads (collaborator/protection/mirror/deploy-key
    /// listings). Classic `repo`; FG `administration:read`.
    RepoAdminRead,
    /// Repo administration writes. Classic `repo`; FG `administration:write`.
    RepoAdminWrite,
    /// Repository invitations. Classic `repo:invite`|`repo`; FG
    /// `administration:write`.
    RepoInvite,
    /// `repo.softDelete` — classic `delete_repo` only (`repo` does NOT imply
    /// delete, GitHub parity); FG `administration:write`.
    RepoDelete,

    // ---- account-level classes (no repo target) ----
    /// `notification.list`/`unreadCount`. Classic `notifications`|`repo`
    /// (legacy umbrella); FG `notifications` account perm.
    NotificationsRead,
    /// `notification.markRead`/`markAllRead`.
    NotificationsWrite,
    /// `email.list`. Classic `user:email` (implied by `user`); FG `emails`.
    EmailRead,
    /// `email.add`/`remove`/`setPrimary`/`resendVerify`. Classic `user`;
    /// FG `emails:write`.
    EmailWrite,
    /// `user.update_profile`. Classic `user`; FG `profile:write`.
    ProfileWrite,
    /// `user.follow`/`unfollow`. Classic `user:follow`|`user`; FG `followers`.
    FollowWrite,
    /// `user.followers.list`/`user.following.list`. Classic `read:user`+; FG
    /// `followers` read.
    FollowRead,
    /// `user.listStarred`. Classic `repo`|`read:user`; FG `starring` read.
    StarringRead,
    /// `user.listWatched`. Classic `repo`|`read:user`; FG `watching` read.
    WatchingRead,
    /// `repo.star`/`unstar` — repo-targeted. Classic `repo`|`public_repo`;
    /// FG `starring:write` (account perm — no coverage needed).
    StarringWrite,
    /// `repo.watch`/`unwatch` — repo-targeted. Classic `repo`|`public_repo`;
    /// FG `watching:write`.
    WatchingWrite,
    /// `sshKey.list`. Classic `read:public_key`+implications; FG `ssh_keys`.
    SshKeyRead,
    /// `sshKey.add`/`revoke`. Classic `write:public_key`+`admin:public_key`.
    SshKeyWrite,
    /// `gpgKey.list` / `gpgKey.add`/`revoke` — `*:gpg_key` scopes.
    GpgKeyRead,
    GpgKeyWrite,
    /// Org profile/membership reads (`org.listMine`, `org.members.list`,
    /// `org.invites.list`). Classic `read:org`|`write:org`|`repo` (legacy);
    /// FG tokens are repo-bound — denied.
    OrgRead,
    /// Org creation/settings/membership/invite writes. Classic `write:org`
    /// only; FG denied.
    OrgWrite,

    /// `packages.list` — `package:read`|`package:write`; FG also requires a
    /// covered `repository_id` in the input.
    PackageRead,
    /// Package writes (reserved — no write procedures exist yet).
    PackageWrite,
    /// `packages.deleteVersion` — `package:delete` (held back from
    /// `package:write`, GitHub parity); FG denied (no repository target).
    PackageDelete,
    /// `pat.list` / `pat.revoke` — callable with any valid token.
    /// `pat.revoke` additionally enforces self-only at the handler (a token
    /// may revoke itself — `ox auth refresh` — never another token).
    PatSelfManage,
    /// Session-cookie only — never callable with a token.
    SessionOnly,
}

/// Procedure → class. Fail-closed: unlisted → `SessionOnly`.
pub fn classify_pat_proc(procedure: &str) -> PatProc {
    use PatProc::*;
    match procedure {
        // Public / anonymous-safe.
        "system.health"
        | "system.manifest"
        | "system.echo"
        | "system.db_probe"
        | "auth.provider_config"
        | "auth.bootstrap_status"
        | "user.getPublicProfile"
        | "org.get"
        | "repo.explore"
        | "invites.get" => Open,

        // Own-identity reads. `pat.self`/`pat.policy`/`pat.catalog` are
        // introspection surfaces — any valid credential may call them.
        "auth.me" | "user.get_profile" | "user.lookup" | "pat.self" | "pat.policy"
        | "pat.catalog" => Identity,

        // Contents/metadata reads (`{owner,name}` repo target where present;
        // account-scoped reads like repo.listMine have no target — classic
        // `repo` only, FG denied).
        "repo.get"
        | "repo.tree"
        | "repo.blob"
        | "repo.refs"
        | "repo.commits"
        | "repo.pathLastCommits"
        | "repo.commitCount"
        | "repo.contributors.list"
        | "repo.languages"
        | "repo.activity.list"
        | "repo.compare"
        | "repo.blame"
        | "repo.search"
        | "repo.stargazers.list"
        | "repo.watchers.list"
        | "repo.forks.list"
        | "repo.lfs.getEnabled"
        | "repo.lfs.getStatus"
        | "repo.lfs.getUsage"
        | "repo.lfs.listObjects"
        | "repo.lfs.download"
        | "repo.templates.getEnabled"
        | "repo.actions.getEnabled"
        | "repo.actions.getPullRequestTarget"
        | "repo.actions.getForkPrApproval"
        | "repo.actions.getDefaultPermissions"
        | "repo.mergeSettings.get"
        | "repo.quota.get"
        | "repo.createDefaults"
        | "repo.topicsSuggest"
        | "repo.forkStatus"
        | "repo.file.commitPolicy"
        | "repo.insights.commitActivity"
        | "repo.insights.contributors"
        | "repo.insights.forkNetwork"
        | "repo.issues.getEnabled"
        | "repo.pulls.getEnabled"
        | "repo.wiki.getEnabled"
        | "repo.projects.getEnabled"
        | "wiki.pages.list"
        | "wiki.pages.get"
        | "wiki.revisions.list"
        | "repo.templates.list"
        | "repo.listMine"
        | "repo.listByOwner"
        | "search.global" => RepoRead,

        // Contents writes + create/fork (the `repo.file.*` editor ops all
        // commit through the contents domain).
        "repo.create" | "repo.fork" | "repo.commit" | "repo.branchCreate" | "repo.branchRename"
        | "repo.branchDelete" | "repo.syncFork" | "repo.file.create" | "repo.file.delete"
        | "repo.file.mkdir" | "repo.file.rename" | "repo.file.update" | "repo.file.upload"
        | "wiki.pages.create" | "wiki.pages.update" | "wiki.pages.delete" => RepoWrite,

        "issue.get"
        | "issue.list"
        | "issue.history"
        | "issue.comments.list"
        | "issue.comments.history"
        | "issue.links.list"
        | "issue.assigneeCandidates"
        | "milestone.list"
        | "milestone.get"
        | "label.listForRepo" => IssuesRead,

        "issue.create"
        | "issue.update"
        | "issue.close"
        | "issue.reopen"
        | "issue.comments.create"
        | "issue.comments.update"
        | "issue.comments.delete"
        | "issue.labels.set"
        | "issue.assignees.set"
        | "issue.reactions.toggle"
        | "issue.links.add"
        | "issue.links.remove"
        | "issue.milestone.set"
        | "milestone.create"
        | "milestone.update"
        | "milestone.delete"
        | "label.create"
        | "label.update"
        | "label.delete" => IssuesWrite,

        "pull.get"
        | "pull.list"
        | "pull.files"
        | "pull.commits"
        | "pull.comments.list"
        | "pull.comments.history"
        | "pull.reviews.list"
        | "pull.reviewRequests.list"
        | "pull.branchStatus"
        | "merge_queue.list"
        | "merge_queue.get"
        | "merge_queue.groups" => PullsRead,

        "pull.create"
        | "pull.update"
        | "pull.close"
        | "pull.reopen"
        | "pull.comments.create"
        | "pull.comments.update"
        | "pull.comments.delete"
        | "pull.comments.resolve"
        | "pull.reviews.submit"
        | "pull.reviews.dismiss"
        | "pull.reviewRequests.add"
        | "pull.reviewRequests.remove"
        | "pull.merge"
        | "pull.updateBranch"
        | "merge_queue.enqueue"
        | "merge_queue.dequeue" => PullsWrite,

        "repo.actions.listRuns"
        | "repo.actions.getRun"
        | "repo.actions.getJob"
        | "repo.actions.getJobLog"
        | "repo.actions.jobAnnotations"
        | "repo.actions.listWorkflows"
        | "repo.actions.listArtifacts"
        | "repo.actions.listAllArtifacts"
        | "repo.actions.getArtifact"
        | "repo.actions.listCaches"
        | "repo.actions.listRunners"
        | "repo.actions.getRunner"
        | "repo.actions.workflowTemplates.list"
        | "repo.actions.workflowTemplates.get" => ActionsRead,

        "repo.actions.dispatchWorkflow"
        | "repo.actions.dispatchRepository"
        | "repo.actions.rerunRun"
        | "repo.actions.cancelRun"
        | "repo.actions.approveRun"
        | "repo.actions.approveJob"
        | "repo.actions.reviewDeployment"
        | "repo.actions.deleteRun"
        | "repo.actions.deleteRunLogs"
        | "repo.actions.setWorkflowEnabled"
        | "repo.actions.deleteArtifact"
        | "repo.actions.deleteCache"
        | "repo.actions.deleteRunner"
        | "repo.actions.createRegistrationToken" => ActionsWrite,

        // Configuration variables mirror the secrets families — GitHub
        // groups both under the actions_variables fine-grained permission.
        "repo.actions.variables.list" | "repo.actions.environments.variablesList" => SecretsRead,
        "repo.actions.variables.put"
        | "repo.actions.variables.delete"
        | "repo.actions.environments.variablesPut"
        | "repo.actions.environments.variablesDelete" => SecretsWrite,

        "repo.actions.environments.list" | "repo.actions.environments.get" => EnvironmentsRead,
        "repo.actions.environments.secretsList" | "repo.actions.environments.secretsPublicKey" => {
            SecretsRead
        }
        "repo.actions.environments.upsert" | "repo.actions.environments.delete" => {
            EnvironmentsWrite
        }
        "repo.actions.environments.secretsPut" | "repo.actions.environments.secretsDelete" => {
            SecretsWrite
        }
        "repo.actions.listDeployments" => DeploymentsRead,
        "deployment.list" | "deployment.get" | "deployment.status.list" => DeploymentsRead,
        "deployment.create" | "deployment.status.create" | "deployment.delete" => DeploymentsWrite,
        "check.suites.list"
        | "check.suites.get"
        | "check.runs.listForSuite"
        | "check.runs.listForRef"
        | "check.runs.get"
        | "check.annotations.list" => ChecksRead,
        "check.suites.create"
        | "check.suites.rerequest"
        | "check.runs.create"
        | "check.runs.update"
        | "check.runs.rerequest" => ChecksWrite,

        "projects.list" | "projects.get" | "projects.columns.list" | "projects.cards.list" => {
            ProjectsRead
        }
        "projects.create"
        | "projects.update"
        | "projects.delete"
        | "projects.columns.create"
        | "projects.columns.update"
        | "projects.columns.move"
        | "projects.columns.delete"
        | "projects.cards.create"
        | "projects.cards.update"
        | "projects.cards.move"
        | "projects.cards.convert"
        | "projects.cards.delete" => ProjectsWrite,

        "discussions.list"
        | "discussions.get"
        | "discussions.categories.list"
        | "discussions.comments.list" => DiscussionsRead,
        "discussions.create"
        | "discussions.update"
        | "discussions.delete"
        | "discussions.pin"
        | "discussions.lock"
        | "discussions.transfer"
        | "discussions.labels.set"
        | "discussions.answer.set"
        | "discussions.answer.clear"
        | "discussions.categories.create"
        | "discussions.categories.update"
        | "discussions.categories.delete"
        | "discussions.comments.create"
        | "discussions.comments.update"
        | "discussions.comments.delete" => DiscussionsWrite,

        "pages.get" | "pages.builds.list" | "pages.builds.get" => PagesRead,
        "pages.enable"
        | "pages.update"
        | "pages.disable"
        | "pages.builds.request"
        | "pages.deploys.create" => PagesWrite,

        // Drafting a workflow file is a contents write (ActionsWrite ⇒ classic
        // `repo` or `workflow`, FG `actions:write`).
        "repo.actions.workflows.create" => ActionsWrite,

        "repo.commitStatus.list" => StatusesRead,
        "repo.commitStatus.create" => StatusesWrite,

        "repo.actions.secrets.list" | "repo.actions.secrets.publicKey" => SecretsRead,
        "repo.actions.secrets.put" | "repo.actions.secrets.delete" => SecretsWrite,

        "release.list" | "release.get" => ReleasesRead,
        "release.create" | "release.update" | "release.delete" | "release.deleteAsset" => {
            ReleasesWrite
        }

        "webhook.list" | "webhook.get" | "webhook.deliveries.list" | "webhook.deliveries.get" => {
            WebhooksRead
        }
        "webhook.create" | "webhook.update" | "webhook.delete" | "webhook.ping"
        | "webhook.redeliver" => WebhooksWrite,

        "repo.collaborators.list"
        | "repo.branchProtection.list"
        | "repo.tagProtection.list"
        | "repo.mirror.get"
        | "repo.deployKey.list" => RepoAdminRead,

        "repo.updateMetadata"
        | "repo.updateVisibility"
        | "repo.rename"
        | "repo.transfer"
        | "repo.setArchived"
        | "repo.collaborators.add"
        | "repo.collaborators.update"
        | "repo.collaborators.remove"
        | "repo.branchProtection.create"
        | "repo.branchProtection.update"
        | "repo.branchProtection.delete"
        | "repo.tagProtection.create"
        | "repo.tagProtection.update"
        | "repo.tagProtection.delete"
        | "repo.issues.setEnabled"
        | "repo.pulls.setEnabled"
        | "repo.wiki.setEnabled"
        | "repo.projects.setEnabled"
        | "repo.mirror.upsert"
        | "repo.mirror.delete"
        | "repo.mirror.syncNow"
        | "repo.mirror.generateSshKey"
        | "repo.mirror.rotateWebhookSecret"
        | "repo.mirror.fetchHostKey"
        | "repo.deployKey.create"
        | "repo.deployKey.delete"
        | "repo.actions.setEnabled"
        | "repo.actions.setPullRequestTarget"
        | "repo.actions.setForkPrApproval"
        | "repo.actions.setDefaultPermissions"
        | "repo.lfs.setEnabled"
        | "repo.templates.setEnabled"
        | "repo.mergeSettings.update"
        | "repo.quota.set"
        | "issue.delete" => RepoAdminWrite,

        "repo.invites.list"
        | "repo.invites.create"
        | "repo.invites.createLink"
        | "repo.invites.revoke" => RepoInvite,

        "repo.softDelete" => RepoDelete,

        "repo.star" | "repo.unstar" => StarringWrite,
        "repo.watch" | "repo.unwatch" => WatchingWrite,

        "notification.list" | "notification.unreadCount" => NotificationsRead,
        "notification.markRead" | "notification.markAllRead" => NotificationsWrite,

        "email.list" => EmailRead,
        "email.add" | "email.remove" | "email.setPrimary" | "email.resendVerify" => EmailWrite,

        "user.update_profile" => ProfileWrite,
        "user.follow" | "user.unfollow" => FollowWrite,
        "user.followers.list" | "user.following.list" => FollowRead,
        "user.listStarred" => StarringRead,
        "user.listWatched" => WatchingRead,

        "sshKey.list" => SshKeyRead,
        "sshKey.add" | "sshKey.revoke" => SshKeyWrite,
        "gpgKey.list" => GpgKeyRead,
        "gpgKey.add" | "gpgKey.revoke" => GpgKeyWrite,

        "label.listForOrg" | "org.listMine" | "org.members.list" | "org.invites.list" => OrgRead,
        // Org Actions secrets — classified with the secrets families like
        // their repo-level siblings (FG `secrets` perm).
        "org.actions.secrets.list"
        | "org.actions.secrets.listRepos"
        | "org.actions.secrets.publicKey" => SecretsRead,
        "org.actions.secrets.put"
        | "org.actions.secrets.delete"
        | "org.actions.secrets.setRepos" => SecretsWrite,
        "org.actions.variables.list" | "org.actions.variables.listRepos" => SecretsRead,
        "org.actions.variables.put"
        | "org.actions.variables.delete"
        | "org.actions.variables.setRepos" => SecretsWrite,
        "org.create"
        | "org.updateSettings"
        | "org.members.add"
        | "org.members.remove"
        | "org.members.updateRole"
        | "org.invites.create"
        | "org.invites.createLink"
        | "org.invites.revoke" => OrgWrite,

        "packages.list" => PackageRead,
        "packages.deleteVersion" => PackageDelete,

        // Narrow self-management: a token may list its owner's tokens and
        // revoke itself (handler enforces `id == self`). `pat.create*` stays
        // SessionOnly — a token must never mint tokens.
        "pat.list" | "pat.revoke" => PatSelfManage,

        // Agentic VCS protocol (contract C1): read tier — the `vcs.*` name set
        // is closed (BR1.2); anything unlisted stays SessionOnly below.
        "vcs.status" | "vcs.changeset_get" | "vcs.log" | "vcs.diff" | "vcs.tree"
        | "vcs.file_get" | "vcs.blame" | "vcs.path_history" | "vcs.merge_base" | "vcs.ref_list" => {
            RepoRead
        }

        // Write tier.
        "vcs.changeset_create"
        | "vcs.merge"
        | "vcs.rebase"
        | "vcs.ref_update"
        | "vcs.git_import"
        | "vcs.git_export" => RepoWrite,

        // Repo flag toggle — the contract's "admin" class is RepoAdminWrite.
        "vcs.repo_enable" => RepoAdminWrite,

        // Object-route aliases (contract C2): classified so the `/api/vcs/*`
        // routes can authorize token callers — never listed in PROCEDURES.
        "vcs.object_get" => RepoRead,
        "vcs.object_put" => RepoWrite,

        // Everything else is session-only: auth lifecycle (`auth.login`,
        // `auth.logout`, verify/reset/bootstrap…), PAT minting
        // (`pat.createClassic`/`createFineGrained`), `admin.*` and
        // `packages.admin*`, invite accepts, OAuth app + device-grant
        // procedures. Unknown procedures fall here too — fail closed.
        _ => SessionOnly,
    }
}

/// Whether `public_repo` (held instead of `repo`) may satisfy `class` —
/// public-repo collaboration only, never the admin/credential surface.
pub fn public_repo_covers(class: PatProc) -> bool {
    use PatProc::*;
    matches!(
        class,
        RepoRead
            | RepoWrite
            | IssuesRead
            | IssuesWrite
            | PullsRead
            | PullsWrite
            | ActionsRead
            | ActionsWrite
            | StatusesRead
            | StatusesWrite
            | ReleasesRead
            | ReleasesWrite
            | ProjectsRead
            | ProjectsWrite
            | DiscussionsRead
            | DiscussionsWrite
            | StarringWrite
            | WatchingWrite
    )
}

/// Classic scopes that satisfy `class` when `public_repo` doesn't apply.
/// Account-level and admin classes are listed here; repo-domain classes get
/// `repo` plus their narrower alternatives.
pub fn classic_scopes_for(class: PatProc) -> &'static [ClassicPatScope] {
    use ClassicPatScope as S;
    use PatProc::*;
    match class {
        // `repo` umbrella classes (narrower scopes add on below).
        RepoRead | RepoWrite | IssuesRead | IssuesWrite | PullsRead | PullsWrite | ActionsRead
        | EnvironmentsRead | EnvironmentsWrite | DeploymentsRead | DeploymentsWrite
        | ChecksRead | ChecksWrite | SecretsRead | SecretsWrite | ReleasesRead | ReleasesWrite
        | ProjectsRead | ProjectsWrite | DiscussionsRead | DiscussionsWrite | PagesRead
        | PagesWrite | RepoAdminRead | RepoAdminWrite => &[S::Repo],
        ActionsWrite => &[S::Repo, S::Workflow],
        StatusesRead | StatusesWrite => &[S::Repo, S::RepoStatus],
        WebhooksRead | WebhooksWrite => &[S::Repo, S::AdminRepoHook],
        RepoInvite => &[S::Repo, S::RepoInvite],
        RepoDelete => &[S::DeleteRepo],
        StarringRead | WatchingRead | StarringWrite | WatchingWrite => &[S::Repo, S::ReadUser],
        NotificationsRead | NotificationsWrite => &[S::Repo, S::Notifications],
        EmailRead => &[S::UserEmail],
        EmailWrite | ProfileWrite => &[S::User],
        FollowWrite => &[S::UserFollow],
        FollowRead => &[S::ReadUser],
        SshKeyRead => &[S::ReadPublicKey],
        SshKeyWrite => &[S::WritePublicKey],
        GpgKeyRead => &[S::ReadGpgKey],
        GpgKeyWrite => &[S::WriteGpgKey],
        OrgRead => &[S::ReadOrg, S::Repo],
        OrgWrite => &[S::WriteOrg],
        PackageRead => &[S::PackageRead],
        PackageWrite => &[S::PackageWrite],
        PackageDelete => &[S::PackageDelete],
        Open | Identity | PatSelfManage | SessionOnly => &[],
    }
}

/// Scope names `class` accepts, for denial messages that can name what the
/// caller is missing (`requires scope "repo" (or "workflow")`).
pub fn required_scope_names(class: PatProc) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = classic_scopes_for(class)
        .iter()
        .map(|s| s.as_str())
        .collect();
    if public_repo_covers(class) {
        names.push(ClassicPatScope::PublicRepo.as_str());
    }
    names
}

/// The FG `perms` family a class maps to — for scoped denial messages.
pub fn fg_family_name(class: PatProc) -> Option<&'static str> {
    use PatProc::*;
    Some(match class {
        RepoRead | RepoWrite => "contents",
        IssuesRead | IssuesWrite => "issues",
        PullsRead | PullsWrite => "pull_requests",
        ActionsRead | ActionsWrite => "actions",
        EnvironmentsRead | EnvironmentsWrite => "environments",
        DeploymentsRead | DeploymentsWrite => "deployments",
        ChecksRead | ChecksWrite => "checks",
        ProjectsRead | ProjectsWrite => "repository_projects",
        DiscussionsRead | DiscussionsWrite => "discussions",
        PagesRead | PagesWrite => "pages",
        StatusesRead | StatusesWrite => "commit_statuses",
        SecretsRead | SecretsWrite => "secrets",
        ReleasesRead | ReleasesWrite => "releases",
        WebhooksRead | WebhooksWrite => "webhooks",
        RepoAdminRead | RepoAdminWrite | RepoInvite | RepoDelete => "administration",
        NotificationsRead | NotificationsWrite => "notifications",
        EmailRead | EmailWrite => "emails",
        ProfileWrite => "profile",
        FollowWrite | FollowRead => "followers",
        StarringRead | StarringWrite => "starring",
        WatchingRead | WatchingWrite => "watching",
        SshKeyRead | SshKeyWrite => "ssh_keys",
        GpgKeyRead | GpgKeyWrite => "gpg_keys",
        _ => return None,
    })
}

/// Whether a classic-scope set covers `procedure` — the client-side mirror
/// of `authorize_classic`, without the public-repo target lookup (a held
/// `public_repo` counts as coverage even though private targets would still
/// be denied at call time). `SessionOnly` procedures are never covered —
/// that's the "not callable with a token" signal the CLI surfaces.
pub fn proc_covered_by_scopes(procedure: &str, held: &[ClassicPatScope]) -> bool {
    let class = classify_pat_proc(procedure);
    match class {
        PatProc::Open | PatProc::Identity | PatProc::PatSelfManage => true,
        PatProc::SessionOnly => false,
        _ => {
            classic_scopes_for(class)
                .iter()
                .any(|s| crate::classic_has_scope(held, *s))
                || (public_repo_covers(class) && held.contains(&ClassicPatScope::PublicRepo))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classic_scope_satisfies as scope_satisfies;

    #[test]
    fn session_only_classification() {
        for p in [
            "admin.users.list",
            "admin.auth.get_settings",
            "admin.actions.listRunners",
            "packages.adminUsage",
            "packages.adminSetQuota",
            "auth.login",
            "auth.logout",
            "auth.signup",
            "auth.verify",
            "pat.createClassic",
            "pat.createFineGrained",
            "invites.accept",
            "org.invites.accept",
            "oauthApp.authorize",
            "oauth.deviceInfo",
            "oauth.deviceAuthorize",
            "does.not.exist",
        ] {
            assert_eq!(classify_pat_proc(p), PatProc::SessionOnly, "{p}");
        }
    }

    #[test]
    fn open_and_identity_classification() {
        for p in [
            "system.health",
            "system.manifest",
            "system.echo",
            "system.db_probe",
            "auth.provider_config",
            "auth.bootstrap_status",
            "user.getPublicProfile",
            "org.get",
            "repo.explore",
            "invites.get",
        ] {
            assert_eq!(classify_pat_proc(p), PatProc::Open, "{p}");
        }
        assert_eq!(classify_pat_proc("auth.me"), PatProc::Identity);
        assert_eq!(classify_pat_proc("user.get_profile"), PatProc::Identity);
        assert_eq!(classify_pat_proc("user.lookup"), PatProc::Identity);
    }

    #[test]
    fn repo_domain_classification_samples() {
        assert_eq!(classify_pat_proc("repo.get"), PatProc::RepoRead);
        assert_eq!(classify_pat_proc("repo.commit"), PatProc::RepoWrite);
        assert_eq!(classify_pat_proc("issue.get"), PatProc::IssuesRead);
        assert_eq!(
            classify_pat_proc("issue.assigneeCandidates"),
            PatProc::IssuesRead
        );
        assert_eq!(classify_pat_proc("issue.create"), PatProc::IssuesWrite);
        assert_eq!(classify_pat_proc("issue.delete"), PatProc::RepoAdminWrite);
        assert_eq!(classify_pat_proc("pull.merge"), PatProc::PullsWrite);
        assert_eq!(
            classify_pat_proc("pull.reviews.submit"),
            PatProc::PullsWrite
        );
        assert_eq!(
            classify_pat_proc("repo.commitStatus.create"),
            PatProc::StatusesWrite
        );
        assert_eq!(classify_pat_proc("repo.listMine"), PatProc::RepoRead);
        assert_eq!(
            classify_pat_proc("repo.actions.dispatchWorkflow"),
            PatProc::ActionsWrite
        );
        assert_eq!(
            classify_pat_proc("repo.actions.getRun"),
            PatProc::ActionsRead
        );
        assert_eq!(
            classify_pat_proc("repo.actions.secrets.list"),
            PatProc::SecretsRead
        );
        assert_eq!(classify_pat_proc("webhook.create"), PatProc::WebhooksWrite);
        assert_eq!(
            classify_pat_proc("webhook.deliveries.list"),
            PatProc::WebhooksRead
        );
        assert_eq!(
            classify_pat_proc("repo.collaborators.add"),
            PatProc::RepoAdminWrite
        );
        assert_eq!(
            classify_pat_proc("repo.collaborators.list"),
            PatProc::RepoAdminRead
        );
        assert_eq!(
            classify_pat_proc("repo.invites.create"),
            PatProc::RepoInvite
        );
        assert_eq!(classify_pat_proc("repo.softDelete"), PatProc::RepoDelete);
        assert_eq!(classify_pat_proc("repo.star"), PatProc::StarringWrite);
        assert_eq!(classify_pat_proc("user.listStarred"), PatProc::StarringRead);
        assert_eq!(
            classify_pat_proc("notification.list"),
            PatProc::NotificationsRead
        );
        assert_eq!(
            classify_pat_proc("notification.markRead"),
            PatProc::NotificationsWrite
        );
        assert_eq!(classify_pat_proc("email.list"), PatProc::EmailRead);
        assert_eq!(classify_pat_proc("email.add"), PatProc::EmailWrite);
        assert_eq!(
            classify_pat_proc("user.update_profile"),
            PatProc::ProfileWrite
        );
        assert_eq!(classify_pat_proc("user.follow"), PatProc::FollowWrite);
        assert_eq!(classify_pat_proc("sshKey.list"), PatProc::SshKeyRead);
        assert_eq!(classify_pat_proc("sshKey.add"), PatProc::SshKeyWrite);
        assert_eq!(classify_pat_proc("gpgKey.add"), PatProc::GpgKeyWrite);
        assert_eq!(classify_pat_proc("org.members.list"), PatProc::OrgRead);
        assert_eq!(classify_pat_proc("org.create"), PatProc::OrgWrite);
        assert_eq!(classify_pat_proc("packages.list"), PatProc::PackageRead);
        assert_eq!(
            classify_pat_proc("packages.deleteVersion"),
            PatProc::PackageDelete
        );
    }

    /// The pinned 19-name `vcs.*` mapping (BR1.2/BR1.3): 10 read, 6 write, 1
    /// admin, 2 object-route aliases — a new `vcs.*` name without a tier entry
    /// fails here rather than silently inheriting access.
    #[test]
    fn vcs_classification() {
        for p in [
            "vcs.status",
            "vcs.changeset_get",
            "vcs.log",
            "vcs.diff",
            "vcs.tree",
            "vcs.file_get",
            "vcs.blame",
            "vcs.path_history",
            "vcs.merge_base",
            "vcs.ref_list",
        ] {
            assert_eq!(classify_pat_proc(p), PatProc::RepoRead, "{p}");
        }
        for p in [
            "vcs.changeset_create",
            "vcs.merge",
            "vcs.rebase",
            "vcs.ref_update",
            "vcs.git_import",
            "vcs.git_export",
        ] {
            assert_eq!(classify_pat_proc(p), PatProc::RepoWrite, "{p}");
        }
        assert_eq!(
            classify_pat_proc("vcs.repo_enable"),
            PatProc::RepoAdminWrite
        );
        // Object-route aliases classify for route authorization only.
        assert_eq!(classify_pat_proc("vcs.object_get"), PatProc::RepoRead);
        assert_eq!(classify_pat_proc("vcs.object_put"), PatProc::RepoWrite);
        // Unlisted vcs.* names fail closed.
        for p in [
            "vcs.object_delete",
            "vcs.checkout",
            "vcs.",
            "vcs.status.extra",
        ] {
            assert_eq!(classify_pat_proc(p), PatProc::SessionOnly, "{p}");
        }
    }

    #[test]
    fn classic_scope_implications() {
        use ClassicPatScope as S;
        // `user` umbrella implies the read:user/user:email/user:follow family.
        assert!(scope_satisfies(S::User, S::ReadUser));
        assert!(scope_satisfies(S::User, S::UserEmail));
        assert!(scope_satisfies(S::User, S::UserFollow));
        assert!(!scope_satisfies(S::ReadUser, S::User));
        // Key tiers: admin ⊃ write ⊃ read.
        assert!(scope_satisfies(S::AdminPublicKey, S::ReadPublicKey));
        assert!(scope_satisfies(S::AdminPublicKey, S::WritePublicKey));
        assert!(scope_satisfies(S::WritePublicKey, S::ReadPublicKey));
        assert!(!scope_satisfies(S::ReadPublicKey, S::WritePublicKey));
        assert!(scope_satisfies(S::AdminGpgKey, S::WriteGpgKey));
        // write:org ⊃ read:org; package:write ⊃ package:read.
        assert!(scope_satisfies(S::WriteOrg, S::ReadOrg));
        assert!(scope_satisfies(S::PackageWrite, S::PackageRead));
        assert!(!scope_satisfies(S::PackageRead, S::PackageWrite));
        // delete_repo and package:delete are standalone (GH parity).
        assert!(!scope_satisfies(S::Repo, S::DeleteRepo));
        assert!(!scope_satisfies(S::PackageWrite, S::PackageDelete));
    }
}
