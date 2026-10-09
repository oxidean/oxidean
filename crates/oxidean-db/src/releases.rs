//! Releases + release_assets persistence (Phase 15 / GIT-14).

use crate::pool::DbPool;
use sqlx::Row;

#[derive(Debug, Clone)]
pub struct ReleaseRow {
    pub id: String,
    pub repo_id: String,
    pub tag_name: String,
    pub title: String,
    pub body: String,
    pub draft: bool,
    pub prerelease: bool,
    pub author_id: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ReleaseAssetRow {
    pub id: String,
    pub release_id: String,
    pub filename: String,
    pub content_type: String,
    pub byte_size: i64,
    pub uploader_id: String,
    pub created_at: String,
    pub updated_at: String,
}

macro_rules! map_rel {
    ($row:expr) => {{
        let row = $row;
        // PG `CASE … THEN 1 ELSE 0` is INT4 (i32); MySQL/SQLite often i64; BOOLEAN also appears.
        let draft_i: i64 = row
            .try_get::<i32, _>("draft")
            .map(|v| i64::from(v))
            .or_else(|_| row.try_get::<i64, _>("draft"))
            .or_else(|_| {
                row.try_get::<bool, _>("draft")
                    .map(|b| if b { 1 } else { 0 })
            })
            .map_err(|e| format!("draft: {e}"))?;
        let pre_i: i64 = row
            .try_get::<i32, _>("prerelease")
            .map(|v| i64::from(v))
            .or_else(|_| row.try_get::<i64, _>("prerelease"))
            .or_else(|_| {
                row.try_get::<bool, _>("prerelease")
                    .map(|b| if b { 1 } else { 0 })
            })
            .map_err(|e| format!("prerelease: {e}"))?;
        ReleaseRow {
            id: row.try_get("id").map_err(|e| format!("id: {e}"))?,
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("repo_id: {e}"))?,
            tag_name: row
                .try_get("tag_name")
                .map_err(|e| format!("tag_name: {e}"))?,
            title: row.try_get("title").map_err(|e| format!("title: {e}"))?,
            body: row.try_get("body").map_err(|e| format!("body: {e}"))?,
            draft: draft_i != 0,
            prerelease: pre_i != 0,
            author_id: row
                .try_get("author_id")
                .map_err(|e| format!("author_id: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("created_at: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("updated_at: {e}"))?,
        }
    }};
}

macro_rules! map_asset {
    ($row:expr) => {{
        let row = $row;
        ReleaseAssetRow {
            id: row.try_get("id").map_err(|e| format!("id: {e}"))?,
            release_id: row
                .try_get("release_id")
                .map_err(|e| format!("release_id: {e}"))?,
            filename: row
                .try_get("filename")
                .map_err(|e| format!("filename: {e}"))?,
            content_type: row
                .try_get("content_type")
                .map_err(|e| format!("content_type: {e}"))?,
            byte_size: row
                .try_get("byte_size")
                .map_err(|e| format!("byte_size: {e}"))?,
            uploader_id: row
                .try_get("uploader_id")
                .map_err(|e| format!("uploader_id: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("created_at: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("updated_at: {e}"))?,
        }
    }};
}

const SEL_PG: &str = "SELECT id, repo_id, tag_name, title, body, CASE WHEN draft THEN 1::bigint ELSE 0::bigint END AS draft, CASE WHEN prerelease THEN 1::bigint ELSE 0::bigint END AS prerelease, author_id, to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at, to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at FROM releases";
const SEL_MY: &str = "SELECT id, repo_id, tag_name, title, body, draft, prerelease, author_id, DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at, DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at FROM releases";
const SEL_SQ: &str = "SELECT id, repo_id, tag_name, title, body, draft, prerelease, author_id, strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at, strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at FROM releases";

const ASEL_PG: &str = "SELECT id, release_id, filename, content_type, byte_size, uploader_id, to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at, to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at FROM release_assets";
const ASEL_MY: &str = "SELECT id, release_id, filename, content_type, byte_size, uploader_id, DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at, DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at FROM release_assets";
const ASEL_SQ: &str = "SELECT id, release_id, filename, content_type, byte_size, uploader_id, strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at, strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at FROM release_assets";

pub async fn insert_release(
    pool: &DbPool,
    id: &str,
    repo_id: &str,
    tag_name: &str,
    title: &str,
    body: &str,
    draft: bool,
    prerelease: bool,
    author_id: &str,
) -> Result<ReleaseRow, String> {
    let d: i64 = if draft { 1 } else { 0 };
    let p: i64 = if prerelease { 1 } else { 0 };
    match pool {
        DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO releases (id, repo_id, tag_name, title, body, draft, prerelease, author_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
                .bind(id).bind(repo_id).bind(tag_name).bind(title).bind(body).bind(draft).bind(prerelease).bind(author_id)
                .execute(pool).await.map_err(|e| format!("insert release: {e}"))?;
        }
        DbPool::MySql(pool) => {
            sqlx::query("INSERT INTO releases (id, repo_id, tag_name, title, body, draft, prerelease, author_id) VALUES (?,?,?,?,?,?,?,?)")
                .bind(id).bind(repo_id).bind(tag_name).bind(title).bind(body).bind(d).bind(p).bind(author_id)
                .execute(pool).await.map_err(|e| format!("insert release: {e}"))?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT INTO releases (id, repo_id, tag_name, title, body, draft, prerelease, author_id) VALUES (?,?,?,?,?,?,?,?)")
                .bind(id).bind(repo_id).bind(tag_name).bind(title).bind(body).bind(d).bind(p).bind(author_id)
                .execute(pool).await.map_err(|e| format!("insert release: {e}"))?;
        }
    }
    find_release_by_id(pool, id)
        .await?
        .ok_or_else(|| "release missing after insert".into())
}

pub async fn find_release_by_id(pool: &DbPool, id: &str) -> Result<Option<ReleaseRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{SEL_PG} WHERE id = $1");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_rel!(r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{SEL_MY} WHERE id = ?");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_rel!(r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{SEL_SQ} WHERE id = ?");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_rel!(r))),
                None => Ok(None),
            }
        }
    }
}

pub async fn find_release_by_repo_tag(
    pool: &DbPool,
    repo_id: &str,
    tag_name: &str,
) -> Result<Option<ReleaseRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{SEL_PG} WHERE repo_id = $1 AND tag_name = $2");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(tag_name)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find tag: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_rel!(r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{SEL_MY} WHERE repo_id = ? AND tag_name = ?");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(tag_name)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find tag: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_rel!(r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{SEL_SQ} WHERE repo_id = ? AND tag_name = ?");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(tag_name)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find tag: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_rel!(r))),
                None => Ok(None),
            }
        }
    }
}

pub async fn list_releases_for_repo(
    pool: &DbPool,
    repo_id: &str,
    include_drafts: bool,
) -> Result<Vec<ReleaseRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = if include_drafts {
                format!("{SEL_PG} WHERE repo_id = $1 ORDER BY created_at DESC")
            } else {
                format!("{SEL_PG} WHERE repo_id = $1 AND draft = FALSE ORDER BY created_at DESC")
            };
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list: {e}"))?;
            {
                let mut out = Vec::new();
                for r in rows {
                    out.push(map_rel!(r));
                }
                Ok(out)
            }
        }
        DbPool::MySql(p) => {
            let sql = if include_drafts {
                format!("{SEL_MY} WHERE repo_id = ? ORDER BY created_at DESC")
            } else {
                format!("{SEL_MY} WHERE repo_id = ? AND draft = 0 ORDER BY created_at DESC")
            };
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list: {e}"))?;
            {
                let mut out = Vec::new();
                for r in rows {
                    out.push(map_rel!(r));
                }
                Ok(out)
            }
        }
        DbPool::Sqlite(p) => {
            let sql = if include_drafts {
                format!("{SEL_SQ} WHERE repo_id = ? ORDER BY created_at DESC")
            } else {
                format!("{SEL_SQ} WHERE repo_id = ? AND draft = 0 ORDER BY created_at DESC")
            };
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list: {e}"))?;
            {
                let mut out = Vec::new();
                for r in rows {
                    out.push(map_rel!(r));
                }
                Ok(out)
            }
        }
    }
}

pub async fn update_release(
    pool: &DbPool,
    id: &str,
    title: &str,
    body: &str,
    draft: bool,
    prerelease: bool,
) -> Result<ReleaseRow, String> {
    let d: i64 = if draft { 1 } else { 0 };
    let pflag: i64 = if prerelease { 1 } else { 0 };
    match pool {
        DbPool::Postgres(pool) => {
            sqlx::query("UPDATE releases SET title=$1, body=$2, draft=$3, prerelease=$4, updated_at=now() WHERE id=$5")
                .bind(title).bind(body).bind(draft).bind(prerelease).bind(id)
                .execute(pool).await.map_err(|e| format!("update: {e}"))?;
        }
        DbPool::MySql(pool) => {
            sqlx::query("UPDATE releases SET title=?, body=?, draft=?, prerelease=?, updated_at=CURRENT_TIMESTAMP WHERE id=?")
                .bind(title).bind(body).bind(d).bind(pflag).bind(id)
                .execute(pool).await.map_err(|e| format!("update: {e}"))?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("UPDATE releases SET title=?, body=?, draft=?, prerelease=?, updated_at=strftime('%Y-%m-%d %H:%M:%S','now') WHERE id=?")
                .bind(title).bind(body).bind(d).bind(pflag).bind(id)
                .execute(pool).await.map_err(|e| format!("update: {e}"))?;
        }
    }
    find_release_by_id(pool, id)
        .await?
        .ok_or_else(|| "release not found".into())
}

pub async fn delete_release(pool: &DbPool, id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM releases WHERE id=$1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM releases WHERE id=?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM releases WHERE id=?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete: {e}"))?;
        }
    }
    Ok(())
}

pub async fn list_assets_for_release(
    pool: &DbPool,
    release_id: &str,
) -> Result<Vec<ReleaseAssetRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{ASEL_PG} WHERE release_id = $1 ORDER BY filename");
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(release_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list assets: {e}"))?;
            {
                let mut out = Vec::new();
                for r in rows {
                    out.push(map_asset!(r));
                }
                Ok(out)
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{ASEL_MY} WHERE release_id = ? ORDER BY filename");
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(release_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list assets: {e}"))?;
            {
                let mut out = Vec::new();
                for r in rows {
                    out.push(map_asset!(r));
                }
                Ok(out)
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{ASEL_SQ} WHERE release_id = ? ORDER BY filename");
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(release_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list assets: {e}"))?;
            {
                let mut out = Vec::new();
                for r in rows {
                    out.push(map_asset!(r));
                }
                Ok(out)
            }
        }
    }
}

pub async fn find_asset_by_id(pool: &DbPool, id: &str) -> Result<Option<ReleaseAssetRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{ASEL_PG} WHERE id = $1");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find asset: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_asset!(r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{ASEL_MY} WHERE id = ?");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find asset: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_asset!(r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{ASEL_SQ} WHERE id = ?");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find asset: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_asset!(r))),
                None => Ok(None),
            }
        }
    }
}

pub async fn find_asset_by_release_filename(
    pool: &DbPool,
    release_id: &str,
    filename: &str,
) -> Result<Option<ReleaseAssetRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{ASEL_PG} WHERE release_id = $1 AND filename = $2");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(release_id)
                .bind(filename)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find asset by name: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_asset!(r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{ASEL_MY} WHERE release_id = ? AND filename = ?");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(release_id)
                .bind(filename)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find asset by name: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_asset!(r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{ASEL_SQ} WHERE release_id = ? AND filename = ?");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(release_id)
                .bind(filename)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find asset by name: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_asset!(r))),
                None => Ok(None),
            }
        }
    }
}

pub async fn insert_asset(
    pool: &DbPool,
    id: &str,
    release_id: &str,
    filename: &str,
    content_type: &str,
    byte_size: i64,
    uploader_id: &str,
) -> Result<ReleaseAssetRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("INSERT INTO release_assets (id, release_id, filename, content_type, byte_size, uploader_id) VALUES ($1,$2,$3,$4,$5,$6)")
                .bind(id).bind(release_id).bind(filename).bind(content_type).bind(byte_size).bind(uploader_id)
                .execute(p).await.map_err(|e| format!("insert asset: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("INSERT INTO release_assets (id, release_id, filename, content_type, byte_size, uploader_id) VALUES (?,?,?,?,?,?)")
                .bind(id).bind(release_id).bind(filename).bind(content_type).bind(byte_size).bind(uploader_id)
                .execute(p).await.map_err(|e| format!("insert asset: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("INSERT INTO release_assets (id, release_id, filename, content_type, byte_size, uploader_id) VALUES (?,?,?,?,?,?)")
                .bind(id).bind(release_id).bind(filename).bind(content_type).bind(byte_size).bind(uploader_id)
                .execute(p).await.map_err(|e| format!("insert asset: {e}"))?;
        }
    }
    find_asset_by_id(pool, id)
        .await?
        .ok_or_else(|| "asset missing after insert".into())
}

pub async fn update_asset_bytes(
    pool: &DbPool,
    id: &str,
    content_type: &str,
    byte_size: i64,
) -> Result<ReleaseAssetRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE release_assets SET content_type=$1, byte_size=$2, updated_at=now() WHERE id=$3")
                .bind(content_type).bind(byte_size).bind(id).execute(p).await.map_err(|e| format!("update asset: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE release_assets SET content_type=?, byte_size=?, updated_at=CURRENT_TIMESTAMP WHERE id=?")
                .bind(content_type).bind(byte_size).bind(id).execute(p).await.map_err(|e| format!("update asset: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE release_assets SET content_type=?, byte_size=?, updated_at=strftime('%Y-%m-%d %H:%M:%S','now') WHERE id=?")
                .bind(content_type).bind(byte_size).bind(id).execute(p).await.map_err(|e| format!("update asset: {e}"))?;
        }
    }
    find_asset_by_id(pool, id)
        .await?
        .ok_or_else(|| "asset not found".into())
}

pub async fn delete_asset(pool: &DbPool, id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM release_assets WHERE id=$1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete asset: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM release_assets WHERE id=?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete asset: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM release_assets WHERE id=?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete asset: {e}"))?;
        }
    }
    Ok(())
}
