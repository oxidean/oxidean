//! Dual-scope labels + issue label/assignee assignment helpers (D-ISS-05, D-ISS-06).

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct LabelRow {
    pub id: String,
    pub name: String,
    pub color: String,
    pub description: String,
    pub org_id: Option<String>,
    pub repo_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

macro_rules! map_label {
    ($row:expr) => {{
        let row = $row;
        LabelRow {
            id: row.try_get("id").map_err(|e| format!("label row: {e}"))?,
            name: row.try_get("name").map_err(|e| format!("label row: {e}"))?,
            color: row
                .try_get("color")
                .map_err(|e| format!("label row: {e}"))?,
            description: row
                .try_get("description")
                .map_err(|e| format!("label row: {e}"))?,
            org_id: row
                .try_get("org_id")
                .map_err(|e| format!("label row: {e}"))?,
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("label row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("label row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("label row: {e}"))?,
        }
    }};
}

const LABEL_SELECT_PG: &str = "SELECT id, name, color, description, org_id, repo_id,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at
FROM labels";

const LABEL_SELECT_MYSQL: &str = "SELECT id, name, color, description, org_id, repo_id,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
FROM labels";

const LABEL_SELECT_SQLITE: &str = "SELECT id, name, color, description, org_id, repo_id,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at
FROM labels";

/// Insert an org-scoped or repo-local label. Exactly one of `org_id` / `repo_id` must be set.
pub async fn insert_label(
    pool: &DbPool,
    id: &str,
    name: &str,
    color: &str,
    description: &str,
    org_id: Option<&str>,
    repo_id: Option<&str>,
) -> Result<LabelRow, String> {
    match (org_id, repo_id) {
        (Some(_), None) | (None, Some(_)) => {}
        _ => {
            return Err("label scope requires exactly one of org_id or repo_id".into());
        }
    }
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO labels (id, name, color, description, org_id, repo_id)
VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(id)
            .bind(name)
            .bind(color)
            .bind(description)
            .bind(org_id)
            .bind(repo_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert label failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO labels (id, name, color, description, org_id, repo_id)
VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(name)
            .bind(color)
            .bind(description)
            .bind(org_id)
            .bind(repo_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert label failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO labels (id, name, color, description, org_id, repo_id)
VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(id)
            .bind(name)
            .bind(color)
            .bind(description)
            .bind(org_id)
            .bind(repo_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert label failed: {e}"))?;
        }
    }
    find_label_by_id(pool, id)
        .await?
        .ok_or_else(|| "insert label failed: row missing after insert".into())
}

pub async fn find_label_by_id(pool: &DbPool, id: &str) -> Result<Option<LabelRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{LABEL_SELECT_PG} WHERE id = $1"
            )))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find label by id failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_label!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{LABEL_SELECT_MYSQL} WHERE id = ?"
            )))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find label by id failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_label!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{LABEL_SELECT_SQLITE} WHERE id = ?1"
            )))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find label by id failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_label!(&r)),
                None => None,
            })
        }
    }
}

pub async fn update_label(
    pool: &DbPool,
    id: &str,
    name: &str,
    color: &str,
    description: &str,
) -> Result<LabelRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE labels SET name = $2, color = $3, description = $4, updated_at = NOW()
WHERE id = $1",
            )
            .bind(id)
            .bind(name)
            .bind(color)
            .bind(description)
            .execute(p)
            .await
            .map_err(|e| format!("update label failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE labels SET name = ?, color = ?, description = ?, updated_at = UTC_TIMESTAMP()
WHERE id = ?",
            )
            .bind(name)
            .bind(color)
            .bind(description)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("update label failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE labels SET name = ?2, color = ?3, description = ?4,
       updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
WHERE id = ?1",
            )
            .bind(id)
            .bind(name)
            .bind(color)
            .bind(description)
            .execute(p)
            .await
            .map_err(|e| format!("update label failed: {e}"))?;
        }
    }
    find_label_by_id(pool, id)
        .await?
        .ok_or_else(|| "update label failed: row missing after update".into())
}

pub async fn delete_label(pool: &DbPool, id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM labels WHERE id = $1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete label failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM labels WHERE id = ?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete label failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM labels WHERE id = ?1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete label failed: {e}"))?;
        }
    }
    Ok(())
}

pub async fn list_labels_for_org(pool: &DbPool, org_id: &str) -> Result<Vec<LabelRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{LABEL_SELECT_PG} WHERE org_id = $1 ORDER BY lower(name)"
            )))
            .bind(org_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list org labels failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_label!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{LABEL_SELECT_MYSQL} WHERE org_id = ? ORDER BY LOWER(name)"
            )))
            .bind(org_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list org labels failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_label!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{LABEL_SELECT_SQLITE} WHERE org_id = ?1 ORDER BY lower(name)"
            )))
            .bind(org_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list org labels failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_label!(r));
            }
            Ok(out)
        }
    }
}

pub async fn list_labels_for_repo(pool: &DbPool, repo_id: &str) -> Result<Vec<LabelRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{LABEL_SELECT_PG} WHERE repo_id = $1 ORDER BY lower(name)"
            )))
            .bind(repo_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list repo labels failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_label!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{LABEL_SELECT_MYSQL} WHERE repo_id = ? ORDER BY LOWER(name)"
            )))
            .bind(repo_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list repo labels failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_label!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{LABEL_SELECT_SQLITE} WHERE repo_id = ?1 ORDER BY lower(name)"
            )))
            .bind(repo_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list repo labels failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_label!(r));
            }
            Ok(out)
        }
    }
}

pub async fn list_hidden_label_ids(pool: &DbPool, repo_id: &str) -> Result<Vec<String>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query("SELECT label_id FROM repo_hidden_labels WHERE repo_id = $1")
                .bind(repo_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list hidden labels failed: {e}"))?;
            rows.iter()
                .map(|r| {
                    r.try_get::<String, _>("label_id")
                        .map_err(|e| format!("hidden label row: {e}"))
                })
                .collect()
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query("SELECT label_id FROM repo_hidden_labels WHERE repo_id = ?")
                .bind(repo_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list hidden labels failed: {e}"))?;
            rows.iter()
                .map(|r| {
                    r.try_get::<String, _>("label_id")
                        .map_err(|e| format!("hidden label row: {e}"))
                })
                .collect()
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query("SELECT label_id FROM repo_hidden_labels WHERE repo_id = ?1")
                .bind(repo_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list hidden labels failed: {e}"))?;
            rows.iter()
                .map(|r| {
                    r.try_get::<String, _>("label_id")
                        .map_err(|e| format!("hidden label row: {e}"))
                })
                .collect()
        }
    }
}

pub async fn set_repo_label_hidden(
    pool: &DbPool,
    repo_id: &str,
    label_id: &str,
    hidden: bool,
) -> Result<(), String> {
    if hidden {
        match pool {
            DbPool::Postgres(p) => {
                sqlx::query(
                    "INSERT INTO repo_hidden_labels (repo_id, label_id) VALUES ($1, $2)
ON CONFLICT DO NOTHING",
                )
                .bind(repo_id)
                .bind(label_id)
                .execute(p)
                .await
                .map_err(|e| format!("hide label failed: {e}"))?;
            }
            DbPool::MySql(p) => {
                sqlx::query(
                    "INSERT IGNORE INTO repo_hidden_labels (repo_id, label_id) VALUES (?, ?)",
                )
                .bind(repo_id)
                .bind(label_id)
                .execute(p)
                .await
                .map_err(|e| format!("hide label failed: {e}"))?;
            }
            DbPool::Sqlite(p) => {
                sqlx::query(
                    "INSERT OR IGNORE INTO repo_hidden_labels (repo_id, label_id) VALUES (?1, ?2)",
                )
                .bind(repo_id)
                .bind(label_id)
                .execute(p)
                .await
                .map_err(|e| format!("hide label failed: {e}"))?;
            }
        }
    } else {
        match pool {
            DbPool::Postgres(p) => {
                sqlx::query("DELETE FROM repo_hidden_labels WHERE repo_id = $1 AND label_id = $2")
                    .bind(repo_id)
                    .bind(label_id)
                    .execute(p)
                    .await
                    .map_err(|e| format!("unhide label failed: {e}"))?;
            }
            DbPool::MySql(p) => {
                sqlx::query("DELETE FROM repo_hidden_labels WHERE repo_id = ? AND label_id = ?")
                    .bind(repo_id)
                    .bind(label_id)
                    .execute(p)
                    .await
                    .map_err(|e| format!("unhide label failed: {e}"))?;
            }
            DbPool::Sqlite(p) => {
                sqlx::query("DELETE FROM repo_hidden_labels WHERE repo_id = ?1 AND label_id = ?2")
                    .bind(repo_id)
                    .bind(label_id)
                    .execute(p)
                    .await
                    .map_err(|e| format!("unhide label failed: {e}"))?;
            }
        }
    }
    Ok(())
}

pub async fn list_labels_for_issue(pool: &DbPool, issue_id: &str) -> Result<Vec<LabelRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{LABEL_SELECT_PG}
INNER JOIN issue_labels il ON il.label_id = labels.id
WHERE il.issue_id = $1
ORDER BY lower(labels.name)"
            )))
            .bind(issue_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list issue labels failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_label!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{LABEL_SELECT_MYSQL}
INNER JOIN issue_labels il ON il.label_id = labels.id
WHERE il.issue_id = ?
ORDER BY LOWER(labels.name)"
            )))
            .bind(issue_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list issue labels failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_label!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{LABEL_SELECT_SQLITE}
INNER JOIN issue_labels il ON il.label_id = labels.id
WHERE il.issue_id = ?1
ORDER BY lower(labels.name)"
            )))
            .bind(issue_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list issue labels failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_label!(r));
            }
            Ok(out)
        }
    }
}

/// Batch `list_labels_for_issue` — one `IN (...)` round trip; `(issue_id, label)` pairs.
pub async fn list_labels_for_issues(
    pool: &DbPool,
    issue_ids: &[String],
) -> Result<Vec<(String, LabelRow)>, String> {
    if issue_ids.is_empty() {
        return Ok(Vec::new());
    }
    macro_rules! map_pair {
        ($row:expr) => {{
            let row = $row;
            let issue_id: String = row
                .try_get("issue_id")
                .map_err(|e| format!("issue label row: {e}"))?;
            Ok((issue_id, map_label!(row)))
        }};
    }
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT il.issue_id, labels.*
FROM ({LABEL_SELECT_PG}) labels
INNER JOIN issue_labels il ON il.label_id = labels.id
WHERE il.issue_id = ANY($1)
ORDER BY lower(labels.name)"
            )))
            .bind(issue_ids)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list issue labels failed: {e}"))?;
            rows.iter().map(|r| map_pair!(r)).collect()
        }
        DbPool::MySql(p) => {
            let in_list =
                crate::dialect::in_placeholders(crate::dialect::Dialect::MySql, 1, issue_ids.len());
            let q_str = format!(
                "SELECT il.issue_id, labels.*
FROM ({LABEL_SELECT_MYSQL}) labels
INNER JOIN issue_labels il ON il.label_id = labels.id
WHERE il.issue_id IN ({in_list})
ORDER BY LOWER(labels.name)"
            );
            let q = sqlx::query(sqlx::AssertSqlSafe(&*q_str));
            let q = issue_ids.iter().fold(q, |q, id| q.bind(id));
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("list issue labels failed: {e}"))?;
            rows.iter().map(|r| map_pair!(r)).collect()
        }
        DbPool::Sqlite(p) => {
            let in_list = crate::dialect::in_placeholders(
                crate::dialect::Dialect::Sqlite,
                1,
                issue_ids.len(),
            );
            let q_str = format!(
                "SELECT il.issue_id, labels.*
FROM ({LABEL_SELECT_SQLITE}) labels
INNER JOIN issue_labels il ON il.label_id = labels.id
WHERE il.issue_id IN ({in_list})
ORDER BY lower(labels.name)"
            );
            let q = sqlx::query(sqlx::AssertSqlSafe(&*q_str));
            let q = issue_ids.iter().fold(q, |q, id| q.bind(id));
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("list issue labels failed: {e}"))?;
            rows.iter().map(|r| map_pair!(r)).collect()
        }
    }
}

/// Replace the label set on an issue (M:N via `issue_labels`).
pub async fn set_issue_labels(
    pool: &DbPool,
    issue_id: &str,
    label_ids: &[String],
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin set issue labels tx failed: {e}"))?;
            sqlx::query("DELETE FROM issue_labels WHERE issue_id = $1")
                .bind(issue_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("clear issue labels failed: {e}"))?;
            for label_id in label_ids {
                sqlx::query("INSERT INTO issue_labels (issue_id, label_id) VALUES ($1, $2)")
                    .bind(issue_id)
                    .bind(label_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| format!("insert issue label failed: {e}"))?;
            }
            tx.commit()
                .await
                .map_err(|e| format!("commit set issue labels tx failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin set issue labels tx failed: {e}"))?;
            sqlx::query("DELETE FROM issue_labels WHERE issue_id = ?")
                .bind(issue_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("clear issue labels failed: {e}"))?;
            for label_id in label_ids {
                sqlx::query("INSERT INTO issue_labels (issue_id, label_id) VALUES (?, ?)")
                    .bind(issue_id)
                    .bind(label_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| format!("insert issue label failed: {e}"))?;
            }
            tx.commit()
                .await
                .map_err(|e| format!("commit set issue labels tx failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin set issue labels tx failed: {e}"))?;
            sqlx::query("DELETE FROM issue_labels WHERE issue_id = ?1")
                .bind(issue_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("clear issue labels failed: {e}"))?;
            for label_id in label_ids {
                sqlx::query("INSERT INTO issue_labels (issue_id, label_id) VALUES (?1, ?2)")
                    .bind(issue_id)
                    .bind(label_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| format!("insert issue label failed: {e}"))?;
            }
            tx.commit()
                .await
                .map_err(|e| format!("commit set issue labels tx failed: {e}"))?;
        }
    }
    Ok(())
}

/// Assignee row joined to users for issue detail / candidates.
#[derive(Debug, Clone)]
pub struct IssueAssigneeRow {
    pub user_id: String,
    pub username: String,
    pub display_name: String,
}

/// List assignees for an issue (username ascending).
pub async fn list_issue_assignees(
    pool: &DbPool,
    issue_id: &str,
) -> Result<Vec<IssueAssigneeRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT a.user_id, u.username, u.display_name
FROM issue_assignees a
JOIN users u ON u.id = a.user_id
WHERE a.issue_id = $1
ORDER BY lower(u.username)",
            )
            .bind(issue_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list issue assignees failed: {e}"))?;
            rows.into_iter()
                .map(|row| {
                    Ok(IssueAssigneeRow {
                        user_id: row
                            .try_get("user_id")
                            .map_err(|e| format!("assignee row: {e}"))?,
                        username: row
                            .try_get("username")
                            .map_err(|e| format!("assignee row: {e}"))?,
                        display_name: row
                            .try_get("display_name")
                            .map_err(|e| format!("assignee row: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT a.user_id, u.username, u.display_name
FROM issue_assignees a
JOIN users u ON u.id = a.user_id
WHERE a.issue_id = ?
ORDER BY LOWER(u.username)",
            )
            .bind(issue_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list issue assignees failed: {e}"))?;
            rows.into_iter()
                .map(|row| {
                    Ok(IssueAssigneeRow {
                        user_id: row
                            .try_get("user_id")
                            .map_err(|e| format!("assignee row: {e}"))?,
                        username: row
                            .try_get("username")
                            .map_err(|e| format!("assignee row: {e}"))?,
                        display_name: row
                            .try_get("display_name")
                            .map_err(|e| format!("assignee row: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT a.user_id, u.username, u.display_name
FROM issue_assignees a
JOIN users u ON u.id = a.user_id
WHERE a.issue_id = ?1
ORDER BY lower(u.username)",
            )
            .bind(issue_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list issue assignees failed: {e}"))?;
            rows.into_iter()
                .map(|row| {
                    Ok(IssueAssigneeRow {
                        user_id: row
                            .try_get("user_id")
                            .map_err(|e| format!("assignee row: {e}"))?,
                        username: row
                            .try_get("username")
                            .map_err(|e| format!("assignee row: {e}"))?,
                        display_name: row
                            .try_get("display_name")
                            .map_err(|e| format!("assignee row: {e}"))?,
                    })
                })
                .collect()
        }
    }
}

/// Batch `list_issue_assignees` — one `IN (...)` round trip; `(issue_id, assignee)` pairs.
pub async fn list_assignees_for_issues(
    pool: &DbPool,
    issue_ids: &[String],
) -> Result<Vec<(String, IssueAssigneeRow)>, String> {
    if issue_ids.is_empty() {
        return Ok(Vec::new());
    }
    macro_rules! map_pair {
        ($rows:expr) => {
            $rows
                .into_iter()
                .map(|row| {
                    Ok((
                        row.try_get("issue_id")
                            .map_err(|e| format!("assignee row: {e}"))?,
                        IssueAssigneeRow {
                            user_id: row
                                .try_get("user_id")
                                .map_err(|e| format!("assignee row: {e}"))?,
                            username: row
                                .try_get("username")
                                .map_err(|e| format!("assignee row: {e}"))?,
                            display_name: row
                                .try_get("display_name")
                                .map_err(|e| format!("assignee row: {e}"))?,
                        },
                    ))
                })
                .collect::<Result<Vec<_>, String>>()
        };
    }
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT a.issue_id, a.user_id, u.username, u.display_name
FROM issue_assignees a
JOIN users u ON u.id = a.user_id
WHERE a.issue_id = ANY($1)
ORDER BY lower(u.username)",
            )
            .bind(issue_ids)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list issue assignees failed: {e}"))?;
            map_pair!(rows)
        }
        DbPool::MySql(p) => {
            let in_list =
                crate::dialect::in_placeholders(crate::dialect::Dialect::MySql, 1, issue_ids.len());
            let q_str = format!(
                "SELECT a.issue_id, a.user_id, u.username, u.display_name
FROM issue_assignees a
JOIN users u ON u.id = a.user_id
WHERE a.issue_id IN ({in_list})
ORDER BY LOWER(u.username)"
            );
            let q = sqlx::query(sqlx::AssertSqlSafe(&*q_str));
            let q = issue_ids.iter().fold(q, |q, id| q.bind(id));
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("list issue assignees failed: {e}"))?;
            map_pair!(rows)
        }
        DbPool::Sqlite(p) => {
            let in_list = crate::dialect::in_placeholders(
                crate::dialect::Dialect::Sqlite,
                1,
                issue_ids.len(),
            );
            let q_str = format!(
                "SELECT a.issue_id, a.user_id, u.username, u.display_name
FROM issue_assignees a
JOIN users u ON u.id = a.user_id
WHERE a.issue_id IN ({in_list})
ORDER BY lower(u.username)"
            );
            let q = sqlx::query(sqlx::AssertSqlSafe(&*q_str));
            let q = issue_ids.iter().fold(q, |q, id| q.bind(id));
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("list issue assignees failed: {e}"))?;
            map_pair!(rows)
        }
    }
}

/// Replace assignees on an issue (M:N via `issue_assignees`).
pub async fn set_issue_assignees(
    pool: &DbPool,
    issue_id: &str,
    user_ids: &[String],
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin set issue assignees tx failed: {e}"))?;
            sqlx::query("DELETE FROM issue_assignees WHERE issue_id = $1")
                .bind(issue_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("clear issue assignees failed: {e}"))?;
            for user_id in user_ids {
                sqlx::query("INSERT INTO issue_assignees (issue_id, user_id) VALUES ($1, $2)")
                    .bind(issue_id)
                    .bind(user_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| format!("insert issue assignee failed: {e}"))?;
            }
            tx.commit()
                .await
                .map_err(|e| format!("commit set issue assignees tx failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin set issue assignees tx failed: {e}"))?;
            sqlx::query("DELETE FROM issue_assignees WHERE issue_id = ?")
                .bind(issue_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("clear issue assignees failed: {e}"))?;
            for user_id in user_ids {
                sqlx::query("INSERT INTO issue_assignees (issue_id, user_id) VALUES (?, ?)")
                    .bind(issue_id)
                    .bind(user_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| format!("insert issue assignee failed: {e}"))?;
            }
            tx.commit()
                .await
                .map_err(|e| format!("commit set issue assignees tx failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin set issue assignees tx failed: {e}"))?;
            sqlx::query("DELETE FROM issue_assignees WHERE issue_id = ?1")
                .bind(issue_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("clear issue assignees failed: {e}"))?;
            for user_id in user_ids {
                sqlx::query("INSERT INTO issue_assignees (issue_id, user_id) VALUES (?1, ?2)")
                    .bind(issue_id)
                    .bind(user_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| format!("insert issue assignee failed: {e}"))?;
            }
            tx.commit()
                .await
                .map_err(|e| format!("commit set issue assignees tx failed: {e}"))?;
        }
    }
    Ok(())
}
