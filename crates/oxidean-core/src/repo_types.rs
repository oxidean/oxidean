//! Repository name validation and DTOs (D-06). Separate from username rules.

use serde::{Deserialize, Serialize};

use crate::auth_types::is_reserved_username;
use crate::org_types::{CollaboratorPermission, OwnerType};

/// Repo visibility. Serialized lowercase: `public` | `private`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RepoVisibility {
    Public,
    Private,
}

impl RepoVisibility {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Private => "private",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "public" => Ok(Self::Public),
            "private" => Ok(Self::Private),
            other => Err(format!("invalid visibility: {other}")),
        }
    }
}

/// Per-user repository watch level — the notification matrix (DEBT-06).
/// Serialized snake_case: `all` | `participating` | `ignore`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WatchLevel {
    /// Notified on all repo activity (default for `repo.watch`).
    #[default]
    All,
    /// Notified only when participating or @-mentioned (stored default).
    Participating,
    /// Never notified for this repo — suppresses participation + mention rows.
    Ignore,
}

impl WatchLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Participating => "participating",
            Self::Ignore => "ignore",
        }
    }

    /// True when the level counts as an active watch subscription
    /// (`watch_count`, `viewer_is_watching`, watchers list membership).
    pub const fn is_active(self) -> bool {
        !matches!(self, Self::Ignore)
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "all" => Ok(Self::All),
            "participating" => Ok(Self::Participating),
            "ignore" => Ok(Self::Ignore),
            other => Err(format!("invalid watch level: {other}")),
        }
    }
}

/// Provenance for `/new` template picker cards (issue #18).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TemplateProvenance {
    #[default]
    Builtin,
    Instance,
    User,
}

impl TemplateProvenance {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Instance => "instance",
            Self::User => "user",
        }
    }
}

/// Create-repository input (RPC wired in 07-12; templates in 07-03 / issue #18).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateRepoRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// When omitted, API uses instance `default_visibility` (else public) — D-08.
    #[serde(default)]
    pub visibility: Option<RepoVisibility>,
    /// Stack preset pack id under `assets/stack-presets/` (omit / null = none).
    /// Mutually exclusive with `instance_pack_id` / `template_repo_id`.
    #[serde(default)]
    pub stack_id: Option<String>,
    /// Instance-admin template pack id (omit / null = none). Mutually exclusive
    /// with `stack_id` / `template_repo_id`.
    #[serde(default)]
    pub instance_pack_id: Option<String>,
    /// Source template repository id (omit / null = none). Mutually exclusive
    /// with `stack_id` / `instance_pack_id`.
    #[serde(default)]
    pub template_repo_id: Option<String>,
    /// SPDX license id or omit / null / `"none"` for no LICENSE file.
    #[serde(default)]
    pub license_id: Option<String>,
    /// Gitignore catalog id under `assets/gitignore/` (omit / null / `"none"` = none).
    #[serde(default)]
    pub gitignore_id: Option<String>,
    /// Optional owner slug (username or org). Omit → session user (A5 / D-ORG-01).
    #[serde(default)]
    pub owner: Option<String>,
}

/// Public create-form defaults + catalog metadata (D-02–D-04, D-08, issue #18).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoCreateDefaults {
    pub default_visibility: RepoVisibility,
    pub stacks: Vec<RepoTemplateOption>,
    pub gitignores: Vec<RepoTemplateOption>,
}

/// Catalog option for stack / gitignore / template pickers (modal cards on `/new`).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoTemplateOption {
    pub id: String,
    pub label: String,
    pub group: String,
    /// Short human description shown in the picker modal.
    pub description: String,
    /// When set on a stack pack, `/new` auto-selects this gitignore and create
    /// seeds it unless the client sends an explicit gitignore (including `"none"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_gitignore: Option<String>,
    /// Built-in pack vs instance admin pack vs user/org template repo.
    #[serde(default)]
    pub provenance: TemplateProvenance,
    /// For user templates: `@owner/name` display. For instance: slug.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_label: Option<String>,
}

/// Parent summary when this repo is a fork (D-SOC-16).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ForkParentSummary {
    pub id: String,
    pub owner: String,
    pub name: String,
}

/// Deserialization default for per-repo unit flags (COL-13): payloads that
/// predate `issues_enabled` / `pulls_enabled` mean "enabled" — unit toggles
/// never hide data unless an admin explicitly turned the unit off.
fn repo_unit_enabled_default() -> bool {
    true
}

/// Public repository metadata returned over RPC.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoPublic {
    pub id: String,
    pub owner_id: String,
    /// Polymorphic owner: `user` | `org` (D-ORG-01).
    pub owner_type: OwnerType,
    /// Public slug label (username or org slug).
    pub owner_username: String,
    pub name: String,
    pub description: String,
    pub visibility: RepoVisibility,
    pub default_branch: String,
    pub updated_at: String,
    /// Caller has Admin capability (D-ORG-05 / settings UI).
    #[serde(default)]
    pub can_admin: bool,
    /// Caller has Write capability (D-ORG-05).
    #[serde(default)]
    pub can_write: bool,
    /// Denormalized star counter (D-SOC-02 / D-SOC-03).
    #[serde(default)]
    pub star_count: i64,
    /// Open issue count for the Issues tab badge (populated by repo.get).
    #[serde(default)]
    pub open_issue_count: i64,
    /// Open pull-request count for the Pulls tab badge (populated by repo.get).
    #[serde(default)]
    pub open_pull_count: i64,
    /// Whether the authenticated viewer has starred this repo.
    #[serde(default)]
    pub viewer_has_starred: bool,
    /// True when this repository is a fork of another.
    #[serde(default)]
    pub is_fork: bool,
    /// True when owners expose this repo as a create-from template (issue #18).
    #[serde(default)]
    pub is_template: bool,
    /// Read-only archive mode (GIT-20): browse/clone stay open; pushes and
    /// content writes (issues, pulls, releases, branches, LFS uploads) are blocked.
    #[serde(default)]
    pub archived: bool,
    /// Project homepage URL / text (issue #23).
    #[serde(default)]
    pub homepage: String,
    /// Topic slugs (issue #23).
    #[serde(default)]
    pub topics: Vec<String>,
    /// Active forks in this repo's network (issue #23).
    #[serde(default)]
    pub fork_count: i64,
    /// Watch / subscribe counter (issue #23).
    #[serde(default)]
    pub watch_count: i64,
    /// Whether the authenticated viewer is watching this repo
    /// (subscription row at a non-`ignore` level).
    #[serde(default)]
    pub viewer_is_watching: bool,
    /// Authenticated viewer's watch level — `all` | `participating` | `ignore`;
    /// absent when the viewer has no subscription row (DEBT-06).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewer_watch_level: Option<WatchLevel>,
    /// Issues unit enabled — gates `issue.*` RPCs + the Issues tab (COL-13).
    /// Populated by `repo.get`; list endpoints may report the default `true`.
    #[serde(default = "repo_unit_enabled_default")]
    pub issues_enabled: bool,
    /// Pull-requests unit enabled — gates `pull.*` RPCs + the Pulls tab (COL-13).
    /// Populated by `repo.get`; list endpoints may report the default `true`.
    #[serde(default = "repo_unit_enabled_default")]
    pub pulls_enabled: bool,
    /// Fork network root id (own id for roots) — D-SOC-14 / D-PR-01.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork_network_id: Option<String>,
    /// Immediate parent when `is_fork` (D-SOC-16).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forked_from: Option<ForkParentSummary>,
}

/// `repo.star` / `repo.unstar` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoStarRequest {
    pub owner: String,
    pub name: String,
}

/// Public stargazer row for `repo.stargazers.list` (no email). Write+ only.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoStargazerPublic {
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    /// When the user starred (ISO-8601).
    pub starred_at: String,
}

/// `repo.stargazers.list` — paginated stargazers; Write+ gated; optional username search.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoStargazersListRequest {
    pub owner: String,
    pub name: String,
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub offset: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoStargazersListResponse {
    pub stargazers: Vec<RepoStargazerPublic>,
    pub total: i64,
}

/// `repo.watch` / `repo.unwatch` input. `level` applies to `repo.watch` only
/// (`unwatch` removes the row regardless); omitted → `all` (DEBT-06).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoWatchRequest {
    pub owner: String,
    pub name: String,
    #[serde(default)]
    pub level: Option<WatchLevel>,
}

/// Public watcher row for `repo.watchers.list` (no email).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoWatcherPublic {
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    /// When the user started watching (ISO-8601).
    pub watched_at: String,
}

/// `repo.watchers.list` — paginated watchers; optional username/display_name search.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoWatchersListRequest {
    pub owner: String,
    pub name: String,
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub offset: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoWatchersListResponse {
    pub watchers: Vec<RepoWatcherPublic>,
    /// Total matching rows (after `q` filter).
    pub total: i64,
}

/// Sort keys for `repo.forks.list` (subset we can support with stored data).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RepoForksSort {
    #[default]
    Stars,
    Updated,
    Created,
}

impl RepoForksSort {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "updated" | "recently_updated" => Self::Updated,
            "created" | "recently_created" => Self::Created,
            _ => Self::Stars,
        }
    }
}

/// Public fork row for `repo.forks.list`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoForkPublic {
    pub id: String,
    pub owner_username: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub star_count: i64,
    pub fork_count: i64,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_avatar_url: Option<String>,
}

/// `repo.forks.list` — paginated forks in the network; search by owner/name.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoForksListRequest {
    pub owner: String,
    pub name: String,
    #[serde(default)]
    pub q: Option<String>,
    /// `stars` (default) | `updated` | `created`
    #[serde(default)]
    pub sort: Option<String>,
    #[serde(default)]
    pub offset: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoForksListResponse {
    pub forks: Vec<RepoForkPublic>,
    pub total: i64,
}

/// `repo.updateMetadata` — Admin updates description / homepage / topics (issue #23).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoUpdateMetadataRequest {
    pub owner: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub topics: Option<Vec<String>>,
}

/// `repo.topicsSuggest` — topic autocomplete for the About/settings chips
/// editor. Anonymous OK — topic names are public metadata.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoTopicsSuggestRequest {
    /// Prefix to match (normalized to a topic slug server-side).
    pub q: String,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoTopicSuggestion {
    pub name: String,
    /// Number of repositories linked to the topic.
    pub repo_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoTopicsSuggestResponse {
    pub topics: Vec<RepoTopicSuggestion>,
}

/// `repo.pathLastCommits` — last commit per tree entry name (issue #23).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoPathLastCommitsRequest {
    pub owner: String,
    pub name: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    #[serde(default)]
    pub path: Option<String>,
}

/// Map of entry basename → last commit that touched that path.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoPathLastCommitsResponse {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub path: String,
    pub commits: std::collections::BTreeMap<String, RepoCommitSummary>,
}

/// `repo.commitCount` — `rev-list --count` for the commits header (issue #23).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoCommitCountRequest {
    pub owner: String,
    pub name: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoCommitCountResponse {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub count: u64,
}

/// Public contributor row for About sidebar (no email).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoContributorPublic {
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    pub commit_count: i64,
}

/// `repo.contributors.list` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoContributorsListRequest {
    pub owner: String,
    pub name: String,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoContributorsListResponse {
    pub contributors: Vec<RepoContributorPublic>,
}

/// One language in the About sidebar breakdown (linguist-lite, byte-weighted).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoLanguageStat {
    pub name: String,
    /// Raw byte total for this language on the default branch.
    pub bytes: u64,
    /// Linguist-conventional hex color (`#rrggbb`), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// `repo.languages` input — default-branch language stats for About.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoLanguagesRequest {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoLanguagesResponse {
    pub languages: Vec<RepoLanguageStat>,
}

/// Actor on a repository activity feed item (public fields only).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActivityActor {
    pub login: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    /// Profile path (`/{login}`).
    pub path: String,
}

/// One push / branch / merge event for `repo.activity.list`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActivityItem {
    pub id: String,
    /// `push` | `force_push` | `pr_merge` | `branch_creation` | `branch_deletion`
    pub push_type: String,
    pub ref_name: String,
    /// Short branch/tag name when `ref_name` is under `refs/heads/` or `refs/tags/`.
    pub ref_short: String,
    pub before: String,
    pub after: String,
    pub pushed_at: String,
    pub commits_count: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_number: Option<i64>,
    pub pusher: RepoActivityActor,
}

/// `repo.activity.list` — repo activity feed (Read+).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActivityListRequest {
    pub owner: String,
    pub name: String,
    /// Optional push_type filter (`push`, `force_push`, `pr_merge`, `branch_creation`,
    /// `branch_deletion`, `branch_rename`, …).
    #[serde(default)]
    pub push_type: Option<String>,
    /// Optional lower bound as ISO-8601 UTC (`2024-01-01T00:00:00Z`).
    #[serde(default)]
    pub since: Option<String>,
    /// Convenience period: `week` | `month` | `year` | `all` (default all).
    #[serde(default)]
    pub period: Option<String>,
    #[serde(default)]
    pub offset: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActivityListResponse {
    pub items: Vec<RepoActivityItem>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

/// `repo.insights.contributors` — top committers on the default branch (GIT-26).
/// The history walk is bounded by the server's scan cap (`truncated`).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoInsightsContributorsRequest {
    pub owner: String,
    pub name: String,
    /// Max authors returned (default 30, clamp 1..=100).
    #[serde(default)]
    pub limit: Option<i64>,
}

/// One committer row in the insights contributors view.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoInsightContributor {
    /// Author name from the newest scanned commit (resolved user's display
    /// name when the email maps to an account).
    pub name: String,
    /// Author email as recorded in git history (public commit data).
    pub email: String,
    pub commit_count: i64,
    /// Oxidean username when the author email resolves to an account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    /// Oldest scanned commit by this author (for `/commit/` links).
    pub first_commit_sha: String,
    /// Committer unix timestamp of the author's oldest scanned commit.
    pub first_commit_unix: i64,
    /// Newest scanned commit by this author.
    pub last_commit_sha: String,
    /// Committer unix timestamp of the author's newest scanned commit.
    pub last_commit_unix: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoInsightsContributorsResponse {
    pub contributors: Vec<RepoInsightContributor>,
    /// Default-branch commits walked (bounded by the server cap).
    pub scanned_commits: u64,
    /// True when history is deeper than the scan cap.
    pub truncated: bool,
}

/// `repo.insights.commitActivity` — weekly commit buckets on the default
/// branch, GitHub `/stats/commit_activity` shape (GIT-26).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoInsightsCommitActivityRequest {
    pub owner: String,
    pub name: String,
    /// Trailing ISO weeks to include, ending with the current week
    /// (default 52, clamp 1..=104).
    #[serde(default)]
    pub weeks: Option<i64>,
}

/// One ISO week bucket. `week` is the Sunday 00:00:00 UTC epoch; `days` is
/// commits per weekday with index 0 = Sunday .. 6 = Saturday.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoCommitActivityWeek {
    pub week: i64,
    pub days: [i64; 7],
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoInsightsCommitActivityResponse {
    /// Oldest-first week buckets; the last bucket is the in-progress week.
    pub weeks: Vec<RepoCommitActivityWeek>,
    /// Total commits counted inside the window.
    pub total: i64,
    /// Default-branch commits walked (bounded by the server cap).
    pub scanned_commits: u64,
    /// True when the walk hit the scan cap before covering the window.
    pub truncated: bool,
}

/// `repo.insights.forkNetwork` — fork tree across the repo's network (GIT-26).
/// Public members only (plus the queried repo itself); private forks of other
/// users are not listed.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoInsightsForkNetworkRequest {
    pub owner: String,
    pub name: String,
    /// Max member nodes returned (default 100, clamp 1..=500).
    #[serde(default)]
    pub limit: Option<i64>,
}

/// One node of the fork-network tree.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoForkNetworkNode {
    pub id: String,
    pub owner: String,
    pub name: String,
    /// Immediate parent (`forked_from`) owner/name when the row exists.
    /// Absent on the network root and when the parent row is gone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_name: Option<String>,
    pub star_count: i64,
    /// Direct forks of this node within the network.
    pub fork_count: i64,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_avatar_url: Option<String>,
    /// True on the network root (id == `fork_network_id`).
    pub is_root: bool,
    /// True on the queried repository.
    pub is_current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoInsightsForkNetworkResponse {
    /// Members oldest-first so clients can build the tree in one pass
    /// (parents precede children apart from the queried repo edge case).
    pub nodes: Vec<RepoForkNetworkNode>,
    /// Total matching members before `limit` was applied.
    pub total: i64,
    /// True when `limit` clipped the member list.
    pub truncated: bool,
}

/// `user.listStarred` — caller's starred repos (D-SOC-03).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ListStarredRequest {
    #[serde(default)]
    pub offset: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
}

/// `repo.explore` — public discovery listing (D-SOC-09…11).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoExploreRequest {
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub offset: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
}

/// `user.listWatched` — caller's repo subscriptions at any watch level
/// (`all` | `participating` | `ignore`), newest first (DEBT-06).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ListWatchedRequest {
    #[serde(default)]
    pub offset: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
}

/// `user.getPublicProfile` — public profile by username (D-SOC-06 / D-SOC-08).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetPublicProfileRequest {
    pub username: String,
}

/// Public profile DTO — never includes email (D-SOC-06).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PublicUserProfile {
    pub username: String,
    pub display_name: String,
    pub bio: String,
    pub avatar_url: Option<String>,
    /// Accounts following this user (DEBT-06).
    #[serde(default)]
    pub follower_count: i64,
    /// Accounts this user follows (DEBT-06).
    #[serde(default)]
    pub following_count: i64,
    /// Whether the authenticated viewer follows this user (DEBT-06).
    #[serde(default)]
    pub viewer_is_following: bool,
}

/// `user.follow` / `user.unfollow` input (DEBT-06).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UserFollowRequest {
    pub username: String,
}

/// `user.followers.list` / `user.following.list` input — paginated, optional
/// username/display_name filter (DEBT-06).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UserFollowListRequest {
    pub username: String,
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub offset: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
}

/// Public follower/following row — no email (DEBT-06).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UserFollowPublic {
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    /// When the follow edge was created (ISO-8601).
    pub followed_at: String,
}

/// Shared page shape for `user.followers.list` and `user.following.list`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UserFollowListResponse {
    pub users: Vec<UserFollowPublic>,
    /// Total matching rows (after `q` filter).
    pub total: i64,
}

/// `repo.listMine` — caller's non-deleted repos, recently updated first (GIT-01 / D-13).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoListMineResponse {
    pub repos: Vec<RepoPublic>,
}

/// `repo.listByOwner` — repos under a user/org slug the caller can read (D-ORG-06 overview).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoListByOwnerRequest {
    pub owner: String,
}

/// `repo.get` / `repo.refs` input — owner + name (GIT-05).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoGetRequest {
    pub owner: String,
    pub name: String,
}

/// `repo.tree` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoTreeRequest {
    pub owner: String,
    pub name: String,
    /// Branch/tag/sha. Serialized as `ref` for API ergonomics.
    #[serde(rename = "ref")]
    pub ref_name: String,
    #[serde(default)]
    pub path: Option<String>,
}

/// One `ls-tree` entry for RPC.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoTreeEntry {
    pub mode: String,
    pub kind: String,
    pub oid: String,
    pub name: String,
}

/// `repo.tree` response — empty repo sets `empty: true` without 500.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoTreeResponse {
    pub empty: bool,
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub path: String,
    pub entries: Vec<RepoTreeEntry>,
}

/// `repo.blob` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoBlobRequest {
    pub owner: String,
    pub name: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub path: String,
}

/// Soft-capped blob payload for UI (D-20).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoBlobResponse {
    pub path: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub size: u64,
    pub truncated: bool,
    pub is_binary: bool,
    pub encoding: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub soft_max_bytes: u64,
}

/// One ref from `repo.refs`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoRefEntry {
    pub name: String,
    pub oid: String,
    /// Tip commit author name when the backend could resolve it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tip_author_name: Option<String>,
    /// Tip commit committer date (ISO-8601) when resolvable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tip_committed_at: Option<String>,
}

/// `repo.refs` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoRefsResponse {
    pub refs: Vec<RepoRefEntry>,
}

/// `repo.commits` (log) input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoCommitsRequest {
    pub owner: String,
    pub name: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    #[serde(default)]
    pub skip: u32,
    #[serde(default = "default_commits_limit")]
    pub limit: u32,
}

fn default_commits_limit() -> u32 {
    30
}

/// `repo.search` type discriminator (D-SRCH-14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RepoSearchType {
    Code,
    Commits,
    Issues,
    Pulls,
}

impl RepoSearchType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Commits => "commits",
            Self::Issues => "issues",
            Self::Pulls => "pulls",
        }
    }
}

fn default_search_limit() -> u32 {
    30
}

/// `repo.search` input (GIT-18 / D-SRCH-14).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoSearchRequest {
    pub owner: String,
    pub name: String,
    #[serde(rename = "type")]
    pub search_type: RepoSearchType,
    #[serde(default)]
    pub q: String,
    /// Optional tree-ish; omit / empty → default branch (D-SRCH-06).
    #[serde(default, rename = "ref")]
    pub ref_name: Option<String>,
    #[serde(default)]
    pub offset: u32,
    #[serde(default = "default_search_limit")]
    pub limit: u32,
}

/// One hit in `repo.search` results (tagged by `kind`).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RepoSearchHit {
    Code {
        path: String,
        line: u32,
        content: String,
    },
    Commit {
        sha: String,
        short_sha: String,
        subject: String,
        author_name: String,
        authored_at: String,
    },
    Issue {
        number: i64,
        title: String,
        state: String,
    },
    Pull {
        number: i64,
        title: String,
        state: String,
    },
}

/// `repo.search` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoSearchResponse {
    #[serde(rename = "type")]
    pub search_type: RepoSearchType,
    pub q: String,
    pub hits: Vec<RepoSearchHit>,
    /// Soft cap / timeout truncated (D-SRCH-08).
    #[serde(default)]
    pub truncated: bool,
    pub offset: u32,
    pub limit: u32,
}

/// One commit row for history list.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoCommitSummary {
    pub sha: String,
    pub short_sha: String,
    pub subject: String,
    pub author_name: String,
    pub author_email: String,
    pub authored_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_avatar_url: Option<String>,
    /// `none` | `valid` | `invalid` | `unknown`
    #[serde(default = "default_signature_none")]
    pub signature_status: String,
    /// `ssh` | `gpg` | empty
    #[serde(default)]
    pub signature_kind: String,
}

fn default_signature_none() -> String {
    "none".into()
}

/// `repo.commits` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoCommitsResponse {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub commits: Vec<RepoCommitSummary>,
    pub skip: u32,
    pub limit: u32,
}

/// `repo.commit` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoCommitRequest {
    pub owner: String,
    pub name: String,
    pub sha: String,
}

/// One file in a commit/compare diff.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoDiffFile {
    pub path: String,
    pub status: String,
    pub patch: String,
}

/// `repo.commit` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoCommitResponse {
    pub sha: String,
    pub short_sha: String,
    pub subject: String,
    pub body: String,
    pub author_name: String,
    pub author_email: String,
    pub authored_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_avatar_url: Option<String>,
    #[serde(default = "default_signature_none")]
    pub signature_status: String,
    #[serde(default)]
    pub signature_kind: String,
    pub parents: Vec<String>,
    pub files: Vec<RepoDiffFile>,
    pub truncated: bool,
}

/// `repo.compare` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoCompareRequest {
    pub owner: String,
    pub name: String,
    pub base: String,
    pub head: String,
}

/// `repo.compare` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoCompareResponse {
    pub base: String,
    pub head: String,
    pub empty: bool,
    pub truncated: bool,
    pub files: Vec<RepoDiffFile>,
}

/// `repo.forkStatus` input (GIT-24).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoForkStatusRequest {
    pub owner: String,
    pub name: String,
    /// Fork branch to compare (default → the repository's default branch);
    /// compared against the same-named branch on the upstream repository.
    #[serde(default)]
    pub branch: Option<String>,
}

/// `repo.forkStatus` response — fork branch vs upstream branch freshness.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoForkStatusResponse {
    /// Fork-side branch name.
    pub branch: String,
    pub upstream_owner: String,
    pub upstream_name: String,
    /// Upstream branch compared against (same name as `branch`).
    pub upstream_branch: String,
    /// Commits on the fork branch that upstream lacks.
    pub ahead_count: i64,
    /// Commits on the upstream branch that the fork branch lacks.
    pub behind_count: i64,
    /// `up_to_date` | `behind` | `diverged` (`ahead` reports as `up_to_date`).
    pub status: String,
}

/// `repo.syncFork` input (GIT-24).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoSyncForkRequest {
    pub owner: String,
    pub name: String,
    /// Branch to sync (default → the repository's default branch); synced
    /// from the same-named branch on the upstream repository.
    #[serde(default)]
    pub branch: Option<String>,
}

/// `repo.syncFork` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoSyncForkResponse {
    /// `up_to_date` | `fast_forwarded` | `merged`.
    pub status: String,
    pub branch: String,
    pub upstream_owner: String,
    pub upstream_name: String,
    pub upstream_branch: String,
    /// Fork branch tip before the operation.
    pub before_sha: String,
    /// Fork branch tip after the operation.
    pub after_sha: String,
    /// Merge commit SHA when `status == "merged"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_commit_sha: Option<String>,
}

/// `repo.blame` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoBlameRequest {
    pub owner: String,
    pub name: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub path: String,
}

/// One blame line.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoBlameLine {
    pub sha: String,
    pub author_name: String,
    #[serde(default)]
    pub author_email: String,
    pub authored_at: String,
    pub line_number: u32,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_avatar_url: Option<String>,
}

/// `repo.blame` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoBlameResponse {
    pub path: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub lines: Vec<RepoBlameLine>,
    pub truncated: bool,
}

/// `repo.branchCreate` input (GIT-06).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoBranchCreateRequest {
    pub owner: String,
    pub name: String,
    pub branch: String,
    /// Start point (branch/tag/sha). Empty/omit → repository default branch.
    #[serde(default)]
    pub start: Option<String>,
}

/// `repo.branchRename` input (GIT-06).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoBranchRenameRequest {
    pub owner: String,
    pub name: String,
    pub from: String,
    pub to: String,
}

/// `repo.branchDelete` input (GIT-06).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoBranchDeleteRequest {
    pub owner: String,
    pub name: String,
    pub branch: String,
}

/// Branch mutate response — name of the resulting branch (create/rename) or deleted name.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoBranchMutationResponse {
    pub branch: String,
}

/// Shared commit-target options for `repo.file.*` mutations (GIT-19).
///
/// Flattened into every file-mutation request: the change applies on top of
/// `branch` (default: repository default branch). When `new_branch` is set the
/// server creates it at `branch`'s tip and commits there instead — the
/// protected-branch web flow. When `open_pr` is true (default once
/// `new_branch` is set) the server also opens a pull request
/// `new_branch` → `branch`.
///
/// When `branch` is protected and no `new_branch` is supplied, the server
/// performs the same fallback automatically (generated `web-edit/*` branch +
/// PR) rather than rejecting the write.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoFileCommitOptions {
    /// Base branch the change applies on top of (default branch when absent).
    #[serde(default)]
    pub branch: Option<String>,
    /// Create this branch at `branch`'s tip and commit the change there.
    #[serde(default)]
    pub new_branch: Option<String>,
    /// Open a pull request `new_branch` → `branch` (default: true when
    /// `new_branch` is set, false otherwise).
    #[serde(default)]
    pub open_pr: Option<bool>,
    /// Pull request title (default: first line of the commit message).
    #[serde(default)]
    pub pr_title: Option<String>,
    /// Pull request body.
    #[serde(default)]
    pub pr_body: Option<String>,
}

/// `repo.file.create` input — create a file (GIT-19). `path` must not exist.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoFileCreateRequest {
    pub owner: String,
    pub name: String,
    /// Repository-relative destination path (no `..`, absolute, or `.git`
    /// segments; empty file allowed).
    pub path: String,
    /// UTF-8 text content (text-editor path; may be empty for an empty file).
    #[serde(default)]
    pub content: Option<String>,
    /// Base64-encoded bytes (binary-safe upload path). Exactly one of
    /// `content` / `content_base64` may be set.
    #[serde(default)]
    pub content_base64: Option<String>,
    /// Commit message (subject line; required, non-empty).
    pub message: String,
    #[serde(flatten)]
    pub target: RepoFileCommitOptions,
}

/// `repo.file.update` input — overwrite an existing text file (GIT-19).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoFileUpdateRequest {
    pub owner: String,
    pub name: String,
    pub path: String,
    /// UTF-8 text content (may be empty — truncating to empty file is legal).
    #[serde(default)]
    pub content: Option<String>,
    /// Base64-encoded bytes — only way to replace a binary file's content.
    #[serde(default)]
    pub content_base64: Option<String>,
    pub message: String,
    #[serde(flatten)]
    pub target: RepoFileCommitOptions,
}

/// `repo.file.delete` input — delete a file or a whole directory (GIT-19).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoFileDeleteRequest {
    pub owner: String,
    pub name: String,
    /// File path, or a directory path (removes every blob under it).
    pub path: String,
    pub message: String,
    #[serde(flatten)]
    pub target: RepoFileCommitOptions,
}

/// `repo.file.rename` input — move/rename a file (GIT-19). File-only:
/// renaming a directory is rejected (rename its children instead).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoFileRenameRequest {
    pub owner: String,
    pub name: String,
    /// Existing file path.
    pub from_path: String,
    /// Destination path (must not exist).
    pub to_path: String,
    /// Optional new content — absent preserves the file's bytes (pure rename).
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub content_base64: Option<String>,
    pub message: String,
    #[serde(flatten)]
    pub target: RepoFileCommitOptions,
}

/// `repo.file.upload` input — multi-file commit (GIT-19). Each path is
/// created or overwritten; binaries go through `content_base64`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoFileUploadEntry {
    /// Repository-relative destination path.
    pub path: String,
    /// Base64-encoded file bytes.
    pub content_base64: String,
}

/// `repo.file.upload` input — commit several files atomically (GIT-19).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoFileUploadRequest {
    pub owner: String,
    pub name: String,
    pub files: Vec<RepoFileUploadEntry>,
    pub message: String,
    #[serde(flatten)]
    pub target: RepoFileCommitOptions,
}

/// `repo.file.mkdir` input — create a directory (GIT-19). Git does not track
/// empty directories; the commit materializes `{path}/.gitkeep` (empty file),
/// matching the common convention. `path` must not exist.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoFileMkdirRequest {
    pub owner: String,
    pub name: String,
    /// Directory path to create.
    pub path: String,
    pub message: String,
    #[serde(flatten)]
    pub target: RepoFileCommitOptions,
}

/// `repo.file.commitPolicy` input — which commit target the caller may use.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoFileCommitPolicyRequest {
    pub owner: String,
    pub name: String,
    /// Branch being considered (default: repository default branch).
    #[serde(default)]
    pub branch: Option<String>,
}

/// `repo.file.commitPolicy` response — drives the commit-target picker.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoFileCommitPolicyResponse {
    /// Resolved base branch.
    pub branch: String,
    /// Caller may commit directly onto `branch`.
    pub direct_commit_allowed: bool,
    /// A matching protection rule blocks direct push — commit must go through
    /// the new-branch + pull-request flow.
    pub requires_pr: bool,
}

/// `repo.file.*` mutation response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoFileCommitResponse {
    /// New commit SHA.
    pub commit_sha: String,
    /// Branch that received the commit (== `new_branch` when one was used).
    pub branch: String,
    /// True when the commit landed on a branch this call created.
    pub created_branch: bool,
    /// Pull request number when a PR was opened.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_number: Option<i64>,
}

/// `repo.updateVisibility` input (D-26).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoUpdateVisibilityRequest {
    pub owner: String,
    pub name: String,
    pub visibility: RepoVisibility,
}

/// `repo.setArchived` input — Admin-only read-only archive toggle (GIT-20).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoSetArchivedRequest {
    pub owner: String,
    pub name: String,
    pub archived: bool,
}

/// `repo.lfs.setEnabled` input — Admin-only per-repo LFS toggle (D-LFS-10).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoLfsSetEnabledRequest {
    pub owner: String,
    pub name: String,
    pub enabled: bool,
}

/// `repo.lfs.setEnabled` / `repo.lfs.getEnabled` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoLfsEnabledResponse {
    pub enabled: bool,
}

/// `repo.lfs.getEnabled` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoLfsGetEnabledRequest {
    pub owner: String,
    pub name: String,
}

/// `repo.lfs.getStatus` — enable flag + light usage snapshot for Settings (D-LFS-16/19).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoLfsStatusResponse {
    pub enabled: bool,
    pub object_count: i64,
    pub logical_bytes: i64,
}

/// Top / listed LFS object row for usage + browser.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoLfsObjectEntry {
    pub oid: String,
    pub size: i64,
    pub refcount: i64,
}

/// `repo.lfs.getUsage` — this-repo breakdown (D-LFS-19).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoLfsUsageResponse {
    pub enabled: bool,
    pub object_count: i64,
    pub logical_bytes: i64,
    pub quota_repo_bytes: i64,
    pub objects: Vec<RepoLfsObjectEntry>,
}

/// `repo.lfs.listObjects` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoLfsListObjectsRequest {
    pub owner: String,
    pub name: String,
    #[serde(default)]
    pub limit: Option<i64>,
}

/// `repo.lfs.listObjects` response — in-app LFS browser (D-LFS-16).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoLfsListObjectsResponse {
    pub enabled: bool,
    pub objects: Vec<RepoLfsObjectEntry>,
}

/// `repo.lfs.download` input — session Read path (D-LFS-18 / A2).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoLfsDownloadRequest {
    pub owner: String,
    pub name: String,
    pub oid: String,
}

/// Soft-capped base64 payload for browser Download (not git-lfs PAT path).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoLfsDownloadResponse {
    pub oid: String,
    pub size: i64,
    pub encoding: String,
    pub content: String,
}

/// `repo.quota.get` / `repo.quota.set` — bare-repo disk usage + git object size quota (GIT-25).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoQuotaPublic {
    /// Measured on-disk size of the bare repository (bytes).
    pub size_bytes: i64,
    /// Effective quota: per-repo override → instance setting → env default.
    /// `null` = unlimited.
    pub effective_quota_bytes: Option<i64>,
    /// Per-repo override stored on the repository row (`null` = inherit instance default).
    pub size_quota_bytes: Option<i64>,
    /// Resolved instance default (admin override when set, else env/built-in).
    /// `0` or negative = unlimited default.
    pub instance_quota_bytes: i64,
}

/// `repo.quota.get` input (GIT-25).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoGetQuotaRequest {
    pub owner: String,
    pub name: String,
}

/// `repo.quota.set` input — repo Admin capability (GIT-25).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoSetQuotaRequest {
    pub owner: String,
    pub name: String,
    /// Per-repo quota in bytes; `null`/omitted clears the override.
    /// `0` or negative stores an explicit "unlimited" override.
    #[serde(default)]
    pub size_quota_bytes: Option<i64>,
}

/// `admin.git.getSettings` / `admin.git.updateSettings` — instance git quota
/// default (GIT-25).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminGitSettingsPublic {
    /// Effective instance default (admin override when set, else env/built-in).
    pub repo_quota_bytes: i64,
    /// True when an Admin override row is stored (vs env/built-in default).
    pub repo_quota_bytes_overridden: bool,
}

/// `admin.git.updateSettings` input — `null` keeps the current override; see `clear_overrides`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminGitUpdateSettingsRequest {
    #[serde(default)]
    pub repo_quota_bytes: Option<i64>,
    /// When true, clears all stored overrides back to env/built-in defaults.
    #[serde(default)]
    pub clear_overrides: bool,
}

/// Instance-admin template pack metadata (issue #18).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct InstanceTemplatePackPublic {
    pub id: String,
    pub slug: String,
    pub label: String,
    pub group: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_gitignore: Option<String>,
    pub enabled: bool,
    pub byte_size: i64,
    pub content_digest: String,
    pub uploaded_by_user_id: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminTemplatesListResponse {
    pub packs: Vec<InstanceTemplatePackPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminTemplateUpdateRequest {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub default_gitignore: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminTemplateSetEnabledRequest {
    pub id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminTemplateDeleteRequest {
    pub id: String,
}

/// `repo.templates.setEnabled` — mark repository as a create-from template.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoTemplateSetEnabledRequest {
    pub owner: String,
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoTemplateGetEnabledRequest {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoTemplateEnabledResponse {
    pub enabled: bool,
}

/// `repo.templates.list` input — issue/PR templates live in the repo's git tree
/// (COL-02), so the request only needs the repo (default branch is read).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoTemplatesListRequest {
    pub owner: String,
    pub name: String,
}

/// `repo.issues.setEnabled` / `repo.pulls.setEnabled` input — Admin-only
/// per-repo unit toggle (COL-13).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoUnitSetEnabledRequest {
    pub owner: String,
    pub name: String,
    pub enabled: bool,
}

/// `repo.issues.getEnabled` / `repo.pulls.getEnabled` input (COL-13).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoUnitGetEnabledRequest {
    pub owner: String,
    pub name: String,
}

/// One issue/PR template file discovered in the default-branch tree
/// (GitHub-style `---` YAML frontmatter parsed when present).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoFileTemplate {
    /// Display name — frontmatter `name`, else filename stem.
    pub name: String,
    /// Frontmatter `title` — prefill for the subject field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Frontmatter `about` — chooser blurb.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Frontmatter `labels` (comma string or list — GitHub-style).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    /// Markdown body with the frontmatter block stripped.
    pub body: String,
    /// Repo-relative path, e.g. `.github/ISSUE_TEMPLATE/bug.md`.
    pub filename: String,
}

/// `repo.templates.list` response — grouped by target surface.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoTemplatesListResponse {
    pub issues: Vec<RepoFileTemplate>,
    pub pulls: Vec<RepoFileTemplate>,
}

/// `repo.issues.*` / `repo.pulls.*` unit-toggle response (COL-13).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoUnitEnabledResponse {
    pub enabled: bool,
}

/// Per-repo row in admin instance usage breakdown.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminLfsRepoUsageEntry {
    pub repository_id: String,
    pub owner: String,
    pub name: String,
    pub object_count: i64,
    pub logical_bytes: i64,
}

/// Per-owner (user/org) row in admin instance usage breakdown.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminLfsOwnerUsageEntry {
    pub owner_id: String,
    pub owner_slug: String,
    pub object_count: i64,
    pub logical_bytes: i64,
}

/// `admin.lfs.getUsage` — instance breakdown (D-LFS-19).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminLfsUsageResponse {
    pub physical_bytes: i64,
    pub object_count: i64,
    pub logical_bytes: i64,
    pub by_repo: Vec<AdminLfsRepoUsageEntry>,
    pub by_owner: Vec<AdminLfsOwnerUsageEntry>,
}

/// `repo.softDelete` input — typed confirm name required (D-35 / T-07-24).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoSoftDeleteRequest {
    pub owner: String,
    pub name: String,
    #[serde(rename = "confirmName", alias = "confirm_name")]
    pub confirm_name: String,
}

/// Soft-delete acknowledgement (DB row marked; disk purge deferred).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoSoftDeleteResponse {
    pub name: String,
}

/// Effective instance LFS limits (Admin override or env default) — D-LFS-13.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminLfsSettingsPublic {
    pub max_object_bytes: i64,
    pub quota_repo_bytes: i64,
    pub quota_user_bytes: i64,
    /// True when DB override is set for each field.
    pub max_object_bytes_overridden: bool,
    pub quota_repo_bytes_overridden: bool,
    pub quota_user_bytes_overridden: bool,
}

/// `admin.lfs.updateSettings` — null fields clear override (revert to env).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminLfsUpdateSettingsRequest {
    #[serde(default)]
    pub max_object_bytes: Option<i64>,
    #[serde(default)]
    pub quota_repo_bytes: Option<i64>,
    #[serde(default)]
    pub quota_user_bytes: Option<i64>,
    /// When true, clear all overrides (use env defaults).
    #[serde(default)]
    pub clear_overrides: bool,
}
/// `repo.rename` input — Admin only; no type-confirm (D-REL-07 / GIT-16).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoRenameRequest {
    pub owner: String,
    pub name: String,
    #[serde(rename = "newName", alias = "new_name")]
    pub new_name: String,
}

/// `repo.rename` response — updated public repo metadata.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoRenameResponse {
    pub repo: RepoPublic,
}

/// `repo.transfer` input — Admin only; type-confirm required (D-REL-09 / D-REL-10 / GIT-17).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoTransferRequest {
    pub owner: String,
    pub name: String,
    #[serde(rename = "destOwner", alias = "dest_owner")]
    pub dest_owner: String,
    #[serde(rename = "destOwnerType", alias = "dest_owner_type")]
    pub dest_owner_type: OwnerType,
    #[serde(rename = "confirmName", alias = "confirm_name")]
    pub confirm_name: String,
}

/// `repo.transfer` response — updated public repo under the new owner.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoTransferResponse {
    pub repo: RepoPublic,
}
/// Public collaborator row — no email (ORG-03 / D-ORG-02c).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoCollaboratorPublic {
    pub user_id: String,
    pub username: String,
    pub permission: CollaboratorPermission,
    pub created_at: String,
}

/// `repo.collaborators.list` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoCollaboratorsListResponse {
    pub collaborators: Vec<RepoCollaboratorPublic>,
}

/// `repo.collaborators.add` — existing instance user by username.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoCollaboratorsAddRequest {
    pub owner: String,
    pub name: String,
    pub username: String,
    pub permission: CollaboratorPermission,
}

/// `repo.collaborators.update`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoCollaboratorsUpdateRequest {
    pub owner: String,
    pub name: String,
    pub user_id: String,
    pub permission: CollaboratorPermission,
}

/// `repo.collaborators.remove`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoCollaboratorsRemoveRequest {
    pub owner: String,
    pub name: String,
    pub user_id: String,
}

/// Public pending repo invite — never includes token or token_hash.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoInvitePublic {
    pub id: String,
    /// `None` = shareable link (not bound to a recipient email).
    pub email: Option<String>,
    pub permission: CollaboratorPermission,
    /// `None` = never expires.
    pub expires_at: Option<String>,
    pub invited_by: String,
    pub created_at: String,
    /// `None` = unlimited seats.
    pub max_uses: Option<i64>,
    pub use_count: i64,
}

/// `repo.invites.list` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoInvitesListResponse {
    pub invites: Vec<RepoInvitePublic>,
}

/// `repo.invites.create` — bulk: one email-bound invite per address.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoInvitesCreateRequest {
    pub owner: String,
    pub name: String,
    pub emails: Vec<String>,
    pub permission: CollaboratorPermission,
}

/// Per-recipient outcome of a bulk `repo.invites.create`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoInvitesCreateItemResult {
    pub email: String,
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub invite: Option<RepoInvitePublic>,
    /// One-time copyable URL (present only when `ok`).
    #[serde(default)]
    pub invite_url: Option<String>,
}

/// `repo.invites.create` response — one entry per submitted email.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoInvitesCreateResponse {
    pub results: Vec<RepoInvitesCreateItemResult>,
}

/// `repo.invites.createLink` — shareable link, no bound email.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoInvitesCreateLinkRequest {
    pub owner: String,
    pub name: String,
    pub permission: CollaboratorPermission,
    /// ISO-8601 UTC; `None`/absent = never expires.
    #[serde(default)]
    pub expires_at: Option<String>,
    /// `None`/absent = unlimited seats.
    #[serde(default)]
    pub max_uses: Option<i64>,
}

/// `repo.invites.createLink` — invite metadata + one-time copyable URL.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoInvitesCreateLinkResponse {
    pub invite: RepoInvitePublic,
    pub invite_url: String,
}

/// `repo.invites.revoke`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoInvitesRevokeRequest {
    pub owner: String,
    pub name: String,
    pub invite_id: String,
}

/// Org profile meta-repos (Oxidean-first; `.github` kept for compatibility). Leading `.` is otherwise rejected.
const ALLOWED_DOT_REPO_NAMES: &[&str] = &[".oxidean", ".github"];

fn is_allowed_dot_repo_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    ALLOWED_DOT_REPO_NAMES
        .iter()
        .any(|allowed| *allowed == lower.as_str())
}

/// Repo name rules (D-06): 1–100 chars, ascii letters/digits/hyphen/underscore/period;
/// no leading/trailing `.` or `-`; not `.` / `..`; not a reserved path segment.
/// Exception: `.oxidean` and `.github` (org profile README special repos).
pub fn validate_repo_name(raw: &str) -> Result<(), String> {
    let name = raw.trim();
    if name.is_empty() || name.len() > 100 {
        return Err("repository name must be 1–100 characters".into());
    }
    if name == "." || name == ".." {
        return Err("repository name is invalid".into());
    }
    let allow_leading_dot = is_allowed_dot_repo_name(name);
    if name.starts_with('-')
        || name.ends_with('-')
        || (name.starts_with('.') && !allow_leading_dot)
        || name.ends_with('.')
    {
        return Err("repository name cannot start or end with a hyphen or period".into());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err("repository name must be alphanumeric, hyphen, underscore, or period".into());
    }
    if is_reserved_username(name) {
        return Err("repository name is reserved".into());
    }
    Ok(())
}

/// Sys-admin manual `git gc` (D-37). Omit owner+name to GC all active repos.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoGcRequest {
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

/// Result of a manual or scheduled GC pass.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoGcResponse {
    pub ok: bool,
    pub gc_count: u32,
    pub error_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth_types::is_reserved_username;

    #[test]
    fn validate_repo_name_accepts_my_app() {
        assert!(
            validate_repo_name("my_app").is_ok(),
            "D-06: underscore must be allowed in repo names"
        );
    }

    #[test]
    fn validate_repo_name_accepts_dotted_name() {
        assert!(validate_repo_name("my.app").is_ok());
    }

    #[test]
    fn validate_repo_name_rejects_empty() {
        assert!(validate_repo_name("").is_err());
        assert!(validate_repo_name("   ").is_err());
    }

    #[test]
    fn validate_repo_name_rejects_reserved() {
        let err = validate_repo_name("login").unwrap_err();
        assert!(
            err.contains("reserved"),
            "expected reserved rejection, got: {err}"
        );
        assert!(is_reserved_username("login"));
    }

    #[test]
    fn validate_repo_name_accepts_profile_special_dot_repos() {
        assert!(
            validate_repo_name(".oxidean").is_ok(),
            "org profile special repo .oxidean must be allowed"
        );
        assert!(
            validate_repo_name(".github").is_ok(),
            "org profile special repo .github must be allowed"
        );
        assert!(
            validate_repo_name(".OXIDEAN").is_ok(),
            "allowlist is case-insensitive"
        );
        assert!(validate_repo_name(".GitHub").is_ok());
    }

    #[test]
    fn validate_repo_name_rejects_other_leading_dot_names() {
        assert!(validate_repo_name(".hidden").is_err());
        assert!(validate_repo_name(".config").is_err());
        assert!(validate_repo_name(".git").is_err());
    }
}
