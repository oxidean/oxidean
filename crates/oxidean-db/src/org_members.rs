//! Organization membership helpers via `DbPool` match.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct OrgMemberRow {
    pub org_id: String,
    pub user_id: String,
    pub role: String,
    pub created_at: String,
}

macro_rules! map_member {
    ($row:expr) => {{
        let row = $row;
        OrgMemberRow {
            org_id: row
                .try_get("org_id")
                .map_err(|e| format!("org member row: {e}"))?,
            user_id: row
                .try_get("user_id")
                .map_err(|e| format!("org member row: {e}"))?,
            role: row
                .try_get("role")
                .map_err(|e| format!("org member row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("org member row: {e}"))?,
        }
    }};
}

const MEMBER_SELECT_PG: &str = "SELECT org_id, user_id, role,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM organization_members";

const MEMBER_SELECT_MYSQL: &str = "SELECT org_id, user_id, role,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM organization_members";

const MEMBER_SELECT_SQLITE: &str = "SELECT org_id, user_id, role,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at
FROM organization_members";

/// Insert a membership row (`owner` | `admin` | `member` — D-ORG-02a).
pub async fn insert_member(
    pool: &DbPool,
    org_id: &str,
    user_id: &str,
    role: &str,
) -> Result<OrgMemberRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO organization_members (org_id, user_id, role)
VALUES ($1, $2, $3)",
            )
            .bind(org_id)
            .bind(user_id)
            .bind(role)
            .execute(p)
            .await
            .map_err(|e| format!("insert org member failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO organization_members (org_id, user_id, role)
VALUES (?, ?, ?)",
            )
            .bind(org_id)
            .bind(user_id)
            .bind(role)
            .execute(p)
            .await
            .map_err(|e| format!("insert org member failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO organization_members (org_id, user_id, role)
VALUES (?1, ?2, ?3)",
            )
            .bind(org_id)
            .bind(user_id)
            .bind(role)
            .execute(p)
            .await
            .map_err(|e| format!("insert org member failed: {e}"))?;
        }
    }
    find_member(pool, org_id, user_id)
        .await?
        .ok_or_else(|| "insert org member failed: row missing after insert".into())
}

/// Insert the creating user as Owner (ORG-01 / D-ORG-02a).
pub async fn insert_owner_membership(
    pool: &DbPool,
    org_id: &str,
    user_id: &str,
) -> Result<OrgMemberRow, String> {
    insert_member(pool, org_id, user_id, "owner").await
}

/// Membership role for ACL (`owner` | `admin` | `member`), or `None` if not a member.
pub async fn find_member_role(
    pool: &DbPool,
    org_id: &str,
    user_id: &str,
) -> Result<Option<String>, String> {
    Ok(find_member(pool, org_id, user_id).await?.map(|m| m.role))
}

/// Membership row with username for `org.members.list` (no email).
#[derive(Debug, Clone)]
pub struct OrgMemberListRow {
    pub org_id: String,
    pub user_id: String,
    pub username: String,
    pub role: String,
    pub created_at: String,
}

/// Org + caller's role for `org.listMine`.
#[derive(Debug, Clone)]
pub struct OrgMineRow {
    pub id: String,
    pub slug: String,
    pub display_name: String,
    pub member_base_permission: String,
    pub role: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Count Owners in an org (last-owner guard).
pub async fn count_owners(pool: &DbPool, org_id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row: (i64,) = sqlx::query_as(
                "SELECT COUNT(*)::bigint FROM organization_members
WHERE org_id = $1 AND role = 'owner'",
            )
            .bind(org_id)
            .fetch_one(p)
            .await
            .map_err(|e| format!("count org owners failed: {e}"))?;
            Ok(row.0)
        }
        DbPool::MySql(p) => {
            let row: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM organization_members
WHERE org_id = ? AND role = 'owner'",
            )
            .bind(org_id)
            .fetch_one(p)
            .await
            .map_err(|e| format!("count org owners failed: {e}"))?;
            Ok(row.0)
        }
        DbPool::Sqlite(p) => {
            let row: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM organization_members
WHERE org_id = ?1 AND role = 'owner'",
            )
            .bind(org_id)
            .fetch_one(p)
            .await
            .map_err(|e| format!("count org owners failed: {e}"))?;
            Ok(row.0)
        }
    }
}

pub async fn update_member_role(
    pool: &DbPool,
    org_id: &str,
    user_id: &str,
    role: &str,
) -> Result<OrgMemberRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            let n = sqlx::query(
                "UPDATE organization_members SET role = $3
WHERE org_id = $1 AND user_id = $2",
            )
            .bind(org_id)
            .bind(user_id)
            .bind(role)
            .execute(p)
            .await
            .map_err(|e| format!("update org member role failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("org member not found".into());
            }
        }
        DbPool::MySql(p) => {
            let n = sqlx::query(
                "UPDATE organization_members SET role = ?
WHERE org_id = ? AND user_id = ?",
            )
            .bind(role)
            .bind(org_id)
            .bind(user_id)
            .execute(p)
            .await
            .map_err(|e| format!("update org member role failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("org member not found".into());
            }
        }
        DbPool::Sqlite(p) => {
            let n = sqlx::query(
                "UPDATE organization_members SET role = ?3
WHERE org_id = ?1 AND user_id = ?2",
            )
            .bind(org_id)
            .bind(user_id)
            .bind(role)
            .execute(p)
            .await
            .map_err(|e| format!("update org member role failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("org member not found".into());
            }
        }
    }
    find_member(pool, org_id, user_id)
        .await?
        .ok_or_else(|| "org member not found".into())
}

pub async fn remove_member(pool: &DbPool, org_id: &str, user_id: &str) -> Result<(), String> {
    let n = match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM organization_members WHERE org_id = $1 AND user_id = $2")
                .bind(org_id)
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("remove org member failed: {e}"))?
                .rows_affected()
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM organization_members WHERE org_id = ? AND user_id = ?")
                .bind(org_id)
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("remove org member failed: {e}"))?
                .rows_affected()
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM organization_members WHERE org_id = ?1 AND user_id = ?2")
                .bind(org_id)
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("remove org member failed: {e}"))?
                .rows_affected()
        }
    };
    if n == 0 {
        return Err("org member not found".into());
    }
    Ok(())
}

pub async fn list_members(pool: &DbPool, org_id: &str) -> Result<Vec<OrgMemberListRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT m.org_id, m.user_id, u.username, m.role,
       to_char(m.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM organization_members m
JOIN users u ON u.id = m.user_id
WHERE m.org_id = $1
ORDER BY m.created_at ASC, m.user_id ASC",
            )
            .bind(org_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list org members failed: {e}"))?;
            rows.into_iter()
                .map(|row| {
                    Ok(OrgMemberListRow {
                        org_id: row
                            .try_get("org_id")
                            .map_err(|e| format!("member list: {e}"))?,
                        user_id: row
                            .try_get("user_id")
                            .map_err(|e| format!("member list: {e}"))?,
                        username: row
                            .try_get("username")
                            .map_err(|e| format!("member list: {e}"))?,
                        role: row
                            .try_get("role")
                            .map_err(|e| format!("member list: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("member list: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT m.org_id, m.user_id, u.username, m.role,
       DATE_FORMAT(m.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM organization_members m
JOIN users u ON u.id = m.user_id
WHERE m.org_id = ?
ORDER BY m.created_at ASC, m.user_id ASC",
            )
            .bind(org_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list org members failed: {e}"))?;
            rows.into_iter()
                .map(|row| {
                    Ok(OrgMemberListRow {
                        org_id: row
                            .try_get("org_id")
                            .map_err(|e| format!("member list: {e}"))?,
                        user_id: row
                            .try_get("user_id")
                            .map_err(|e| format!("member list: {e}"))?,
                        username: row
                            .try_get("username")
                            .map_err(|e| format!("member list: {e}"))?,
                        role: row
                            .try_get("role")
                            .map_err(|e| format!("member list: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("member list: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT m.org_id, m.user_id, u.username, m.role,
       strftime('%Y-%m-%dT%H:%M:%SZ', m.created_at) AS created_at
FROM organization_members m
JOIN users u ON u.id = m.user_id
WHERE m.org_id = ?1
ORDER BY m.created_at ASC, m.user_id ASC",
            )
            .bind(org_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list org members failed: {e}"))?;
            rows.into_iter()
                .map(|row| {
                    Ok(OrgMemberListRow {
                        org_id: row
                            .try_get("org_id")
                            .map_err(|e| format!("member list: {e}"))?,
                        user_id: row
                            .try_get("user_id")
                            .map_err(|e| format!("member list: {e}"))?,
                        username: row
                            .try_get("username")
                            .map_err(|e| format!("member list: {e}"))?,
                        role: row
                            .try_get("role")
                            .map_err(|e| format!("member list: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("member list: {e}"))?,
                    })
                })
                .collect()
        }
    }
}

pub async fn list_orgs_for_user(pool: &DbPool, user_id: &str) -> Result<Vec<OrgMineRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT o.id, o.slug, o.display_name, o.member_base_permission, m.role,
       to_char(o.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(o.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at
FROM organization_members m
JOIN organizations o ON o.id = m.org_id
WHERE m.user_id = $1
ORDER BY o.slug ASC",
            )
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list orgs for user failed: {e}"))?;
            rows.into_iter()
                .map(|row| {
                    Ok(OrgMineRow {
                        id: row.try_get("id").map_err(|e| format!("org mine: {e}"))?,
                        slug: row.try_get("slug").map_err(|e| format!("org mine: {e}"))?,
                        display_name: row
                            .try_get("display_name")
                            .map_err(|e| format!("org mine: {e}"))?,
                        member_base_permission: row
                            .try_get("member_base_permission")
                            .map_err(|e| format!("org mine: {e}"))?,
                        role: row.try_get("role").map_err(|e| format!("org mine: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("org mine: {e}"))?,
                        updated_at: row
                            .try_get("updated_at")
                            .map_err(|e| format!("org mine: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT o.id, o.slug, o.display_name, o.member_base_permission, m.role,
       DATE_FORMAT(o.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       DATE_FORMAT(o.updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
FROM organization_members m
JOIN organizations o ON o.id = m.org_id
WHERE m.user_id = ?
ORDER BY o.slug ASC",
            )
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list orgs for user failed: {e}"))?;
            rows.into_iter()
                .map(|row| {
                    Ok(OrgMineRow {
                        id: row.try_get("id").map_err(|e| format!("org mine: {e}"))?,
                        slug: row.try_get("slug").map_err(|e| format!("org mine: {e}"))?,
                        display_name: row
                            .try_get("display_name")
                            .map_err(|e| format!("org mine: {e}"))?,
                        member_base_permission: row
                            .try_get("member_base_permission")
                            .map_err(|e| format!("org mine: {e}"))?,
                        role: row.try_get("role").map_err(|e| format!("org mine: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("org mine: {e}"))?,
                        updated_at: row
                            .try_get("updated_at")
                            .map_err(|e| format!("org mine: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT o.id, o.slug, o.display_name, o.member_base_permission, m.role,
       strftime('%Y-%m-%dT%H:%M:%SZ', o.created_at) AS created_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', o.updated_at) AS updated_at
FROM organization_members m
JOIN organizations o ON o.id = m.org_id
WHERE m.user_id = ?1
ORDER BY o.slug ASC",
            )
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list orgs for user failed: {e}"))?;
            rows.into_iter()
                .map(|row| {
                    Ok(OrgMineRow {
                        id: row.try_get("id").map_err(|e| format!("org mine: {e}"))?,
                        slug: row.try_get("slug").map_err(|e| format!("org mine: {e}"))?,
                        display_name: row
                            .try_get("display_name")
                            .map_err(|e| format!("org mine: {e}"))?,
                        member_base_permission: row
                            .try_get("member_base_permission")
                            .map_err(|e| format!("org mine: {e}"))?,
                        role: row.try_get("role").map_err(|e| format!("org mine: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("org mine: {e}"))?,
                        updated_at: row
                            .try_get("updated_at")
                            .map_err(|e| format!("org mine: {e}"))?,
                    })
                })
                .collect()
        }
    }
}

pub async fn find_member(
    pool: &DbPool,
    org_id: &str,
    user_id: &str,
) -> Result<Option<OrgMemberRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{MEMBER_SELECT_PG} WHERE org_id = $1 AND user_id = $2"
            )))
            .bind(org_id)
            .bind(user_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find org member failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_member!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{MEMBER_SELECT_MYSQL} WHERE org_id = ? AND user_id = ?"
            )))
            .bind(org_id)
            .bind(user_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find org member failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_member!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{MEMBER_SELECT_SQLITE} WHERE org_id = ?1 AND user_id = ?2"
            )))
            .bind(org_id)
            .bind(user_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find org member failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_member!(&r)),
                None => None,
            })
        }
    }
}
