//! Branch protection rules + commit statuses (Phase 13 / ORG-05 / D-11).

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct BranchProtectionRuleRow {
    pub id: String,
    pub repo_id: String,
    pub pattern: String,
    pub require_reviews: bool,
    pub required_approving_review_count: i32,
    pub dismiss_stale_reviews: bool,
    pub require_conversation_resolution: bool,
    pub require_last_push_approval: bool,
    pub required_status_contexts: String,
    pub strict_status_checks: bool,
    pub allow_force_pushes: bool,
    pub allow_deletions: bool,
    pub enforce_admins: bool,
    pub required_linear_history: bool,
    pub lock_branch: bool,
    pub require_signed_commits: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct CommitStatusRow {
    pub id: String,
    pub repo_id: String,
    pub sha: String,
    pub context: String,
    pub state: String,
    pub description: String,
    pub target_url: Option<String>,
    pub creator_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

macro_rules! flag_col {
    ($row:expr, $col:expr) => {{
        match $row.try_get::<bool, _>($col) {
            Ok(v) => v,
            Err(_) => {
                let n: i64 = $row
                    .try_get($col)
                    .map_err(|e| format!("branch_protection {}: {e}", $col))?;
                n != 0
            }
        }
    }};
}

macro_rules! map_rule {
    ($row:expr) => {{
        let row = $row;
        // Postgres INTEGER is INT4; SQLite INTEGER often decodes as i64.
        let count: i32 = match row.try_get::<i32, _>("required_approving_review_count") {
            Ok(v) => v,
            Err(_) => {
                let n: i64 = row
                    .try_get("required_approving_review_count")
                    .map_err(|e| format!("rule review count: {e}"))?;
                i32::try_from(n).map_err(|_| format!("rule review count out of range: {n}"))?
            }
        };
        BranchProtectionRuleRow {
            id: row.try_get("id").map_err(|e| format!("rule id: {e}"))?,
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("rule repo_id: {e}"))?,
            pattern: row
                .try_get("pattern")
                .map_err(|e| format!("rule pattern: {e}"))?,
            require_reviews: flag_col!(row, "require_reviews"),
            required_approving_review_count: count,
            dismiss_stale_reviews: flag_col!(row, "dismiss_stale_reviews"),
            require_conversation_resolution: flag_col!(row, "require_conversation_resolution"),
            require_last_push_approval: flag_col!(row, "require_last_push_approval"),
            required_status_contexts: row
                .try_get("required_status_contexts")
                .map_err(|e| format!("rule contexts: {e}"))?,
            strict_status_checks: flag_col!(row, "strict_status_checks"),
            allow_force_pushes: flag_col!(row, "allow_force_pushes"),
            allow_deletions: flag_col!(row, "allow_deletions"),
            enforce_admins: flag_col!(row, "enforce_admins"),
            required_linear_history: flag_col!(row, "required_linear_history"),
            lock_branch: flag_col!(row, "lock_branch"),
            require_signed_commits: flag_col!(row, "require_signed_commits"),
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("rule created_at: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("rule updated_at: {e}"))?,
        }
    }};
}

macro_rules! map_status {
    ($row:expr) => {{
        let row = $row;
        CommitStatusRow {
            id: row.try_get("id").map_err(|e| format!("status id: {e}"))?,
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("status repo_id: {e}"))?,
            sha: row.try_get("sha").map_err(|e| format!("status sha: {e}"))?,
            context: row
                .try_get("context")
                .map_err(|e| format!("status context: {e}"))?,
            state: row
                .try_get("state")
                .map_err(|e| format!("status state: {e}"))?,
            description: row
                .try_get("description")
                .map_err(|e| format!("status description: {e}"))?,
            target_url: row
                .try_get("target_url")
                .map_err(|e| format!("status target_url: {e}"))?,
            creator_id: row
                .try_get("creator_id")
                .map_err(|e| format!("status creator_id: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("status created_at: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("status updated_at: {e}"))?,
        }
    }};
}

const RULE_SELECT_PG: &str =
    "SELECT id, repo_id, pattern, require_reviews, required_approving_review_count, \
 dismiss_stale_reviews, require_conversation_resolution, require_last_push_approval, \
 required_status_contexts, strict_status_checks, allow_force_pushes, allow_deletions, \
 enforce_admins, required_linear_history, lock_branch, require_signed_commits, \
 to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at, \
 to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at \
 FROM branch_protection_rules";

const RULE_SELECT_MYSQL: &str =
    "SELECT id, repo_id, pattern, require_reviews, required_approving_review_count, \
 dismiss_stale_reviews, require_conversation_resolution, require_last_push_approval, \
 required_status_contexts, strict_status_checks, allow_force_pushes, allow_deletions, \
 enforce_admins, required_linear_history, lock_branch, require_signed_commits, \
 DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at, \
 DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at \
 FROM branch_protection_rules";

const RULE_SELECT_SQLITE: &str =
    "SELECT id, repo_id, pattern, require_reviews, required_approving_review_count, \
 dismiss_stale_reviews, require_conversation_resolution, require_last_push_approval, \
 required_status_contexts, strict_status_checks, allow_force_pushes, allow_deletions, \
 enforce_admins, required_linear_history, lock_branch, require_signed_commits, \
 strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at, \
 strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at \
 FROM branch_protection_rules";

const STATUS_SELECT_PG: &str =
    "SELECT id, repo_id, sha, context, state, description, target_url, creator_id, \
 to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at, \
 to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at \
 FROM commit_statuses";

const STATUS_SELECT_MYSQL: &str =
    "SELECT id, repo_id, sha, context, state, description, target_url, creator_id, \
 DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at, \
 DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at \
 FROM commit_statuses";

const STATUS_SELECT_SQLITE: &str =
    "SELECT id, repo_id, sha, context, state, description, target_url, creator_id, \
 strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at, \
 strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at \
 FROM commit_statuses";

fn as_int(b: bool) -> i32 {
    i32::from(b)
}

pub async fn list_rules(
    pool: &DbPool,
    repo_id: &str,
) -> Result<Vec<BranchProtectionRuleRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{RULE_SELECT_PG} WHERE repo_id = $1 ORDER BY created_at ASC, id ASC"
            )))
            .bind(repo_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list branch_protection_rules: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_rule!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{RULE_SELECT_MYSQL} WHERE repo_id = ? ORDER BY created_at ASC, id ASC"
            )))
            .bind(repo_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list branch_protection_rules: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_rule!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{RULE_SELECT_SQLITE} WHERE repo_id = ?1 ORDER BY created_at ASC, id ASC"
            )))
            .bind(repo_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list branch_protection_rules: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_rule!(r));
            }
            Ok(out)
        }
    }
}

pub async fn find_rule(
    pool: &DbPool,
    repo_id: &str,
    rule_id: &str,
) -> Result<Option<BranchProtectionRuleRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{RULE_SELECT_PG} WHERE repo_id = $1 AND id = $2"
            )))
            .bind(repo_id)
            .bind(rule_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find branch_protection_rule: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_rule!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{RULE_SELECT_MYSQL} WHERE repo_id = ? AND id = ?"
            )))
            .bind(repo_id)
            .bind(rule_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find branch_protection_rule: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_rule!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{RULE_SELECT_SQLITE} WHERE repo_id = ?1 AND id = ?2"
            )))
            .bind(repo_id)
            .bind(rule_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find branch_protection_rule: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_rule!(&r)),
                None => None,
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_rule(
    pool: &DbPool,
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
    require_signed_commits: bool,
) -> Result<BranchProtectionRuleRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO branch_protection_rules (\
               id, repo_id, pattern, require_reviews, required_approving_review_count, \
               dismiss_stale_reviews, require_conversation_resolution, require_last_push_approval, \
               required_status_contexts, strict_status_checks, allow_force_pushes, allow_deletions, \
               enforce_admins, required_linear_history, lock_branch, require_signed_commits) \
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(pattern)
            .bind(require_reviews)
            .bind(required_approving_review_count)
            .bind(dismiss_stale_reviews)
            .bind(require_conversation_resolution)
            .bind(require_last_push_approval)
            .bind(required_status_contexts)
            .bind(strict_status_checks)
            .bind(allow_force_pushes)
            .bind(allow_deletions)
            .bind(enforce_admins)
            .bind(required_linear_history)
            .bind(lock_branch)
            .bind(require_signed_commits)
            .execute(p)
            .await
            .map_err(|e| format!("insert branch_protection_rule: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO branch_protection_rules (\
               id, repo_id, pattern, require_reviews, required_approving_review_count, \
               dismiss_stale_reviews, require_conversation_resolution, require_last_push_approval, \
               required_status_contexts, strict_status_checks, allow_force_pushes, allow_deletions, \
               enforce_admins, required_linear_history, lock_branch, require_signed_commits) \
               VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(pattern)
            .bind(as_int(require_reviews))
            .bind(required_approving_review_count)
            .bind(as_int(dismiss_stale_reviews))
            .bind(as_int(require_conversation_resolution))
            .bind(as_int(require_last_push_approval))
            .bind(required_status_contexts)
            .bind(as_int(strict_status_checks))
            .bind(as_int(allow_force_pushes))
            .bind(as_int(allow_deletions))
            .bind(as_int(enforce_admins))
            .bind(as_int(required_linear_history))
            .bind(as_int(lock_branch))
            .bind(as_int(require_signed_commits))
            .execute(p)
            .await
            .map_err(|e| format!("insert branch_protection_rule: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO branch_protection_rules (\
               id, repo_id, pattern, require_reviews, required_approving_review_count, \
               dismiss_stale_reviews, require_conversation_resolution, require_last_push_approval, \
               required_status_contexts, strict_status_checks, allow_force_pushes, allow_deletions, \
               enforce_admins, required_linear_history, lock_branch, require_signed_commits) \
               VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(pattern)
            .bind(as_int(require_reviews))
            .bind(required_approving_review_count)
            .bind(as_int(dismiss_stale_reviews))
            .bind(as_int(require_conversation_resolution))
            .bind(as_int(require_last_push_approval))
            .bind(required_status_contexts)
            .bind(as_int(strict_status_checks))
            .bind(as_int(allow_force_pushes))
            .bind(as_int(allow_deletions))
            .bind(as_int(enforce_admins))
            .bind(as_int(required_linear_history))
            .bind(as_int(lock_branch))
            .bind(as_int(require_signed_commits))
            .execute(p)
            .await
            .map_err(|e| format!("insert branch_protection_rule: {e}"))?;
        }
    }
    find_rule(pool, repo_id, id)
        .await?
        .ok_or_else(|| "branch protection rule missing after insert".into())
}

#[allow(clippy::too_many_arguments)]
pub async fn update_rule(
    pool: &DbPool,
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
    require_signed_commits: bool,
) -> Result<BranchProtectionRuleRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            let res = sqlx::query(
                "UPDATE branch_protection_rules SET pattern=$1, require_reviews=$2, \
                  required_approving_review_count=$3, dismiss_stale_reviews=$4, \
                  require_conversation_resolution=$5, require_last_push_approval=$6, \
                  required_status_contexts=$7, strict_status_checks=$8, allow_force_pushes=$9, \
                  allow_deletions=$10, enforce_admins=$11, required_linear_history=$12, \
                  lock_branch=$13, require_signed_commits=$14, updated_at=now() WHERE repo_id=$15 AND id=$16",
            )
            .bind(pattern)
            .bind(require_reviews)
            .bind(required_approving_review_count)
            .bind(dismiss_stale_reviews)
            .bind(require_conversation_resolution)
            .bind(require_last_push_approval)
            .bind(required_status_contexts)
            .bind(strict_status_checks)
            .bind(allow_force_pushes)
            .bind(allow_deletions)
            .bind(enforce_admins)
            .bind(required_linear_history)
            .bind(lock_branch)
            .bind(require_signed_commits)
            .bind(repo_id)
            .bind(rule_id)
            .execute(p)
            .await
            .map_err(|e| format!("update branch_protection_rule: {e}"))?;
            if res.rows_affected() == 0 {
                return Err("branch protection rule not found".into());
            }
        }
        DbPool::MySql(p) => {
            let res = sqlx::query(
                "UPDATE branch_protection_rules SET pattern=?, require_reviews=?, \
                  required_approving_review_count=?, dismiss_stale_reviews=?, \
                  require_conversation_resolution=?, require_last_push_approval=?, \
                  required_status_contexts=?, strict_status_checks=?, allow_force_pushes=?, \
                  allow_deletions=?, enforce_admins=?, required_linear_history=?, \
                  lock_branch=?, require_signed_commits=?, \
                  updated_at=CURRENT_TIMESTAMP WHERE repo_id=? AND id=?",
            )
            .bind(pattern)
            .bind(as_int(require_reviews))
            .bind(required_approving_review_count)
            .bind(as_int(dismiss_stale_reviews))
            .bind(as_int(require_conversation_resolution))
            .bind(as_int(require_last_push_approval))
            .bind(required_status_contexts)
            .bind(as_int(strict_status_checks))
            .bind(as_int(allow_force_pushes))
            .bind(as_int(allow_deletions))
            .bind(as_int(enforce_admins))
            .bind(as_int(required_linear_history))
            .bind(as_int(lock_branch))
            .bind(as_int(require_signed_commits))
            .bind(repo_id)
            .bind(rule_id)
            .execute(p)
            .await
            .map_err(|e| format!("update branch_protection_rule: {e}"))?;
            if res.rows_affected() == 0 {
                return Err("branch protection rule not found".into());
            }
        }
        DbPool::Sqlite(p) => {
            let res = sqlx::query(
                "UPDATE branch_protection_rules SET pattern=?1, require_reviews=?2, \
                  required_approving_review_count=?3, dismiss_stale_reviews=?4, \
                  require_conversation_resolution=?5, require_last_push_approval=?6, \
                  required_status_contexts=?7, strict_status_checks=?8, allow_force_pushes=?9, \
                  allow_deletions=?10, enforce_admins=?11, required_linear_history=?12, \
                  lock_branch=?13, require_signed_commits=?14, \
                  updated_at=strftime('%Y-%m-%d %H:%M:%S','now') \
                  WHERE repo_id=?15 AND id=?16",
            )
            .bind(pattern)
            .bind(as_int(require_reviews))
            .bind(required_approving_review_count)
            .bind(as_int(dismiss_stale_reviews))
            .bind(as_int(require_conversation_resolution))
            .bind(as_int(require_last_push_approval))
            .bind(required_status_contexts)
            .bind(as_int(strict_status_checks))
            .bind(as_int(allow_force_pushes))
            .bind(as_int(allow_deletions))
            .bind(as_int(enforce_admins))
            .bind(as_int(required_linear_history))
            .bind(as_int(lock_branch))
            .bind(as_int(require_signed_commits))
            .bind(repo_id)
            .bind(rule_id)
            .execute(p)
            .await
            .map_err(|e| format!("update branch_protection_rule: {e}"))?;
            if res.rows_affected() == 0 {
                return Err("branch protection rule not found".into());
            }
        }
    }
    find_rule(pool, repo_id, rule_id)
        .await?
        .ok_or_else(|| "branch protection rule missing after update".into())
}

pub async fn delete_rule(pool: &DbPool, repo_id: &str, rule_id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            let res =
                sqlx::query("DELETE FROM branch_protection_rules WHERE repo_id = $1 AND id = $2")
                    .bind(repo_id)
                    .bind(rule_id)
                    .execute(p)
                    .await
                    .map_err(|e| format!("delete branch_protection_rule: {e}"))?;
            if res.rows_affected() == 0 {
                return Err("branch protection rule not found".into());
            }
        }
        DbPool::MySql(p) => {
            let res =
                sqlx::query("DELETE FROM branch_protection_rules WHERE repo_id = ? AND id = ?")
                    .bind(repo_id)
                    .bind(rule_id)
                    .execute(p)
                    .await
                    .map_err(|e| format!("delete branch_protection_rule: {e}"))?;
            if res.rows_affected() == 0 {
                return Err("branch protection rule not found".into());
            }
        }
        DbPool::Sqlite(p) => {
            let res =
                sqlx::query("DELETE FROM branch_protection_rules WHERE repo_id = ?1 AND id = ?2")
                    .bind(repo_id)
                    .bind(rule_id)
                    .execute(p)
                    .await
                    .map_err(|e| format!("delete branch_protection_rule: {e}"))?;
            if res.rows_affected() == 0 {
                return Err("branch protection rule not found".into());
            }
        }
    }
    Ok(())
}

pub async fn list_statuses_for_sha(
    pool: &DbPool,
    repo_id: &str,
    sha: &str,
) -> Result<Vec<CommitStatusRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{STATUS_SELECT_PG} WHERE repo_id = $1 AND sha = $2 ORDER BY context ASC"
            )))
            .bind(repo_id)
            .bind(sha)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list commit_statuses: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_status!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{STATUS_SELECT_MYSQL} WHERE repo_id = ? AND sha = ? ORDER BY context ASC"
            )))
            .bind(repo_id)
            .bind(sha)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list commit_statuses: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_status!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{STATUS_SELECT_SQLITE} WHERE repo_id = ?1 AND sha = ?2 ORDER BY context ASC"
            )))
            .bind(repo_id)
            .bind(sha)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list commit_statuses: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_status!(r));
            }
            Ok(out)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_status(
    pool: &DbPool,
    id: &str,
    repo_id: &str,
    sha: &str,
    context: &str,
    state: &str,
    description: &str,
    target_url: Option<&str>,
    creator_id: Option<&str>,
) -> Result<CommitStatusRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO commit_statuses (id, repo_id, sha, context, state, description, target_url, creator_id) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8) \
                 ON CONFLICT (repo_id, sha, context) DO UPDATE SET \
                   state = EXCLUDED.state, description = EXCLUDED.description, \
                   target_url = EXCLUDED.target_url, creator_id = EXCLUDED.creator_id, \
                   updated_at = now()",
            )
            .bind(id)
            .bind(repo_id)
            .bind(sha)
            .bind(context)
            .bind(state)
            .bind(description)
            .bind(target_url)
            .bind(creator_id)
            .execute(p)
            .await
            .map_err(|e| format!("upsert commit_status: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO commit_statuses (id, repo_id, sha, context, state, description, target_url, creator_id) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8) \
                 ON CONFLICT (repo_id, sha, context) DO UPDATE SET \
                   state = excluded.state, description = excluded.description, \
                   target_url = excluded.target_url, creator_id = excluded.creator_id, \
                   updated_at = strftime('%Y-%m-%d %H:%M:%S','now')",
            )
            .bind(id)
            .bind(repo_id)
            .bind(sha)
            .bind(context)
            .bind(state)
            .bind(description)
            .bind(target_url)
            .bind(creator_id)
            .execute(p)
            .await
            .map_err(|e| format!("upsert commit_status: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO commit_statuses (id, repo_id, sha, context, state, description, target_url, creator_id) \
                 VALUES (?,?,?,?,?,?,?,?) \
                 ON DUPLICATE KEY UPDATE \
                   state = VALUES(state), description = VALUES(description), \
                   target_url = VALUES(target_url), creator_id = VALUES(creator_id), \
                   updated_at = CURRENT_TIMESTAMP",
            )
            .bind(id)
            .bind(repo_id)
            .bind(sha)
            .bind(context)
            .bind(state)
            .bind(description)
            .bind(target_url)
            .bind(creator_id)
            .execute(p)
            .await
            .map_err(|e| format!("upsert commit_status: {e}"))?;
        }
    }
    let rows = list_statuses_for_sha(pool, repo_id, sha).await?;
    rows.into_iter()
        .find(|r| r.context == context)
        .ok_or_else(|| "commit status missing after upsert".into())
}
