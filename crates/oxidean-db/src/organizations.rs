//! Organization CRUD via `DbPool` match — dialect branching stays in this crate.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct OrganizationRow {
    pub id: String,
    pub slug: String,
    pub display_name: String,
    pub member_base_permission: String,
    pub created_at: String,
    pub updated_at: String,
}

macro_rules! map_org {
    ($row:expr) => {{
        let row = $row;
        OrganizationRow {
            id: row.try_get("id").map_err(|e| format!("org row: {e}"))?,
            slug: row.try_get("slug").map_err(|e| format!("org row: {e}"))?,
            display_name: row
                .try_get("display_name")
                .map_err(|e| format!("org row: {e}"))?,
            member_base_permission: row
                .try_get("member_base_permission")
                .map_err(|e| format!("org row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("org row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("org row: {e}"))?,
        }
    }};
}

const ORG_SELECT_PG: &str = "SELECT id, slug, display_name, member_base_permission,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at
FROM organizations";

const ORG_SELECT_MYSQL: &str = "SELECT id, slug, display_name, member_base_permission,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
FROM organizations";

const ORG_SELECT_SQLITE: &str = "SELECT id, slug, display_name, member_base_permission,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at
FROM organizations";

/// Org-level Member base permission string (`none` | `read` | `write`), if org exists.
pub async fn find_member_base_permission(
    pool: &DbPool,
    org_id: &str,
) -> Result<Option<String>, String> {
    Ok(find_by_id(pool, org_id)
        .await?
        .map(|o| o.member_base_permission))
}

/// Insert an organization. Default `member_base_permission` is `none` (D-ORG-02b).
pub async fn insert_organization(
    pool: &DbPool,
    id: &str,
    slug: &str,
    display_name: &str,
    member_base_permission: &str,
) -> Result<OrganizationRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO organizations (id, slug, display_name, member_base_permission)
VALUES ($1, $2, $3, $4)",
            )
            .bind(id)
            .bind(slug)
            .bind(display_name)
            .bind(member_base_permission)
            .execute(p)
            .await
            .map_err(|e| format!("insert organization failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO organizations (id, slug, display_name, member_base_permission)
VALUES (?, ?, ?, ?)",
            )
            .bind(id)
            .bind(slug)
            .bind(display_name)
            .bind(member_base_permission)
            .execute(p)
            .await
            .map_err(|e| format!("insert organization failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO organizations (id, slug, display_name, member_base_permission)
VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(id)
            .bind(slug)
            .bind(display_name)
            .bind(member_base_permission)
            .execute(p)
            .await
            .map_err(|e| format!("insert organization failed: {e}"))?;
        }
    }
    find_by_id(pool, id)
        .await?
        .ok_or_else(|| "insert organization failed: row missing after insert".into())
}

/// Update org profile settings (`member_base_permission` / `display_name`).
pub async fn update_settings(
    pool: &DbPool,
    id: &str,
    member_base_permission: Option<&str>,
    display_name: Option<&str>,
) -> Result<OrganizationRow, String> {
    let current = find_by_id(pool, id)
        .await?
        .ok_or_else(|| "organization not found".to_string())?;
    let base = member_base_permission.unwrap_or(current.member_base_permission.as_str());
    let name = display_name.unwrap_or(current.display_name.as_str());
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE organizations
SET member_base_permission = $2, display_name = $3, updated_at = now()
WHERE id = $1",
            )
            .bind(id)
            .bind(base)
            .bind(name)
            .execute(p)
            .await
            .map_err(|e| format!("update organization failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE organizations
SET member_base_permission = ?, display_name = ?, updated_at = NOW()
WHERE id = ?",
            )
            .bind(base)
            .bind(name)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("update organization failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE organizations
SET member_base_permission = ?2, display_name = ?3,
    updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
WHERE id = ?1",
            )
            .bind(id)
            .bind(base)
            .bind(name)
            .execute(p)
            .await
            .map_err(|e| format!("update organization failed: {e}"))?;
        }
    }
    find_by_id(pool, id)
        .await?
        .ok_or_else(|| "update organization failed: row missing after update".into())
}

pub async fn find_by_id(pool: &DbPool, id: &str) -> Result<Option<OrganizationRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(&format!("{ORG_SELECT_PG} WHERE id = $1"))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find organization failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_org!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(&format!("{ORG_SELECT_MYSQL} WHERE id = ?"))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find organization failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_org!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(&format!("{ORG_SELECT_SQLITE} WHERE id = ?1"))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find organization failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_org!(&r)),
                None => None,
            })
        }
    }
}

/// Batch `find_by_id` — one `IN (...)` round trip (pull list head-owner enrichment).
pub async fn find_many_by_id(
    pool: &DbPool,
    ids: &[String],
) -> Result<Vec<OrganizationRow>, String> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(&format!("{ORG_SELECT_PG} WHERE id = ANY($1)"))
                .bind(ids)
                .fetch_all(p)
                .await
                .map_err(|e| format!("find organizations by ids failed: {e}"))?;
            rows.iter().map(|r| Ok(map_org!(r))).collect()
        }
        DbPool::MySql(p) => {
            let in_list =
                crate::dialect::in_placeholders(crate::dialect::Dialect::MySql, 1, ids.len());
            let q_str = format!("{ORG_SELECT_MYSQL} WHERE id IN ({in_list})");
            let q = sqlx::query(&q_str);
            let q = ids.iter().fold(q, |q, id| q.bind(id));
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("find organizations by ids failed: {e}"))?;
            rows.iter().map(|r| Ok(map_org!(r))).collect()
        }
        DbPool::Sqlite(p) => {
            let in_list =
                crate::dialect::in_placeholders(crate::dialect::Dialect::Sqlite, 1, ids.len());
            let q_str = format!("{ORG_SELECT_SQLITE} WHERE id IN ({in_list})");
            let q = sqlx::query(&q_str);
            let q = ids.iter().fold(q, |q, id| q.bind(id));
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("find organizations by ids failed: {e}"))?;
            rows.iter().map(|r| Ok(map_org!(r))).collect()
        }
    }
}

/// Hard-delete an organization row (member/invite FKs cascade).
pub async fn delete_organization(pool: &DbPool, id: &str) -> Result<(), String> {
    let n = match pool {
        DbPool::Postgres(p) => sqlx::query("DELETE FROM organizations WHERE id = $1")
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("delete organization failed: {e}"))?
            .rows_affected(),
        DbPool::MySql(p) => sqlx::query("DELETE FROM organizations WHERE id = ?")
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("delete organization failed: {e}"))?
            .rows_affected(),
        DbPool::Sqlite(p) => sqlx::query("DELETE FROM organizations WHERE id = ?1")
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("delete organization failed: {e}"))?
            .rows_affected(),
    };
    if n == 0 {
        return Err("organization not found".into());
    }
    Ok(())
}

/// Case-insensitive slug lookup (D-ORG-01 shared namespace).
pub async fn find_by_slug(pool: &DbPool, slug: &str) -> Result<Option<OrganizationRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(&format!("{ORG_SELECT_PG} WHERE lower(slug) = lower($1)"))
                .bind(slug)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find organization by slug failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_org!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(&format!("{ORG_SELECT_MYSQL} WHERE LOWER(slug) = LOWER(?)"))
                .bind(slug)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find organization by slug failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_org!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(&format!("{ORG_SELECT_SQLITE} WHERE lower(slug) = lower(?1)"))
                .bind(slug)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find organization by slug failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_org!(&r)),
                None => None,
            })
        }
    }
}
