//! Repository CRUD via `DbPool` match — dialect branching stays in this crate.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct RepositoryRow {
    pub id: String,
    pub owner_id: String,
    /// Polymorphic owner discriminant: `user` | `org` (D-ORG-01).
    pub owner_type: String,
    pub name: String,
    pub visibility: String,
    pub description: String,
    pub default_branch: String,
    /// Cached on-disk size of the bare repo in bytes (GIT-25 bookkeeping).
    pub size_bytes: i64,
    /// Per-repo git object size quota override; NULL = inherit instance default
    /// (GIT-25). `<= 0` is stored but treated as unlimited by enforcement.
    pub size_quota_bytes: Option<i64>,
    pub deleted_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

macro_rules! map_repo {
    ($row:expr) => {{
        let row = $row;
        RepositoryRow {
            id: row.try_get("id").map_err(|e| format!("repo row: {e}"))?,
            owner_id: row
                .try_get("owner_id")
                .map_err(|e| format!("repo row: {e}"))?,
            owner_type: row
                .try_get("owner_type")
                .map_err(|e| format!("repo row: {e}"))?,
            name: row.try_get("name").map_err(|e| format!("repo row: {e}"))?,
            visibility: row
                .try_get("visibility")
                .map_err(|e| format!("repo row: {e}"))?,
            description: row
                .try_get("description")
                .map_err(|e| format!("repo row: {e}"))?,
            default_branch: row
                .try_get("default_branch")
                .map_err(|e| format!("repo row: {e}"))?,
            size_bytes: row
                .try_get("size_bytes")
                .map_err(|e| format!("repo row: {e}"))?,
            size_quota_bytes: row
                .try_get("size_quota_bytes")
                .map_err(|e| format!("repo row: {e}"))?,
            deleted_at: row
                .try_get("deleted_at")
                .map_err(|e| format!("repo row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("repo row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("repo row: {e}"))?,
        }
    }};
}

const REPO_SELECT_PG: &str = "SELECT id, owner_id, owner_type, name, visibility, description, default_branch, size_bytes, size_quota_bytes,
       CASE WHEN deleted_at IS NULL THEN NULL
            ELSE to_char(deleted_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') END AS deleted_at,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at
FROM repositories";

const REPO_SELECT_MYSQL: &str = "SELECT id, owner_id, owner_type, name, visibility, description, default_branch, size_bytes, size_quota_bytes,
       CASE WHEN deleted_at IS NULL THEN NULL
            ELSE DATE_FORMAT(deleted_at, '%Y-%m-%dT%H:%i:%sZ') END AS deleted_at,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
FROM repositories";

const REPO_SELECT_SQLITE: &str = "SELECT id, owner_id, owner_type, name, visibility, description, default_branch, size_bytes, size_quota_bytes,
       CASE WHEN deleted_at IS NULL THEN NULL
            ELSE strftime('%Y-%m-%dT%H:%M:%SZ', deleted_at) END AS deleted_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at
FROM repositories";

pub async fn insert_repository(
    pool: &DbPool,
    id: &str,
    owner_id: &str,
    owner_type: &str,
    name: &str,
    visibility: &str,
    description: &str,
    default_branch: &str,
) -> Result<RepositoryRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO repositories (id, owner_id, owner_type, name, visibility, description, default_branch)
VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(id)
            .bind(owner_id)
            .bind(owner_type)
            .bind(name)
            .bind(visibility)
            .bind(description)
            .bind(default_branch)
            .execute(p)
            .await
            .map_err(|e| format!("insert repository failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO repositories (id, owner_id, owner_type, name, visibility, description, default_branch)
VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(owner_id)
            .bind(owner_type)
            .bind(name)
            .bind(visibility)
            .bind(description)
            .bind(default_branch)
            .execute(p)
            .await
            .map_err(|e| format!("insert repository failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO repositories (id, owner_id, owner_type, name, visibility, description, default_branch)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .bind(id)
            .bind(owner_id)
            .bind(owner_type)
            .bind(name)
            .bind(visibility)
            .bind(description)
            .bind(default_branch)
            .execute(p)
            .await
            .map_err(|e| format!("insert repository failed: {e}"))?;
        }
    }
    find_by_owner_and_name(pool, owner_id, name)
        .await?
        .ok_or_else(|| "insert repository failed: row missing after insert".into())
}

/// Lookup by owner + name among non-deleted rows (case-insensitive name match).
pub async fn find_by_owner_and_name(
    pool: &DbPool,
    owner_id: &str,
    name: &str,
) -> Result<Option<RepositoryRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(&format!(
                "{REPO_SELECT_PG} WHERE owner_id = $1 AND lower(name) = lower($2) AND deleted_at IS NULL"
            ))
            .bind(owner_id)
            .bind(name)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find repository failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_repo!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(&format!(
                "{REPO_SELECT_MYSQL} WHERE owner_id = ? AND LOWER(name) = LOWER(?) AND deleted_at IS NULL"
            ))
            .bind(owner_id)
            .bind(name)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find repository failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_repo!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(&format!(
                "{REPO_SELECT_SQLITE} WHERE owner_id = ?1 AND lower(name) = lower(?2) AND deleted_at IS NULL"
            ))
            .bind(owner_id)
            .bind(name)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find repository failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_repo!(&r)),
                None => None,
            })
        }
    }
}

/// List non-deleted repos for an owner, most recently updated first (GIT-01 / D-13).
pub async fn list_by_owner(
    pool: &DbPool,
    owner_id: &str,
) -> Result<Vec<RepositoryRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(&format!(
                "{REPO_SELECT_PG} WHERE owner_id = $1 AND deleted_at IS NULL ORDER BY updated_at DESC"
            ))
            .bind(owner_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list repositories failed: {e}"))?;
            rows.iter().map(|r| Ok(map_repo!(r))).collect()
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(&format!(
                "{REPO_SELECT_MYSQL} WHERE owner_id = ? AND deleted_at IS NULL ORDER BY updated_at DESC"
            ))
            .bind(owner_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list repositories failed: {e}"))?;
            rows.iter().map(|r| Ok(map_repo!(r))).collect()
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(&format!(
                "{REPO_SELECT_SQLITE} WHERE owner_id = ?1 AND deleted_at IS NULL ORDER BY updated_at DESC"
            ))
            .bind(owner_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list repositories failed: {e}"))?;
            rows.iter().map(|r| Ok(map_repo!(r))).collect()
        }
    }
}

/// Update repository name for a non-deleted row (GIT-16 / D-REL-07).
pub async fn update_name(pool: &DbPool, id: &str, name: &str) -> Result<RepositoryRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            let n = sqlx::query(
                "UPDATE repositories SET name = $2, updated_at = now()
WHERE id = $1 AND deleted_at IS NULL",
            )
            .bind(id)
            .bind(name)
            .execute(p)
            .await
            .map_err(|e| format!("update repository name failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("repository not found".into());
            }
        }
        DbPool::MySql(p) => {
            let n = sqlx::query(
                "UPDATE repositories SET name = ?, updated_at = NOW()
WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(name)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("update repository name failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("repository not found".into());
            }
        }
        DbPool::Sqlite(p) => {
            let n = sqlx::query(
                "UPDATE repositories SET name = ?2, updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
WHERE id = ?1 AND deleted_at IS NULL",
            )
            .bind(id)
            .bind(name)
            .execute(p)
            .await
            .map_err(|e| format!("update repository name failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("repository not found".into());
            }
        }
    }
    find_by_id(pool, id)
        .await?
        .ok_or_else(|| "repository not found after name update".into())
}

/// Rewrite polymorphic owner for transfer (GIT-17 / D-REL-10).
pub async fn update_owner(
    pool: &DbPool,
    id: &str,
    owner_id: &str,
    owner_type: &str,
) -> Result<RepositoryRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            let n = sqlx::query(
                "UPDATE repositories SET owner_id = $2, owner_type = $3, updated_at = now()
WHERE id = $1 AND deleted_at IS NULL",
            )
            .bind(id)
            .bind(owner_id)
            .bind(owner_type)
            .execute(p)
            .await
            .map_err(|e| format!("update repository owner failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("repository not found".into());
            }
        }
        DbPool::MySql(p) => {
            let n = sqlx::query(
                "UPDATE repositories SET owner_id = ?, owner_type = ?, updated_at = NOW()
WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(owner_id)
            .bind(owner_type)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("update repository owner failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("repository not found".into());
            }
        }
        DbPool::Sqlite(p) => {
            let n = sqlx::query(
                "UPDATE repositories SET owner_id = ?2, owner_type = ?3, updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
WHERE id = ?1 AND deleted_at IS NULL",
            )
            .bind(id)
            .bind(owner_id)
            .bind(owner_type)
            .execute(p)
            .await
            .map_err(|e| format!("update repository owner failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("repository not found".into());
            }
        }
    }
    find_by_id(pool, id)
        .await?
        .ok_or_else(|| "repository not found after owner update".into())
}

/// Update visibility for a non-deleted repository (D-26).
pub async fn update_visibility(
    pool: &DbPool,
    id: &str,
    visibility: &str,
) -> Result<RepositoryRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            let n = sqlx::query(
                "UPDATE repositories SET visibility = $2, updated_at = now()
WHERE id = $1 AND deleted_at IS NULL",
            )
            .bind(id)
            .bind(visibility)
            .execute(p)
            .await
            .map_err(|e| format!("update repository visibility failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("repository not found".into());
            }
        }
        DbPool::MySql(p) => {
            let n = sqlx::query(
                "UPDATE repositories SET visibility = ?, updated_at = NOW()
WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(visibility)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("update repository visibility failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("repository not found".into());
            }
        }
        DbPool::Sqlite(p) => {
            let n = sqlx::query(
                "UPDATE repositories SET visibility = ?2, updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
WHERE id = ?1 AND deleted_at IS NULL",
            )
            .bind(id)
            .bind(visibility)
            .execute(p)
            .await
            .map_err(|e| format!("update repository visibility failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("repository not found".into());
            }
        }
    }
    find_by_id(pool, id)
        .await?
        .ok_or_else(|| "repository not found after visibility update".into())
}

/// Persist the measured on-disk size of the bare repo (GIT-25 bookkeeping).
pub async fn update_size_bytes(pool: &DbPool, id: &str, size_bytes: i64) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE repositories SET size_bytes = $2 WHERE id = $1")
                .bind(id)
                .bind(size_bytes)
                .execute(p)
                .await
                .map_err(|e| format!("update repository size_bytes failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE repositories SET size_bytes = ? WHERE id = ?")
                .bind(size_bytes)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("update repository size_bytes failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE repositories SET size_bytes = ?2 WHERE id = ?1")
                .bind(id)
                .bind(size_bytes)
                .execute(p)
                .await
                .map_err(|e| format!("update repository size_bytes failed: {e}"))?;
        }
    }
    Ok(())
}

/// Set/clear the per-repo git object size quota override (GIT-25).
/// `None` restores the instance default; `Some(v <= 0)` is stored but treated
/// as unlimited by the enforcement path.
pub async fn update_size_quota_bytes(
    pool: &DbPool,
    id: &str,
    size_quota_bytes: Option<i64>,
) -> Result<RepositoryRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            let n = sqlx::query(
                "UPDATE repositories SET size_quota_bytes = $2, updated_at = now()
WHERE id = $1 AND deleted_at IS NULL",
            )
            .bind(id)
            .bind(size_quota_bytes)
            .execute(p)
            .await
            .map_err(|e| format!("update repository size quota failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("repository not found".into());
            }
        }
        DbPool::MySql(p) => {
            let n = sqlx::query(
                "UPDATE repositories SET size_quota_bytes = ?, updated_at = NOW()
WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(size_quota_bytes)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("update repository size quota failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("repository not found".into());
            }
        }
        DbPool::Sqlite(p) => {
            let n = sqlx::query(
                "UPDATE repositories SET size_quota_bytes = ?2, updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
WHERE id = ?1 AND deleted_at IS NULL",
            )
            .bind(id)
            .bind(size_quota_bytes)
            .execute(p)
            .await
            .map_err(|e| format!("update repository size quota failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("repository not found".into());
            }
        }
    }
    find_by_id(pool, id)
        .await?
        .ok_or_else(|| "repository not found after size quota update".into())
}

/// Soft-delete: set `deleted_at` (disk purge deferred — D-35).
/// When soft-deleting a fork, recount `fork_count` for its network (issue #23).
pub async fn soft_delete(pool: &DbPool, id: &str) -> Result<(), String> {
    let network_id = get_fork_network_id_for_repo(pool, id).await?;
    let was_fork = get_forked_from_repo_id(pool, id).await?.is_some();

    match pool {
        DbPool::Postgres(p) => {
            let n = sqlx::query(
                "UPDATE repositories SET deleted_at = now(), updated_at = now()
WHERE id = $1 AND deleted_at IS NULL",
            )
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("soft-delete repository failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("repository not found".into());
            }
        }
        DbPool::MySql(p) => {
            let n = sqlx::query(
                "UPDATE repositories SET deleted_at = NOW(), updated_at = NOW()
WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("soft-delete repository failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("repository not found".into());
            }
        }
        DbPool::Sqlite(p) => {
            let n = sqlx::query(
                "UPDATE repositories SET deleted_at = strftime('%Y-%m-%d %H:%M:%S','now'),
updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
WHERE id = ?1 AND deleted_at IS NULL",
            )
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("soft-delete repository failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("repository not found".into());
            }
        }
    }

    if was_fork {
        if let Some(nid) = network_id {
            recount_fork_count_for_network(pool, &nid).await?;
        }
    }
    Ok(())
}

/// On-disk identity for a repository row (active or soft-deleted).
#[derive(Debug, Clone)]
pub struct RepoDiskRef {
    pub id: String,
    pub owner_username: String,
    pub name: String,
    pub deleted_at: Option<String>,
}

/// Resolve disk-path slug for user- or org-owned repos (D-ORG-01).
const DISK_REF_SELECT_PG: &str = "SELECT r.id,
       CASE WHEN r.owner_type = 'org' THEN o.slug ELSE u.username END AS owner_username,
       r.name,
       CASE WHEN r.deleted_at IS NULL THEN NULL
            ELSE to_char(r.deleted_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') END AS deleted_at
FROM repositories r
LEFT JOIN users u ON r.owner_type = 'user' AND u.id = r.owner_id
LEFT JOIN organizations o ON r.owner_type = 'org' AND o.id = r.owner_id";

const DISK_REF_SELECT_MYSQL: &str = "SELECT r.id,
       CASE WHEN r.owner_type = 'org' THEN o.slug ELSE u.username END AS owner_username,
       r.name,
       CASE WHEN r.deleted_at IS NULL THEN NULL
            ELSE DATE_FORMAT(r.deleted_at, '%Y-%m-%dT%H:%i:%sZ') END AS deleted_at
FROM repositories r
LEFT JOIN users u ON r.owner_type = 'user' AND u.id = r.owner_id
LEFT JOIN organizations o ON r.owner_type = 'org' AND o.id = r.owner_id";

const DISK_REF_SELECT_SQLITE: &str = "SELECT r.id,
       CASE WHEN r.owner_type = 'org' THEN o.slug ELSE u.username END AS owner_username,
       r.name,
       CASE WHEN r.deleted_at IS NULL THEN NULL
            ELSE strftime('%Y-%m-%dT%H:%M:%SZ', r.deleted_at) END AS deleted_at
FROM repositories r
LEFT JOIN users u ON r.owner_type = 'user' AND u.id = r.owner_id
LEFT JOIN organizations o ON r.owner_type = 'org' AND o.id = r.owner_id";

/// All repository rows that still own a disk path (active + soft-deleted).
pub async fn list_repo_disk_refs(pool: &DbPool) -> Result<Vec<RepoDiskRef>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(DISK_REF_SELECT_PG)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list repo disk refs failed: {e}"))?;
            rows.into_iter()
                .map(|row| {
                    Ok(RepoDiskRef {
                        id: row.try_get("id").map_err(|e| format!("repo disk ref: {e}"))?,
                        owner_username: row
                            .try_get("owner_username")
                            .map_err(|e| format!("repo disk ref: {e}"))?,
                        name: row.try_get("name").map_err(|e| format!("repo disk ref: {e}"))?,
                        deleted_at: row
                            .try_get("deleted_at")
                            .map_err(|e| format!("repo disk ref: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(DISK_REF_SELECT_MYSQL)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list repo disk refs failed: {e}"))?;
            rows.into_iter()
                .map(|row| {
                    Ok(RepoDiskRef {
                        id: row.try_get("id").map_err(|e| format!("repo disk ref: {e}"))?,
                        owner_username: row
                            .try_get("owner_username")
                            .map_err(|e| format!("repo disk ref: {e}"))?,
                        name: row.try_get("name").map_err(|e| format!("repo disk ref: {e}"))?,
                        deleted_at: row
                            .try_get("deleted_at")
                            .map_err(|e| format!("repo disk ref: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(DISK_REF_SELECT_SQLITE)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list repo disk refs failed: {e}"))?;
            rows.into_iter()
                .map(|row| {
                    Ok(RepoDiskRef {
                        id: row.try_get("id").map_err(|e| format!("repo disk ref: {e}"))?,
                        owner_username: row
                            .try_get("owner_username")
                            .map_err(|e| format!("repo disk ref: {e}"))?,
                        name: row.try_get("name").map_err(|e| format!("repo disk ref: {e}"))?,
                        deleted_at: row
                            .try_get("deleted_at")
                            .map_err(|e| format!("repo disk ref: {e}"))?,
                    })
                })
                .collect()
        }
    }
}

/// Hard-delete a repository row (after disk purge — D-35/D-36).
pub async fn hard_delete(pool: &DbPool, id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM repositories WHERE id = $1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("hard-delete repository failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM repositories WHERE id = ?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("hard-delete repository failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM repositories WHERE id = ?1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("hard-delete repository failed: {e}"))?;
        }
    }
    Ok(())
}

/// Hard-delete all repositories for an owner (including soft-deleted rows).
pub async fn hard_delete_by_owner(
    pool: &DbPool,
    owner_id: &str,
    owner_type: &str,
) -> Result<u64, String> {
    let n = match pool {
        DbPool::Postgres(p) => sqlx::query(
            "DELETE FROM repositories WHERE owner_id = $1 AND owner_type = $2",
        )
        .bind(owner_id)
        .bind(owner_type)
        .execute(p)
        .await
        .map_err(|e| format!("hard-delete repositories by owner failed: {e}"))?
        .rows_affected(),
        DbPool::MySql(p) => sqlx::query(
            "DELETE FROM repositories WHERE owner_id = ? AND owner_type = ?",
        )
        .bind(owner_id)
        .bind(owner_type)
        .execute(p)
        .await
        .map_err(|e| format!("hard-delete repositories by owner failed: {e}"))?
        .rows_affected(),
        DbPool::Sqlite(p) => sqlx::query(
            "DELETE FROM repositories WHERE owner_id = ?1 AND owner_type = ?2",
        )
        .bind(owner_id)
        .bind(owner_type)
        .execute(p)
        .await
        .map_err(|e| format!("hard-delete repositories by owner failed: {e}"))?
        .rows_affected(),
    };
    Ok(n)
}

pub async fn find_by_id(pool: &DbPool, id: &str) -> Result<Option<RepositoryRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(&format!(
                "{REPO_SELECT_PG} WHERE id = $1 AND deleted_at IS NULL"
            ))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find repository by id failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_repo!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(&format!(
                "{REPO_SELECT_MYSQL} WHERE id = ? AND deleted_at IS NULL"
            ))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find repository by id failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_repo!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(&format!(
                "{REPO_SELECT_SQLITE} WHERE id = ?1 AND deleted_at IS NULL"
            ))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find repository by id failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_repo!(&r)),
                None => None,
            })
        }
    }
}

/// Batch `find_by_id` — one `IN (...)` round trip (pull list head-repo enrichment).
pub async fn find_many_by_id(pool: &DbPool, ids: &[String]) -> Result<Vec<RepositoryRow>, String> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(&format!(
                "{REPO_SELECT_PG} WHERE id = ANY($1) AND deleted_at IS NULL"
            ))
            .bind(ids)
            .fetch_all(p)
            .await
            .map_err(|e| format!("find repositories by ids failed: {e}"))?;
            rows.iter().map(|r| Ok(map_repo!(r))).collect()
        }
        DbPool::MySql(p) => {
            let in_list =
                crate::dialect::in_placeholders(crate::dialect::Dialect::MySql, 1, ids.len());
            let q_str =
                format!("{REPO_SELECT_MYSQL} WHERE id IN ({in_list}) AND deleted_at IS NULL");
            let q = sqlx::query(&q_str);
            let q = ids.iter().fold(q, |q, id| q.bind(id));
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("find repositories by ids failed: {e}"))?;
            rows.iter().map(|r| Ok(map_repo!(r))).collect()
        }
        DbPool::Sqlite(p) => {
            let in_list =
                crate::dialect::in_placeholders(crate::dialect::Dialect::Sqlite, 1, ids.len());
            let q_str =
                format!("{REPO_SELECT_SQLITE} WHERE id IN ({in_list}) AND deleted_at IS NULL");
            let q = sqlx::query(&q_str);
            let q = ids.iter().fold(q, |q, id| q.bind(id));
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("find repositories by ids failed: {e}"))?;
            rows.iter().map(|r| Ok(map_repo!(r))).collect()
        }
    }
}

/// Homepage URL / text (issue #23) — separate get to avoid rewriting REPO_SELECT.
pub async fn get_homepage(pool: &DbPool, id: &str) -> Result<String, String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT COALESCE(homepage, '') FROM repositories WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(p)
        .await
        .map_err(|e| format!("get_homepage: {e}"))
        .map(|o| o.unwrap_or_default()),
        DbPool::MySql(p) => sqlx::query_scalar(
            "SELECT COALESCE(homepage, '') FROM repositories WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(p)
        .await
        .map_err(|e| format!("get_homepage: {e}"))
        .map(|o| o.unwrap_or_default()),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT COALESCE(homepage, '') FROM repositories WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(p)
        .await
        .map_err(|e| format!("get_homepage: {e}"))
        .map(|o| o.unwrap_or_default()),
    }
}

/// Update description + homepage + updated_at (issue #23).
pub async fn update_metadata(
    pool: &DbPool,
    id: &str,
    description: &str,
    homepage: &str,
) -> Result<RepositoryRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            let n = sqlx::query(
                "UPDATE repositories SET description = $2, homepage = $3, updated_at = now()
WHERE id = $1 AND deleted_at IS NULL",
            )
            .bind(id)
            .bind(description)
            .bind(homepage)
            .execute(p)
            .await
            .map_err(|e| format!("update_metadata: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("repository not found".into());
            }
        }
        DbPool::MySql(p) => {
            let n = sqlx::query(
                "UPDATE repositories SET description = ?, homepage = ?, updated_at = NOW()
WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(description)
            .bind(homepage)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("update_metadata: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("repository not found".into());
            }
        }
        DbPool::Sqlite(p) => {
            let n = sqlx::query(
                "UPDATE repositories SET description = ?2, homepage = ?3,
updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
WHERE id = ?1 AND deleted_at IS NULL",
            )
            .bind(id)
            .bind(description)
            .bind(homepage)
            .execute(p)
            .await
            .map_err(|e| format!("update_metadata: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("repository not found".into());
            }
        }
    }
    find_by_id(pool, id)
        .await?
        .ok_or_else(|| "repository not found after metadata update".into())
}

pub async fn get_fork_count(pool: &DbPool, id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT COALESCE(fork_count, 0) FROM repositories WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(p)
        .await
        .map_err(|e| format!("get_fork_count: {e}"))
        .map(|o| o.unwrap_or(0)),
        DbPool::MySql(p) => sqlx::query_scalar(
            "SELECT COALESCE(fork_count, 0) FROM repositories WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(p)
        .await
        .map_err(|e| format!("get_fork_count: {e}"))
        .map(|o| o.unwrap_or(0)),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT COALESCE(fork_count, 0) FROM repositories WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(p)
        .await
        .map_err(|e| format!("get_fork_count: {e}"))
        .map(|o| o.unwrap_or(0)),
    }
}

async fn get_fork_network_id_for_repo(
    pool: &DbPool,
    id: &str,
) -> Result<Option<String>, String> {
    let nested: Option<Option<String>> = match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT fork_network_id FROM repositories WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(p)
        .await
        .map_err(|e| format!("get fork_network_id: {e}"))?,
        DbPool::MySql(p) => {
            sqlx::query_scalar("SELECT fork_network_id FROM repositories WHERE id = ?")
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("get fork_network_id: {e}"))?
        }
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT fork_network_id FROM repositories WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(p)
        .await
        .map_err(|e| format!("get fork_network_id: {e}"))?,
    };
    Ok(nested.flatten())
}

async fn get_forked_from_repo_id(pool: &DbPool, id: &str) -> Result<Option<String>, String> {
    let nested: Option<Option<String>> = match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT forked_from_repo_id FROM repositories WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(p)
        .await
        .map_err(|e| format!("get forked_from: {e}"))?,
        DbPool::MySql(p) => {
            sqlx::query_scalar("SELECT forked_from_repo_id FROM repositories WHERE id = ?")
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("get forked_from: {e}"))?
        }
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT forked_from_repo_id FROM repositories WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(p)
        .await
        .map_err(|e| format!("get forked_from: {e}"))?,
    };
    Ok(nested.flatten())
}

/// Count active forks in a network and set `fork_count` on every repo in that network.
pub async fn recount_fork_count_for_network(
    pool: &DbPool,
    network_id: &str,
) -> Result<i64, String> {
    let count: i64 = match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM repositories
             WHERE fork_network_id = $1
               AND forked_from_repo_id IS NOT NULL
               AND deleted_at IS NULL",
        )
        .bind(network_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("count forks: {e}"))?,
        DbPool::MySql(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM repositories
             WHERE fork_network_id = ?
               AND forked_from_repo_id IS NOT NULL
               AND deleted_at IS NULL",
        )
        .bind(network_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("count forks: {e}"))?,
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM repositories
             WHERE fork_network_id = ?1
               AND forked_from_repo_id IS NOT NULL
               AND deleted_at IS NULL",
        )
        .bind(network_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("count forks: {e}"))?,
    };

    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE repositories SET fork_count = $2 WHERE fork_network_id = $1",
            )
            .bind(network_id)
            .bind(count)
            .execute(p)
            .await
            .map_err(|e| format!("set fork_count: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE repositories SET fork_count = ? WHERE fork_network_id = ?")
                .bind(count)
                .bind(network_id)
                .execute(p)
                .await
                .map_err(|e| format!("set fork_count: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE repositories SET fork_count = ?2 WHERE fork_network_id = ?1")
                .bind(network_id)
                .bind(count)
                .execute(p)
                .await
                .map_err(|e| format!("set fork_count: {e}"))?;
        }
    }
    Ok(count)
}

/// Alias used by Database wrapper naming.
pub async fn recount_fork_network(pool: &DbPool, network_id: &str) -> Result<i64, String> {
    recount_fork_count_for_network(pool, network_id).await
}
