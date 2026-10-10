//! Bulk repo read-access predicates (DEBT-06 follow-up). Dialect SQL only.
//!
//! These queries mirror `oxidean-api`'s `effective_capability`/`coalesce`
//! read rules so access-loss pruning can check many repos/users in one round
//! trip instead of an N×M capability walk:
//!   * personal owner of a `user`-owned repo → read (owner row must exist)
//!   * org owner/admin member of the owning org → read
//!   * org member + org `member_base_permission` read|write → read
//!   * `repository_collaborators` row with read|write|admin → read
//!   * non-`private` visibility → read for everyone
//!
//! Values pass through `LOWER(TRIM(...))` to match the Rust parse helpers.

use std::collections::HashSet;

use crate::pool::DbPool;

/// Which of `repo_ids` `user_id` can still read. Missing/deleted repos are
/// simply absent from the result — callers treat the diff as access lost.
pub async fn readable_repo_ids(
    pool: &DbPool,
    user_id: &str,
    repo_ids: &[String],
) -> Result<HashSet<String>, String> {
    if repo_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let ids = match pool {
        DbPool::Postgres(p) => {
            sqlx::query_scalar::<_, String>(
                "SELECT r.id FROM repositories r
                 WHERE r.id = ANY($2) AND r.deleted_at IS NULL AND (
                     LOWER(r.visibility) <> 'private'
                     OR (LOWER(r.owner_type) = 'user' AND r.owner_id = $1
                         AND EXISTS (SELECT 1 FROM users u WHERE u.id = r.owner_id))
                     OR EXISTS (SELECT 1 FROM repository_collaborators c
                                WHERE c.repo_id = r.id AND c.user_id = $1
                                  AND LOWER(TRIM(c.permission)) IN ('read', 'write', 'admin'))
                     OR (LOWER(r.owner_type) = 'org' AND EXISTS (
                         SELECT 1 FROM organization_members m
                         JOIN organizations o ON o.id = m.org_id
                         WHERE m.org_id = r.owner_id AND m.user_id = $1
                           AND (LOWER(TRIM(m.role)) IN ('owner', 'admin')
                                OR (LOWER(TRIM(m.role)) = 'member'
                                    AND LOWER(TRIM(o.member_base_permission)) IN ('read', 'write')))))
                 )",
            )
            .bind(user_id)
            .bind(repo_ids)
            .fetch_all(p)
            .await
            .map_err(|e| format!("readable repo ids failed: {e}"))?
        }
        DbPool::MySql(p) => {
            let in_list =
                crate::dialect::in_placeholders(crate::dialect::Dialect::MySql, 1, repo_ids.len());
            let q_str = format!(
                "SELECT r.id FROM repositories r
                 WHERE r.id IN ({in_list}) AND r.deleted_at IS NULL AND (
                     LOWER(r.visibility) <> 'private'
                     OR (LOWER(r.owner_type) = 'user' AND r.owner_id = ?
                         AND EXISTS (SELECT 1 FROM users u WHERE u.id = r.owner_id))
                     OR EXISTS (SELECT 1 FROM repository_collaborators c
                                WHERE c.repo_id = r.id AND c.user_id = ?
                                  AND LOWER(TRIM(c.permission)) IN ('read', 'write', 'admin'))
                     OR (LOWER(r.owner_type) = 'org' AND EXISTS (
                         SELECT 1 FROM organization_members m
                         JOIN organizations o ON o.id = m.org_id
                         WHERE m.org_id = r.owner_id AND m.user_id = ?
                           AND (LOWER(TRIM(m.role)) IN ('owner', 'admin')
                                OR (LOWER(TRIM(m.role)) = 'member'
                                    AND LOWER(TRIM(o.member_base_permission)) IN ('read', 'write')))))
                 )",
            );
            let q = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(&*q_str));
            // `?` binds are positional: ids first (the IN list), then uid ×3.
            let q = repo_ids.iter().fold(q, |q, id| q.bind(id));
            let q = q.bind(user_id).bind(user_id).bind(user_id);
            q.fetch_all(p)
                .await
                .map_err(|e| format!("readable repo ids failed: {e}"))?
        }
        DbPool::Sqlite(p) => {
            let in_list =
                crate::dialect::in_placeholders(crate::dialect::Dialect::Sqlite, 2, repo_ids.len());
            let q_str = format!(
                "SELECT r.id FROM repositories r
                 WHERE r.id IN ({in_list}) AND r.deleted_at IS NULL AND (
                     LOWER(r.visibility) <> 'private'
                     OR (LOWER(r.owner_type) = 'user' AND r.owner_id = ?1
                         AND EXISTS (SELECT 1 FROM users u WHERE u.id = r.owner_id))
                     OR EXISTS (SELECT 1 FROM repository_collaborators c
                                WHERE c.repo_id = r.id AND c.user_id = ?1
                                  AND LOWER(TRIM(c.permission)) IN ('read', 'write', 'admin'))
                     OR (LOWER(r.owner_type) = 'org' AND EXISTS (
                         SELECT 1 FROM organization_members m
                         JOIN organizations o ON o.id = m.org_id
                         WHERE m.org_id = r.owner_id AND m.user_id = ?1
                           AND (LOWER(TRIM(m.role)) IN ('owner', 'admin')
                                OR (LOWER(TRIM(m.role)) = 'member'
                                    AND LOWER(TRIM(o.member_base_permission)) IN ('read', 'write')))))
                 )",
            );
            let q = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(&*q_str));
            let q = q.bind(user_id);
            let q = repo_ids.iter().fold(q, |q, id| q.bind(id));
            q.fetch_all(p)
                .await
                .map_err(|e| format!("readable repo ids failed: {e}"))?
        }
    };
    Ok(ids.into_iter().collect())
}

/// Which of `user_ids` can still read `repo_id`. Callers are expected to
/// shortcut non-private repos (everyone reads) and missing/deleted repos
/// (nobody reads) before calling — the query covers the private-repo grants.
pub async fn readers_of_repo(
    pool: &DbPool,
    repo_id: &str,
    user_ids: &[String],
) -> Result<HashSet<String>, String> {
    if user_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let ids = match pool {
        DbPool::Postgres(p) => sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT sub.uid FROM (
                     SELECT r.owner_id AS uid FROM repositories r
                       WHERE r.id = $1 AND LOWER(r.owner_type) = 'user'
                         AND EXISTS (SELECT 1 FROM users u WHERE u.id = r.owner_id)
                     UNION
                     SELECT c.user_id FROM repository_collaborators c
                       WHERE c.repo_id = $1
                         AND LOWER(TRIM(c.permission)) IN ('read', 'write', 'admin')
                     UNION
                     SELECT m.user_id FROM organization_members m
                       JOIN organizations o ON o.id = m.org_id
                       JOIN repositories r2 ON r2.owner_id = o.id
                       WHERE r2.id = $1 AND LOWER(r2.owner_type) = 'org'
                         AND (LOWER(TRIM(m.role)) IN ('owner', 'admin')
                              OR (LOWER(TRIM(m.role)) = 'member'
                                  AND LOWER(TRIM(o.member_base_permission)) IN ('read', 'write')))
                 ) sub WHERE sub.uid = ANY($2)",
        )
        .bind(repo_id)
        .bind(user_ids)
        .fetch_all(p)
        .await
        .map_err(|e| format!("repo readers failed: {e}"))?,
        DbPool::MySql(p) => {
            let in_list =
                crate::dialect::in_placeholders(crate::dialect::Dialect::MySql, 1, user_ids.len());
            let q_str = format!(
                "SELECT DISTINCT sub.uid FROM (
                     SELECT r.owner_id AS uid FROM repositories r
                       WHERE r.id = ? AND LOWER(r.owner_type) = 'user'
                         AND EXISTS (SELECT 1 FROM users u WHERE u.id = r.owner_id)
                     UNION
                     SELECT c.user_id FROM repository_collaborators c
                       WHERE c.repo_id = ?
                         AND LOWER(TRIM(c.permission)) IN ('read', 'write', 'admin')
                     UNION
                     SELECT m.user_id FROM organization_members m
                       JOIN organizations o ON o.id = m.org_id
                       JOIN repositories r2 ON r2.owner_id = o.id
                       WHERE r2.id = ? AND LOWER(r2.owner_type) = 'org'
                         AND (LOWER(TRIM(m.role)) IN ('owner', 'admin')
                              OR (LOWER(TRIM(m.role)) = 'member'
                                  AND LOWER(TRIM(o.member_base_permission)) IN ('read', 'write')))
                 ) sub WHERE sub.uid IN ({in_list})",
            );
            let q = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(&*q_str));
            // `?` binds are positional: repo_id ×3 (UNION arms), then uids.
            let q = q.bind(repo_id).bind(repo_id).bind(repo_id);
            let q = user_ids.iter().fold(q, |q, id| q.bind(id));
            q.fetch_all(p)
                .await
                .map_err(|e| format!("repo readers failed: {e}"))?
        }
        DbPool::Sqlite(p) => {
            let in_list =
                crate::dialect::in_placeholders(crate::dialect::Dialect::Sqlite, 2, user_ids.len());
            let q_str = format!(
                "SELECT DISTINCT sub.uid FROM (
                     SELECT r.owner_id AS uid FROM repositories r
                       WHERE r.id = ?1 AND LOWER(r.owner_type) = 'user'
                         AND EXISTS (SELECT 1 FROM users u WHERE u.id = r.owner_id)
                     UNION
                     SELECT c.user_id FROM repository_collaborators c
                       WHERE c.repo_id = ?1
                         AND LOWER(TRIM(c.permission)) IN ('read', 'write', 'admin')
                     UNION
                     SELECT m.user_id FROM organization_members m
                       JOIN organizations o ON o.id = m.org_id
                       JOIN repositories r2 ON r2.owner_id = o.id
                       WHERE r2.id = ?1 AND LOWER(r2.owner_type) = 'org'
                         AND (LOWER(TRIM(m.role)) IN ('owner', 'admin')
                              OR (LOWER(TRIM(m.role)) = 'member'
                                  AND LOWER(TRIM(o.member_base_permission)) IN ('read', 'write')))
                 ) sub WHERE sub.uid IN ({in_list})",
            );
            let q = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(&*q_str));
            let q = q.bind(repo_id);
            let q = user_ids.iter().fold(q, |q, id| q.bind(id));
            q.fetch_all(p)
                .await
                .map_err(|e| format!("repo readers failed: {e}"))?
        }
    };
    Ok(ids.into_iter().collect())
}
