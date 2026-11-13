//! Read-side scan for comment-attachment references.

use sqlx::Row;

use crate::pool::DbPool;

const SELECTS: &[&str] = &[
    "SELECT body FROM issues WHERE body LIKE {p}",
    "SELECT body FROM issue_comments WHERE body LIKE {p}",
    "SELECT body FROM issue_revisions WHERE body LIKE {p}",
    "SELECT body FROM comment_revisions WHERE body LIKE {p}",
    "SELECT body FROM pull_requests WHERE body LIKE {p}",
    "SELECT body FROM pull_comments WHERE body LIKE {p}",
    "SELECT body FROM pull_comment_revisions WHERE body LIKE {p}",
    "SELECT body FROM pull_reviews WHERE body LIKE {p}",
    "SELECT body FROM discussions WHERE body LIKE {p}",
    "SELECT body FROM discussion_comments WHERE body LIKE {p}",
    "SELECT body FROM releases WHERE body LIKE {p}",
    "SELECT body FROM projects WHERE body LIKE {p}",
    "SELECT note AS body FROM project_cards WHERE note LIKE {p}",
    "SELECT body FROM saved_replies WHERE body LIKE {p}",
];

const MARKER_LIKE: &str = "%/uploads/comment-attachments/%";

macro_rules! collect_bodies {
    ($pool:expr, $placeholder:literal) => {{
        let mut out = Vec::new();
        for select in SELECTS {
            let sql = select.replace("{p}", $placeholder);
            let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(MARKER_LIKE)
                .fetch_all($pool)
                .await
                .map_err(|e| format!("comment attachment reference scan failed: {e}"))?;
            for row in rows {
                if let Some(body) = row
                    .try_get::<Option<String>, _>("body")
                    .map_err(|e| format!("comment attachment reference row: {e}"))?
                {
                    out.push(body);
                }
            }
        }
        out
    }};
}

pub async fn bodies_containing_comment_attachment_refs(
    pool: &DbPool,
) -> Result<Vec<String>, String> {
    match pool {
        DbPool::Postgres(p) => Ok(collect_bodies!(p, "$1")),
        DbPool::MySql(p) => Ok(collect_bodies!(p, "?")),
        DbPool::Sqlite(p) => Ok(collect_bodies!(p, "?1")),
    }
}
