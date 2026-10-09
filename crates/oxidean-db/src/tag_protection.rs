//! Protected tag rulesets (GIT-21) — `tag_protection_rules` CRUD across dialects.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct TagProtectionRuleRow {
    pub id: String,
    pub repo_id: String,
    pub pattern: String,
    pub allow_create: bool,
    pub allow_update: bool,
    pub allow_delete: bool,
    pub enforce_admins: bool,
    pub created_at: String,
    pub updated_at: String,
}

macro_rules! flag_col {
    ($row:expr, $col:expr) => {{
        match $row.try_get::<bool, _>($col) {
            Ok(v) => v,
            Err(_) => {
                let n: i64 = $row
                    .try_get($col)
                    .map_err(|e| format!("tag_protection {}: {e}", $col))?;
                n != 0
            }
        }
    }};
}

macro_rules! map_rule {
    ($row:expr) => {{
        let row = $row;
        TagProtectionRuleRow {
            id: row.try_get("id").map_err(|e| format!("tag rule id: {e}"))?,
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("tag rule repo_id: {e}"))?,
            pattern: row
                .try_get("pattern")
                .map_err(|e| format!("tag rule pattern: {e}"))?,
            allow_create: flag_col!(row, "allow_create"),
            allow_update: flag_col!(row, "allow_update"),
            allow_delete: flag_col!(row, "allow_delete"),
            enforce_admins: flag_col!(row, "enforce_admins"),
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("tag rule created_at: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("tag rule updated_at: {e}"))?,
        }
    }};
}

const RULE_SELECT_PG: &str =
    "SELECT id, repo_id, pattern, allow_create, allow_update, allow_delete, \
 enforce_admins, \
 to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at, \
 to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at \
 FROM tag_protection_rules";

const RULE_SELECT_MYSQL: &str =
    "SELECT id, repo_id, pattern, allow_create, allow_update, allow_delete, \
 enforce_admins, \
 DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at, \
 DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at \
 FROM tag_protection_rules";

const RULE_SELECT_SQLITE: &str =
    "SELECT id, repo_id, pattern, allow_create, allow_update, allow_delete, \
 enforce_admins, \
 strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at, \
 strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at \
 FROM tag_protection_rules";

fn as_int(b: bool) -> i32 {
    i32::from(b)
}

pub async fn list_rules(pool: &DbPool, repo_id: &str) -> Result<Vec<TagProtectionRuleRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{RULE_SELECT_PG} WHERE repo_id = $1 ORDER BY created_at ASC, id ASC"
            )))
            .bind(repo_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list tag_protection_rules: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_rule!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{RULE_SELECT_MYSQL} WHERE repo_id = ? ORDER BY created_at ASC, id ASC"
            )))
            .bind(repo_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list tag_protection_rules: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_rule!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{RULE_SELECT_SQLITE} WHERE repo_id = ?1 ORDER BY created_at ASC, id ASC"
            )))
            .bind(repo_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list tag_protection_rules: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_rule!(r));
            }
            Ok(out)
        }
    }
}

pub async fn find_rule(
    pool: &DbPool,
    repo_id: &str,
    rule_id: &str,
) -> Result<Option<TagProtectionRuleRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{RULE_SELECT_PG} WHERE repo_id = $1 AND id = $2"
            )))
            .bind(repo_id)
            .bind(rule_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find tag_protection_rule: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_rule!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{RULE_SELECT_MYSQL} WHERE repo_id = ? AND id = ?"
            )))
            .bind(repo_id)
            .bind(rule_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find tag_protection_rule: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_rule!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{RULE_SELECT_SQLITE} WHERE repo_id = ?1 AND id = ?2"
            )))
            .bind(repo_id)
            .bind(rule_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find tag_protection_rule: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_rule!(&r)),
                None => None,
            })
        }
    }
}

pub async fn insert_rule(
    pool: &DbPool,
    id: &str,
    repo_id: &str,
    pattern: &str,
    allow_create: bool,
    allow_update: bool,
    allow_delete: bool,
    enforce_admins: bool,
) -> Result<TagProtectionRuleRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO tag_protection_rules (\
               id, repo_id, pattern, allow_create, allow_update, allow_delete, enforce_admins) \
               VALUES ($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(pattern)
            .bind(allow_create)
            .bind(allow_update)
            .bind(allow_delete)
            .bind(enforce_admins)
            .execute(p)
            .await
            .map_err(|e| format!("insert tag_protection_rule: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO tag_protection_rules (\
               id, repo_id, pattern, allow_create, allow_update, allow_delete, enforce_admins) \
               VALUES (?,?,?,?,?,?,?)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(pattern)
            .bind(as_int(allow_create))
            .bind(as_int(allow_update))
            .bind(as_int(allow_delete))
            .bind(as_int(enforce_admins))
            .execute(p)
            .await
            .map_err(|e| format!("insert tag_protection_rule: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO tag_protection_rules (\
               id, repo_id, pattern, allow_create, allow_update, allow_delete, enforce_admins) \
               VALUES (?1,?2,?3,?4,?5,?6,?7)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(pattern)
            .bind(as_int(allow_create))
            .bind(as_int(allow_update))
            .bind(as_int(allow_delete))
            .bind(as_int(enforce_admins))
            .execute(p)
            .await
            .map_err(|e| format!("insert tag_protection_rule: {e}"))?;
        }
    }
    find_rule(pool, repo_id, id)
        .await?
        .ok_or_else(|| "tag protection rule missing after insert".into())
}

pub async fn update_rule(
    pool: &DbPool,
    repo_id: &str,
    rule_id: &str,
    pattern: &str,
    allow_create: bool,
    allow_update: bool,
    allow_delete: bool,
    enforce_admins: bool,
) -> Result<TagProtectionRuleRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            let res = sqlx::query(
                "UPDATE tag_protection_rules SET pattern=$1, allow_create=$2, allow_update=$3, \
                  allow_delete=$4, enforce_admins=$5, updated_at=now() \
                  WHERE repo_id=$6 AND id=$7",
            )
            .bind(pattern)
            .bind(allow_create)
            .bind(allow_update)
            .bind(allow_delete)
            .bind(enforce_admins)
            .bind(repo_id)
            .bind(rule_id)
            .execute(p)
            .await
            .map_err(|e| format!("update tag_protection_rule: {e}"))?;
            if res.rows_affected() == 0 {
                return Err("tag protection rule not found".into());
            }
        }
        DbPool::MySql(p) => {
            let res = sqlx::query(
                "UPDATE tag_protection_rules SET pattern=?, allow_create=?, allow_update=?, \
                  allow_delete=?, enforce_admins=?, updated_at=CURRENT_TIMESTAMP \
                  WHERE repo_id=? AND id=?",
            )
            .bind(pattern)
            .bind(as_int(allow_create))
            .bind(as_int(allow_update))
            .bind(as_int(allow_delete))
            .bind(as_int(enforce_admins))
            .bind(repo_id)
            .bind(rule_id)
            .execute(p)
            .await
            .map_err(|e| format!("update tag_protection_rule: {e}"))?;
            if res.rows_affected() == 0 {
                return Err("tag protection rule not found".into());
            }
        }
        DbPool::Sqlite(p) => {
            let res = sqlx::query(
                "UPDATE tag_protection_rules SET pattern=?1, allow_create=?2, allow_update=?3, \
                  allow_delete=?4, enforce_admins=?5, \
                  updated_at=strftime('%Y-%m-%d %H:%M:%S','now') \
                  WHERE repo_id=?6 AND id=?7",
            )
            .bind(pattern)
            .bind(as_int(allow_create))
            .bind(as_int(allow_update))
            .bind(as_int(allow_delete))
            .bind(as_int(enforce_admins))
            .bind(repo_id)
            .bind(rule_id)
            .execute(p)
            .await
            .map_err(|e| format!("update tag_protection_rule: {e}"))?;
            if res.rows_affected() == 0 {
                return Err("tag protection rule not found".into());
            }
        }
    }
    find_rule(pool, repo_id, rule_id)
        .await?
        .ok_or_else(|| "tag protection rule missing after update".into())
}

pub async fn delete_rule(pool: &DbPool, repo_id: &str, rule_id: &str) -> Result<(), String> {
    let affected = match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM tag_protection_rules WHERE repo_id = $1 AND id = $2")
                .bind(repo_id)
                .bind(rule_id)
                .execute(p)
                .await
                .map_err(|e| format!("delete tag_protection_rule: {e}"))?
                .rows_affected()
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM tag_protection_rules WHERE repo_id = ? AND id = ?")
                .bind(repo_id)
                .bind(rule_id)
                .execute(p)
                .await
                .map_err(|e| format!("delete tag_protection_rule: {e}"))?
                .rows_affected()
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM tag_protection_rules WHERE repo_id = ?1 AND id = ?2")
                .bind(repo_id)
                .bind(rule_id)
                .execute(p)
                .await
                .map_err(|e| format!("delete tag_protection_rule: {e}"))?
                .rows_affected()
        }
    };
    if affected == 0 {
        return Err("tag protection rule not found".into());
    }
    Ok(())
}
