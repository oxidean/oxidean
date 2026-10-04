//! Sitewide search queries for `search.global` (DEBT-03).
//!
//! Every repository-scoped query applies the same viewer-visibility predicate
//! used by `list_template_repositories` (templates.rs): public repos, plus
//! private repos the viewer owns, collaborates on, or can read via org role /
//! member base permission. Anonymous viewers only ever see `public` rows.
//!
//! `search_users` never selects email — callers should gate it on a verified
//! session (anti-enumeration), mirroring `user.lookup`.
//!
//! Commits/code hits are not SQL-backed: the API layer fans out to
//! `git grep` / `git log --grep` over the bounded repo list from
//! [`list_scan_repos`] (an index replaces that scan under SRCH-01).

use sqlx::FromRow;

use crate::pool::DbPool;

/// Escape `LIKE`/`ILIKE` wildcards (`%`, `_`, `\`) so user input is literal.
fn escape_like_pattern(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        match c {
            '\\' | '%' | '_' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Viewer-visibility predicate on `repositories r` — Postgres `$1` form.
/// Mirrors the `Capability::Read` coalesce in `oxidean-api::repo::acl`.
const VIS_PRED_PG: &str = r#"(r.visibility = 'public'
    OR (r.owner_type = 'user' AND r.owner_id = $1)
    OR EXISTS (
        SELECT 1 FROM repository_collaborators c
        WHERE c.repo_id = r.id AND c.user_id = $1)
    OR (r.owner_type = 'org' AND EXISTS (
        SELECT 1 FROM organization_members m
        INNER JOIN organizations og ON og.id = m.org_id
        WHERE m.org_id = r.owner_id AND m.user_id = $1
          AND (m.role IN ('owner', 'admin')
               OR og.member_base_permission IN ('read', 'write')))))"#;

/// Same predicate for numbered `?N` binds (SQLite): viewer is `?1`.
const VIS_PRED_SQLITE: &str = r#"(r.visibility = 'public'
    OR (r.owner_type = 'user' AND r.owner_id = ?1)
    OR EXISTS (
        SELECT 1 FROM repository_collaborators c
        WHERE c.repo_id = r.id AND c.user_id = ?1)
    OR (r.owner_type = 'org' AND EXISTS (
        SELECT 1 FROM organization_members m
        INNER JOIN organizations og ON og.id = m.org_id
        WHERE m.org_id = r.owner_id AND m.user_id = ?1
          AND (m.role IN ('owner', 'admin')
               OR og.member_base_permission IN ('read', 'write')))))"#;

/// Same predicate for positional `?` binds (MySQL): three viewer binds, in order.
const VIS_PRED_MYSQL: &str = r#"(r.visibility = 'public'
    OR (r.owner_type = 'user' AND r.owner_id = ?)
    OR EXISTS (
        SELECT 1 FROM repository_collaborators c
        WHERE c.repo_id = r.id AND c.user_id = ?)
    OR (r.owner_type = 'org' AND EXISTS (
        SELECT 1 FROM organization_members m
        INNER JOIN organizations og ON og.id = m.org_id
        WHERE m.org_id = r.owner_id AND m.user_id = ?
          AND (m.role IN ('owner', 'admin')
               OR og.member_base_permission IN ('read', 'write')))))"#;

const PUBLIC_ONLY_PRED: &str = "r.visibility = 'public'";

/// Owner joins shared by every repo-scoped query — resolve the slug from the
/// shared `/{owner}` namespace (username for user repos, slug for org repos).
const OWNER_JOINS: &str = r#"LEFT JOIN users u ON u.id = r.owner_id AND r.owner_type = 'user'
LEFT JOIN organizations o ON o.id = r.owner_id AND r.owner_type = 'org'"#;

fn pred_pg(viewer: Option<&str>) -> &'static str {
    if viewer.is_some() {
        VIS_PRED_PG
    } else {
        PUBLIC_ONLY_PRED
    }
}

fn pred_sqlite(viewer: Option<&str>) -> &'static str {
    if viewer.is_some() {
        VIS_PRED_SQLITE
    } else {
        PUBLIC_ONLY_PRED
    }
}

fn pred_mysql(viewer: Option<&str>) -> &'static str {
    if viewer.is_some() {
        VIS_PRED_MYSQL
    } else {
        PUBLIC_ONLY_PRED
    }
}

/// Repository row for the sitewide `repositories` group.
#[derive(Debug, Clone, FromRow)]
pub struct GlobalRepoHitRow {
    pub owner_slug: String,
    pub owner_type: String,
    pub name: String,
    pub description: String,
    pub visibility: String,
    pub star_count: i64,
    pub updated_at: String,
}

/// Issue row for the sitewide `issues` group.
#[derive(Debug, Clone, FromRow)]
pub struct GlobalIssueHitRow {
    pub repo_owner: String,
    pub repo_name: String,
    pub number: i64,
    pub title: String,
    pub state: String,
    pub author_username: Option<String>,
    pub comment_count: i64,
    pub updated_at: String,
}

/// Pull-request row for the sitewide `pulls` group.
#[derive(Debug, Clone, FromRow)]
pub struct GlobalPullHitRow {
    pub repo_owner: String,
    pub repo_name: String,
    pub number: i64,
    pub title: String,
    pub state: String,
    pub draft: bool,
    pub author_username: Option<String>,
    pub comment_count: i64,
    pub updated_at: String,
}

/// User row for the sitewide `users` group (never email).
#[derive(Debug, Clone, FromRow)]
pub struct GlobalUserHitRow {
    pub username: String,
    pub display_name: String,
    pub avatar_path: Option<String>,
}

/// Organization row for the sitewide `organizations` group.
#[derive(Debug, Clone, FromRow)]
pub struct GlobalOrgHitRow {
    pub slug: String,
    pub display_name: String,
}

/// Minimal repo ref for the bounded git fan-out (commits/code scans).
#[derive(Debug, Clone, FromRow)]
pub struct ScanRepoRow {
    pub owner_slug: String,
    pub name: String,
    pub default_branch: String,
}

const REPO_HIT_COLS_PG: &str = r#"COALESCE(u.username, o.slug, '') AS owner_slug,
    r.owner_type, r.name, r.visibility,
    COALESCE(r.description, '') AS description,
    COALESCE(r.star_count, 0)::bigint AS star_count,
    to_char(r.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS updated_at"#;

const REPO_HIT_COLS_MYSQL: &str = r#"COALESCE(u.username, o.slug, '') AS owner_slug,
    r.owner_type, r.name, r.visibility,
    COALESCE(r.description, '') AS description,
    CAST(COALESCE(r.star_count, 0) AS SIGNED) AS star_count,
    DATE_FORMAT(r.updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at"#;

const REPO_HIT_COLS_SQLITE: &str = r#"COALESCE(u.username, o.slug, '') AS owner_slug,
    r.owner_type, r.name, r.visibility,
    COALESCE(r.description, '') AS description,
    COALESCE(r.star_count, 0) AS star_count,
    strftime('%Y-%m-%dT%H:%M:%SZ', r.updated_at) AS updated_at"#;

/// Match repositories by name, owner login, or description (case-insensitive
/// substring). Returns `(hits, total)` newest/starred first.
pub async fn search_repositories(
    pool: &DbPool,
    viewer_user_id: Option<&str>,
    q: &str,
    offset: i64,
    limit: i64,
) -> Result<(Vec<GlobalRepoHitRow>, i64), String> {
    let pat = format!("%{}%", escape_like_pattern(q));
    match pool {
        DbPool::Postgres(p) => {
            let vis = pred_pg(viewer_user_id);
            // $1 is the viewer when signed in; pattern/limit/offset shift down
            // by one for anonymous calls (no viewer bind).
            let sh = usize::from(viewer_user_id.is_none());
            let ph = |i: usize| format!("${}", i - sh);
            let cond = format!(
                "r.deleted_at IS NULL AND {vis} \
                 AND (r.name ILIKE {a} ESCAPE '\\' \
                      OR COALESCE(u.username, o.slug, '') ILIKE {a} ESCAPE '\\' \
                      OR COALESCE(r.description, '') ILIKE {a} ESCAPE '\\')",
                a = ph(2)
            );
            let total_q_sql =
                format!("SELECT COUNT(*)::bigint FROM repositories r {OWNER_JOINS} WHERE {cond}");
            let total_q = sqlx::query_scalar::<_, i64>(&total_q_sql);
            let total: i64 = bind_opt(total_q, viewer_user_id)
                .bind(&pat)
                .fetch_one(p)
                .await
                .map_err(|e| format!("global repo search count: {e}"))?;
            let rows = bind_opt(
                sqlx::query_as::<_, GlobalRepoHitRow>(&format!(
                    "SELECT {REPO_HIT_COLS_PG} FROM repositories r {OWNER_JOINS} WHERE {cond} \
                     ORDER BY r.star_count DESC, r.updated_at DESC LIMIT {l} OFFSET {o}",
                    l = ph(3),
                    o = ph(4)
                )),
                viewer_user_id,
            )
            .bind(&pat)
            .bind(limit)
            .bind(offset)
            .fetch_all(p)
            .await
            .map_err(|e| format!("global repo search: {e}"))?;
            Ok((rows, total))
        }
        DbPool::MySql(p) => {
            let vis = pred_mysql(viewer_user_id);
            let cond = format!(
                "r.deleted_at IS NULL AND {vis} \
                 AND (LOWER(r.name) LIKE LOWER(?) ESCAPE '\\\\' \
                      OR LOWER(COALESCE(u.username, o.slug, '')) LIKE LOWER(?) ESCAPE '\\\\' \
                      OR LOWER(COALESCE(r.description, '')) LIKE LOWER(?) ESCAPE '\\\\')"
            );
            let count_q_sql =
                format!("SELECT COUNT(*) FROM repositories r {OWNER_JOINS} WHERE {cond}");
            let count_q = sqlx::query_scalar::<_, i64>(&count_q_sql);
            let count_q = bind_viewer_qm(count_q, viewer_user_id)
                .bind(&pat)
                .bind(&pat)
                .bind(&pat);
            let total = count_q
                .fetch_one(p)
                .await
                .map_err(|e| format!("global repo search count: {e}"))?;
            let rows_q_sql = format!(
                "SELECT {REPO_HIT_COLS_MYSQL} FROM repositories r {OWNER_JOINS} WHERE {cond} \
                 ORDER BY r.star_count DESC, r.updated_at DESC LIMIT ? OFFSET ?"
            );
            let rows_q = sqlx::query_as::<_, GlobalRepoHitRow>(&rows_q_sql);
            let rows = bind_viewer_qm(rows_q, viewer_user_id)
                .bind(&pat)
                .bind(&pat)
                .bind(&pat)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
                .map_err(|e| format!("global repo search: {e}"))?;
            Ok((rows, total))
        }
        DbPool::Sqlite(p) => {
            let vis = pred_sqlite(viewer_user_id);
            let sh = usize::from(viewer_user_id.is_none());
            let ph = |i: usize| format!("?{}", i - sh);
            let cond = format!(
                "r.deleted_at IS NULL AND {vis} \
                 AND (LOWER(r.name) LIKE LOWER({a}) ESCAPE '\\' \
                      OR LOWER(COALESCE(u.username, o.slug, '')) LIKE LOWER({a}) ESCAPE '\\' \
                      OR LOWER(COALESCE(r.description, '')) LIKE LOWER({a}) ESCAPE '\\')",
                a = ph(2)
            );
            let count_q_sql =
                format!("SELECT COUNT(*) FROM repositories r {OWNER_JOINS} WHERE {cond}");
            let count_q = sqlx::query_scalar::<_, i64>(&count_q_sql);
            let total = bind_opt(count_q, viewer_user_id)
                .bind(&pat)
                .fetch_one(p)
                .await
                .map_err(|e| format!("global repo search count: {e}"))?;
            let rows = bind_opt(
                sqlx::query_as::<_, GlobalRepoHitRow>(&format!(
                    "SELECT {REPO_HIT_COLS_SQLITE} FROM repositories r {OWNER_JOINS} WHERE {cond} \
                     ORDER BY r.star_count DESC, r.updated_at DESC LIMIT {l} OFFSET {o}",
                    l = ph(3),
                    o = ph(4)
                )),
                viewer_user_id,
            )
            .bind(&pat)
            .bind(limit)
            .bind(offset)
            .fetch_all(p)
            .await
            .map_err(|e| format!("global repo search: {e}"))?;
            Ok((rows, total))
        }
    }
}

/// ACL-filtered issue substring search (title/body) across all visible repos.
/// `q`/`state`/`author_id` are optional filters; `state` is `open`|`closed`.
pub async fn search_issues(
    pool: &DbPool,
    viewer_user_id: Option<&str>,
    q: Option<&str>,
    state: Option<&str>,
    author_id: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<(Vec<GlobalIssueHitRow>, i64), String> {
    let pat = q.map(|s| format!("%{}%", escape_like_pattern(s)));
    match pool {
        DbPool::Postgres(p) => {
            let vis = pred_pg(viewer_user_id);
            let sh = usize::from(viewer_user_id.is_none());
            let ph = |i: usize| format!("${}", i - sh);
            let cond = format!(
                "{vis} \
                 AND ({a}::text IS NULL OR i.state = {a}) \
                 AND ({b}::text IS NULL OR i.author_id = {b}) \
                 AND ({c}::text IS NULL OR i.title ILIKE {c} ESCAPE '\\' \
                      OR i.body ILIKE {c} ESCAPE '\\')",
                a = ph(2),
                b = ph(3),
                c = ph(4)
            );
            let total: i64 = bind_opt(
                sqlx::query_scalar(&format!(
                    "SELECT COUNT(*)::bigint FROM issues i \
                     JOIN repositories r ON r.id = i.repo_id AND r.deleted_at IS NULL \
                     {OWNER_JOINS} WHERE {cond}"
                )),
                viewer_user_id,
            )
            .bind(state)
            .bind(author_id)
            .bind(pat.as_deref())
            .fetch_one(p)
            .await
            .map_err(|e| format!("global issue search count: {e}"))?;
            let rows = bind_opt(
                sqlx::query_as::<_, GlobalIssueHitRow>(&format!(
                    "SELECT COALESCE(u.username, o.slug, '') AS repo_owner, r.name AS repo_name, \
                     i.number, i.title, i.state, au.username AS author_username, \
                     (SELECT COUNT(*)::bigint FROM issue_comments c WHERE c.issue_id = i.id) AS comment_count, \
                     to_char(i.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at \
                     FROM issues i \
                     JOIN repositories r ON r.id = i.repo_id AND r.deleted_at IS NULL \
                     {OWNER_JOINS} \
                     LEFT JOIN users au ON au.id = i.author_id \
                     WHERE {cond} ORDER BY i.updated_at DESC LIMIT {l} OFFSET {o}",
                    l = ph(5),
                    o = ph(6)
                )),
                viewer_user_id,
            )
            .bind(state)
            .bind(author_id)
            .bind(pat.as_deref())
            .bind(limit)
            .bind(offset)
            .fetch_all(p)
            .await
            .map_err(|e| format!("global issue search: {e}"))?;
            Ok((rows, total))
        }
        DbPool::MySql(p) => {
            let vis = pred_mysql(viewer_user_id);
            let cond = format!(
                "{vis} \
                 AND (? IS NULL OR i.state = ?) \
                 AND (? IS NULL OR i.author_id = ?) \
                 AND (? IS NULL OR LOWER(i.title) LIKE LOWER(?) ESCAPE '\\\\' \
                      OR LOWER(i.body) LIKE LOWER(?) ESCAPE '\\\\')"
            );
            let from = "FROM issues i \
                     JOIN repositories r ON r.id = i.repo_id AND r.deleted_at IS NULL";
            let count_q_sql = format!("SELECT COUNT(*) {from} {OWNER_JOINS} WHERE {cond}");
            let count_q = sqlx::query_scalar::<_, i64>(&count_q_sql);
            let count_q = bind_viewer_qm(count_q, viewer_user_id)
                .bind(state)
                .bind(state)
                .bind(author_id)
                .bind(author_id)
                .bind(pat.as_deref())
                .bind(pat.as_deref())
                .bind(pat.as_deref());
            let total = count_q
                .fetch_one(p)
                .await
                .map_err(|e| format!("global issue search count: {e}"))?;
            let rows_q_sql = format!(
                "SELECT COALESCE(u.username, o.slug, '') AS repo_owner, r.name AS repo_name, \
                 i.number, i.title, i.state, au.username AS author_username, \
                 CAST((SELECT COUNT(*) FROM issue_comments c WHERE c.issue_id = i.id) AS SIGNED) AS comment_count, \
                 DATE_FORMAT(i.updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at \
                 {from} {OWNER_JOINS} \
                 LEFT JOIN users au ON au.id = i.author_id \
                 WHERE {cond} ORDER BY i.updated_at DESC LIMIT ? OFFSET ?"
            );
            let rows_q = sqlx::query_as::<_, GlobalIssueHitRow>(&rows_q_sql);
            let rows = bind_viewer_qm(rows_q, viewer_user_id)
                .bind(state)
                .bind(state)
                .bind(author_id)
                .bind(author_id)
                .bind(pat.as_deref())
                .bind(pat.as_deref())
                .bind(pat.as_deref())
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
                .map_err(|e| format!("global issue search: {e}"))?;
            Ok((rows, total))
        }
        DbPool::Sqlite(p) => {
            let vis = pred_sqlite(viewer_user_id);
            let sh = usize::from(viewer_user_id.is_none());
            let ph = |i: usize| format!("?{}", i - sh);
            let cond = format!(
                "{vis} \
                 AND ({a} IS NULL OR i.state = {a}) \
                 AND ({b} IS NULL OR i.author_id = {b}) \
                 AND ({c} IS NULL OR LOWER(i.title) LIKE LOWER({c}) ESCAPE '\\' \
                      OR LOWER(i.body) LIKE LOWER({c}) ESCAPE '\\')",
                a = ph(2),
                b = ph(3),
                c = ph(4)
            );
            let total: i64 = bind_opt(
                sqlx::query_scalar(&format!(
                    "SELECT COUNT(*) FROM issues i \
                     JOIN repositories r ON r.id = i.repo_id AND r.deleted_at IS NULL \
                     {OWNER_JOINS} WHERE {cond}"
                )),
                viewer_user_id,
            )
            .bind(state)
            .bind(author_id)
            .bind(pat.as_deref())
            .fetch_one(p)
            .await
            .map_err(|e| format!("global issue search count: {e}"))?;
            let rows = bind_opt(
                sqlx::query_as::<_, GlobalIssueHitRow>(&format!(
                    "SELECT COALESCE(u.username, o.slug, '') AS repo_owner, r.name AS repo_name, \
                     i.number, i.title, i.state, au.username AS author_username, \
                     (SELECT COUNT(*) FROM issue_comments c WHERE c.issue_id = i.id) AS comment_count, \
                     strftime('%Y-%m-%dT%H:%M:%SZ', i.updated_at) AS updated_at \
                     FROM issues i \
                     JOIN repositories r ON r.id = i.repo_id AND r.deleted_at IS NULL \
                     {OWNER_JOINS} \
                     LEFT JOIN users au ON au.id = i.author_id \
                     WHERE {cond} ORDER BY i.updated_at DESC LIMIT {l} OFFSET {o}",
                    l = ph(5),
                    o = ph(6)
                )),
                viewer_user_id,
            )
            .bind(state)
            .bind(author_id)
            .bind(pat.as_deref())
            .bind(limit)
            .bind(offset)
            .fetch_all(p)
            .await
            .map_err(|e| format!("global issue search: {e}"))?;
            Ok((rows, total))
        }
    }
}

/// ACL-filtered pull-request substring search (title/body) across all visible
/// repos. `state` is `open`|`closed`|`merged`; NULL → all.
pub async fn search_pulls(
    pool: &DbPool,
    viewer_user_id: Option<&str>,
    q: Option<&str>,
    state: Option<&str>,
    author_id: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<(Vec<GlobalPullHitRow>, i64), String> {
    let pat = q.map(|s| format!("%{}%", escape_like_pattern(s)));
    match pool {
        DbPool::Postgres(p) => {
            let vis = pred_pg(viewer_user_id);
            let sh = usize::from(viewer_user_id.is_none());
            let ph = |i: usize| format!("${}", i - sh);
            let cond = format!(
                "{vis} \
                 AND ({a}::text IS NULL OR i.state = {a}) \
                 AND ({b}::text IS NULL OR i.author_id = {b}) \
                 AND ({c}::text IS NULL OR i.title ILIKE {c} ESCAPE '\\' \
                      OR i.body ILIKE {c} ESCAPE '\\')",
                a = ph(2),
                b = ph(3),
                c = ph(4)
            );
            let total: i64 = bind_opt(
                sqlx::query_scalar(&format!(
                    "SELECT COUNT(*)::bigint FROM pull_requests i \
                     JOIN repositories r ON r.id = i.repo_id AND r.deleted_at IS NULL \
                     {OWNER_JOINS} WHERE {cond}"
                )),
                viewer_user_id,
            )
            .bind(state)
            .bind(author_id)
            .bind(pat.as_deref())
            .fetch_one(p)
            .await
            .map_err(|e| format!("global pull search count: {e}"))?;
            let rows = bind_opt(
                sqlx::query_as::<_, GlobalPullHitRow>(&format!(
                    "SELECT COALESCE(u.username, o.slug, '') AS repo_owner, r.name AS repo_name, \
                     i.number, i.title, i.state, i.draft, au.username AS author_username, \
                     (SELECT COUNT(*)::bigint FROM pull_comments c WHERE c.pull_id = i.id) AS comment_count, \
                     to_char(i.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at \
                     FROM pull_requests i \
                     JOIN repositories r ON r.id = i.repo_id AND r.deleted_at IS NULL \
                     {OWNER_JOINS} \
                     LEFT JOIN users au ON au.id = i.author_id \
                     WHERE {cond} ORDER BY i.updated_at DESC LIMIT {l} OFFSET {o}",
                    l = ph(5),
                    o = ph(6)
                )),
                viewer_user_id,
            )
            .bind(state)
            .bind(author_id)
            .bind(pat.as_deref())
            .bind(limit)
            .bind(offset)
            .fetch_all(p)
            .await
            .map_err(|e| format!("global pull search: {e}"))?;
            Ok((rows, total))
        }
        DbPool::MySql(p) => {
            let vis = pred_mysql(viewer_user_id);
            let cond = format!(
                "{vis} \
                 AND (? IS NULL OR i.state = ?) \
                 AND (? IS NULL OR i.author_id = ?) \
                 AND (? IS NULL OR LOWER(i.title) LIKE LOWER(?) ESCAPE '\\\\' \
                      OR LOWER(i.body) LIKE LOWER(?) ESCAPE '\\\\')"
            );
            let from = "FROM pull_requests i \
                     JOIN repositories r ON r.id = i.repo_id AND r.deleted_at IS NULL";
            let count_q_sql = format!("SELECT COUNT(*) {from} {OWNER_JOINS} WHERE {cond}");
            let count_q = sqlx::query_scalar::<_, i64>(&count_q_sql);
            let count_q = bind_viewer_qm(count_q, viewer_user_id)
                .bind(state)
                .bind(state)
                .bind(author_id)
                .bind(author_id)
                .bind(pat.as_deref())
                .bind(pat.as_deref())
                .bind(pat.as_deref());
            let total = count_q
                .fetch_one(p)
                .await
                .map_err(|e| format!("global pull search count: {e}"))?;
            let rows_q_sql = format!(
                "SELECT COALESCE(u.username, o.slug, '') AS repo_owner, r.name AS repo_name, \
                 i.number, i.title, i.state, i.draft, au.username AS author_username, \
                 CAST((SELECT COUNT(*) FROM pull_comments c WHERE c.pull_id = i.id) AS SIGNED) AS comment_count, \
                 DATE_FORMAT(i.updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at \
                 {from} {OWNER_JOINS} \
                 LEFT JOIN users au ON au.id = i.author_id \
                 WHERE {cond} ORDER BY i.updated_at DESC LIMIT ? OFFSET ?"
            );
            let rows_q = sqlx::query_as::<_, GlobalPullHitRow>(&rows_q_sql);
            let rows = bind_viewer_qm(rows_q, viewer_user_id)
                .bind(state)
                .bind(state)
                .bind(author_id)
                .bind(author_id)
                .bind(pat.as_deref())
                .bind(pat.as_deref())
                .bind(pat.as_deref())
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
                .map_err(|e| format!("global pull search: {e}"))?;
            Ok((rows, total))
        }
        DbPool::Sqlite(p) => {
            let vis = pred_sqlite(viewer_user_id);
            let sh = usize::from(viewer_user_id.is_none());
            let ph = |i: usize| format!("?{}", i - sh);
            let cond = format!(
                "{vis} \
                 AND ({a} IS NULL OR i.state = {a}) \
                 AND ({b} IS NULL OR i.author_id = {b}) \
                 AND ({c} IS NULL OR LOWER(i.title) LIKE LOWER({c}) ESCAPE '\\' \
                      OR LOWER(i.body) LIKE LOWER({c}) ESCAPE '\\')",
                a = ph(2),
                b = ph(3),
                c = ph(4)
            );
            let total: i64 = bind_opt(
                sqlx::query_scalar(&format!(
                    "SELECT COUNT(*) FROM pull_requests i \
                     JOIN repositories r ON r.id = i.repo_id AND r.deleted_at IS NULL \
                     {OWNER_JOINS} WHERE {cond}"
                )),
                viewer_user_id,
            )
            .bind(state)
            .bind(author_id)
            .bind(pat.as_deref())
            .fetch_one(p)
            .await
            .map_err(|e| format!("global pull search count: {e}"))?;
            let rows = bind_opt(
                sqlx::query_as::<_, GlobalPullHitRow>(&format!(
                    "SELECT COALESCE(u.username, o.slug, '') AS repo_owner, r.name AS repo_name, \
                     i.number, i.title, i.state, i.draft, au.username AS author_username, \
                     (SELECT COUNT(*) FROM pull_comments c WHERE c.pull_id = i.id) AS comment_count, \
                     strftime('%Y-%m-%dT%H:%M:%SZ', i.updated_at) AS updated_at \
                     FROM pull_requests i \
                     JOIN repositories r ON r.id = i.repo_id AND r.deleted_at IS NULL \
                     {OWNER_JOINS} \
                     LEFT JOIN users au ON au.id = i.author_id \
                     WHERE {cond} ORDER BY i.updated_at DESC LIMIT {l} OFFSET {o}",
                    l = ph(5),
                    o = ph(6)
                )),
                viewer_user_id,
            )
            .bind(state)
            .bind(author_id)
            .bind(pat.as_deref())
            .bind(limit)
            .bind(offset)
            .fetch_all(p)
            .await
            .map_err(|e| format!("global pull search: {e}"))?;
            Ok((rows, total))
        }
    }
}

/// Substring search over users by username/display name. Never returns email;
/// callers gate on a verified session. `banned_at` rows are excluded.
pub async fn search_users(
    pool: &DbPool,
    q: &str,
    offset: i64,
    limit: i64,
) -> Result<(Vec<GlobalUserHitRow>, i64), String> {
    let pat = format!("%{}%", escape_like_pattern(q));
    match pool {
        DbPool::Postgres(p) => {
            let cond = "banned_at IS NULL AND (username ILIKE $1 ESCAPE '\\' \
                        OR display_name ILIKE $1 ESCAPE '\\')";
            let total: i64 =
                sqlx::query_scalar(&format!("SELECT COUNT(*)::bigint FROM users WHERE {cond}"))
                    .bind(&pat)
                    .fetch_one(p)
                    .await
                    .map_err(|e| format!("global user search count: {e}"))?;
            let rows = sqlx::query_as::<_, GlobalUserHitRow>(&format!(
                "SELECT username, display_name, avatar_path FROM users WHERE {cond} \
                 ORDER BY username LIMIT $2 OFFSET $3"
            ))
            .bind(&pat)
            .bind(limit)
            .bind(offset)
            .fetch_all(p)
            .await
            .map_err(|e| format!("global user search: {e}"))?;
            Ok((rows, total))
        }
        DbPool::MySql(p) => {
            let cond = "banned_at IS NULL AND (LOWER(username) LIKE LOWER(?) ESCAPE '\\\\' \
                        OR LOWER(display_name) LIKE LOWER(?) ESCAPE '\\\\')";
            let total: i64 =
                sqlx::query_scalar(&format!("SELECT COUNT(*) FROM users WHERE {cond}"))
                    .bind(&pat)
                    .bind(&pat)
                    .fetch_one(p)
                    .await
                    .map_err(|e| format!("global user search count: {e}"))?;
            let rows = sqlx::query_as::<_, GlobalUserHitRow>(&format!(
                "SELECT username, display_name, avatar_path FROM users WHERE {cond} \
                 ORDER BY username LIMIT ? OFFSET ?"
            ))
            .bind(&pat)
            .bind(&pat)
            .bind(limit)
            .bind(offset)
            .fetch_all(p)
            .await
            .map_err(|e| format!("global user search: {e}"))?;
            Ok((rows, total))
        }
        DbPool::Sqlite(p) => {
            let cond = "banned_at IS NULL AND (LOWER(username) LIKE LOWER(?1) ESCAPE '\\' \
                        OR LOWER(display_name) LIKE LOWER(?1) ESCAPE '\\')";
            let total: i64 =
                sqlx::query_scalar(&format!("SELECT COUNT(*) FROM users WHERE {cond}"))
                    .bind(&pat)
                    .fetch_one(p)
                    .await
                    .map_err(|e| format!("global user search count: {e}"))?;
            let rows = sqlx::query_as::<_, GlobalUserHitRow>(&format!(
                "SELECT username, display_name, avatar_path FROM users WHERE {cond} \
                 ORDER BY username LIMIT ?2 OFFSET ?3"
            ))
            .bind(&pat)
            .bind(limit)
            .bind(offset)
            .fetch_all(p)
            .await
            .map_err(|e| format!("global user search: {e}"))?;
            Ok((rows, total))
        }
    }
}

/// Substring search over organizations by slug/display name. Organization
/// profiles are public directory entries (`/{slug}`), so no viewer gating.
pub async fn search_orgs(
    pool: &DbPool,
    q: &str,
    offset: i64,
    limit: i64,
) -> Result<(Vec<GlobalOrgHitRow>, i64), String> {
    let pat = format!("%{}%", escape_like_pattern(q));
    match pool {
        DbPool::Postgres(p) => {
            let cond = "slug ILIKE $1 ESCAPE '\\' OR display_name ILIKE $1 ESCAPE '\\'";
            let total: i64 = sqlx::query_scalar(&format!(
                "SELECT COUNT(*)::bigint FROM organizations WHERE {cond}"
            ))
            .bind(&pat)
            .fetch_one(p)
            .await
            .map_err(|e| format!("global org search count: {e}"))?;
            let rows = sqlx::query_as::<_, GlobalOrgHitRow>(&format!(
                "SELECT slug, display_name FROM organizations WHERE {cond} \
                 ORDER BY slug LIMIT $2 OFFSET $3"
            ))
            .bind(&pat)
            .bind(limit)
            .bind(offset)
            .fetch_all(p)
            .await
            .map_err(|e| format!("global org search: {e}"))?;
            Ok((rows, total))
        }
        DbPool::MySql(p) => {
            let cond = "LOWER(slug) LIKE LOWER(?) ESCAPE '\\\\' \
                        OR LOWER(display_name) LIKE LOWER(?) ESCAPE '\\\\'";
            let total: i64 =
                sqlx::query_scalar(&format!("SELECT COUNT(*) FROM organizations WHERE {cond}"))
                    .bind(&pat)
                    .bind(&pat)
                    .fetch_one(p)
                    .await
                    .map_err(|e| format!("global org search count: {e}"))?;
            let rows = sqlx::query_as::<_, GlobalOrgHitRow>(&format!(
                "SELECT slug, display_name FROM organizations WHERE {cond} \
                 ORDER BY slug LIMIT ? OFFSET ?"
            ))
            .bind(&pat)
            .bind(&pat)
            .bind(limit)
            .bind(offset)
            .fetch_all(p)
            .await
            .map_err(|e| format!("global org search: {e}"))?;
            Ok((rows, total))
        }
        DbPool::Sqlite(p) => {
            let cond = "LOWER(slug) LIKE LOWER(?1) ESCAPE '\\' \
                        OR LOWER(display_name) LIKE LOWER(?1) ESCAPE '\\'";
            let total: i64 =
                sqlx::query_scalar(&format!("SELECT COUNT(*) FROM organizations WHERE {cond}"))
                    .bind(&pat)
                    .fetch_one(p)
                    .await
                    .map_err(|e| format!("global org search count: {e}"))?;
            let rows = sqlx::query_as::<_, GlobalOrgHitRow>(&format!(
                "SELECT slug, display_name FROM organizations WHERE {cond} \
                 ORDER BY slug LIMIT ?2 OFFSET ?3"
            ))
            .bind(&pat)
            .bind(limit)
            .bind(offset)
            .fetch_all(p)
            .await
            .map_err(|e| format!("global org search: {e}"))?;
            Ok((rows, total))
        }
    }
}

/// Recently-updated repos the viewer can read — the bounded candidate set for
/// the cross-repo `git grep`/`git log` scans. Caller enforces the cap; request
/// one extra row to detect truncation.
pub async fn list_scan_repos(
    pool: &DbPool,
    viewer_user_id: Option<&str>,
    limit: i64,
) -> Result<Vec<ScanRepoRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let vis = pred_pg(viewer_user_id);
            let lp = if viewer_user_id.is_some() { "$2" } else { "$1" };
            let rows = bind_opt(
                sqlx::query_as::<_, ScanRepoRow>(&format!(
                    "SELECT COALESCE(u.username, o.slug, '') AS owner_slug, r.name, r.default_branch \
                     FROM repositories r {OWNER_JOINS} \
                     WHERE r.deleted_at IS NULL AND {vis} \
                     ORDER BY r.updated_at DESC LIMIT {lp}"
                )),
                viewer_user_id,
            )
            .bind(limit)
            .fetch_all(p)
            .await
            .map_err(|e| format!("global scan repos: {e}"))?;
            Ok(rows)
        }
        DbPool::MySql(p) => {
            let vis = pred_mysql(viewer_user_id);
            let rows = bind_viewer_qm(
                sqlx::query_as::<_, ScanRepoRow>(&format!(
                    "SELECT COALESCE(u.username, o.slug, '') AS owner_slug, r.name, r.default_branch \
                     FROM repositories r {OWNER_JOINS} \
                     WHERE r.deleted_at IS NULL AND {vis} \
                     ORDER BY r.updated_at DESC LIMIT ?"
                )),
                viewer_user_id,
            )
            .bind(limit)
            .fetch_all(p)
            .await
            .map_err(|e| format!("global scan repos: {e}"))?;
            Ok(rows)
        }
        DbPool::Sqlite(p) => {
            let vis = pred_sqlite(viewer_user_id);
            let lp = if viewer_user_id.is_some() { "?2" } else { "?1" };
            let rows = bind_opt(
                sqlx::query_as::<_, ScanRepoRow>(&format!(
                    "SELECT COALESCE(u.username, o.slug, '') AS owner_slug, r.name, r.default_branch \
                     FROM repositories r {OWNER_JOINS} \
                     WHERE r.deleted_at IS NULL AND {vis} \
                     ORDER BY r.updated_at DESC LIMIT {lp}"
                )),
                viewer_user_id,
            )
            .bind(limit)
            .fetch_all(p)
            .await
            .map_err(|e| format!("global scan repos: {e}"))?;
            Ok(rows)
        }
    }
}

/// Bind the viewer id once for numbered-placeholder dialects (PG/SQLite).
fn bind_opt<'q, Q>(q: Q, viewer: Option<&'q str>) -> Q
where
    Q: Bindable<'q>,
{
    match viewer {
        Some(uid) => q.bind_val(uid),
        None => q,
    }
}

/// Bind the viewer id three times for the positional `?` predicate (MySQL).
fn bind_viewer_qm<'q, Q>(q: Q, viewer: Option<&'q str>) -> Q
where
    Q: Bindable<'q>,
{
    match viewer {
        Some(uid) => q.bind_val(uid).bind_val(uid).bind_val(uid),
        None => q,
    }
}

/// Small helper trait so `bind_opt`/`bind_viewer_qm` work for both
/// `sqlx::Query` and `sqlx::QueryAs` without spelling out database generics.
trait Bindable<'q> {
    fn bind_val(self, v: &'q str) -> Self;
}

impl<'q, DB> Bindable<'q> for sqlx::query::Query<'q, DB, <DB as sqlx::Database>::Arguments<'q>>
where
    DB: sqlx::Database,
    &'q str: sqlx::Encode<'q, DB> + sqlx::Type<DB>,
{
    fn bind_val(self, v: &'q str) -> Self {
        self.bind(v)
    }
}

impl<'q, DB, T> Bindable<'q>
    for sqlx::query::QueryAs<'q, DB, T, <DB as sqlx::Database>::Arguments<'q>>
where
    DB: sqlx::Database,
    &'q str: sqlx::Encode<'q, DB> + sqlx::Type<DB>,
{
    fn bind_val(self, v: &'q str) -> Self {
        self.bind(v)
    }
}

impl<'q, DB, T> Bindable<'q>
    for sqlx::query::QueryScalar<'q, DB, T, <DB as sqlx::Database>::Arguments<'q>>
where
    DB: sqlx::Database,
    &'q str: sqlx::Encode<'q, DB> + sqlx::Type<DB>,
{
    fn bind_val(self, v: &'q str) -> Self {
        self.bind(v)
    }
}
