//! Per-user email addresses (primary + secondary) for account identity and Verified commits.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct UserEmailRow {
    pub id: String,
    pub user_id: String,
    pub email: String,
    pub is_primary: bool,
    pub verified_at: Option<String>,
    pub created_at: String,
}

macro_rules! map_bool {
    ($row:expr, $name:expr) => {{
        $row.try_get::<i64, _>($name)
            .or_else(|_| $row.try_get::<i32, _>($name).map(|v| i64::from(v)))
            .or_else(|_| $row.try_get::<i16, _>($name).map(|v| i64::from(v)))
            .or_else(|_| $row.try_get::<i8, _>($name).map(|v| i64::from(v)))
            .or_else(|_| {
                $row.try_get::<bool, _>($name)
                    .map(|v| if v { 1 } else { 0 })
            })
            .unwrap_or(0)
            != 0
    }};
}

macro_rules! map_user_email {
    ($row:expr) => {{
        let row = $row;
        UserEmailRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("user email row: {e}"))?,
            user_id: row
                .try_get("user_id")
                .map_err(|e| format!("user email row: {e}"))?,
            email: row
                .try_get("email")
                .map_err(|e| format!("user email row: {e}"))?,
            is_primary: map_bool!(row, "is_primary"),
            verified_at: row
                .try_get("verified_at")
                .map_err(|e| format!("user email row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("user email row: {e}"))?,
        }
    }};
}

const SELECT_PG: &str = "SELECT id, user_id, email,
       CASE WHEN is_primary THEN 1 ELSE 0 END AS is_primary,
       CASE WHEN verified_at IS NULL THEN NULL
            ELSE to_char(verified_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') END AS verified_at,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM user_emails";

const SELECT_MYSQL: &str = "SELECT id, user_id, email, is_primary,
       CASE WHEN verified_at IS NULL THEN NULL
            ELSE DATE_FORMAT(verified_at, '%Y-%m-%dT%H:%i:%sZ') END AS verified_at,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM user_emails";

const SELECT_SQLITE: &str = "SELECT id, user_id, email, is_primary,
       CASE WHEN verified_at IS NULL THEN NULL
            ELSE strftime('%Y-%m-%dT%H:%M:%SZ', verified_at) END AS verified_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at
FROM user_emails";

/// Insert an email row for a user.
pub async fn create(
    pool: &DbPool,
    id: &str,
    user_id: &str,
    email: &str,
    is_primary: bool,
    verified_at: Option<&str>,
) -> Result<UserEmailRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO user_emails (id, user_id, email, is_primary, verified_at)
VALUES ($1, $2, $3, $4, $5::timestamptz)",
            )
            .bind(id)
            .bind(user_id)
            .bind(email)
            .bind(is_primary)
            .bind(verified_at)
            .execute(p)
            .await
            .map_err(|e| format!("create user email failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO user_emails (id, user_id, email, is_primary, verified_at)
VALUES (?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(user_id)
            .bind(email)
            .bind(is_primary)
            .bind(verified_at)
            .execute(p)
            .await
            .map_err(|e| format!("create user email failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO user_emails (id, user_id, email, is_primary, verified_at)
VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(id)
            .bind(user_id)
            .bind(email)
            .bind(if is_primary { 1 } else { 0 })
            .bind(verified_at)
            .execute(p)
            .await
            .map_err(|e| format!("create user email failed: {e}"))?;
        }
    }
    find_by_id(pool, id)
        .await?
        .ok_or_else(|| "create user email failed: row missing after insert".into())
}

pub async fn find_by_id(pool: &DbPool, id: &str) -> Result<Option<UserEmailRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!("{SELECT_PG} WHERE id = $1")))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find user email failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_user_email!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!("{SELECT_MYSQL} WHERE id = ?")))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find user email failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_user_email!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_SQLITE} WHERE id = ?1"
            )))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find user email failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_user_email!(&r)),
                None => None,
            })
        }
    }
}

pub async fn find_by_email(pool: &DbPool, email: &str) -> Result<Option<UserEmailRow>, String> {
    let email = email.trim().to_ascii_lowercase();
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_PG} WHERE lower(email) = lower($1)"
            )))
            .bind(&email)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find user email by address failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_user_email!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_MYSQL} WHERE lower(email) = lower(?)"
            )))
            .bind(&email)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find user email by address failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_user_email!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_SQLITE} WHERE lower(email) = lower(?1)"
            )))
            .bind(&email)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find user email by address failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_user_email!(&r)),
                None => None,
            })
        }
    }
}

/// Batch `find_by_email` — one `IN (...)` round trip (commit author resolution).
/// `emails` are matched case-insensitively against stored `email`.
pub async fn find_many_by_email(
    pool: &DbPool,
    emails: &[String],
) -> Result<Vec<UserEmailRow>, String> {
    let emails: Vec<String> = emails
        .iter()
        .map(|e| e.trim().to_ascii_lowercase())
        .filter(|e| !e.is_empty())
        .collect();
    if emails.is_empty() {
        return Ok(Vec::new());
    }
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_PG} WHERE lower(email) = ANY($1)"
            )))
            .bind(&emails)
            .fetch_all(p)
            .await
            .map_err(|e| format!("find user emails by addresses failed: {e}"))?;
            rows.iter().map(|r| Ok(map_user_email!(r))).collect()
        }
        DbPool::MySql(p) => {
            let in_list =
                crate::dialect::in_placeholders(crate::dialect::Dialect::MySql, 1, emails.len());
            let q_str = format!("{SELECT_MYSQL} WHERE lower(email) IN ({in_list})");
            let q = emails
                .iter()
                .fold(sqlx::query(sqlx::AssertSqlSafe(&*q_str)), |q, e| q.bind(e));
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("find user emails by addresses failed: {e}"))?;
            rows.iter().map(|r| Ok(map_user_email!(r))).collect()
        }
        DbPool::Sqlite(p) => {
            let in_list =
                crate::dialect::in_placeholders(crate::dialect::Dialect::Sqlite, 1, emails.len());
            let q_str = format!("{SELECT_SQLITE} WHERE lower(email) IN ({in_list})");
            let q = emails
                .iter()
                .fold(sqlx::query(sqlx::AssertSqlSafe(&*q_str)), |q, e| q.bind(e));
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("find user emails by addresses failed: {e}"))?;
            rows.iter().map(|r| Ok(map_user_email!(r))).collect()
        }
    }
}

/// Verified `(user_id, email)` pairs for many users — one round trip
/// (commit signature policy + allowed_signers batching).
pub async fn list_verified_emails_for_users(
    pool: &DbPool,
    user_ids: &[String],
) -> Result<Vec<(String, String)>, String> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT user_id, email FROM user_emails
                 WHERE user_id = ANY($1) AND verified_at IS NOT NULL",
            )
            .bind(user_ids)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list verified user emails failed: {e}"))?;
            rows.iter()
                .map(|r| {
                    Ok((
                        r.try_get::<String, _>("user_id")
                            .map_err(|e| format!("verified email row: {e}"))?,
                        r.try_get::<String, _>("email")
                            .map_err(|e| format!("verified email row: {e}"))?,
                    ))
                })
                .collect()
        }
        DbPool::MySql(p) => {
            let in_list =
                crate::dialect::in_placeholders(crate::dialect::Dialect::MySql, 1, user_ids.len());
            let q_str = format!(
                "SELECT user_id, email FROM user_emails
                 WHERE user_id IN ({in_list}) AND verified_at IS NOT NULL"
            );
            let q = user_ids
                .iter()
                .fold(sqlx::query(sqlx::AssertSqlSafe(&*q_str)), |q, id| {
                    q.bind(id)
                });
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("list verified user emails failed: {e}"))?;
            rows.iter()
                .map(|r| {
                    Ok((
                        r.try_get::<String, _>("user_id")
                            .map_err(|e| format!("verified email row: {e}"))?,
                        r.try_get::<String, _>("email")
                            .map_err(|e| format!("verified email row: {e}"))?,
                    ))
                })
                .collect()
        }
        DbPool::Sqlite(p) => {
            let in_list =
                crate::dialect::in_placeholders(crate::dialect::Dialect::Sqlite, 1, user_ids.len());
            let q_str = format!(
                "SELECT user_id, email FROM user_emails
                 WHERE user_id IN ({in_list}) AND verified_at IS NOT NULL"
            );
            let q = user_ids
                .iter()
                .fold(sqlx::query(sqlx::AssertSqlSafe(&*q_str)), |q, id| {
                    q.bind(id)
                });
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("list verified user emails failed: {e}"))?;
            rows.iter()
                .map(|r| {
                    Ok((
                        r.try_get::<String, _>("user_id")
                            .map_err(|e| format!("verified email row: {e}"))?,
                        r.try_get::<String, _>("email")
                            .map_err(|e| format!("verified email row: {e}"))?,
                    ))
                })
                .collect()
        }
    }
}

pub async fn list_for_user(pool: &DbPool, user_id: &str) -> Result<Vec<UserEmailRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_PG} WHERE user_id = $1 ORDER BY is_primary DESC, created_at ASC"
            )))
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list user emails failed: {e}"))?;
            let mut mapped = Vec::with_capacity(rows.len());
            for r in rows {
                mapped.push(map_user_email!(&r));
            }
            Ok(mapped)
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_MYSQL} WHERE user_id = ? ORDER BY is_primary DESC, created_at ASC"
            )))
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list user emails failed: {e}"))?;
            let mut mapped = Vec::with_capacity(rows.len());
            for r in rows {
                mapped.push(map_user_email!(&r));
            }
            Ok(mapped)
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_SQLITE} WHERE user_id = ?1 ORDER BY is_primary DESC, created_at ASC"
            )))
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list user emails failed: {e}"))?;
            let mut mapped = Vec::with_capacity(rows.len());
            for r in rows {
                mapped.push(map_user_email!(&r));
            }
            Ok(mapped)
        }
    }
}

pub async fn count_for_user(pool: &DbPool, user_id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*)::bigint FROM user_emails WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("count user emails failed: {e}")),
        DbPool::MySql(p) => {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM user_emails WHERE user_id = ?")
                .bind(user_id)
                .fetch_one(p)
                .await
                .map_err(|e| format!("count user emails failed: {e}"))
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM user_emails WHERE user_id = ?1")
                .bind(user_id)
                .fetch_one(p)
                .await
                .map_err(|e| format!("count user emails failed: {e}"))
        }
    }
}

pub async fn set_verified_at(
    pool: &DbPool,
    id: &str,
    verified_at: Option<&str>,
) -> Result<UserEmailRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE user_emails SET verified_at = $2::timestamptz WHERE id = $1")
                .bind(id)
                .bind(verified_at)
                .execute(p)
                .await
                .map_err(|e| format!("set user email verified_at failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE user_emails SET verified_at = ? WHERE id = ?")
                .bind(verified_at)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("set user email verified_at failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE user_emails SET verified_at = ?2 WHERE id = ?1")
                .bind(id)
                .bind(verified_at)
                .execute(p)
                .await
                .map_err(|e| format!("set user email verified_at failed: {e}"))?;
        }
    }
    find_by_id(pool, id)
        .await?
        .ok_or_else(|| "set user email verified_at failed: not found".into())
}

/// Make `id` the sole primary email for its user; returns the new primary row.
pub async fn set_primary(pool: &DbPool, id: &str) -> Result<UserEmailRow, String> {
    let row = find_by_id(pool, id)
        .await?
        .ok_or_else(|| "set primary email failed: not found".to_string())?;
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("set primary email tx: {e}"))?;
            sqlx::query("UPDATE user_emails SET is_primary = false WHERE user_id = $1")
                .bind(&row.user_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("clear primary flags failed: {e}"))?;
            sqlx::query("UPDATE user_emails SET is_primary = true WHERE id = $1")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("set primary flag failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("set primary email commit: {e}"))?;
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("set primary email tx: {e}"))?;
            sqlx::query("UPDATE user_emails SET is_primary = 0 WHERE user_id = ?")
                .bind(&row.user_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("clear primary flags failed: {e}"))?;
            sqlx::query("UPDATE user_emails SET is_primary = 1 WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("set primary flag failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("set primary email commit: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("set primary email tx: {e}"))?;
            sqlx::query("UPDATE user_emails SET is_primary = 0 WHERE user_id = ?1")
                .bind(&row.user_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("clear primary flags failed: {e}"))?;
            sqlx::query("UPDATE user_emails SET is_primary = 1 WHERE id = ?1")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("set primary flag failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("set primary email commit: {e}"))?;
        }
    }
    find_by_id(pool, id)
        .await?
        .ok_or_else(|| "set primary email failed: row missing".into())
}

pub async fn update_email_address(
    pool: &DbPool,
    id: &str,
    email: &str,
) -> Result<UserEmailRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE user_emails SET email = $2 WHERE id = $1")
                .bind(id)
                .bind(email)
                .execute(p)
                .await
                .map_err(|e| format!("update user email address failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE user_emails SET email = ? WHERE id = ?")
                .bind(email)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("update user email address failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE user_emails SET email = ?2 WHERE id = ?1")
                .bind(id)
                .bind(email)
                .execute(p)
                .await
                .map_err(|e| format!("update user email address failed: {e}"))?;
        }
    }
    find_by_id(pool, id)
        .await?
        .ok_or_else(|| "update user email address failed: not found".into())
}

pub async fn delete(pool: &DbPool, id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM user_emails WHERE id = $1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete user email failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM user_emails WHERE id = ?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete user email failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM user_emails WHERE id = ?1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete user email failed: {e}"))?;
        }
    }
    Ok(())
}

pub async fn find_primary_for_user(
    pool: &DbPool,
    user_id: &str,
) -> Result<Option<UserEmailRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_PG} WHERE user_id = $1 AND is_primary = true LIMIT 1"
            )))
            .bind(user_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find primary user email failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_user_email!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_MYSQL} WHERE user_id = ? AND is_primary = 1 LIMIT 1"
            )))
            .bind(user_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find primary user email failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_user_email!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SELECT_SQLITE} WHERE user_id = ?1 AND is_primary = 1 LIMIT 1"
            )))
            .bind(user_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find primary user email failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_user_email!(&r)),
                None => None,
            })
        }
    }
}

/// List verified email addresses for a user (for allowed_signers / Verified policy).
pub async fn list_verified_emails_for_user(
    pool: &DbPool,
    user_id: &str,
) -> Result<Vec<String>, String> {
    let rows = list_for_user(pool, user_id).await?;
    Ok(rows
        .into_iter()
        .filter(|r| r.verified_at.is_some())
        .map(|r| r.email)
        .collect())
}
