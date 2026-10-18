//! Repo-scoped discussion forums — categories, discussions, threaded
//! comments, and label joins (GitHub Discussions shape). Discussion numbers
//! come from the shared `issue_counters` sequence (D-PR-02); the API layer
//! allocates before insert. Category positions are contiguous 0-based per
//! repo. Comments thread one level deep via `parent_id` — a reply points at
//! a top-level comment (code enforces the depth cap).

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct DiscussionCategoryRow {
    pub id: String,
    pub repo_id: String,
    pub name: String,
    pub slug: String,
    pub emoji: String,
    pub description: String,
    pub is_answerable: bool,
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct DiscussionRow {
    pub id: String,
    pub repo_id: String,
    pub number: i64,
    pub title: String,
    pub body: String,
    pub category_id: Option<String>,
    pub author_id: Option<String>,
    pub locked: bool,
    pub pinned: bool,
    pub answer_comment_id: Option<String>,
    pub answered_at: Option<String>,
    pub answered_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct DiscussionCommentRow {
    pub id: String,
    pub discussion_id: String,
    pub parent_id: Option<String>,
    pub author_id: Option<String>,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
}

macro_rules! map_category {
    ($row:expr) => {{
        let row = $row;
        DiscussionCategoryRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("category row: {e}"))?,
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("category row: {e}"))?,
            name: row
                .try_get("name")
                .map_err(|e| format!("category row: {e}"))?,
            slug: row
                .try_get("slug")
                .map_err(|e| format!("category row: {e}"))?,
            emoji: row
                .try_get("emoji")
                .map_err(|e| format!("category row: {e}"))?,
            description: row
                .try_get("description")
                .map_err(|e| format!("category row: {e}"))?,
            is_answerable: row
                .try_get::<bool, _>("is_answerable")
                .or_else(|_| row.try_get::<i64, _>("is_answerable").map(|v| v != 0))
                .or_else(|_| row.try_get::<i8, _>("is_answerable").map(|v| v != 0))
                .map_err(|e| format!("category row: {e}"))?,
            position: row
                .try_get::<i64, _>("position")
                .or_else(|_| row.try_get::<i32, _>("position").map(i64::from))
                .map_err(|e| format!("category row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("category row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("category row: {e}"))?,
        }
    }};
}

macro_rules! map_discussion {
    ($row:expr) => {{
        let row = $row;
        DiscussionRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("discussion row: {e}"))?,
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("discussion row: {e}"))?,
            number: row
                .try_get::<i64, _>("number")
                .map_err(|e| format!("discussion row: {e}"))?,
            title: row
                .try_get("title")
                .map_err(|e| format!("discussion row: {e}"))?,
            body: row
                .try_get("body")
                .map_err(|e| format!("discussion row: {e}"))?,
            category_id: row
                .try_get("category_id")
                .map_err(|e| format!("discussion row: {e}"))?,
            author_id: row
                .try_get("author_id")
                .map_err(|e| format!("discussion row: {e}"))?,
            locked: row
                .try_get::<bool, _>("locked")
                .or_else(|_| row.try_get::<i64, _>("locked").map(|v| v != 0))
                .or_else(|_| row.try_get::<i8, _>("locked").map(|v| v != 0))
                .map_err(|e| format!("discussion row: {e}"))?,
            pinned: row
                .try_get::<bool, _>("pinned")
                .or_else(|_| row.try_get::<i64, _>("pinned").map(|v| v != 0))
                .or_else(|_| row.try_get::<i8, _>("pinned").map(|v| v != 0))
                .map_err(|e| format!("discussion row: {e}"))?,
            answer_comment_id: row
                .try_get("answer_comment_id")
                .map_err(|e| format!("discussion row: {e}"))?,
            answered_at: row
                .try_get("answered_at")
                .map_err(|e| format!("discussion row: {e}"))?,
            answered_by: row
                .try_get("answered_by")
                .map_err(|e| format!("discussion row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("discussion row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("discussion row: {e}"))?,
        }
    }};
}

macro_rules! map_comment {
    ($row:expr) => {{
        let row = $row;
        DiscussionCommentRow {
            id: row.try_get("id").map_err(|e| format!("comment row: {e}"))?,
            discussion_id: row
                .try_get("discussion_id")
                .map_err(|e| format!("comment row: {e}"))?,
            parent_id: row
                .try_get("parent_id")
                .map_err(|e| format!("comment row: {e}"))?,
            author_id: row
                .try_get("author_id")
                .map_err(|e| format!("comment row: {e}"))?,
            body: row
                .try_get("body")
                .map_err(|e| format!("comment row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("comment row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("comment row: {e}"))?,
        }
    }};
}

const CATEGORY_SELECT_PG: &str = "SELECT c.id, c.repo_id, c.name, c.slug, c.emoji, c.description,
       c.is_answerable, c.position,
       to_char(c.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(c.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at
FROM discussion_categories c";

const CATEGORY_SELECT_MYSQL: &str =
    "SELECT c.id, c.repo_id, c.name, c.slug, c.emoji, c.description,
       c.is_answerable, c.position,
       DATE_FORMAT(c.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       DATE_FORMAT(c.updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
FROM discussion_categories c";

const CATEGORY_SELECT_SQLITE: &str =
    "SELECT c.id, c.repo_id, c.name, c.slug, c.emoji, c.description,
       c.is_answerable, c.position,
       strftime('%Y-%m-%dT%H:%M:%SZ', c.created_at) AS created_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', c.updated_at) AS updated_at
FROM discussion_categories c";

const DISCUSSION_SELECT_PG: &str = "SELECT d.id, d.repo_id, d.number, d.title, d.body,
       d.category_id, d.author_id, d.locked, d.pinned,
       d.answer_comment_id,
       CASE WHEN d.answered_at IS NULL THEN NULL
            ELSE to_char(d.answered_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') END AS answered_at,
       d.answered_by,
       to_char(d.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(d.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at
FROM discussions d";

const DISCUSSION_SELECT_MYSQL: &str = "SELECT d.id, d.repo_id, d.number, d.title, d.body,
       d.category_id, d.author_id, d.locked, d.pinned,
       d.answer_comment_id,
       DATE_FORMAT(d.answered_at, '%Y-%m-%dT%H:%i:%sZ') AS answered_at,
       d.answered_by,
       DATE_FORMAT(d.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       DATE_FORMAT(d.updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
FROM discussions d";

const DISCUSSION_SELECT_SQLITE: &str = "SELECT d.id, d.repo_id, d.number, d.title, d.body,
       d.category_id, d.author_id, d.locked, d.pinned,
       d.answer_comment_id,
       CASE WHEN d.answered_at IS NULL THEN NULL
            ELSE strftime('%Y-%m-%dT%H:%M:%SZ', d.answered_at) END AS answered_at,
       d.answered_by,
       strftime('%Y-%m-%dT%H:%M:%SZ', d.created_at) AS created_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', d.updated_at) AS updated_at
FROM discussions d";

const COMMENT_SELECT_PG: &str =
    "SELECT cm.id, cm.discussion_id, cm.parent_id, cm.author_id, cm.body,
       to_char(cm.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(cm.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at
FROM discussion_comments cm";

const COMMENT_SELECT_MYSQL: &str =
    "SELECT cm.id, cm.discussion_id, cm.parent_id, cm.author_id, cm.body,
       DATE_FORMAT(cm.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       DATE_FORMAT(cm.updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
FROM discussion_comments cm";

const COMMENT_SELECT_SQLITE: &str =
    "SELECT cm.id, cm.discussion_id, cm.parent_id, cm.author_id, cm.body,
       strftime('%Y-%m-%dT%H:%M:%SZ', cm.created_at) AS created_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', cm.updated_at) AS updated_at
FROM discussion_comments cm";

fn now_expr(pool: &DbPool) -> &'static str {
    match pool {
        DbPool::Sqlite(_) => "strftime('%Y-%m-%d %H:%M:%S','now')",
        _ => "CURRENT_TIMESTAMP",
    }
}

// ---------------------------------------------------------------------------
// Categories
// ---------------------------------------------------------------------------

/// Insert a category, appended at the next contiguous `position`.
#[allow(clippy::too_many_arguments)]
pub async fn insert_category(
    pool: &DbPool,
    id: &str,
    repo_id: &str,
    name: &str,
    slug: &str,
    emoji: &str,
    description: &str,
    is_answerable: bool,
) -> Result<DiscussionCategoryRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO discussion_categories
                     (id, repo_id, name, slug, emoji, description, is_answerable, position)
                 VALUES ($1, $2, $3, $4, $5, $6, $7,
                     (SELECT COALESCE(MAX(position) + 1, 0) FROM discussion_categories WHERE repo_id = $2))",
            )
            .bind(id)
            .bind(repo_id)
            .bind(name)
            .bind(slug)
            .bind(emoji)
            .bind(description)
            .bind(is_answerable)
            .execute(p)
            .await
            .map_err(|e| format!("insert discussion category failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO discussion_categories
                     (id, repo_id, name, slug, emoji, description, is_answerable, position)
                 SELECT ?, ?, ?, ?, ?, ?, ?, COALESCE(MAX(position) + 1, 0)
                 FROM discussion_categories WHERE repo_id = ?",
            )
            .bind(id)
            .bind(repo_id)
            .bind(name)
            .bind(slug)
            .bind(emoji)
            .bind(description)
            .bind(is_answerable)
            .bind(repo_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert discussion category failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO discussion_categories
                     (id, repo_id, name, slug, emoji, description, is_answerable, position)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7,
                     (SELECT COALESCE(MAX(position) + 1, 0) FROM discussion_categories WHERE repo_id = ?2))",
            )
            .bind(id)
            .bind(repo_id)
            .bind(name)
            .bind(slug)
            .bind(emoji)
            .bind(description)
            .bind(is_answerable)
            .execute(p)
            .await
            .map_err(|e| format!("insert discussion category failed: {e}"))?;
        }
    }
    find_category_by_id(pool, id)
        .await?
        .ok_or_else(|| "insert discussion category: row missing after insert".to_string())
}

pub async fn find_category_by_id(
    pool: &DbPool,
    id: &str,
) -> Result<Option<DiscussionCategoryRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{CATEGORY_SELECT_PG} WHERE c.id = $1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find discussion category failed: {e}"))?
            {
                Some(r) => Ok(Some(map_category!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{CATEGORY_SELECT_MYSQL} WHERE c.id = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find discussion category failed: {e}"))?
            {
                Some(r) => Ok(Some(map_category!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{CATEGORY_SELECT_SQLITE} WHERE c.id = ?1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find discussion category failed: {e}"))?
            {
                Some(r) => Ok(Some(map_category!(&r))),
                None => Ok(None),
            }
        }
    }
}

pub async fn find_category_by_slug(
    pool: &DbPool,
    repo_id: &str,
    slug: &str,
) -> Result<Option<DiscussionCategoryRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{CATEGORY_SELECT_PG} WHERE c.repo_id = $1 AND c.slug = $2");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(slug)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find discussion category failed: {e}"))?
            {
                Some(r) => Ok(Some(map_category!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{CATEGORY_SELECT_MYSQL} WHERE c.repo_id = ? AND c.slug = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(slug)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find discussion category failed: {e}"))?
            {
                Some(r) => Ok(Some(map_category!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{CATEGORY_SELECT_SQLITE} WHERE c.repo_id = ?1 AND c.slug = ?2");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(slug)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find discussion category failed: {e}"))?
            {
                Some(r) => Ok(Some(map_category!(&r))),
                None => Ok(None),
            }
        }
    }
}

pub async fn list_categories(
    pool: &DbPool,
    repo_id: &str,
) -> Result<Vec<DiscussionCategoryRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{CATEGORY_SELECT_PG} WHERE c.repo_id = $1 ORDER BY c.position ASC");
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list discussion categories failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_category!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let sql =
                format!("{CATEGORY_SELECT_MYSQL} WHERE c.repo_id = ? ORDER BY c.position ASC");
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list discussion categories failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_category!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let sql =
                format!("{CATEGORY_SELECT_SQLITE} WHERE c.repo_id = ?1 ORDER BY c.position ASC");
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list discussion categories failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_category!(r));
            }
            Ok(out)
        }
    }
}

pub async fn update_category(
    pool: &DbPool,
    id: &str,
    name: Option<&str>,
    emoji: Option<&str>,
    description: Option<&str>,
    is_answerable: Option<bool>,
) -> Result<Option<DiscussionCategoryRow>, String> {
    let now = now_expr(pool);
    macro_rules! build {
        ($ty:ty) => {{
            let mut qb = sqlx::QueryBuilder::<$ty>::new("UPDATE discussion_categories SET ");
            {
                let mut sep = qb.separated(", ");
                if let Some(n) = name {
                    sep.push("name = ").push_bind_unseparated(n);
                }
                if let Some(e) = emoji {
                    sep.push("emoji = ").push_bind_unseparated(e);
                }
                if let Some(d) = description {
                    sep.push("description = ").push_bind_unseparated(d);
                }
                if let Some(a) = is_answerable {
                    sep.push("is_answerable = ").push_bind_unseparated(a);
                }
                sep.push(format!("updated_at = {now}"));
            }
            qb.push(" WHERE id = ").push_bind(id);
            qb
        }};
    }
    match pool {
        DbPool::Postgres(p) => {
            build!(sqlx::Postgres)
                .build()
                .execute(p)
                .await
                .map_err(|e| format!("update discussion category failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            build!(sqlx::MySql)
                .build()
                .execute(p)
                .await
                .map_err(|e| format!("update discussion category failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            build!(sqlx::Sqlite)
                .build()
                .execute(p)
                .await
                .map_err(|e| format!("update discussion category failed: {e}"))?;
        }
    }
    find_category_by_id(pool, id).await
}

/// Hard-delete — discussions keep their rows (`category_id` is
/// `ON DELETE SET NULL`), matching GitHub where deleting a category leaves
/// its discussions uncategorized.
pub async fn delete_category(pool: &DbPool, id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM discussion_categories WHERE id = $1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete discussion category failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM discussion_categories WHERE id = ?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete discussion category failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM discussion_categories WHERE id = ?1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete discussion category failed: {e}"))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Discussions
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub async fn insert_discussion(
    pool: &DbPool,
    id: &str,
    repo_id: &str,
    number: i64,
    title: &str,
    body: &str,
    category_id: Option<&str>,
    author_id: Option<&str>,
) -> Result<DiscussionRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO discussions
                     (id, repo_id, number, title, body, category_id, author_id)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(number)
            .bind(title)
            .bind(body)
            .bind(category_id)
            .bind(author_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert discussion failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO discussions
                     (id, repo_id, number, title, body, category_id, author_id)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(number)
            .bind(title)
            .bind(body)
            .bind(category_id)
            .bind(author_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert discussion failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO discussions
                     (id, repo_id, number, title, body, category_id, author_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(number)
            .bind(title)
            .bind(body)
            .bind(category_id)
            .bind(author_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert discussion failed: {e}"))?;
        }
    }
    find_discussion_by_id(pool, id)
        .await?
        .ok_or_else(|| "insert discussion: row missing after insert".to_string())
}

pub async fn find_discussion_by_id(
    pool: &DbPool,
    id: &str,
) -> Result<Option<DiscussionRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{DISCUSSION_SELECT_PG} WHERE d.id = $1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find discussion failed: {e}"))?
            {
                Some(r) => Ok(Some(map_discussion!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{DISCUSSION_SELECT_MYSQL} WHERE d.id = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find discussion failed: {e}"))?
            {
                Some(r) => Ok(Some(map_discussion!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{DISCUSSION_SELECT_SQLITE} WHERE d.id = ?1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find discussion failed: {e}"))?
            {
                Some(r) => Ok(Some(map_discussion!(&r))),
                None => Ok(None),
            }
        }
    }
}

pub async fn find_discussion_by_number(
    pool: &DbPool,
    repo_id: &str,
    number: i64,
) -> Result<Option<DiscussionRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{DISCUSSION_SELECT_PG} WHERE d.repo_id = $1 AND d.number = $2");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(number)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find discussion failed: {e}"))?
            {
                Some(r) => Ok(Some(map_discussion!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{DISCUSSION_SELECT_MYSQL} WHERE d.repo_id = ? AND d.number = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(number)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find discussion failed: {e}"))?
            {
                Some(r) => Ok(Some(map_discussion!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{DISCUSSION_SELECT_SQLITE} WHERE d.repo_id = ?1 AND d.number = ?2");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(number)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find discussion failed: {e}"))?
            {
                Some(r) => Ok(Some(map_discussion!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// List discussions — pinned first, then most-recently-updated (GitHub's
/// default ordering). Optional `category_id` / `answered` filters.
pub async fn list_discussions(
    pool: &DbPool,
    repo_id: &str,
    category_id: Option<&str>,
    answered: Option<bool>,
) -> Result<Vec<DiscussionRow>, String> {
    macro_rules! build {
        ($ty:ty, $select:expr) => {{
            let mut qb = sqlx::QueryBuilder::<$ty>::new($select);
            qb.push(" WHERE d.repo_id = ").push_bind(repo_id);
            if let Some(c) = category_id {
                qb.push(" AND d.category_id = ").push_bind(c);
            }
            if let Some(a) = answered {
                if a {
                    qb.push(" AND d.answer_comment_id IS NOT NULL");
                } else {
                    qb.push(" AND d.answer_comment_id IS NULL");
                }
            }
            qb.push(" ORDER BY d.pinned DESC, d.updated_at DESC");
            qb
        }};
    }
    match pool {
        DbPool::Postgres(p) => {
            let rows = build!(sqlx::Postgres, DISCUSSION_SELECT_PG)
                .build()
                .fetch_all(p)
                .await
                .map_err(|e| format!("list discussions failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_discussion!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let rows = build!(sqlx::MySql, DISCUSSION_SELECT_MYSQL)
                .build()
                .fetch_all(p)
                .await
                .map_err(|e| format!("list discussions failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_discussion!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let rows = build!(sqlx::Sqlite, DISCUSSION_SELECT_SQLITE)
                .build()
                .fetch_all(p)
                .await
                .map_err(|e| format!("list discussions failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_discussion!(r));
            }
            Ok(out)
        }
    }
}

/// Title/body/category edits. `category_id` is a double-Option: `Some(None)`
/// clears the category (GitHub allows uncategorized discussions).
pub async fn update_discussion(
    pool: &DbPool,
    id: &str,
    title: Option<&str>,
    body: Option<&str>,
    category_id: Option<Option<&str>>,
) -> Result<Option<DiscussionRow>, String> {
    let now = now_expr(pool);
    macro_rules! build {
        ($ty:ty) => {{
            let mut qb = sqlx::QueryBuilder::<$ty>::new("UPDATE discussions SET ");
            {
                let mut sep = qb.separated(", ");
                if let Some(t) = title {
                    sep.push("title = ").push_bind_unseparated(t);
                }
                if let Some(b) = body {
                    sep.push("body = ").push_bind_unseparated(b);
                }
                if let Some(c) = category_id {
                    match c {
                        Some(cid) => {
                            sep.push("category_id = ").push_bind_unseparated(cid);
                        }
                        None => {
                            sep.push("category_id = NULL");
                        }
                    }
                }
                sep.push(format!("updated_at = {now}"));
            }
            qb.push(" WHERE id = ").push_bind(id);
            qb
        }};
    }
    match pool {
        DbPool::Postgres(p) => {
            build!(sqlx::Postgres)
                .build()
                .execute(p)
                .await
                .map_err(|e| format!("update discussion failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            build!(sqlx::MySql)
                .build()
                .execute(p)
                .await
                .map_err(|e| format!("update discussion failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            build!(sqlx::Sqlite)
                .build()
                .execute(p)
                .await
                .map_err(|e| format!("update discussion failed: {e}"))?;
        }
    }
    find_discussion_by_id(pool, id).await
}

/// Pin/lock flag updates — separate from `update_discussion` so the API can
/// emit `pinned`/`locked` activity types instead of `edited`.
pub async fn set_discussion_flags(
    pool: &DbPool,
    id: &str,
    pinned: Option<bool>,
    locked: Option<bool>,
) -> Result<Option<DiscussionRow>, String> {
    let now = now_expr(pool);
    macro_rules! build {
        ($ty:ty) => {{
            let mut qb = sqlx::QueryBuilder::<$ty>::new("UPDATE discussions SET ");
            {
                let mut sep = qb.separated(", ");
                if let Some(pn) = pinned {
                    sep.push("pinned = ").push_bind_unseparated(pn);
                }
                if let Some(lk) = locked {
                    sep.push("locked = ").push_bind_unseparated(lk);
                }
                sep.push(format!("updated_at = {now}"));
            }
            qb.push(" WHERE id = ").push_bind(id);
            qb
        }};
    }
    match pool {
        DbPool::Postgres(p) => {
            build!(sqlx::Postgres)
                .build()
                .execute(p)
                .await
                .map_err(|e| format!("set discussion flags failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            build!(sqlx::MySql)
                .build()
                .execute(p)
                .await
                .map_err(|e| format!("set discussion flags failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            build!(sqlx::Sqlite)
                .build()
                .execute(p)
                .await
                .map_err(|e| format!("set discussion flags failed: {e}"))?;
        }
    }
    find_discussion_by_id(pool, id).await
}

/// Mark/clear the accepted answer. `comment` is `Some(id)` to mark, `None`
/// to clear; `actor_id` stamps `answered_by`.
pub async fn set_discussion_answer(
    pool: &DbPool,
    id: &str,
    comment_id: Option<&str>,
    actor_id: Option<&str>,
) -> Result<Option<DiscussionRow>, String> {
    let now = now_expr(pool);
    let (answer_expr, answered_by) = match comment_id {
        Some(_) => (now, actor_id),
        None => ("NULL", None),
    };
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "UPDATE discussions SET answer_comment_id = $1, answered_at = {answer_expr},
                 answered_by = $2, updated_at = {now} WHERE id = $3"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(comment_id)
                .bind(answered_by)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("set discussion answer failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "UPDATE discussions SET answer_comment_id = ?, answered_at = {answer_expr},
                 answered_by = ?, updated_at = {now} WHERE id = ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(comment_id)
                .bind(answered_by)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("set discussion answer failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "UPDATE discussions SET answer_comment_id = ?1, answered_at = {answer_expr},
                 answered_by = ?2, updated_at = {now} WHERE id = ?3"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(comment_id)
                .bind(answered_by)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("set discussion answer failed: {e}"))?;
        }
    }
    find_discussion_by_id(pool, id).await
}

/// Transfer — re-homes the row under `target_repo_id` with a freshly
/// allocated `target_number` (the destination's next issue-counter value).
/// The original (repo, number) pair is released; GitHub leaves a redirect
/// behind — we don't retain one (v1).
pub async fn transfer_discussion(
    pool: &DbPool,
    id: &str,
    target_repo_id: &str,
    target_number: i64,
) -> Result<Option<DiscussionRow>, String> {
    let now = now_expr(pool);
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "UPDATE discussions SET repo_id = $1, number = $2, category_id = NULL,
                 answer_comment_id = NULL, answered_at = NULL, answered_by = NULL,
                 updated_at = {now} WHERE id = $3"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(target_repo_id)
                .bind(target_number)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("transfer discussion failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "UPDATE discussions SET repo_id = ?, number = ?, category_id = NULL,
                 answer_comment_id = NULL, answered_at = NULL, answered_by = NULL,
                 updated_at = {now} WHERE id = ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(target_repo_id)
                .bind(target_number)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("transfer discussion failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "UPDATE discussions SET repo_id = ?1, number = ?2, category_id = NULL,
                 answer_comment_id = NULL, answered_at = NULL, answered_by = NULL,
                 updated_at = {now} WHERE id = ?3"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(target_repo_id)
                .bind(target_number)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("transfer discussion failed: {e}"))?;
        }
    }
    find_discussion_by_id(pool, id).await
}

/// Hard-delete — comments and label joins cascade. Numbers are not reclaimed.
pub async fn delete_discussion(pool: &DbPool, id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM discussions WHERE id = $1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete discussion failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM discussions WHERE id = ?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete discussion failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM discussions WHERE id = ?1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete discussion failed: {e}"))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Comments
// ---------------------------------------------------------------------------

pub async fn insert_comment(
    pool: &DbPool,
    id: &str,
    discussion_id: &str,
    parent_id: Option<&str>,
    author_id: Option<&str>,
    body: &str,
) -> Result<DiscussionCommentRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO discussion_comments (id, discussion_id, parent_id, author_id, body)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(id)
            .bind(discussion_id)
            .bind(parent_id)
            .bind(author_id)
            .bind(body)
            .execute(p)
            .await
            .map_err(|e| format!("insert discussion comment failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO discussion_comments (id, discussion_id, parent_id, author_id, body)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(discussion_id)
            .bind(parent_id)
            .bind(author_id)
            .bind(body)
            .execute(p)
            .await
            .map_err(|e| format!("insert discussion comment failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO discussion_comments (id, discussion_id, parent_id, author_id, body)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(id)
            .bind(discussion_id)
            .bind(parent_id)
            .bind(author_id)
            .bind(body)
            .execute(p)
            .await
            .map_err(|e| format!("insert discussion comment failed: {e}"))?;
        }
    }
    find_comment_by_id(pool, id)
        .await?
        .ok_or_else(|| "insert discussion comment: row missing after insert".to_string())
}

pub async fn find_comment_by_id(
    pool: &DbPool,
    id: &str,
) -> Result<Option<DiscussionCommentRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{COMMENT_SELECT_PG} WHERE cm.id = $1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find discussion comment failed: {e}"))?
            {
                Some(r) => Ok(Some(map_comment!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{COMMENT_SELECT_MYSQL} WHERE cm.id = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find discussion comment failed: {e}"))?
            {
                Some(r) => Ok(Some(map_comment!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{COMMENT_SELECT_SQLITE} WHERE cm.id = ?1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find discussion comment failed: {e}"))?
            {
                Some(r) => Ok(Some(map_comment!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// Chronological comment list — replies interleave by `created_at`; the
/// `parent_id` pointer carries threading.
pub async fn list_comments(
    pool: &DbPool,
    discussion_id: &str,
) -> Result<Vec<DiscussionCommentRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "{COMMENT_SELECT_PG} WHERE cm.discussion_id = $1 ORDER BY cm.created_at ASC, cm.id ASC"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(discussion_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list discussion comments failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_comment!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "{COMMENT_SELECT_MYSQL} WHERE cm.discussion_id = ? ORDER BY cm.created_at ASC, cm.id ASC"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(discussion_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list discussion comments failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_comment!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "{COMMENT_SELECT_SQLITE} WHERE cm.discussion_id = ?1 ORDER BY cm.created_at ASC, cm.id ASC"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(discussion_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list discussion comments failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_comment!(r));
            }
            Ok(out)
        }
    }
}

pub async fn update_comment(
    pool: &DbPool,
    id: &str,
    body: &str,
) -> Result<Option<DiscussionCommentRow>, String> {
    let now = now_expr(pool);
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "UPDATE discussion_comments SET body = $1, updated_at = {now} WHERE id = $2"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(body)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("update discussion comment failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let sql =
                format!("UPDATE discussion_comments SET body = ?, updated_at = {now} WHERE id = ?");
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(body)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("update discussion comment failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "UPDATE discussion_comments SET body = ?1, updated_at = {now} WHERE id = ?2"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(body)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("update discussion comment failed: {e}"))?;
        }
    }
    find_comment_by_id(pool, id).await
}

/// Deleting a comment clears a dangling `answer_comment_id` on its parent
/// discussion first (GitHub un-marks the answer when the comment goes).
pub async fn delete_comment(pool: &DbPool, id: &str) -> Result<(), String> {
    let now = now_expr(pool);
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "UPDATE discussions SET answer_comment_id = NULL, answered_at = NULL,
                 answered_by = NULL, updated_at = {now} WHERE answer_comment_id = $1"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("clear discussion answer failed: {e}"))?;
            sqlx::query("DELETE FROM discussion_comments WHERE id = $1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete discussion comment failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "UPDATE discussions SET answer_comment_id = NULL, answered_at = NULL,
                 answered_by = NULL, updated_at = {now} WHERE answer_comment_id = ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("clear discussion answer failed: {e}"))?;
            sqlx::query("DELETE FROM discussion_comments WHERE id = ?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete discussion comment failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "UPDATE discussions SET answer_comment_id = NULL, answered_at = NULL,
                 answered_by = NULL, updated_at = {now} WHERE answer_comment_id = ?1"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("clear discussion answer failed: {e}"))?;
            sqlx::query("DELETE FROM discussion_comments WHERE id = ?1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete discussion comment failed: {e}"))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Labels
// ---------------------------------------------------------------------------

/// Replace the discussion's label set (same semantics as `issue.labels.set`).
/// Returns nothing — callers re-read via `list_discussion_label_ids`.
pub async fn set_discussion_labels(
    pool: &DbPool,
    discussion_id: &str,
    label_ids: &[String],
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin set discussion labels tx failed: {e}"))?;
            sqlx::query("DELETE FROM discussion_labels WHERE discussion_id = $1")
                .bind(discussion_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("clear discussion labels failed: {e}"))?;
            for label_id in label_ids {
                sqlx::query(
                    "INSERT INTO discussion_labels (discussion_id, label_id) VALUES ($1, $2)
                     ON CONFLICT DO NOTHING",
                )
                .bind(discussion_id)
                .bind(label_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("insert discussion label failed: {e}"))?;
            }
            tx.commit()
                .await
                .map_err(|e| format!("commit set discussion labels failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin set discussion labels tx failed: {e}"))?;
            sqlx::query("DELETE FROM discussion_labels WHERE discussion_id = ?")
                .bind(discussion_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("clear discussion labels failed: {e}"))?;
            for label_id in label_ids {
                sqlx::query(
                    "INSERT IGNORE INTO discussion_labels (discussion_id, label_id) VALUES (?, ?)",
                )
                .bind(discussion_id)
                .bind(label_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("insert discussion label failed: {e}"))?;
            }
            tx.commit()
                .await
                .map_err(|e| format!("commit set discussion labels failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin set discussion labels tx failed: {e}"))?;
            sqlx::query("DELETE FROM discussion_labels WHERE discussion_id = ?1")
                .bind(discussion_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("clear discussion labels failed: {e}"))?;
            for label_id in label_ids {
                sqlx::query(
                    "INSERT OR IGNORE INTO discussion_labels (discussion_id, label_id) VALUES (?1, ?2)",
                )
                .bind(discussion_id)
                .bind(label_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("insert discussion label failed: {e}"))?;
            }
            tx.commit()
                .await
                .map_err(|e| format!("commit set discussion labels failed: {e}"))?;
        }
    }
    Ok(())
}

pub async fn list_discussion_label_ids(
    pool: &DbPool,
    discussion_id: &str,
) -> Result<Vec<String>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT label_id FROM discussion_labels WHERE discussion_id = $1 ORDER BY label_id",
            )
            .bind(discussion_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list discussion labels failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(
                    r.try_get::<String, _>("label_id")
                        .map_err(|e| format!("discussion label row: {e}"))?,
                );
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT label_id FROM discussion_labels WHERE discussion_id = ? ORDER BY label_id",
            )
            .bind(discussion_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list discussion labels failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(
                    r.try_get::<String, _>("label_id")
                        .map_err(|e| format!("discussion label row: {e}"))?,
                );
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT label_id FROM discussion_labels WHERE discussion_id = ?1 ORDER BY label_id",
            )
            .bind(discussion_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list discussion labels failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(
                    r.try_get::<String, _>("label_id")
                        .map_err(|e| format!("discussion label row: {e}"))?,
                );
            }
            Ok(out)
        }
    }
}
