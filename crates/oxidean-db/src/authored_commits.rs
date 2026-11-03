//! `authored_commits` — per-commit attribution for the contribution graph
//! (migration 0095). A row means "this commit reached the repo's default
//! branch and its author email resolves to this verified account" — GitHub's
//! authored-commit rule, replacing the pushed-range `commits_count`
//! approximation. Re-pushes dedupe on `(repository_id, sha)`; the same sha in
//! another repo is a separate contribution.
//!
//! Recording is best-effort from the push/merge paths; `contribution_days`
//! bumps ride along only for rows that actually insert, so the rollup stays
//! dedup-consistent.
//!
//! `contribution_backfills` holds claim markers for the lazy backfills:
//! `repo:<id>` = default-branch history walked; `user:<id>` = rollup rebuilt.

use std::collections::{BTreeMap, HashMap, HashSet};

use sqlx::Row;

use crate::contribution_days;
use crate::dialect::{in_placeholders, Dialect};
use crate::pool::DbPool;
use crate::user_emails;
use crate::users;

/// A commit observed on a default branch, pre-attribution: sha + git author
/// fields (`%aI` author date).
#[derive(Debug, Clone)]
pub struct ObservedCommit {
    pub sha: String,
    pub author_email: String,
    pub authored_at: String,
}

/// A repo the profile owner plausibly authored commits in — pushed to,
/// merged a PR into, or opened a (since-merged) PR against. Feeds lazy
/// history backfill.
#[derive(Debug, Clone)]
pub struct ContributionRepoRow {
    pub id: String,
    /// `owner/name` path slug for `bare_repo_path` (username or org slug).
    pub owner_slug: String,
    pub name: String,
    pub default_branch: String,
    pub visibility: String,
}

/// UTC `YYYY-MM-DD` of an RFC-3339 author timestamp; `None` when unparsable.
fn authored_day(authored_at: &str) -> Option<String> {
    let dt = chrono::DateTime::parse_from_rfc3339(authored_at.trim()).ok()?;
    Some(
        dt.with_timezone(&chrono::Utc)
            .date_naive()
            .format("%Y-%m-%d")
            .to_string(),
    )
}

/// Resolve commit author emails to verified user ids: `user_emails` rows with
/// `verified_at`, plus primary `users.email` on verified accounts.
async fn resolve_authors(
    pool: &DbPool,
    emails: &[String],
) -> Result<HashMap<String, String>, String> {
    let mut out: HashMap<String, String> = HashMap::new();
    for row in user_emails::find_many_by_email(pool, emails).await? {
        if row.verified_at.is_some() {
            out.insert(row.email.to_ascii_lowercase(), row.user_id);
        }
    }
    for row in users::find_many_by_email(pool, emails).await? {
        if row.email_verified_at.is_some() {
            out.entry(row.email.to_ascii_lowercase()).or_insert(row.id);
        }
    }
    Ok(out)
}

/// Attribute observed commits to verified users and insert the new ones;
/// bumps `user_contribution_days` per (user, day) for rows that land.
/// Returns the number of newly recorded commits. Best-effort per commit —
/// unresolvable emails and unparsable dates are skipped, not fatal.
pub async fn record_authored(
    pool: &DbPool,
    repository_id: &str,
    commits: &[ObservedCommit],
) -> Result<u64, String> {
    if commits.is_empty() {
        return Ok(0);
    }
    let emails: Vec<String> = {
        let mut seen = HashSet::new();
        commits
            .iter()
            .filter_map(|c| {
                let e = c.author_email.trim().to_ascii_lowercase();
                (!e.is_empty() && seen.insert(e.clone())).then_some(e)
            })
            .collect()
    };
    let author_map = resolve_authors(pool, &emails).await?;
    // (repo-dedup) — a sha already recorded for this repo never re-counts.
    let known = known_shas(pool, repository_id, commits.iter().map(|c| c.sha.as_str())).await?;
    let mut per_user_day: BTreeMap<(String, String), i64> = BTreeMap::new();
    let mut inserted = 0u64;
    for c in commits {
        if known.contains(&c.sha) {
            continue;
        }
        let Some(user_id) = author_map.get(&c.author_email.trim().to_ascii_lowercase()) else {
            continue;
        };
        let Some(day) = authored_day(&c.authored_at) else {
            continue;
        };
        if insert_one(pool, repository_id, &c.sha, user_id, &day, &c.authored_at).await? {
            *per_user_day.entry((user_id.clone(), day)).or_insert(0) += 1;
            inserted += 1;
        }
    }
    for ((user_id, day), n) in per_user_day {
        contribution_days::bump(pool, &user_id, repository_id, &day, "commits", n).await?;
    }
    Ok(inserted)
}

/// Shas from `shas` already recorded for `repository_id` (IN-chunked).
async fn known_shas<'a>(
    pool: &DbPool,
    repository_id: &str,
    shas: impl Iterator<Item = &'a str>,
) -> Result<HashSet<String>, String> {
    let all: Vec<&str> = shas.collect();
    let mut out = HashSet::with_capacity(all.len());
    for chunk in all.chunks(500) {
        let dialect = match pool {
            DbPool::Postgres(_) => Dialect::Postgres,
            DbPool::MySql(_) => Dialect::MySql,
            DbPool::Sqlite(_) => Dialect::Sqlite,
        };
        let in_list = in_placeholders(dialect, 2, chunk.len());
        let sql = format!(
            "SELECT sha FROM authored_commits WHERE repository_id = {{p1}} AND sha IN ({in_list})"
        );
        macro_rules! bind_all {
            ($q:expr) => {{
                let mut q = $q.bind(repository_id);
                for s in chunk {
                    q = q.bind(*s);
                }
                q
            }};
        }
        match pool {
            DbPool::Postgres(p) => {
                let q = sqlx::query(sqlx::AssertSqlSafe(sql.replace("{p1}", "$1")));
                let rows = bind_all!(q)
                    .fetch_all(p)
                    .await
                    .map_err(|e| format!("known authored shas failed: {e}"))?;
                out.extend(
                    rows.iter()
                        .filter_map(|r| r.try_get::<String, _>("sha").ok()),
                );
            }
            DbPool::MySql(p) => {
                let q = sqlx::query(sqlx::AssertSqlSafe(sql.replace("{p1}", "?")));
                let rows = bind_all!(q)
                    .fetch_all(p)
                    .await
                    .map_err(|e| format!("known authored shas failed: {e}"))?;
                out.extend(
                    rows.iter()
                        .filter_map(|r| r.try_get::<String, _>("sha").ok()),
                );
            }
            DbPool::Sqlite(p) => {
                let q = sqlx::query(sqlx::AssertSqlSafe(sql.replace("{p1}", "?1")));
                let rows = bind_all!(q)
                    .fetch_all(p)
                    .await
                    .map_err(|e| format!("known authored shas failed: {e}"))?;
                out.extend(
                    rows.iter()
                        .filter_map(|r| r.try_get::<String, _>("sha").ok()),
                );
            }
        }
    }
    Ok(out)
}

/// Insert-ignore one row; `true` when it landed.
async fn insert_one(
    pool: &DbPool,
    repository_id: &str,
    sha: &str,
    user_id: &str,
    day: &str,
    authored_at: &str,
) -> Result<bool, String> {
    let affected = match pool {
        DbPool::Postgres(p) => sqlx::query(
            "INSERT INTO authored_commits
               (repository_id, sha, user_id, authored_day, authored_at)
             VALUES ($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING",
        )
        .bind(repository_id)
        .bind(sha)
        .bind(user_id)
        .bind(day)
        .bind(authored_at)
        .execute(p)
        .await
        .map(|r| r.rows_affected()),
        DbPool::MySql(p) => sqlx::query(
            "INSERT IGNORE INTO authored_commits
               (repository_id, sha, user_id, authored_day, authored_at)
             VALUES (?,?,?,?,?)",
        )
        .bind(repository_id)
        .bind(sha)
        .bind(user_id)
        .bind(day)
        .bind(authored_at)
        .execute(p)
        .await
        .map(|r| r.rows_affected()),
        DbPool::Sqlite(p) => sqlx::query(
            "INSERT OR IGNORE INTO authored_commits
               (repository_id, sha, user_id, authored_day, authored_at)
             VALUES (?1,?2,?3,?4,?5)",
        )
        .bind(repository_id)
        .bind(sha)
        .bind(user_id)
        .bind(day)
        .bind(authored_at)
        .execute(p)
        .await
        .map(|r| r.rows_affected()),
    }
    .map_err(|e| format!("authored commit insert failed: {e}"))?;
    Ok(affected > 0)
}

// ---------------------------------------------------------------------------
// Backfill claims + repo discovery
// ---------------------------------------------------------------------------

/// Atomically claim a backfill scope; `true` for the caller that won the
/// insert. Losers see `false` and must not walk.
pub async fn claim_backfill(pool: &DbPool, scope: &str) -> Result<bool, String> {
    let affected = match pool {
        DbPool::Postgres(p) => sqlx::query(
            "INSERT INTO contribution_backfills (scope) VALUES ($1) ON CONFLICT DO NOTHING",
        )
        .bind(scope)
        .execute(p)
        .await
        .map(|r| r.rows_affected()),
        DbPool::MySql(p) => {
            sqlx::query("INSERT IGNORE INTO contribution_backfills (scope) VALUES (?)")
                .bind(scope)
                .execute(p)
                .await
                .map(|r| r.rows_affected())
        }
        DbPool::Sqlite(p) => {
            sqlx::query("INSERT OR IGNORE INTO contribution_backfills (scope) VALUES (?1)")
                .bind(scope)
                .execute(p)
                .await
                .map(|r| r.rows_affected())
        }
    }
    .map_err(|e| format!("backfill claim failed: {e}"))?;
    Ok(affected > 0)
}

/// Drop a claim so a failed walk can be retried by a later view/push.
pub async fn release_backfill(pool: &DbPool, scope: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM contribution_backfills WHERE scope = $1")
                .bind(scope)
                .execute(p)
                .await
                .map_err(|e| format!("backfill release failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM contribution_backfills WHERE scope = ?")
                .bind(scope)
                .execute(p)
                .await
                .map_err(|e| format!("backfill release failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM contribution_backfills WHERE scope = ?1")
                .bind(scope)
                .execute(p)
                .await
                .map_err(|e| format!("backfill release failed: {e}"))?;
        }
    }
    Ok(())
}

/// `repo:<id>` scopes already claimed — subtract these before spawning walks.
pub async fn claimed_repo_scopes(
    pool: &DbPool,
    repo_ids: &[String],
) -> Result<HashSet<String>, String> {
    if repo_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let scopes: Vec<String> = repo_ids.iter().map(|id| format!("repo:{id}")).collect();
    let mut out = HashSet::new();
    for chunk in scopes.chunks(500) {
        let dialect = match pool {
            DbPool::Postgres(_) => Dialect::Postgres,
            DbPool::MySql(_) => Dialect::MySql,
            DbPool::Sqlite(_) => Dialect::Sqlite,
        };
        let in_list = in_placeholders(dialect, 1, chunk.len());
        let sql = format!("SELECT scope FROM contribution_backfills WHERE scope IN ({in_list})");
        macro_rules! bind_all {
            ($q:expr) => {{
                let mut q = $q;
                for s in chunk {
                    q = q.bind(s);
                }
                q
            }};
        }
        match pool {
            DbPool::Postgres(p) => {
                let rows = bind_all!(sqlx::query(sqlx::AssertSqlSafe(&*sql)))
                    .fetch_all(p)
                    .await
                    .map_err(|e| format!("claimed scopes failed: {e}"))?;
                out.extend(
                    rows.iter()
                        .filter_map(|r| r.try_get::<String, _>("scope").ok()),
                );
            }
            DbPool::MySql(p) => {
                let rows = bind_all!(sqlx::query(sqlx::AssertSqlSafe(&*sql)))
                    .fetch_all(p)
                    .await
                    .map_err(|e| format!("claimed scopes failed: {e}"))?;
                out.extend(
                    rows.iter()
                        .filter_map(|r| r.try_get::<String, _>("scope").ok()),
                );
            }
            DbPool::Sqlite(p) => {
                let rows = bind_all!(sqlx::query(sqlx::AssertSqlSafe(&*sql)))
                    .fetch_all(p)
                    .await
                    .map_err(|e| format!("claimed scopes failed: {e}"))?;
                out.extend(
                    rows.iter()
                        .filter_map(|r| r.try_get::<String, _>("scope").ok()),
                );
            }
        }
    }
    Ok(out)
}

/// Repos `user_id` plausibly authored commits in: pushed to
/// (`repository_activity`), authored a PR against, merged PRs into, or owns.
/// Owner slug joined for `bare_repo_path` resolution.
pub async fn list_actor_contribution_repos(
    pool: &DbPool,
    user_id: &str,
) -> Result<Vec<ContributionRepoRow>, String> {
    let slug = match pool {
        DbPool::MySql(_) => "CONCAT(COALESCE(ru.username, ro.slug), '')",
        _ => "COALESCE(ru.username, ro.slug)",
    };
    let sql = format!(
        "SELECT r.id, {slug} AS owner_slug, r.name, r.default_branch, r.visibility\n\
         FROM repositories r\n\
         LEFT JOIN users ru ON r.owner_type = 'user' AND ru.id = r.owner_id\n\
         LEFT JOIN organizations ro ON r.owner_type = 'org' AND ro.id = r.owner_id\n\
         WHERE r.deleted_at IS NULL AND r.id IN (\n\
           SELECT repository_id FROM repository_activity WHERE actor_id = {{p1}}\n\
           UNION SELECT repo_id FROM pull_requests WHERE author_id = {{p1}} OR merged_by = {{p1}}\n\
           UNION SELECT p.repo_id FROM pull_reviews rv\n\
             JOIN pull_requests p ON p.id = rv.pull_id WHERE rv.author_id = {{p1}}\n\
           UNION SELECT id FROM repositories WHERE owner_type = 'user' AND owner_id = {{p1}}\n\
         )"
    );
    macro_rules! collect {
        ($rows:expr) => {{
            $rows
                .iter()
                .map(|r| {
                    Ok(ContributionRepoRow {
                        id: r.try_get("id").map_err(|e| format!("contrib repo: {e}"))?,
                        owner_slug: r
                            .try_get("owner_slug")
                            .map_err(|e| format!("contrib repo: {e}"))?,
                        name: r
                            .try_get("name")
                            .map_err(|e| format!("contrib repo: {e}"))?,
                        default_branch: r
                            .try_get("default_branch")
                            .map_err(|e| format!("contrib repo: {e}"))?,
                        visibility: r
                            .try_get("visibility")
                            .map_err(|e| format!("contrib repo: {e}"))?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()
        }};
    }
    match pool {
        DbPool::Postgres(p) => {
            let q = sqlx::query(sqlx::AssertSqlSafe(sql.replace("{p1}", "$1"))).bind(user_id);
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("actor contribution repos failed: {e}"))?;
            collect!(rows)
        }
        DbPool::MySql(p) => {
            let q = sqlx::query(sqlx::AssertSqlSafe(sql.replace("{p1}", "?"))).bind(user_id);
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("actor contribution repos failed: {e}"))?;
            collect!(rows)
        }
        DbPool::Sqlite(p) => {
            let q = sqlx::query(sqlx::AssertSqlSafe(sql.replace("{p1}", "?1"))).bind(user_id);
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("actor contribution repos failed: {e}"))?;
            collect!(rows)
        }
    }
}
