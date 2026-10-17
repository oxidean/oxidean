//! Instance-managed workflow templates (admin catalog; actions.templates
//! instance provenance). Categories and file_patterns are stored as JSON
//! arrays — parsing lives in the API layer.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct InstanceWorkflowTemplateRow {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub icon_name: Option<String>,
    pub categories_json: String,
    pub file_patterns_json: String,
    pub filename: String,
    pub content: String,
    pub enabled: bool,
    pub created_by_user_id: String,
    pub created_at: String,
    pub updated_at: String,
}

macro_rules! map_tpl {
    ($row:expr, $enabled:expr) => {{
        let row = $row;
        InstanceWorkflowTemplateRow {
            id: row.get("id"),
            slug: row.get("slug"),
            name: row.get("name"),
            description: row.get("description"),
            icon_name: row.get("icon_name"),
            categories_json: row.get("categories_json"),
            file_patterns_json: row.get("file_patterns_json"),
            filename: row.get("filename"),
            content: row.get("content"),
            enabled: $enabled,
            created_by_user_id: row.get("created_by_user_id"),
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

const SELECT_TPL_PG: &str = r#"SELECT id, slug, name, description, icon_name,
    categories_json, file_patterns_json, filename, content, enabled,
    created_by_user_id,
    created_at::text AS created_at, updated_at::text AS updated_at
    FROM instance_workflow_templates"#;

const SELECT_TPL_MYSQL: &str = r#"SELECT id, slug, name, description, icon_name,
    categories_json, file_patterns_json, filename, content, enabled,
    created_by_user_id,
    CAST(created_at AS CHAR) AS created_at, CAST(updated_at AS CHAR) AS updated_at
    FROM instance_workflow_templates"#;

const SELECT_TPL_SQLITE: &str = r#"SELECT id, slug, name, description, icon_name,
    categories_json, file_patterns_json, filename, content, enabled,
    created_by_user_id, created_at, updated_at
    FROM instance_workflow_templates"#;

pub async fn list_instance_workflow_templates(
    pool: &DbPool,
    enabled_only: bool,
) -> Result<Vec<InstanceWorkflowTemplateRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let sql = if enabled_only {
                format!("{SELECT_TPL_SQLITE} WHERE enabled = 1 ORDER BY name")
            } else {
                format!("{SELECT_TPL_SQLITE} ORDER BY name")
            };
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .fetch_all(p)
                .await
                .map_err(|e| format!("list_instance_workflow_templates: {e}"))?;
            Ok(rows
                .iter()
                .map(|r| map_tpl!(r, enabled_sqlite(r)))
                .collect())
        }
        DbPool::Postgres(p) => {
            let sql = if enabled_only {
                format!("{SELECT_TPL_PG} WHERE enabled = true ORDER BY name")
            } else {
                format!("{SELECT_TPL_PG} ORDER BY name")
            };
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .fetch_all(p)
                .await
                .map_err(|e| format!("list_instance_workflow_templates: {e}"))?;
            Ok(rows.iter().map(|r| map_tpl!(r, enabled_pg(r))).collect())
        }
        DbPool::MySql(p) => {
            let sql = if enabled_only {
                format!("{SELECT_TPL_MYSQL} WHERE enabled = 1 ORDER BY name")
            } else {
                format!("{SELECT_TPL_MYSQL} ORDER BY name")
            };
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .fetch_all(p)
                .await
                .map_err(|e| format!("list_instance_workflow_templates: {e}"))?;
            Ok(rows.iter().map(|r| map_tpl!(r, enabled_mysql(r))).collect())
        }
    }
}

pub async fn find_instance_workflow_template_by_id(
    pool: &DbPool,
    id: &str,
) -> Result<Option<InstanceWorkflowTemplateRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_TPL_SQLITE} WHERE id = ?"
            )))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find_instance_workflow_template_by_id: {e}"))?;
            Ok(row.as_ref().map(|r| map_tpl!(r, enabled_sqlite(r))))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_TPL_PG} WHERE id = $1"
            )))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find_instance_workflow_template_by_id: {e}"))?;
            Ok(row.as_ref().map(|r| map_tpl!(r, enabled_pg(r))))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_TPL_MYSQL} WHERE id = ?"
            )))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find_instance_workflow_template_by_id: {e}"))?;
            Ok(row.as_ref().map(|r| map_tpl!(r, enabled_mysql(r))))
        }
    }
}

pub async fn find_instance_workflow_template_by_slug(
    pool: &DbPool,
    slug: &str,
) -> Result<Option<InstanceWorkflowTemplateRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_TPL_SQLITE} WHERE slug = ?"
            )))
            .bind(slug)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find_instance_workflow_template_by_slug: {e}"))?;
            Ok(row.as_ref().map(|r| map_tpl!(r, enabled_sqlite(r))))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_TPL_PG} WHERE slug = $1"
            )))
            .bind(slug)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find_instance_workflow_template_by_slug: {e}"))?;
            Ok(row.as_ref().map(|r| map_tpl!(r, enabled_pg(r))))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_TPL_MYSQL} WHERE slug = ?"
            )))
            .bind(slug)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find_instance_workflow_template_by_slug: {e}"))?;
            Ok(row.as_ref().map(|r| map_tpl!(r, enabled_mysql(r))))
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_instance_workflow_template(
    pool: &DbPool,
    id: &str,
    slug: &str,
    name: &str,
    description: &str,
    icon_name: Option<&str>,
    categories_json: &str,
    file_patterns_json: &str,
    filename: &str,
    content: &str,
    created_by_user_id: &str,
) -> Result<InstanceWorkflowTemplateRow, String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                r#"INSERT INTO instance_workflow_templates
                (id, slug, name, description, icon_name, categories_json,
                 file_patterns_json, filename, content, enabled, created_by_user_id)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?)"#,
            )
            .bind(id)
            .bind(slug)
            .bind(name)
            .bind(description)
            .bind(icon_name)
            .bind(categories_json)
            .bind(file_patterns_json)
            .bind(filename)
            .bind(content)
            .bind(created_by_user_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert_instance_workflow_template: {e}"))?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"INSERT INTO instance_workflow_templates
                (id, slug, name, description, icon_name, categories_json,
                 file_patterns_json, filename, content, enabled, created_by_user_id)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, true, $10)"#,
            )
            .bind(id)
            .bind(slug)
            .bind(name)
            .bind(description)
            .bind(icon_name)
            .bind(categories_json)
            .bind(file_patterns_json)
            .bind(filename)
            .bind(content)
            .bind(created_by_user_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert_instance_workflow_template: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                r#"INSERT INTO instance_workflow_templates
                (id, slug, name, description, icon_name, categories_json,
                 file_patterns_json, filename, content, enabled, created_by_user_id)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?)"#,
            )
            .bind(id)
            .bind(slug)
            .bind(name)
            .bind(description)
            .bind(icon_name)
            .bind(categories_json)
            .bind(file_patterns_json)
            .bind(filename)
            .bind(content)
            .bind(created_by_user_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert_instance_workflow_template: {e}"))?;
        }
    }
    find_instance_workflow_template_by_id(pool, id)
        .await?
        .ok_or_else(|| "insert_instance_workflow_template: missing after insert".into())
}

#[allow(clippy::too_many_arguments)]
pub async fn update_instance_workflow_template(
    pool: &DbPool,
    id: &str,
    name: &str,
    description: &str,
    icon_name: Option<&str>,
    categories_json: &str,
    file_patterns_json: &str,
    filename: &str,
    content: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                r#"UPDATE instance_workflow_templates SET name = ?, description = ?,
                   icon_name = ?, categories_json = ?, file_patterns_json = ?,
                   filename = ?, content = ?,
                   updated_at = strftime('%Y-%m-%d %H:%M:%S','now') WHERE id = ?"#,
            )
            .bind(name)
            .bind(description)
            .bind(icon_name)
            .bind(categories_json)
            .bind(file_patterns_json)
            .bind(filename)
            .bind(content)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("update_instance_workflow_template: {e}"))?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"UPDATE instance_workflow_templates SET name = $1, description = $2,
                   icon_name = $3, categories_json = $4, file_patterns_json = $5,
                   filename = $6, content = $7, updated_at = now() WHERE id = $8"#,
            )
            .bind(name)
            .bind(description)
            .bind(icon_name)
            .bind(categories_json)
            .bind(file_patterns_json)
            .bind(filename)
            .bind(content)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("update_instance_workflow_template: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                r#"UPDATE instance_workflow_templates SET name = ?, description = ?,
                   icon_name = ?, categories_json = ?, file_patterns_json = ?,
                   filename = ?, content = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?"#,
            )
            .bind(name)
            .bind(description)
            .bind(icon_name)
            .bind(categories_json)
            .bind(file_patterns_json)
            .bind(filename)
            .bind(content)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("update_instance_workflow_template: {e}"))?;
        }
    }
    Ok(())
}

pub async fn set_instance_workflow_template_enabled(
    pool: &DbPool,
    id: &str,
    enabled: bool,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                r#"UPDATE instance_workflow_templates SET enabled = ?,
                   updated_at = strftime('%Y-%m-%d %H:%M:%S','now') WHERE id = ?"#,
            )
            .bind(if enabled { 1 } else { 0 })
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("set_instance_workflow_template_enabled: {e}"))?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"UPDATE instance_workflow_templates SET enabled = $1,
                   updated_at = now() WHERE id = $2"#,
            )
            .bind(enabled)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("set_instance_workflow_template_enabled: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                r#"UPDATE instance_workflow_templates SET enabled = ?,
                   updated_at = CURRENT_TIMESTAMP WHERE id = ?"#,
            )
            .bind(if enabled { 1 } else { 0 })
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("set_instance_workflow_template_enabled: {e}"))?;
        }
    }
    Ok(())
}

pub async fn delete_instance_workflow_template(pool: &DbPool, id: &str) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM instance_workflow_templates WHERE id = ?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete_instance_workflow_template: {e}"))?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM instance_workflow_templates WHERE id = $1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete_instance_workflow_template: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM instance_workflow_templates WHERE id = ?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete_instance_workflow_template: {e}"))?;
        }
    }
    Ok(())
}
