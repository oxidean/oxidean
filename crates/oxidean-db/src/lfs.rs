//! LFS object / link persistence (D-LFS-02, D-LFS-03).

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct LfsObjectRow {
    pub oid: String,
    pub size: i64,
    pub created_at: String,
}

pub async fn get_lfs_enabled(pool: &DbPool, repo_id: &str) -> Result<bool, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query("SELECT lfs_enabled FROM repositories WHERE id = ?")
                .bind(repo_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("get_lfs_enabled: {e}"))?;
            Ok(row
                .map(|r| r.try_get::<i64, _>("lfs_enabled").unwrap_or(0) != 0)
                .unwrap_or(false))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query("SELECT lfs_enabled FROM repositories WHERE id = $1")
                .bind(repo_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("get_lfs_enabled: {e}"))?;
            Ok(row
                .map(|r| r.try_get::<bool, _>("lfs_enabled").unwrap_or(false))
                .unwrap_or(false))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query("SELECT lfs_enabled FROM repositories WHERE id = ?")
                .bind(repo_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("get_lfs_enabled: {e}"))?;
            Ok(row
                .map(|r| r.try_get::<i8, _>("lfs_enabled").unwrap_or(0) != 0)
                .unwrap_or(false))
        }
    }
}

pub async fn set_lfs_enabled(pool: &DbPool, repo_id: &str, enabled: bool) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE repositories SET lfs_enabled = ? WHERE id = ?")
                .bind(if enabled { 1 } else { 0 })
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| format!("set_lfs_enabled: {e}"))?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE repositories SET lfs_enabled = $1 WHERE id = $2")
                .bind(enabled)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| format!("set_lfs_enabled: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE repositories SET lfs_enabled = ? WHERE id = ?")
                .bind(if enabled { 1 } else { 0 })
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| format!("set_lfs_enabled: {e}"))?;
        }
    }
    Ok(())
}

pub async fn find_lfs_object(pool: &DbPool, oid: &str) -> Result<Option<LfsObjectRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                "SELECT oid, size, strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at FROM lfs_objects WHERE oid = ?",
            )
            .bind(oid)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find_lfs_object: {e}"))?;
            row.map(|r| {
                Ok(LfsObjectRow {
                    oid: r.try_get("oid").map_err(|e| format!("oid: {e}"))?,
                    size: r.try_get("size").map_err(|e| format!("size: {e}"))?,
                    created_at: r
                        .try_get("created_at")
                        .map_err(|e| format!("created_at: {e}"))?,
                })
            })
            .transpose()
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                "SELECT oid, size, to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at FROM lfs_objects WHERE oid = $1",
            )
            .bind(oid)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find_lfs_object: {e}"))?;
            row.map(|r| {
                Ok(LfsObjectRow {
                    oid: r.try_get("oid").map_err(|e| format!("oid: {e}"))?,
                    size: r.try_get("size").map_err(|e| format!("size: {e}"))?,
                    created_at: r
                        .try_get("created_at")
                        .map_err(|e| format!("created_at: {e}"))?,
                })
            })
            .transpose()
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                "SELECT oid, size, DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at FROM lfs_objects WHERE oid = ?",
            )
            .bind(oid)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find_lfs_object: {e}"))?;
            row.map(|r| {
                Ok(LfsObjectRow {
                    oid: r.try_get("oid").map_err(|e| format!("oid: {e}"))?,
                    size: r
                        .try_get::<i64, _>("size")
                        .map_err(|e| format!("size: {e}"))?,
                    created_at: r
                        .try_get("created_at")
                        .map_err(|e| format!("created_at: {e}"))?,
                })
            })
            .transpose()
        }
    }
}

pub async fn upsert_lfs_object(pool: &DbPool, oid: &str, size: i64) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO lfs_objects (oid, size) VALUES (?, ?) ON CONFLICT(oid) DO NOTHING",
            )
            .bind(oid)
            .bind(size)
            .execute(p)
            .await
            .map_err(|e| format!("upsert_lfs_object: {e}"))?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO lfs_objects (oid, size) VALUES ($1, $2) ON CONFLICT (oid) DO NOTHING",
            )
            .bind(oid)
            .bind(size)
            .execute(p)
            .await
            .map_err(|e| format!("upsert_lfs_object: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO lfs_objects (oid, size) VALUES (?, ?) ON DUPLICATE KEY UPDATE oid = oid",
            )
            .bind(oid)
            .bind(size)
            .execute(p)
            .await
            .map_err(|e| format!("upsert_lfs_object: {e}"))?;
        }
    }
    Ok(())
}

pub async fn link_lfs_object(pool: &DbPool, repository_id: &str, oid: &str) -> Result<(), String> {
    link_lfs_object_as(pool, repository_id, oid, None).await
}

pub async fn link_lfs_object_as(
    pool: &DbPool,
    repository_id: &str,
    oid: &str,
    uploaded_by: Option<&str>,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO lfs_object_links (repository_id, oid, refcount, uploaded_by_user_id) VALUES (?, ?, 1, ?)
                 ON CONFLICT(repository_id, oid) DO UPDATE SET refcount = refcount + 1",
            )
            .bind(repository_id)
            .bind(oid)
            .bind(uploaded_by)
            .execute(p)
            .await
            .map_err(|e| format!("link_lfs_object: {e}"))?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO lfs_object_links (repository_id, oid, refcount, uploaded_by_user_id) VALUES ($1, $2, 1, $3)
                 ON CONFLICT (repository_id, oid) DO UPDATE SET refcount = lfs_object_links.refcount + 1",
            )
            .bind(repository_id)
            .bind(oid)
            .bind(uploaded_by)
            .execute(p)
            .await
            .map_err(|e| format!("link_lfs_object: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO lfs_object_links (repository_id, oid, refcount, uploaded_by_user_id) VALUES (?, ?, 1, ?)
                 ON DUPLICATE KEY UPDATE refcount = refcount + 1",
            )
            .bind(repository_id)
            .bind(oid)
            .bind(uploaded_by)
            .execute(p)
            .await
            .map_err(|e| format!("link_lfs_object: {e}"))?;
        }
    }
    Ok(())
}

pub async fn has_lfs_link(pool: &DbPool, repository_id: &str, oid: &str) -> Result<bool, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                "SELECT 1 AS ok FROM lfs_object_links WHERE repository_id = ? AND oid = ?",
            )
            .bind(repository_id)
            .bind(oid)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("has_lfs_link: {e}"))?;
            Ok(row.is_some())
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                "SELECT 1 AS ok FROM lfs_object_links WHERE repository_id = $1 AND oid = $2",
            )
            .bind(repository_id)
            .bind(oid)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("has_lfs_link: {e}"))?;
            Ok(row.is_some())
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                "SELECT 1 AS ok FROM lfs_object_links WHERE repository_id = ? AND oid = ?",
            )
            .bind(repository_id)
            .bind(oid)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("has_lfs_link: {e}"))?;
            Ok(row.is_some())
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LfsSettingsRow {
    /// NULL = use env default.
    pub max_object_bytes: Option<i64>,
    pub quota_repo_bytes: Option<i64>,
    pub quota_user_bytes: Option<i64>,
}

pub async fn get_lfs_settings(pool: &DbPool) -> Result<LfsSettingsRow, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                "SELECT max_object_bytes, quota_repo_bytes, quota_user_bytes FROM instance_lfs_settings WHERE id = 1",
            )
            .fetch_one(p)
            .await
            .map_err(|e| format!("get_lfs_settings: {e}"))?;
            Ok(LfsSettingsRow {
                max_object_bytes: row.try_get::<Option<i64>, _>("max_object_bytes").unwrap_or(None),
                quota_repo_bytes: row.try_get::<Option<i64>, _>("quota_repo_bytes").unwrap_or(None),
                quota_user_bytes: row.try_get::<Option<i64>, _>("quota_user_bytes").unwrap_or(None),
            })
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                "SELECT max_object_bytes, quota_repo_bytes, quota_user_bytes FROM instance_lfs_settings WHERE id = 1",
            )
            .fetch_one(p)
            .await
            .map_err(|e| format!("get_lfs_settings: {e}"))?;
            Ok(LfsSettingsRow {
                max_object_bytes: row.try_get::<Option<i64>, _>("max_object_bytes").unwrap_or(None),
                quota_repo_bytes: row.try_get::<Option<i64>, _>("quota_repo_bytes").unwrap_or(None),
                quota_user_bytes: row.try_get::<Option<i64>, _>("quota_user_bytes").unwrap_or(None),
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                "SELECT max_object_bytes, quota_repo_bytes, quota_user_bytes FROM instance_lfs_settings WHERE id = 1",
            )
            .fetch_one(p)
            .await
            .map_err(|e| format!("get_lfs_settings: {e}"))?;
            Ok(LfsSettingsRow {
                max_object_bytes: row.try_get::<Option<i64>, _>("max_object_bytes").unwrap_or(None),
                quota_repo_bytes: row.try_get::<Option<i64>, _>("quota_repo_bytes").unwrap_or(None),
                quota_user_bytes: row.try_get::<Option<i64>, _>("quota_user_bytes").unwrap_or(None),
            })
        }
    }
}

pub async fn update_lfs_settings(
    pool: &DbPool,
    max_object_bytes: Option<i64>,
    quota_repo_bytes: Option<i64>,
    quota_user_bytes: Option<i64>,
) -> Result<LfsSettingsRow, String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE instance_lfs_settings SET
                   max_object_bytes = ?,
                   quota_repo_bytes = ?,
                   quota_user_bytes = ?,
                   updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
                 WHERE id = 1",
            )
            .bind(max_object_bytes)
            .bind(quota_repo_bytes)
            .bind(quota_user_bytes)
            .execute(p)
            .await
            .map_err(|e| format!("update_lfs_settings: {e}"))?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE instance_lfs_settings SET
                   max_object_bytes = $1,
                   quota_repo_bytes = $2,
                   quota_user_bytes = $3,
                   updated_at = NOW()
                 WHERE id = 1",
            )
            .bind(max_object_bytes)
            .bind(quota_repo_bytes)
            .bind(quota_user_bytes)
            .execute(p)
            .await
            .map_err(|e| format!("update_lfs_settings: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE instance_lfs_settings SET
                   max_object_bytes = ?,
                   quota_repo_bytes = ?,
                   quota_user_bytes = ?,
                   updated_at = CURRENT_TIMESTAMP(3)
                 WHERE id = 1",
            )
            .bind(max_object_bytes)
            .bind(quota_repo_bytes)
            .bind(quota_user_bytes)
            .execute(p)
            .await
            .map_err(|e| format!("update_lfs_settings: {e}"))?;
        }
    }
    get_lfs_settings(pool).await
}

/// Logical bytes charged to a repository (sum of linked object sizes).
pub async fn repo_logical_bytes(pool: &DbPool, repository_id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                "SELECT COALESCE(SUM(o.size), 0) AS total
                 FROM lfs_object_links l
                 JOIN lfs_objects o ON o.oid = l.oid
                 WHERE l.repository_id = ?",
            )
            .bind(repository_id)
            .fetch_one(p)
            .await
            .map_err(|e| format!("repo_logical_bytes: {e}"))?;
            Ok(row.try_get::<i64, _>("total").unwrap_or(0))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                "SELECT COALESCE(SUM(o.size), 0) AS total
                 FROM lfs_object_links l
                 JOIN lfs_objects o ON o.oid = l.oid
                 WHERE l.repository_id = $1",
            )
            .bind(repository_id)
            .fetch_one(p)
            .await
            .map_err(|e| format!("repo_logical_bytes: {e}"))?;
            Ok(row.try_get::<i64, _>("total").unwrap_or(0))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                "SELECT COALESCE(SUM(o.size), 0) AS total
                 FROM lfs_object_links l
                 JOIN lfs_objects o ON o.oid = l.oid
                 WHERE l.repository_id = ?",
            )
            .bind(repository_id)
            .fetch_one(p)
            .await
            .map_err(|e| format!("repo_logical_bytes: {e}"))?;
            Ok(row.try_get::<i64, _>("total").unwrap_or(0))
        }
    }
}

/// Logical bytes across all repos owned by `owner_id` (user or org id).
pub async fn owner_logical_bytes(pool: &DbPool, owner_id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                "SELECT COALESCE(SUM(o.size), 0) AS total
                 FROM lfs_object_links l
                 JOIN lfs_objects o ON o.oid = l.oid
                 JOIN repositories r ON r.id = l.repository_id
                 WHERE r.owner_id = ?",
            )
            .bind(owner_id)
            .fetch_one(p)
            .await
            .map_err(|e| format!("owner_logical_bytes: {e}"))?;
            Ok(row.try_get::<i64, _>("total").unwrap_or(0))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                "SELECT COALESCE(SUM(o.size), 0) AS total
                 FROM lfs_object_links l
                 JOIN lfs_objects o ON o.oid = l.oid
                 JOIN repositories r ON r.id = l.repository_id
                 WHERE r.owner_id = $1",
            )
            .bind(owner_id)
            .fetch_one(p)
            .await
            .map_err(|e| format!("owner_logical_bytes: {e}"))?;
            Ok(row.try_get::<i64, _>("total").unwrap_or(0))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                "SELECT COALESCE(SUM(o.size), 0) AS total
                 FROM lfs_object_links l
                 JOIN lfs_objects o ON o.oid = l.oid
                 JOIN repositories r ON r.id = l.repository_id
                 WHERE r.owner_id = ?",
            )
            .bind(owner_id)
            .fetch_one(p)
            .await
            .map_err(|e| format!("owner_logical_bytes: {e}"))?;
            Ok(row.try_get::<i64, _>("total").unwrap_or(0))
        }
    }
}

/// Physical unique OID bytes on the instance.
pub async fn physical_bytes(pool: &DbPool) -> Result<i64, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query("SELECT COALESCE(SUM(size), 0) AS total FROM lfs_objects")
                .fetch_one(p)
                .await
                .map_err(|e| format!("physical_bytes: {e}"))?;
            Ok(row.try_get::<i64, _>("total").unwrap_or(0))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query("SELECT COALESCE(SUM(size), 0) AS total FROM lfs_objects")
                .fetch_one(p)
                .await
                .map_err(|e| format!("physical_bytes: {e}"))?;
            Ok(row.try_get::<i64, _>("total").unwrap_or(0))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query("SELECT COALESCE(SUM(size), 0) AS total FROM lfs_objects")
                .fetch_one(p)
                .await
                .map_err(|e| format!("physical_bytes: {e}"))?;
            Ok(row.try_get::<i64, _>("total").unwrap_or(0))
        }
    }
}

pub async fn list_unreferenced_lfs_oids(
    pool: &DbPool,
    created_before: &str,
) -> Result<Vec<String>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT o.oid AS oid FROM lfs_objects o
                 LEFT JOIN lfs_object_links l ON l.oid = o.oid
                 WHERE l.oid IS NULL
                   AND datetime(o.created_at) < datetime(?)
                 ORDER BY o.oid",
            )
            .bind(created_before)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list_unreferenced_lfs_oids: {e}"))?;
            Ok(rows
                .into_iter()
                .filter_map(|r| r.try_get::<String, _>("oid").ok())
                .collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT o.oid AS oid FROM lfs_objects o
                 LEFT JOIN lfs_object_links l ON l.oid = o.oid
                 WHERE l.oid IS NULL
                   AND o.created_at < $1::timestamptz
                 ORDER BY o.oid",
            )
            .bind(created_before)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list_unreferenced_lfs_oids: {e}"))?;
            Ok(rows
                .into_iter()
                .filter_map(|r| r.try_get::<String, _>("oid").ok())
                .collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT o.oid AS oid FROM lfs_objects o
                 LEFT JOIN lfs_object_links l ON l.oid = o.oid
                 WHERE l.oid IS NULL
                   AND o.created_at < ?
                 ORDER BY o.oid",
            )
            .bind(created_before)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list_unreferenced_lfs_oids: {e}"))?;
            Ok(rows
                .into_iter()
                .filter_map(|r| r.try_get::<String, _>("oid").ok())
                .collect())
        }
    }
}

pub async fn delete_lfs_object(pool: &DbPool, oid: &str) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM lfs_objects WHERE oid = ?")
                .bind(oid)
                .execute(p)
                .await
                .map_err(|e| format!("delete_lfs_object: {e}"))?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM lfs_objects WHERE oid = $1")
                .bind(oid)
                .execute(p)
                .await
                .map_err(|e| format!("delete_lfs_object: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM lfs_objects WHERE oid = ?")
                .bind(oid)
                .execute(p)
                .await
                .map_err(|e| format!("delete_lfs_object: {e}"))?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct LfsLinkedObjectRow {
    pub oid: String,
    pub size: i64,
    pub refcount: i64,
}

/// Linked objects for a repo, largest first.
pub async fn list_repo_lfs_objects(
    pool: &DbPool,
    repository_id: &str,
    limit: i64,
) -> Result<Vec<LfsLinkedObjectRow>, String> {
    let limit = limit.clamp(1, 500);
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT l.oid AS oid, o.size AS size, l.refcount AS refcount
                 FROM lfs_object_links l
                 JOIN lfs_objects o ON o.oid = l.oid
                 WHERE l.repository_id = ?
                 ORDER BY o.size DESC, l.oid ASC
                 LIMIT ?",
            )
            .bind(repository_id)
            .bind(limit)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list_repo_lfs_objects: {e}"))?;
            Ok(rows
                .into_iter()
                .map(|r| LfsLinkedObjectRow {
                    oid: r.try_get::<String, _>("oid").unwrap_or_default(),
                    size: r.try_get::<i64, _>("size").unwrap_or(0),
                    refcount: r.try_get::<i64, _>("refcount").unwrap_or(0),
                })
                .collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT l.oid AS oid, o.size AS size, l.refcount AS refcount
                 FROM lfs_object_links l
                 JOIN lfs_objects o ON o.oid = l.oid
                 WHERE l.repository_id = $1
                 ORDER BY o.size DESC, l.oid ASC
                 LIMIT $2",
            )
            .bind(repository_id)
            .bind(limit)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list_repo_lfs_objects: {e}"))?;
            Ok(rows
                .into_iter()
                .map(|r| LfsLinkedObjectRow {
                    oid: r.try_get::<String, _>("oid").unwrap_or_default(),
                    size: r.try_get::<i64, _>("size").unwrap_or(0),
                    refcount: r.try_get::<i64, _>("refcount").unwrap_or(0),
                })
                .collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT l.oid AS oid, o.size AS size, l.refcount AS refcount
                 FROM lfs_object_links l
                 JOIN lfs_objects o ON o.oid = l.oid
                 WHERE l.repository_id = ?
                 ORDER BY o.size DESC, l.oid ASC
                 LIMIT ?",
            )
            .bind(repository_id)
            .bind(limit)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list_repo_lfs_objects: {e}"))?;
            Ok(rows
                .into_iter()
                .map(|r| LfsLinkedObjectRow {
                    oid: r.try_get::<String, _>("oid").unwrap_or_default(),
                    size: r.try_get::<i64, _>("size").unwrap_or(0),
                    refcount: r.try_get::<i64, _>("refcount").unwrap_or(0),
                })
                .collect())
        }
    }
}

pub async fn repo_lfs_object_count(pool: &DbPool, repository_id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                "SELECT COUNT(*) AS c FROM lfs_object_links WHERE repository_id = ?",
            )
            .bind(repository_id)
            .fetch_one(p)
            .await
            .map_err(|e| format!("repo_lfs_object_count: {e}"))?;
            Ok(row.try_get::<i64, _>("c").unwrap_or(0))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                "SELECT COUNT(*)::bigint AS c FROM lfs_object_links WHERE repository_id = $1",
            )
            .bind(repository_id)
            .fetch_one(p)
            .await
            .map_err(|e| format!("repo_lfs_object_count: {e}"))?;
            Ok(row.try_get::<i64, _>("c").unwrap_or(0))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                "SELECT COUNT(*) AS c FROM lfs_object_links WHERE repository_id = ?",
            )
            .bind(repository_id)
            .fetch_one(p)
            .await
            .map_err(|e| format!("repo_lfs_object_count: {e}"))?;
            Ok(row.try_get::<i64, _>("c").unwrap_or(0))
        }
    }
}

pub async fn instance_lfs_object_count(pool: &DbPool) -> Result<i64, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query("SELECT COUNT(*) AS c FROM lfs_objects")
                .fetch_one(p)
                .await
                .map_err(|e| format!("instance_lfs_object_count: {e}"))?;
            Ok(row.try_get::<i64, _>("c").unwrap_or(0))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query("SELECT COUNT(*)::bigint AS c FROM lfs_objects")
                .fetch_one(p)
                .await
                .map_err(|e| format!("instance_lfs_object_count: {e}"))?;
            Ok(row.try_get::<i64, _>("c").unwrap_or(0))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query("SELECT COUNT(*) AS c FROM lfs_objects")
                .fetch_one(p)
                .await
                .map_err(|e| format!("instance_lfs_object_count: {e}"))?;
            Ok(row.try_get::<i64, _>("c").unwrap_or(0))
        }
    }
}

pub async fn instance_logical_bytes(pool: &DbPool) -> Result<i64, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                "SELECT COALESCE(SUM(o.size), 0) AS total
                 FROM lfs_object_links l
                 JOIN lfs_objects o ON o.oid = l.oid",
            )
            .fetch_one(p)
            .await
            .map_err(|e| format!("instance_logical_bytes: {e}"))?;
            Ok(row.try_get::<i64, _>("total").unwrap_or(0))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                "SELECT COALESCE(SUM(o.size), 0) AS total
                 FROM lfs_object_links l
                 JOIN lfs_objects o ON o.oid = l.oid",
            )
            .fetch_one(p)
            .await
            .map_err(|e| format!("instance_logical_bytes: {e}"))?;
            Ok(row.try_get::<i64, _>("total").unwrap_or(0))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                "SELECT COALESCE(SUM(o.size), 0) AS total
                 FROM lfs_object_links l
                 JOIN lfs_objects o ON o.oid = l.oid",
            )
            .fetch_one(p)
            .await
            .map_err(|e| format!("instance_logical_bytes: {e}"))?;
            Ok(row.try_get::<i64, _>("total").unwrap_or(0))
        }
    }
}

#[derive(Debug, Clone)]
pub struct LfsRepoUsageRow {
    pub repository_id: String,
    pub owner_id: String,
    pub owner_slug: String,
    pub name: String,
    pub object_count: i64,
    pub logical_bytes: i64,
}

/// Usage grouped by repository (largest first).
pub async fn usage_by_repo(pool: &DbPool, limit: i64) -> Result<Vec<LfsRepoUsageRow>, String> {
    let limit = limit.clamp(1, 500);
    // owner slug: prefer users.username, else organizations.slug
    let sql_sqlite = "SELECT r.id AS repository_id, r.owner_id AS owner_id, r.name AS name,
            COALESCE(u.username, o.slug, r.owner_id) AS owner_slug,
            COUNT(l.oid) AS object_count,
            COALESCE(SUM(obj.size), 0) AS logical_bytes
         FROM repositories r
         JOIN lfs_object_links l ON l.repository_id = r.id
         JOIN lfs_objects obj ON obj.oid = l.oid
         LEFT JOIN users u ON u.id = r.owner_id
         LEFT JOIN organizations o ON o.id = r.owner_id
         GROUP BY r.id, r.owner_id, r.name, owner_slug
         ORDER BY logical_bytes DESC
         LIMIT ?";
    let sql_pg = "SELECT r.id AS repository_id, r.owner_id AS owner_id, r.name AS name,
            COALESCE(u.username, o.slug, r.owner_id) AS owner_slug,
            COUNT(l.oid)::bigint AS object_count,
            COALESCE(SUM(obj.size), 0) AS logical_bytes
         FROM repositories r
         JOIN lfs_object_links l ON l.repository_id = r.id
         JOIN lfs_objects obj ON obj.oid = l.oid
         LEFT JOIN users u ON u.id = r.owner_id
         LEFT JOIN organizations o ON o.id = r.owner_id
         GROUP BY r.id, r.owner_id, r.name, u.username, o.slug
         ORDER BY logical_bytes DESC
         LIMIT $1";
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(sql_sqlite)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("usage_by_repo: {e}"))?;
            Ok(rows
                .into_iter()
                .map(|r| LfsRepoUsageRow {
                    repository_id: r.try_get::<String, _>("repository_id").unwrap_or_default(),
                    owner_id: r.try_get::<String, _>("owner_id").unwrap_or_default(),
                    owner_slug: r.try_get::<String, _>("owner_slug").unwrap_or_default(),
                    name: r.try_get::<String, _>("name").unwrap_or_default(),
                    object_count: r.try_get::<i64, _>("object_count").unwrap_or(0),
                    logical_bytes: r.try_get::<i64, _>("logical_bytes").unwrap_or(0),
                })
                .collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sql_pg)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("usage_by_repo: {e}"))?;
            Ok(rows
                .into_iter()
                .map(|r| LfsRepoUsageRow {
                    repository_id: r.try_get::<String, _>("repository_id").unwrap_or_default(),
                    owner_id: r.try_get::<String, _>("owner_id").unwrap_or_default(),
                    owner_slug: r.try_get::<String, _>("owner_slug").unwrap_or_default(),
                    name: r.try_get::<String, _>("name").unwrap_or_default(),
                    object_count: r.try_get::<i64, _>("object_count").unwrap_or(0),
                    logical_bytes: r.try_get::<i64, _>("logical_bytes").unwrap_or(0),
                })
                .collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(sql_sqlite)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("usage_by_repo: {e}"))?;
            Ok(rows
                .into_iter()
                .map(|r| LfsRepoUsageRow {
                    repository_id: r.try_get::<String, _>("repository_id").unwrap_or_default(),
                    owner_id: r.try_get::<String, _>("owner_id").unwrap_or_default(),
                    owner_slug: r.try_get::<String, _>("owner_slug").unwrap_or_default(),
                    name: r.try_get::<String, _>("name").unwrap_or_default(),
                    object_count: r.try_get::<i64, _>("object_count").unwrap_or(0),
                    logical_bytes: r.try_get::<i64, _>("logical_bytes").unwrap_or(0),
                })
                .collect())
        }
    }
}

#[derive(Debug, Clone)]
pub struct LfsOwnerUsageRow {
    pub owner_id: String,
    pub owner_slug: String,
    pub object_count: i64,
    pub logical_bytes: i64,
}

pub async fn usage_by_owner(pool: &DbPool, limit: i64) -> Result<Vec<LfsOwnerUsageRow>, String> {
    let limit = limit.clamp(1, 500);
    let sql_sqlite = "SELECT r.owner_id AS owner_id,
            COALESCE(u.username, o.slug, r.owner_id) AS owner_slug,
            COUNT(l.oid) AS object_count,
            COALESCE(SUM(obj.size), 0) AS logical_bytes
         FROM repositories r
         JOIN lfs_object_links l ON l.repository_id = r.id
         JOIN lfs_objects obj ON obj.oid = l.oid
         LEFT JOIN users u ON u.id = r.owner_id
         LEFT JOIN organizations o ON o.id = r.owner_id
         GROUP BY r.owner_id, owner_slug
         ORDER BY logical_bytes DESC
         LIMIT ?";
    let sql_pg = "SELECT r.owner_id AS owner_id,
            COALESCE(u.username, o.slug, r.owner_id) AS owner_slug,
            COUNT(l.oid)::bigint AS object_count,
            COALESCE(SUM(obj.size), 0) AS logical_bytes
         FROM repositories r
         JOIN lfs_object_links l ON l.repository_id = r.id
         JOIN lfs_objects obj ON obj.oid = l.oid
         LEFT JOIN users u ON u.id = r.owner_id
         LEFT JOIN organizations o ON o.id = r.owner_id
         GROUP BY r.owner_id, u.username, o.slug
         ORDER BY logical_bytes DESC
         LIMIT $1";
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(sql_sqlite)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("usage_by_owner: {e}"))?;
            Ok(rows
                .into_iter()
                .map(|r| LfsOwnerUsageRow {
                    owner_id: r.try_get::<String, _>("owner_id").unwrap_or_default(),
                    owner_slug: r.try_get::<String, _>("owner_slug").unwrap_or_default(),
                    object_count: r.try_get::<i64, _>("object_count").unwrap_or(0),
                    logical_bytes: r.try_get::<i64, _>("logical_bytes").unwrap_or(0),
                })
                .collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sql_pg)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("usage_by_owner: {e}"))?;
            Ok(rows
                .into_iter()
                .map(|r| LfsOwnerUsageRow {
                    owner_id: r.try_get::<String, _>("owner_id").unwrap_or_default(),
                    owner_slug: r.try_get::<String, _>("owner_slug").unwrap_or_default(),
                    object_count: r.try_get::<i64, _>("object_count").unwrap_or(0),
                    logical_bytes: r.try_get::<i64, _>("logical_bytes").unwrap_or(0),
                })
                .collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(sql_sqlite)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("usage_by_owner: {e}"))?;
            Ok(rows
                .into_iter()
                .map(|r| LfsOwnerUsageRow {
                    owner_id: r.try_get::<String, _>("owner_id").unwrap_or_default(),
                    owner_slug: r.try_get::<String, _>("owner_slug").unwrap_or_default(),
                    object_count: r.try_get::<i64, _>("object_count").unwrap_or(0),
                    logical_bytes: r.try_get::<i64, _>("logical_bytes").unwrap_or(0),
                })
                .collect())
        }
    }
}
