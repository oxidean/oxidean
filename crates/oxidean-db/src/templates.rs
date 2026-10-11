//! Instance template packs + repository template flags (issue #18).

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct InstanceTemplatePackRow {
    pub id: String,
    pub slug: String,
    pub label: String,
    pub group: String,
    pub description: String,
    pub default_gitignore: Option<String>,
    pub enabled: bool,
    pub byte_size: i64,
    pub content_digest: String,
    pub uploaded_by_user_id: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct TemplateRepoListRow {
    pub id: String,
    pub owner_id: String,
    pub owner_type: String,
    pub owner_slug: String,
    pub name: String,
    pub description: String,
    pub visibility: String,
}

macro_rules! map_pack_fields {
    ($row:expr, $enabled:expr) => {{
        let row = $row;
        InstanceTemplatePackRow {
            id: row.get("id"),
            slug: row.get("slug"),
            label: row.get("label"),
            group: row.get("group"),
            description: row.get("description"),
            default_gitignore: row.get("default_gitignore"),
            enabled: $enabled,
            byte_size: row
                .try_get::<i64, _>("byte_size")
                .unwrap_or_else(|_| i64::from(row.get::<i32, _>("byte_size"))),
            content_digest: row.get("content_digest"),
            uploaded_by_user_id: row.get("uploaded_by_user_id"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }
    }};
}

fn enabled_sqlite(row: &sqlx::sqlite::SqliteRow) -> bool {
    row.try_get::<i64, _>("enabled").unwrap_or(0) != 0
}

fn enabled_pg(row: &sqlx::postgres::PgRow) -> bool {
    row.try_get::<bool, _>("enabled").unwrap_or(false)
}

fn enabled_mysql(row: &sqlx::mysql::MySqlRow) -> bool {
    row.try_get::<i8, _>("enabled").unwrap_or(0) != 0
}

const SELECT_PACK_PG: &str = r#"SELECT id, slug, label, "group", description, default_gitignore, enabled,
    byte_size, content_digest, uploaded_by_user_id,
    created_at::text AS created_at, updated_at::text AS updated_at
    FROM instance_template_packs"#;

const SELECT_PACK_MYSQL: &str = r#"SELECT id, slug, label, `group`, description, default_gitignore, enabled,
    byte_size, content_digest, uploaded_by_user_id,
    CAST(created_at AS CHAR) AS created_at, CAST(updated_at AS CHAR) AS updated_at
    FROM instance_template_packs"#;

const SELECT_PACK_SQLITE: &str = r#"SELECT id, slug, label, "group", description, default_gitignore, enabled,
    byte_size, content_digest, uploaded_by_user_id, created_at, updated_at
    FROM instance_template_packs"#;

pub async fn list_instance_template_packs(
    pool: &DbPool,
    enabled_only: bool,
) -> Result<Vec<InstanceTemplatePackRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let sql = if enabled_only {
                format!("{SELECT_PACK_SQLITE} WHERE enabled = 1 ORDER BY label")
            } else {
                format!("{SELECT_PACK_SQLITE} ORDER BY label")
            };
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .fetch_all(p)
                .await
                .map_err(|e| format!("list_instance_template_packs: {e}"))?;
            Ok(rows
                .iter()
                .map(|r| map_pack_fields!(r, enabled_sqlite(r)))
                .collect())
        }
        DbPool::Postgres(p) => {
            let sql = if enabled_only {
                format!("{SELECT_PACK_PG} WHERE enabled = true ORDER BY label")
            } else {
                format!("{SELECT_PACK_PG} ORDER BY label")
            };
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .fetch_all(p)
                .await
                .map_err(|e| format!("list_instance_template_packs: {e}"))?;
            Ok(rows
                .iter()
                .map(|r| map_pack_fields!(r, enabled_pg(r)))
                .collect())
        }
        DbPool::MySql(p) => {
            let sql = if enabled_only {
                format!("{SELECT_PACK_MYSQL} WHERE enabled = 1 ORDER BY label")
            } else {
                format!("{SELECT_PACK_MYSQL} ORDER BY label")
            };
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .fetch_all(p)
                .await
                .map_err(|e| format!("list_instance_template_packs: {e}"))?;
            Ok(rows
                .iter()
                .map(|r| map_pack_fields!(r, enabled_mysql(r)))
                .collect())
        }
    }
}

pub async fn get_instance_template_pack(
    pool: &DbPool,
    id: &str,
) -> Result<Option<InstanceTemplatePackRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_PACK_SQLITE} WHERE id = ?"
            )))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("get_instance_template_pack: {e}"))?;
            Ok(row.as_ref().map(|r| map_pack_fields!(r, enabled_sqlite(r))))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_PACK_PG} WHERE id = $1"
            )))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("get_instance_template_pack: {e}"))?;
            Ok(row.as_ref().map(|r| map_pack_fields!(r, enabled_pg(r))))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_PACK_MYSQL} WHERE id = ?"
            )))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("get_instance_template_pack: {e}"))?;
            Ok(row.as_ref().map(|r| map_pack_fields!(r, enabled_mysql(r))))
        }
    }
}

pub async fn get_instance_template_pack_by_slug(
    pool: &DbPool,
    slug: &str,
) -> Result<Option<InstanceTemplatePackRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_PACK_SQLITE} WHERE slug = ?"
            )))
            .bind(slug)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("get_instance_template_pack_by_slug: {e}"))?;
            Ok(row.as_ref().map(|r| map_pack_fields!(r, enabled_sqlite(r))))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_PACK_PG} WHERE slug = $1"
            )))
            .bind(slug)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("get_instance_template_pack_by_slug: {e}"))?;
            Ok(row.as_ref().map(|r| map_pack_fields!(r, enabled_pg(r))))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_PACK_MYSQL} WHERE slug = ?"
            )))
            .bind(slug)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("get_instance_template_pack_by_slug: {e}"))?;
            Ok(row.as_ref().map(|r| map_pack_fields!(r, enabled_mysql(r))))
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_instance_template_pack(
    pool: &DbPool,
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
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                r#"INSERT INTO instance_template_packs
                (id, slug, label, "group", description, default_gitignore, enabled, byte_size, content_digest, uploaded_by_user_id)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(id)
            .bind(slug)
            .bind(label)
            .bind(group)
            .bind(description)
            .bind(default_gitignore)
            .bind(if enabled { 1 } else { 0 })
            .bind(byte_size)
            .bind(content_digest)
            .bind(uploaded_by_user_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert_instance_template_pack: {e}"))?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"INSERT INTO instance_template_packs
                (id, slug, label, "group", description, default_gitignore, enabled, byte_size, content_digest, uploaded_by_user_id)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
            )
            .bind(id)
            .bind(slug)
            .bind(label)
            .bind(group)
            .bind(description)
            .bind(default_gitignore)
            .bind(enabled)
            .bind(byte_size)
            .bind(content_digest)
            .bind(uploaded_by_user_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert_instance_template_pack: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                r#"INSERT INTO instance_template_packs
                (id, slug, label, `group`, description, default_gitignore, enabled, byte_size, content_digest, uploaded_by_user_id)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(id)
            .bind(slug)
            .bind(label)
            .bind(group)
            .bind(description)
            .bind(default_gitignore)
            .bind(if enabled { 1 } else { 0 })
            .bind(byte_size)
            .bind(content_digest)
            .bind(uploaded_by_user_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert_instance_template_pack: {e}"))?;
        }
    }
    get_instance_template_pack(pool, id)
        .await?
        .ok_or_else(|| "insert_instance_template_pack: missing after insert".into())
}

pub async fn update_instance_template_pack(
    pool: &DbPool,
    id: &str,
    label: Option<&str>,
    group: Option<&str>,
    description: Option<&str>,
    default_gitignore: Option<Option<&str>>,
) -> Result<(), String> {
    let current = get_instance_template_pack(pool, id)
        .await?
        .ok_or_else(|| "template pack not found".to_string())?;
    let label = label.unwrap_or(&current.label);
    let group = group.unwrap_or(&current.group);
    let description = description.unwrap_or(&current.description);
    let gi = match default_gitignore {
        Some(v) => v,
        None => current.default_gitignore.as_deref(),
    };
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                r#"UPDATE instance_template_packs SET label = ?, "group" = ?, description = ?,
                   default_gitignore = ?, updated_at = strftime('%Y-%m-%d %H:%M:%S','now') WHERE id = ?"#,
            )
            .bind(label)
            .bind(group)
            .bind(description)
            .bind(gi)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("update_instance_template_pack: {e}"))?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"UPDATE instance_template_packs SET label = $1, "group" = $2, description = $3,
                   default_gitignore = $4, updated_at = now() WHERE id = $5"#,
            )
            .bind(label)
            .bind(group)
            .bind(description)
            .bind(gi)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("update_instance_template_pack: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                r#"UPDATE instance_template_packs SET label = ?, `group` = ?, description = ?,
                   default_gitignore = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?"#,
            )
            .bind(label)
            .bind(group)
            .bind(description)
            .bind(gi)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("update_instance_template_pack: {e}"))?;
        }
    }
    Ok(())
}

pub async fn set_instance_template_pack_enabled(
    pool: &DbPool,
    id: &str,
    enabled: bool,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE instance_template_packs SET enabled = ?, updated_at = strftime('%Y-%m-%d %H:%M:%S','now') WHERE id = ?",
            )
            .bind(if enabled { 1 } else { 0 })
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("set_instance_template_pack_enabled: {e}"))?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE instance_template_packs SET enabled = $1, updated_at = now() WHERE id = $2",
            )
            .bind(enabled)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("set_instance_template_pack_enabled: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE instance_template_packs SET enabled = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            )
            .bind(if enabled { 1 } else { 0 })
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("set_instance_template_pack_enabled: {e}"))?;
        }
    }
    Ok(())
}

pub async fn delete_instance_template_pack(pool: &DbPool, id: &str) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM instance_template_packs WHERE id = ?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete_instance_template_pack: {e}"))?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM instance_template_packs WHERE id = $1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete_instance_template_pack: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM instance_template_packs WHERE id = ?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete_instance_template_pack: {e}"))?;
        }
    }
    Ok(())
}

pub async fn get_repo_is_template(pool: &DbPool, repo_id: &str) -> Result<bool, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query("SELECT is_template FROM repositories WHERE id = ?")
                .bind(repo_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("get_repo_is_template: {e}"))?;
            Ok(row
                .map(|r| r.try_get::<i64, _>("is_template").unwrap_or(0) != 0)
                .unwrap_or(false))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query("SELECT is_template FROM repositories WHERE id = $1")
                .bind(repo_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("get_repo_is_template: {e}"))?;
            Ok(row
                .map(|r| r.try_get::<bool, _>("is_template").unwrap_or(false))
                .unwrap_or(false))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query("SELECT is_template FROM repositories WHERE id = ?")
                .bind(repo_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("get_repo_is_template: {e}"))?;
            Ok(row
                .map(|r| r.try_get::<i8, _>("is_template").unwrap_or(0) != 0)
                .unwrap_or(false))
        }
    }
}

pub async fn set_repo_is_template(
    pool: &DbPool,
    repo_id: &str,
    enabled: bool,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE repositories SET is_template = ? WHERE id = ?")
                .bind(if enabled { 1 } else { 0 })
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| format!("set_repo_is_template: {e}"))?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE repositories SET is_template = $1 WHERE id = $2")
                .bind(enabled)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| format!("set_repo_is_template: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE repositories SET is_template = ? WHERE id = ?")
                .bind(if enabled { 1 } else { 0 })
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| format!("set_repo_is_template: {e}"))?;
        }
    }
    Ok(())
}

pub async fn set_created_from_template_repo(
    pool: &DbPool,
    repo_id: &str,
    template_repo_id: Option<&str>,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE repositories SET created_from_template_repo_id = ? WHERE id = ?")
                .bind(template_repo_id)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| format!("set_created_from_template_repo: {e}"))?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE repositories SET created_from_template_repo_id = $1 WHERE id = $2")
                .bind(template_repo_id)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| format!("set_created_from_template_repo: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE repositories SET created_from_template_repo_id = ? WHERE id = ?")
                .bind(template_repo_id)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| format!("set_created_from_template_repo: {e}"))?;
        }
    }
    Ok(())
}

/// Template repos the viewer may reasonably see on `/new`.
///
/// Includes public templates plus private ones the viewer owns, collaborates on,
/// or can read via org role / member base permission. Callers that enforce full
/// ACL (e.g. `repo.createDefaults`) should still gate non-public rows with
/// `resolve_repo_for_read`.
macro_rules! map_template_repo {
    ($row:expr) => {{
        let r = $row;
        TemplateRepoListRow {
            id: r.get("id"),
            owner_id: r.get("owner_id"),
            owner_type: r.get("owner_type"),
            owner_slug: r.get("owner_slug"),
            name: r.get("name"),
            description: r.get("description"),
            visibility: r.get("visibility"),
        }
    }};
}

pub async fn list_template_repositories(
    pool: &DbPool,
    viewer_user_id: Option<&str>,
) -> Result<Vec<TemplateRepoListRow>, String> {
    // Shared visibility predicate for a logged-in viewer (bound once as `uid`).
    // Mirrors D-ORG-05 coalesce sources that can grant Read on private templates.
    const VIEWER_CAN_SEE: &str = r#"
                       (r.visibility = 'public'
                        OR (r.owner_type = 'user' AND r.owner_id = ?)
                        OR EXISTS (
                              SELECT 1 FROM repository_collaborators c
                              WHERE c.repo_id = r.id AND c.user_id = ?
                            )
                        OR (
                              r.owner_type = 'org'
                              AND EXISTS (
                                SELECT 1 FROM organization_members m
                                INNER JOIN organizations og ON og.id = m.org_id
                                WHERE m.org_id = r.owner_id AND m.user_id = ?
                                  AND (
                                    m.role IN ('owner', 'admin')
                                    OR og.member_base_permission IN ('read', 'write')
                                  )
                              )
                            ))"#;
    const VIEWER_CAN_SEE_PG: &str = r#"
                       (r.visibility = 'public'
                        OR (r.owner_type = 'user' AND r.owner_id = $1)
                        OR EXISTS (
                              SELECT 1 FROM repository_collaborators c
                              WHERE c.repo_id = r.id AND c.user_id = $1
                            )
                        OR (
                              r.owner_type = 'org'
                              AND EXISTS (
                                SELECT 1 FROM organization_members m
                                INNER JOIN organizations og ON og.id = m.org_id
                                WHERE m.org_id = r.owner_id AND m.user_id = $1
                                  AND (
                                    m.role IN ('owner', 'admin')
                                    OR og.member_base_permission IN ('read', 'write')
                                  )
                              )
                            ))"#;

    match pool {
        DbPool::Sqlite(p) => {
            let rows = if let Some(uid) = viewer_user_id {
                let sql = format!(
                    r#"SELECT r.id, r.owner_id, r.owner_type, r.name, r.description, r.visibility,
                       COALESCE(u.username, o.slug, '') AS owner_slug
                       FROM repositories r
                       LEFT JOIN users u ON r.owner_type = 'user' AND r.owner_id = u.id
                       LEFT JOIN organizations o ON r.owner_type = 'org' AND r.owner_id = o.id
                       WHERE r.is_template = 1 AND r.deleted_at IS NULL
                         AND {VIEWER_CAN_SEE}
                       ORDER BY owner_slug, r.name"#
                );
                sqlx::query(sqlx::AssertSqlSafe(&*sql))
                    .bind(uid)
                    .bind(uid)
                    .bind(uid)
                    .fetch_all(p)
                    .await
            } else {
                sqlx::query(
                    r#"SELECT r.id, r.owner_id, r.owner_type, r.name, r.description, r.visibility,
                       COALESCE(u.username, o.slug, '') AS owner_slug
                       FROM repositories r
                       LEFT JOIN users u ON r.owner_type = 'user' AND r.owner_id = u.id
                       LEFT JOIN organizations o ON r.owner_type = 'org' AND r.owner_id = o.id
                       WHERE r.is_template = 1 AND r.deleted_at IS NULL
                         AND r.visibility = 'public'
                       ORDER BY owner_slug, r.name"#,
                )
                .fetch_all(p)
                .await
            }
            .map_err(|e| format!("list_template_repositories: {e}"))?;
            Ok(rows.iter().map(|r| map_template_repo!(r)).collect())
        }
        DbPool::Postgres(p) => {
            let rows = if let Some(uid) = viewer_user_id {
                let sql = format!(
                    r#"SELECT r.id, r.owner_id, r.owner_type, r.name, r.description, r.visibility,
                       COALESCE(u.username, o.slug, '') AS owner_slug
                       FROM repositories r
                       LEFT JOIN users u ON r.owner_type = 'user' AND r.owner_id = u.id
                       LEFT JOIN organizations o ON r.owner_type = 'org' AND r.owner_id = o.id
                       WHERE r.is_template = true AND r.deleted_at IS NULL
                         AND {VIEWER_CAN_SEE_PG}
                       ORDER BY owner_slug, r.name"#
                );
                sqlx::query(sqlx::AssertSqlSafe(&*sql))
                    .bind(uid)
                    .fetch_all(p)
                    .await
            } else {
                sqlx::query(
                    r#"SELECT r.id, r.owner_id, r.owner_type, r.name, r.description, r.visibility,
                       COALESCE(u.username, o.slug, '') AS owner_slug
                       FROM repositories r
                       LEFT JOIN users u ON r.owner_type = 'user' AND r.owner_id = u.id
                       LEFT JOIN organizations o ON r.owner_type = 'org' AND r.owner_id = o.id
                       WHERE r.is_template = true AND r.deleted_at IS NULL
                         AND r.visibility = 'public'
                       ORDER BY owner_slug, r.name"#,
                )
                .fetch_all(p)
                .await
            }
            .map_err(|e| format!("list_template_repositories: {e}"))?;
            Ok(rows.iter().map(|r| map_template_repo!(r)).collect())
        }
        DbPool::MySql(p) => {
            let rows = if let Some(uid) = viewer_user_id {
                let sql = format!(
                    r#"SELECT r.id, r.owner_id, r.owner_type, r.name, r.description, r.visibility,
                       COALESCE(u.username, o.slug, '') AS owner_slug
                       FROM repositories r
                       LEFT JOIN users u ON r.owner_type = 'user' AND r.owner_id = u.id
                       LEFT JOIN organizations o ON r.owner_type = 'org' AND r.owner_id = o.id
                       WHERE r.is_template = 1 AND r.deleted_at IS NULL
                         AND {VIEWER_CAN_SEE}
                       ORDER BY owner_slug, r.name"#
                );
                sqlx::query(sqlx::AssertSqlSafe(&*sql))
                    .bind(uid)
                    .bind(uid)
                    .bind(uid)
                    .fetch_all(p)
                    .await
            } else {
                sqlx::query(
                    r#"SELECT r.id, r.owner_id, r.owner_type, r.name, r.description, r.visibility,
                       COALESCE(u.username, o.slug, '') AS owner_slug
                       FROM repositories r
                       LEFT JOIN users u ON r.owner_type = 'user' AND r.owner_id = u.id
                       LEFT JOIN organizations o ON r.owner_type = 'org' AND r.owner_id = o.id
                       WHERE r.is_template = 1 AND r.deleted_at IS NULL
                         AND r.visibility = 'public'
                       ORDER BY owner_slug, r.name"#,
                )
                .fetch_all(p)
                .await
            }
            .map_err(|e| format!("list_template_repositories: {e}"))?;
            Ok(rows.iter().map(|r| map_template_repo!(r)).collect())
        }
    }
}

/// How many instance packs still reference this content-addressed digest.
pub async fn count_instance_template_packs_by_digest(
    pool: &DbPool,
    digest: &str,
) -> Result<i64, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let n: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM instance_template_packs WHERE content_digest = ?",
            )
            .bind(digest)
            .fetch_one(p)
            .await
            .map_err(|e| format!("count_instance_template_packs_by_digest: {e}"))?;
            Ok(n)
        }
        DbPool::Postgres(p) => {
            let n: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM instance_template_packs WHERE content_digest = $1",
            )
            .bind(digest)
            .fetch_one(p)
            .await
            .map_err(|e| format!("count_instance_template_packs_by_digest: {e}"))?;
            Ok(n)
        }
        DbPool::MySql(p) => {
            let n: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM instance_template_packs WHERE content_digest = ?",
            )
            .bind(digest)
            .fetch_one(p)
            .await
            .map_err(|e| format!("count_instance_template_packs_by_digest: {e}"))?;
            Ok(n)
        }
    }
}
