//! Per-repository collaborator grants (`read` | `write` | `admin` — D-ORG-02c).

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct RepoCollaboratorRow {
    pub repo_id: String,
    pub user_id: String,
    pub permission: String,
    pub created_at: String,
}

/// Collaborator row with username for `repo.collaborators.list` (no email).
#[derive(Debug, Clone)]
pub struct RepoCollaboratorListRow {
    pub repo_id: String,
    pub user_id: String,
    pub username: String,
    pub permission: String,
    pub created_at: String,
}

macro_rules! map_collaborator {
    ($row:expr) => {{
        let row = $row;
        RepoCollaboratorRow {
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("repo collaborator row: {e}"))?,
            user_id: row
                .try_get("user_id")
                .map_err(|e| format!("repo collaborator row: {e}"))?,
            permission: row
                .try_get("permission")
                .map_err(|e| format!("repo collaborator row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("repo collaborator row: {e}"))?,
        }
    }};
}

const COLLAB_SELECT_PG: &str = "SELECT repo_id, user_id, permission,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM repository_collaborators";

const COLLAB_SELECT_MYSQL: &str = "SELECT repo_id, user_id, permission,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM repository_collaborators";

const COLLAB_SELECT_SQLITE: &str = "SELECT repo_id, user_id, permission,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at
FROM repository_collaborators";

/// Look up a collaborator grant for `(repo_id, user_id)`.
pub async fn find_collaborator(
    pool: &DbPool,
    repo_id: &str,
    user_id: &str,
) -> Result<Option<RepoCollaboratorRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{COLLAB_SELECT_PG} WHERE repo_id = $1 AND user_id = $2"
            )))
            .bind(repo_id)
            .bind(user_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find repo collaborator failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_collaborator!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{COLLAB_SELECT_MYSQL} WHERE repo_id = ? AND user_id = ?"
            )))
            .bind(repo_id)
            .bind(user_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find repo collaborator failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_collaborator!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{COLLAB_SELECT_SQLITE} WHERE repo_id = ?1 AND user_id = ?2"
            )))
            .bind(repo_id)
            .bind(user_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find repo collaborator failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_collaborator!(&r)),
                None => None,
            })
        }
    }
}

/// Insert a collaborator grant (`read` | `write` | `admin`).
pub async fn insert_collaborator(
    pool: &DbPool,
    repo_id: &str,
    user_id: &str,
    permission: &str,
) -> Result<RepoCollaboratorRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO repository_collaborators (repo_id, user_id, permission)
VALUES ($1, $2, $3)",
            )
            .bind(repo_id)
            .bind(user_id)
            .bind(permission)
            .execute(p)
            .await
            .map_err(|e| format!("insert repo collaborator failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO repository_collaborators (repo_id, user_id, permission)
VALUES (?, ?, ?)",
            )
            .bind(repo_id)
            .bind(user_id)
            .bind(permission)
            .execute(p)
            .await
            .map_err(|e| format!("insert repo collaborator failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO repository_collaborators (repo_id, user_id, permission)
VALUES (?1, ?2, ?3)",
            )
            .bind(repo_id)
            .bind(user_id)
            .bind(permission)
            .execute(p)
            .await
            .map_err(|e| format!("insert repo collaborator failed: {e}"))?;
        }
    }
    find_collaborator(pool, repo_id, user_id)
        .await?
        .ok_or_else(|| "insert repo collaborator failed: row missing after insert".into())
}

/// Update an existing collaborator permission.
pub async fn update_collaborator_permission(
    pool: &DbPool,
    repo_id: &str,
    user_id: &str,
    permission: &str,
) -> Result<RepoCollaboratorRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            let n = sqlx::query(
                "UPDATE repository_collaborators SET permission = $3
WHERE repo_id = $1 AND user_id = $2",
            )
            .bind(repo_id)
            .bind(user_id)
            .bind(permission)
            .execute(p)
            .await
            .map_err(|e| format!("update repo collaborator failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("repo collaborator not found".into());
            }
        }
        DbPool::MySql(p) => {
            let n = sqlx::query(
                "UPDATE repository_collaborators SET permission = ?
WHERE repo_id = ? AND user_id = ?",
            )
            .bind(permission)
            .bind(repo_id)
            .bind(user_id)
            .execute(p)
            .await
            .map_err(|e| format!("update repo collaborator failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("repo collaborator not found".into());
            }
        }
        DbPool::Sqlite(p) => {
            let n = sqlx::query(
                "UPDATE repository_collaborators SET permission = ?3
WHERE repo_id = ?1 AND user_id = ?2",
            )
            .bind(repo_id)
            .bind(user_id)
            .bind(permission)
            .execute(p)
            .await
            .map_err(|e| format!("update repo collaborator failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("repo collaborator not found".into());
            }
        }
    }
    find_collaborator(pool, repo_id, user_id)
        .await?
        .ok_or_else(|| "repo collaborator not found".into())
}

/// Remove a collaborator grant.
pub async fn remove_collaborator(
    pool: &DbPool,
    repo_id: &str,
    user_id: &str,
) -> Result<(), String> {
    let n = match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM repository_collaborators WHERE repo_id = $1 AND user_id = $2")
                .bind(repo_id)
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("remove repo collaborator failed: {e}"))?
                .rows_affected()
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM repository_collaborators WHERE repo_id = ? AND user_id = ?")
                .bind(repo_id)
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("remove repo collaborator failed: {e}"))?
                .rows_affected()
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM repository_collaborators WHERE repo_id = ?1 AND user_id = ?2")
                .bind(repo_id)
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("remove repo collaborator failed: {e}"))?
                .rows_affected()
        }
    };
    if n == 0 {
        return Err("repo collaborator not found".into());
    }
    Ok(())
}

/// List collaborators for a repo (with username, no email).
pub async fn list_collaborators(
    pool: &DbPool,
    repo_id: &str,
) -> Result<Vec<RepoCollaboratorListRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT c.repo_id, c.user_id, u.username, c.permission,
       to_char(c.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM repository_collaborators c
JOIN users u ON u.id = c.user_id
WHERE c.repo_id = $1
ORDER BY c.created_at ASC, c.user_id ASC",
            )
            .bind(repo_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list repo collaborators failed: {e}"))?;
            rows.into_iter()
                .map(|row| {
                    Ok(RepoCollaboratorListRow {
                        repo_id: row
                            .try_get("repo_id")
                            .map_err(|e| format!("collab list: {e}"))?,
                        user_id: row
                            .try_get("user_id")
                            .map_err(|e| format!("collab list: {e}"))?,
                        username: row
                            .try_get("username")
                            .map_err(|e| format!("collab list: {e}"))?,
                        permission: row
                            .try_get("permission")
                            .map_err(|e| format!("collab list: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("collab list: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT c.repo_id, c.user_id, u.username, c.permission,
       DATE_FORMAT(c.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM repository_collaborators c
JOIN users u ON u.id = c.user_id
WHERE c.repo_id = ?
ORDER BY c.created_at ASC, c.user_id ASC",
            )
            .bind(repo_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list repo collaborators failed: {e}"))?;
            rows.into_iter()
                .map(|row| {
                    Ok(RepoCollaboratorListRow {
                        repo_id: row
                            .try_get("repo_id")
                            .map_err(|e| format!("collab list: {e}"))?,
                        user_id: row
                            .try_get("user_id")
                            .map_err(|e| format!("collab list: {e}"))?,
                        username: row
                            .try_get("username")
                            .map_err(|e| format!("collab list: {e}"))?,
                        permission: row
                            .try_get("permission")
                            .map_err(|e| format!("collab list: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("collab list: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT c.repo_id, c.user_id, u.username, c.permission,
       strftime('%Y-%m-%dT%H:%M:%SZ', c.created_at) AS created_at
FROM repository_collaborators c
JOIN users u ON u.id = c.user_id
WHERE c.repo_id = ?1
ORDER BY c.created_at ASC, c.user_id ASC",
            )
            .bind(repo_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list repo collaborators failed: {e}"))?;
            rows.into_iter()
                .map(|row| {
                    Ok(RepoCollaboratorListRow {
                        repo_id: row
                            .try_get("repo_id")
                            .map_err(|e| format!("collab list: {e}"))?,
                        user_id: row
                            .try_get("user_id")
                            .map_err(|e| format!("collab list: {e}"))?,
                        username: row
                            .try_get("username")
                            .map_err(|e| format!("collab list: {e}"))?,
                        permission: row
                            .try_get("permission")
                            .map_err(|e| format!("collab list: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("collab list: {e}"))?,
                    })
                })
                .collect()
        }
    }
}

/// Collaborator grant with resolved owner slug + repo name (admin access summary).
#[derive(Debug, Clone)]
pub struct RepoCollaboratorGrantRow {
    pub repo_id: String,
    pub owner_slug: String,
    pub name: String,
    pub permission: String,
    pub created_at: String,
}

/// List direct collaborator grants for a user across non-deleted repos.
pub async fn list_grants_for_user(
    pool: &DbPool,
    user_id: &str,
) -> Result<Vec<RepoCollaboratorGrantRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT c.repo_id, r.name, c.permission,
       COALESCE(u.username, o.slug) AS owner_slug,
       to_char(c.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM repository_collaborators c
JOIN repositories r ON r.id = c.repo_id AND r.deleted_at IS NULL
LEFT JOIN users u ON r.owner_type = 'user' AND u.id = r.owner_id
LEFT JOIN organizations o ON r.owner_type = 'org' AND o.id = r.owner_id
WHERE c.user_id = $1
ORDER BY owner_slug ASC, r.name ASC",
            )
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list repo collab grants for user failed: {e}"))?;
            rows.into_iter()
                .map(|row| {
                    Ok(RepoCollaboratorGrantRow {
                        repo_id: row
                            .try_get("repo_id")
                            .map_err(|e| format!("collab grant: {e}"))?,
                        owner_slug: row
                            .try_get("owner_slug")
                            .map_err(|e| format!("collab grant: {e}"))?,
                        name: row
                            .try_get("name")
                            .map_err(|e| format!("collab grant: {e}"))?,
                        permission: row
                            .try_get("permission")
                            .map_err(|e| format!("collab grant: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("collab grant: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT c.repo_id, r.name, c.permission,
       COALESCE(u.username, o.slug) AS owner_slug,
       DATE_FORMAT(c.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM repository_collaborators c
JOIN repositories r ON r.id = c.repo_id AND r.deleted_at IS NULL
LEFT JOIN users u ON r.owner_type = 'user' AND u.id = r.owner_id
LEFT JOIN organizations o ON r.owner_type = 'org' AND o.id = r.owner_id
WHERE c.user_id = ?
ORDER BY owner_slug ASC, r.name ASC",
            )
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list repo collab grants for user failed: {e}"))?;
            rows.into_iter()
                .map(|row| {
                    Ok(RepoCollaboratorGrantRow {
                        repo_id: row
                            .try_get("repo_id")
                            .map_err(|e| format!("collab grant: {e}"))?,
                        owner_slug: row
                            .try_get("owner_slug")
                            .map_err(|e| format!("collab grant: {e}"))?,
                        name: row
                            .try_get("name")
                            .map_err(|e| format!("collab grant: {e}"))?,
                        permission: row
                            .try_get("permission")
                            .map_err(|e| format!("collab grant: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("collab grant: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT c.repo_id, r.name, c.permission,
       COALESCE(u.username, o.slug) AS owner_slug,
       strftime('%Y-%m-%dT%H:%M:%SZ', c.created_at) AS created_at
FROM repository_collaborators c
JOIN repositories r ON r.id = c.repo_id AND r.deleted_at IS NULL
LEFT JOIN users u ON r.owner_type = 'user' AND u.id = r.owner_id
LEFT JOIN organizations o ON r.owner_type = 'org' AND o.id = r.owner_id
WHERE c.user_id = ?1
ORDER BY owner_slug ASC, r.name ASC",
            )
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list repo collab grants for user failed: {e}"))?;
            rows.into_iter()
                .map(|row| {
                    Ok(RepoCollaboratorGrantRow {
                        repo_id: row
                            .try_get("repo_id")
                            .map_err(|e| format!("collab grant: {e}"))?,
                        owner_slug: row
                            .try_get("owner_slug")
                            .map_err(|e| format!("collab grant: {e}"))?,
                        name: row
                            .try_get("name")
                            .map_err(|e| format!("collab grant: {e}"))?,
                        permission: row
                            .try_get("permission")
                            .map_err(|e| format!("collab grant: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("collab grant: {e}"))?,
                    })
                })
                .collect()
        }
    }
}
