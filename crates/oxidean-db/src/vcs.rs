//! Agentic-VCS persistence (contract C5 / BR2.x) — the sole dialect-SQL site
//! for the eight `vcs_*` tables. JSON TEXT columns (`parents`, `payload`) stay
//! `String` on rows; wire decode happens at the `oxidean-vcs` edge.
//!
//! Typed sentinel errors (FD R-13 — exact-match strings so u4/u7 map without
//! substring sniffing):
//! - `vcs.not_found:parent` — a parent changeset id does not exist
//! - `vcs.not_found:object` — a referenced object hash does not exist
//! - `vcs.invalid_input:parents` — `parents` is not a JSON array of strings
//! - `vcs.invalid_input:file_change` — non-delete change without `blob_hash`

use std::collections::BTreeSet;

use sqlx::Row;

use crate::dialect::{in_placeholders, Dialect};
use crate::pool::DbPool;
use crate::Database;

const ERR_NO_PARENT: &str = "vcs.not_found:parent";
const ERR_NO_OBJECT: &str = "vcs.not_found:object";
const ERR_BAD_PARENTS: &str = "vcs.invalid_input:parents";
const ERR_BAD_FILE_CHANGE: &str = "vcs.invalid_input:file_change";

// ---------- rows ----------

/// Per-repo VCS enablement flag — absent row = never-enabled (denies `vcs.*`
/// as `vcs.disabled`); present + `enabled = false` = admin-disabled.
#[derive(Debug, Clone)]
pub struct VcsRepoSurfaceFlagRow {
    pub repo_id: String,
    pub enabled: bool,
    pub enabled_by: Option<String>,
    pub enabled_at: String,
}

/// Global CAS row keyed by bare-hex hash — the row implies the bytes exist on
/// disk. No `repo_id`: objects are shared across repositories by design (FR1.5).
#[derive(Debug, Clone)]
pub struct VcsObjectRow {
    pub hash: String,
    pub size: i64,
    pub created_at: String,
}

/// Per-repo reference/quota attribution for a global object.
#[derive(Debug, Clone)]
pub struct VcsObjectLinkRow {
    pub repo_id: String,
    pub hash: String,
    pub created_at: String,
}

/// Immutable changeset. `parents` is a JSON array of changeset ids stored
/// verbatim (TEXT-JSON — parse at the `oxidean-vcs` boundary).
#[derive(Debug, Clone)]
pub struct VcsChangesetRow {
    pub id: String,
    pub repo_id: String,
    pub parents: String,
    pub author: String,
    pub message: String,
    pub committed_at: String,
    pub manifest_hash: String,
    pub idem_key: Option<String>,
    pub created_at: String,
}

/// One manifest entry — `blob_hash` is `None` only on `kind = "delete"`.
#[derive(Debug, Clone)]
pub struct VcsFileChangeRow {
    pub changeset_id: String,
    pub path: String,
    pub kind: String,
    pub mode: i64,
    pub old_path: Option<String>,
    pub blob_hash: Option<String>,
}

/// The only mutable row — `changeset_id` moves exclusively through the
/// `vcs_cas_ref_update` CAS path. `name` is stored bare (`main`, no
/// `refs/heads/` prefix).
#[derive(Debug, Clone)]
pub struct VcsRefRow {
    pub repo_id: String,
    pub name: String,
    pub changeset_id: Option<String>,
    pub updated_at: String,
}

/// Incremental derived row (walk/diff/blame/path-history/merge-base).
/// `payload` is JSON text; `index_key` is the column name (`key` is
/// mysql-reserved).
#[derive(Debug, Clone)]
pub struct VcsDerivedIndexRow {
    pub repo_id: String,
    pub kind: String,
    pub index_key: String,
    pub payload: String,
    /// `NULL` on u6 import marker rows (`active`/zero-import `cursor`) that
    /// predate any changeset (migration 0055).
    pub covering_changeset: Option<String>,
    pub updated_at: String,
}

/// `git_oid ↔ changeset_id` mapping — import idempotency + export walks.
#[derive(Debug, Clone)]
pub struct VcsImportLinkRow {
    pub repo_id: String,
    pub git_oid: String,
    pub changeset_id: String,
    pub created_at: String,
}

/// Outcome of a ref CAS write — `Result<_, String>` cannot carry `actual`
/// (C1 `vcs.ref_conflict.data.actual`), so the result is a typed enum.
#[derive(Debug)]
pub enum CasOutcome {
    /// The write landed; carries the post-write ref row. A delete carries
    /// `changeset_id: None` — absent refs, not tombstones.
    Applied(VcsRefRow),
    /// The expected value did not match; `actual` is the stored head
    /// (`None` when the ref is absent).
    Conflict { actual: Option<String> },
}

/// Repo-scoped object accounting for `vcs.status` — COUNT + SUM over the
/// link↔object join (`repo_logical_bytes` precedent in `lfs.rs`).
#[derive(Debug, Clone, Copy)]
pub struct VcsRepoObjectCounts {
    pub object_count: i64,
    pub storage_bytes: i64,
}

// ---------- create_tx inputs ----------

/// Changeset column values for `create_tx`. `parents` is the JSON array text
/// stored verbatim (and decoded once for parent pre-validation);
/// `committed_at` is a caller-supplied ISO-8601 UTC timestamp.
pub struct VcsChangesetInsert<'a> {
    pub id: &'a str,
    pub repo_id: &'a str,
    pub parents: &'a str,
    pub author: &'a str,
    pub message: &'a str,
    pub committed_at: &'a str,
    pub manifest_hash: &'a str,
    pub idem_key: Option<&'a str>,
}

/// One manifest row for `create_tx`. `blob_hash` is required on every
/// `kind` except `delete` (asserted before any write).
pub struct VcsFileChangeInsert<'a> {
    pub path: &'a str,
    /// `add` | `modify` | `delete` | `rename`.
    pub kind: &'a str,
    pub mode: i64,
    pub old_path: Option<&'a str>,
    pub blob_hash: Option<&'a str>,
}

/// One derived-index upsert for `create_tx` / `upsert_derived_index_rows`.
/// `payload` is JSON text stored verbatim.
pub struct VcsDerivedIndexUpsert<'a> {
    pub kind: &'a str,
    pub index_key: &'a str,
    pub payload: &'a str,
    /// `None` writes `NULL` — u6 import marker rows (`active`/zero-import
    /// `cursor`) carry no covering changeset (migration 0055).
    pub covering_changeset: Option<&'a str>,
}

// ---------- row mapping ----------

macro_rules! map_object {
    ($row:expr) => {{
        let row = $row;
        VcsObjectRow {
            hash: row
                .try_get("hash")
                .map_err(|e| format!("vcs object row: {e}"))?,
            size: row
                .try_get("size")
                .map_err(|e| format!("vcs object row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("vcs object row: {e}"))?,
        }
    }};
}

macro_rules! map_changeset {
    ($row:expr) => {{
        let row = $row;
        VcsChangesetRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("vcs changeset row: {e}"))?,
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("vcs changeset row: {e}"))?,
            parents: row
                .try_get("parents")
                .map_err(|e| format!("vcs changeset row: {e}"))?,
            author: row
                .try_get("author")
                .map_err(|e| format!("vcs changeset row: {e}"))?,
            message: row
                .try_get("message")
                .map_err(|e| format!("vcs changeset row: {e}"))?,
            committed_at: row
                .try_get("committed_at")
                .map_err(|e| format!("vcs changeset row: {e}"))?,
            manifest_hash: row
                .try_get("manifest_hash")
                .map_err(|e| format!("vcs changeset row: {e}"))?,
            idem_key: row.try_get::<Option<String>, _>("idem_key").unwrap_or(None),
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("vcs changeset row: {e}"))?,
        }
    }};
}

macro_rules! map_file_change {
    ($row:expr) => {{
        let row = $row;
        VcsFileChangeRow {
            changeset_id: row
                .try_get("changeset_id")
                .map_err(|e| format!("vcs file_change row: {e}"))?,
            path: row
                .try_get("path")
                .map_err(|e| format!("vcs file_change row: {e}"))?,
            kind: row
                .try_get("kind")
                .map_err(|e| format!("vcs file_change row: {e}"))?,
            mode: row
                .try_get("mode")
                .map_err(|e| format!("vcs file_change row: {e}"))?,
            old_path: row.try_get::<Option<String>, _>("old_path").unwrap_or(None),
            blob_hash: row
                .try_get::<Option<String>, _>("blob_hash")
                .unwrap_or(None),
        }
    }};
}

macro_rules! map_ref {
    ($row:expr) => {{
        let row = $row;
        VcsRefRow {
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("vcs ref row: {e}"))?,
            name: row
                .try_get("name")
                .map_err(|e| format!("vcs ref row: {e}"))?,
            changeset_id: row
                .try_get::<Option<String>, _>("changeset_id")
                .unwrap_or(None),
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("vcs ref row: {e}"))?,
        }
    }};
}

macro_rules! map_index_row {
    ($row:expr) => {{
        let row = $row;
        VcsDerivedIndexRow {
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("vcs derived_index row: {e}"))?,
            kind: row
                .try_get("kind")
                .map_err(|e| format!("vcs derived_index row: {e}"))?,
            index_key: row
                .try_get("index_key")
                .map_err(|e| format!("vcs derived_index row: {e}"))?,
            payload: row
                .try_get("payload")
                .map_err(|e| format!("vcs derived_index row: {e}"))?,
            covering_changeset: row
                .try_get("covering_changeset")
                .map_err(|e| format!("vcs derived_index row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("vcs derived_index row: {e}"))?,
        }
    }};
}

macro_rules! map_import_link {
    ($row:expr) => {{
        let row = $row;
        VcsImportLinkRow {
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("vcs import_link row: {e}"))?,
            git_oid: row
                .try_get("git_oid")
                .map_err(|e| format!("vcs import_link row: {e}"))?,
            changeset_id: row
                .try_get("changeset_id")
                .map_err(|e| format!("vcs import_link row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("vcs import_link row: {e}"))?,
        }
    }};
}

// ---------- repo surface flag ----------

/// `None` = never-enabled (`vcs.disabled`); `Some(enabled:false)` = admin-disabled.
pub async fn get_repo_surface_flag(
    pool: &DbPool,
    repo_id: &str,
) -> Result<Option<VcsRepoSurfaceFlagRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                "SELECT repo_id, enabled, enabled_by,
                        to_char(enabled_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS enabled_at
                 FROM vcs_repo_surface_flags WHERE repo_id = $1",
            )
            .bind(repo_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("vcs get_repo_surface_flag: {e}"))?;
            row.map(|r| {
                Ok(VcsRepoSurfaceFlagRow {
                    repo_id: r
                        .try_get("repo_id")
                        .map_err(|e| format!("vcs flag row: {e}"))?,
                    enabled: r
                        .try_get::<bool, _>("enabled")
                        .map_err(|e| format!("vcs flag row: {e}"))?,
                    enabled_by: r.try_get::<Option<String>, _>("enabled_by").unwrap_or(None),
                    enabled_at: r
                        .try_get("enabled_at")
                        .map_err(|e| format!("vcs flag row: {e}"))?,
                })
            })
            .transpose()
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                "SELECT repo_id, enabled, enabled_by,
                        DATE_FORMAT(enabled_at, '%Y-%m-%dT%H:%i:%sZ') AS enabled_at
                 FROM vcs_repo_surface_flags WHERE repo_id = ?",
            )
            .bind(repo_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("vcs get_repo_surface_flag: {e}"))?;
            row.map(|r| {
                Ok(VcsRepoSurfaceFlagRow {
                    repo_id: r
                        .try_get("repo_id")
                        .map_err(|e| format!("vcs flag row: {e}"))?,
                    enabled: r.try_get::<i8, _>("enabled").unwrap_or(0) != 0,
                    enabled_by: r.try_get::<Option<String>, _>("enabled_by").unwrap_or(None),
                    enabled_at: r
                        .try_get("enabled_at")
                        .map_err(|e| format!("vcs flag row: {e}"))?,
                })
            })
            .transpose()
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                "SELECT repo_id, enabled, enabled_by,
                        strftime('%Y-%m-%dT%H:%M:%SZ', enabled_at) AS enabled_at
                 FROM vcs_repo_surface_flags WHERE repo_id = ?1",
            )
            .bind(repo_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("vcs get_repo_surface_flag: {e}"))?;
            row.map(|r| {
                Ok(VcsRepoSurfaceFlagRow {
                    repo_id: r
                        .try_get("repo_id")
                        .map_err(|e| format!("vcs flag row: {e}"))?,
                    enabled: r.try_get::<i64, _>("enabled").unwrap_or(0) != 0,
                    enabled_by: r.try_get::<Option<String>, _>("enabled_by").unwrap_or(None),
                    enabled_at: r
                        .try_get("enabled_at")
                        .map_err(|e| format!("vcs flag row: {e}"))?,
                })
            })
            .transpose()
        }
    }
}

/// Upsert on `repo_enable` — a re-enable rewrites `enabled_by`/`enabled_at`.
pub async fn upsert_repo_surface_flag(
    pool: &DbPool,
    repo_id: &str,
    enabled: bool,
    enabled_by: Option<&str>,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO vcs_repo_surface_flags (repo_id, enabled, enabled_by)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (repo_id) DO UPDATE SET
                   enabled = excluded.enabled,
                   enabled_by = excluded.enabled_by,
                   enabled_at = now()",
            )
            .bind(repo_id)
            .bind(enabled)
            .bind(enabled_by)
            .execute(p)
            .await
            .map_err(|e| format!("vcs upsert_repo_surface_flag: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO vcs_repo_surface_flags (repo_id, enabled, enabled_by)
                 VALUES (?, ?, ?)
                 ON DUPLICATE KEY UPDATE
                   enabled = VALUES(enabled),
                   enabled_by = VALUES(enabled_by),
                   enabled_at = CURRENT_TIMESTAMP(6)",
            )
            .bind(repo_id)
            .bind(if enabled { 1 } else { 0 })
            .bind(enabled_by)
            .execute(p)
            .await
            .map_err(|e| format!("vcs upsert_repo_surface_flag: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO vcs_repo_surface_flags (repo_id, enabled, enabled_by)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(repo_id) DO UPDATE SET
                   enabled = excluded.enabled,
                   enabled_by = excluded.enabled_by,
                   enabled_at = strftime('%Y-%m-%d %H:%M:%S','now')",
            )
            .bind(repo_id)
            .bind(if enabled { 1 } else { 0 })
            .bind(enabled_by)
            .execute(p)
            .await
            .map_err(|e| format!("vcs upsert_repo_surface_flag: {e}"))?;
        }
    }
    Ok(())
}

// ---------- objects (global CAS) ----------

pub async fn get_object(pool: &DbPool, hash: &str) -> Result<Option<VcsObjectRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                "SELECT hash, size,
                        to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
                 FROM vcs_objects WHERE hash = $1",
            )
            .bind(hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("vcs get_object: {e}"))?;
            row.map(|r| Ok(map_object!(r))).transpose()
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                "SELECT hash, size,
                        DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
                 FROM vcs_objects WHERE hash = ?",
            )
            .bind(hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("vcs get_object: {e}"))?;
            row.map(|r| Ok(map_object!(r))).transpose()
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                "SELECT hash, size,
                        strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at
                 FROM vcs_objects WHERE hash = ?1",
            )
            .bind(hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("vcs get_object: {e}"))?;
            row.map(|r| Ok(map_object!(r))).transpose()
        }
    }
}

/// Insert-if-absent — `true` when the row was stored, `false` on dedup
/// (u3 maps this to `Stored | Dedup`).
pub async fn insert_object_if_absent(pool: &DbPool, hash: &str, size: i64) -> Result<bool, String> {
    let affected = match pool {
        DbPool::Postgres(p) => sqlx::query(
            "INSERT INTO vcs_objects (hash, size) VALUES ($1, $2)
                 ON CONFLICT DO NOTHING",
        )
        .bind(hash)
        .bind(size)
        .execute(p)
        .await
        .map_err(|e| format!("vcs insert_object_if_absent: {e}"))?
        .rows_affected(),
        DbPool::MySql(p) => {
            sqlx::query("INSERT IGNORE INTO vcs_objects (hash, size) VALUES (?, ?)")
                .bind(hash)
                .bind(size)
                .execute(p)
                .await
                .map_err(|e| format!("vcs insert_object_if_absent: {e}"))?
                .rows_affected()
        }
        DbPool::Sqlite(p) => {
            sqlx::query("INSERT OR IGNORE INTO vcs_objects (hash, size) VALUES (?1, ?2)")
                .bind(hash)
                .bind(size)
                .execute(p)
                .await
                .map_err(|e| format!("vcs insert_object_if_absent: {e}"))?
                .rows_affected()
        }
    };
    Ok(affected > 0)
}

/// Global objects with no live links, created before `grace_ts` (ISO-8601) —
/// the GC scan (`list_unreferenced_lfs_oids` precedent). Rows only: deleting
/// one is blocked by `object_links`/`file_changes` RESTRICT FKs, which is the
/// intended protective assertion.
pub async fn list_orphaned_objects_before(
    pool: &DbPool,
    grace_ts: &str,
) -> Result<Vec<VcsObjectRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT o.hash, o.size,
                        to_char(o.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
                 FROM vcs_objects o
                 LEFT JOIN vcs_object_links l ON l.hash = o.hash
                 WHERE l.hash IS NULL AND o.created_at < $1::timestamptz
                 ORDER BY o.hash",
            )
            .bind(grace_ts)
            .fetch_all(p)
            .await
            .map_err(|e| format!("vcs list_orphaned_objects_before: {e}"))?;
            rows.iter().map(|r| Ok(map_object!(r))).collect()
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT o.hash, o.size,
                        DATE_FORMAT(o.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
                 FROM vcs_objects o
                 LEFT JOIN vcs_object_links l ON l.hash = o.hash
                 WHERE l.hash IS NULL
                   AND o.created_at < STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')
                 ORDER BY o.hash",
            )
            .bind(grace_ts)
            .fetch_all(p)
            .await
            .map_err(|e| format!("vcs list_orphaned_objects_before: {e}"))?;
            rows.iter().map(|r| Ok(map_object!(r))).collect()
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT o.hash, o.size,
                        strftime('%Y-%m-%dT%H:%M:%SZ', o.created_at) AS created_at
                 FROM vcs_objects o
                 LEFT JOIN vcs_object_links l ON l.hash = o.hash
                 WHERE l.hash IS NULL
                   AND datetime(o.created_at) < datetime(?1)
                 ORDER BY o.hash",
            )
            .bind(grace_ts)
            .fetch_all(p)
            .await
            .map_err(|e| format!("vcs list_orphaned_objects_before: {e}"))?;
            rows.iter().map(|r| Ok(map_object!(r))).collect()
        }
    }
}

// ---------- object links ----------

/// Insert-if-absent — later changesets re-referencing an already-linked blob
/// must not collide on the (repo_id, hash) PK.
pub async fn insert_object_link_if_absent(
    pool: &DbPool,
    repo_id: &str,
    hash: &str,
) -> Result<bool, String> {
    let affected = match pool {
        DbPool::Postgres(p) => sqlx::query(
            "INSERT INTO vcs_object_links (repo_id, hash) VALUES ($1, $2)
                 ON CONFLICT DO NOTHING",
        )
        .bind(repo_id)
        .bind(hash)
        .execute(p)
        .await
        .map_err(|e| format!("vcs insert_object_link_if_absent: {e}"))?
        .rows_affected(),
        DbPool::MySql(p) => {
            sqlx::query("INSERT IGNORE INTO vcs_object_links (repo_id, hash) VALUES (?, ?)")
                .bind(repo_id)
                .bind(hash)
                .execute(p)
                .await
                .map_err(|e| format!("vcs insert_object_link_if_absent: {e}"))?
                .rows_affected()
        }
        DbPool::Sqlite(p) => {
            sqlx::query("INSERT OR IGNORE INTO vcs_object_links (repo_id, hash) VALUES (?1, ?2)")
                .bind(repo_id)
                .bind(hash)
                .execute(p)
                .await
                .map_err(|e| format!("vcs insert_object_link_if_absent: {e}"))?
                .rows_affected()
        }
    };
    Ok(affected > 0)
}

/// `{object_count, storage_bytes}` for `vcs.status` — COUNT + SUM over the
/// link↔object join (lfs.rs `repo_logical_bytes` precedent).
pub async fn repo_object_counts(
    pool: &DbPool,
    repo_id: &str,
) -> Result<VcsRepoObjectCounts, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                "SELECT COUNT(*) AS object_count,
                        COALESCE(SUM(o.size), 0)::bigint AS storage_bytes
                 FROM vcs_object_links l
                 JOIN vcs_objects o ON o.hash = l.hash
                 WHERE l.repo_id = $1",
            )
            .bind(repo_id)
            .fetch_one(p)
            .await
            .map_err(|e| format!("vcs repo_object_counts: {e}"))?;
            Ok(VcsRepoObjectCounts {
                object_count: row.try_get("object_count").unwrap_or(0),
                storage_bytes: row.try_get("storage_bytes").unwrap_or(0),
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                "SELECT COUNT(*) AS object_count,
                        COALESCE(SUM(o.size), 0) AS storage_bytes
                 FROM vcs_object_links l
                 JOIN vcs_objects o ON o.hash = l.hash
                 WHERE l.repo_id = ?",
            )
            .bind(repo_id)
            .fetch_one(p)
            .await
            .map_err(|e| format!("vcs repo_object_counts: {e}"))?;
            Ok(VcsRepoObjectCounts {
                object_count: row.try_get("object_count").unwrap_or(0),
                storage_bytes: row.try_get("storage_bytes").unwrap_or(0),
            })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                "SELECT COUNT(*) AS object_count,
                        COALESCE(SUM(o.size), 0) AS storage_bytes
                 FROM vcs_object_links l
                 JOIN vcs_objects o ON o.hash = l.hash
                 WHERE l.repo_id = ?1",
            )
            .bind(repo_id)
            .fetch_one(p)
            .await
            .map_err(|e| format!("vcs repo_object_counts: {e}"))?;
            Ok(VcsRepoObjectCounts {
                object_count: row.try_get("object_count").unwrap_or(0),
                storage_bytes: row.try_get("storage_bytes").unwrap_or(0),
            })
        }
    }
}

/// Membership probe — `true` when a `(repo_id, hash)` link exists
/// (`has_lfs_link` shape). u3's quota gate: a re-reference skips the
/// storage_bytes charge.
pub async fn get_object_link(pool: &DbPool, repo_id: &str, hash: &str) -> Result<bool, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                "SELECT 1 AS ok FROM vcs_object_links WHERE repo_id = $1 AND hash = $2",
            )
            .bind(repo_id)
            .bind(hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("vcs get_object_link: {e}"))?;
            Ok(row.is_some())
        }
        DbPool::MySql(p) => {
            let row =
                sqlx::query("SELECT 1 AS ok FROM vcs_object_links WHERE repo_id = ? AND hash = ?")
                    .bind(repo_id)
                    .bind(hash)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| format!("vcs get_object_link: {e}"))?;
            Ok(row.is_some())
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                "SELECT 1 AS ok FROM vcs_object_links WHERE repo_id = ?1 AND hash = ?2",
            )
            .bind(repo_id)
            .bind(hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("vcs get_object_link: {e}"))?;
            Ok(row.is_some())
        }
    }
}

/// Conditional delete (u3 GC pass): `true` = the row was deleted so the
/// caller may unlink the file; `false` = protected by a live link or a
/// `vcs_file_changes.blob_hash` reference — distinct from `Err` = a genuine
/// failure (no per-pass warn spam on the U4-invariant edge). Dialect-
/// portable: mysql's same-table-subquery ban doesn't apply — the NOT EXISTS
/// clauses target other tables (`idx_vcs_object_links_hash` /
/// `idx_vcs_file_changes_blob` serve them).
pub async fn delete_object_if_unlinked(pool: &DbPool, hash: &str) -> Result<bool, String> {
    let affected = match pool {
        DbPool::Postgres(p) => sqlx::query(
            "DELETE FROM vcs_objects WHERE hash = $1
                 AND NOT EXISTS (SELECT 1 FROM vcs_object_links WHERE hash = $1)
                 AND NOT EXISTS (SELECT 1 FROM vcs_file_changes WHERE blob_hash = $1)",
        )
        .bind(hash)
        .execute(p)
        .await
        .map_err(|e| format!("vcs delete_object_if_unlinked: {e}"))?
        .rows_affected(),
        DbPool::MySql(p) => sqlx::query(
            "DELETE FROM vcs_objects WHERE hash = ?
                 AND NOT EXISTS (SELECT 1 FROM vcs_object_links WHERE hash = ?)
                 AND NOT EXISTS (SELECT 1 FROM vcs_file_changes WHERE blob_hash = ?)",
        )
        .bind(hash)
        .bind(hash)
        .bind(hash)
        .execute(p)
        .await
        .map_err(|e| format!("vcs delete_object_if_unlinked: {e}"))?
        .rows_affected(),
        DbPool::Sqlite(p) => sqlx::query(
            "DELETE FROM vcs_objects WHERE hash = ?1
                 AND NOT EXISTS (SELECT 1 FROM vcs_object_links WHERE hash = ?1)
                 AND NOT EXISTS (SELECT 1 FROM vcs_file_changes WHERE blob_hash = ?1)",
        )
        .bind(hash)
        .execute(p)
        .await
        .map_err(|e| format!("vcs delete_object_if_unlinked: {e}"))?
        .rows_affected(),
    };
    Ok(affected > 0)
}

/// Every object hash linked to this repo — the GC referenced-ness set.
pub async fn live_object_hashes(pool: &DbPool, repo_id: &str) -> Result<Vec<String>, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query_scalar("SELECT hash FROM vcs_object_links WHERE repo_id = $1")
                .bind(repo_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("vcs live_object_hashes: {e}"))
        }
        DbPool::MySql(p) => {
            sqlx::query_scalar("SELECT hash FROM vcs_object_links WHERE repo_id = ?")
                .bind(repo_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("vcs live_object_hashes: {e}"))
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar("SELECT hash FROM vcs_object_links WHERE repo_id = ?1")
                .bind(repo_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("vcs live_object_hashes: {e}"))
        }
    }
}

// ---------- changesets ----------

/// Shared pre-write pass for `create_tx` (BR2.6 + FD R-13): parses `parents`
/// (the stored JSON text is the validation source), asserts the
/// non-delete ⇒ blob_hash rule, and collects every object hash the tx will
/// reference (file-change blobs + link hashes).
fn create_preflight<'a>(
    changeset: &VcsChangesetInsert<'a>,
    file_changes: &[VcsFileChangeInsert<'a>],
    object_links: &[&'a str],
) -> Result<(BTreeSet<String>, BTreeSet<&'a str>), String> {
    let parent_ids: BTreeSet<String> =
        serde_json::from_str(changeset.parents).map_err(|_| ERR_BAD_PARENTS.to_string())?;
    for fc in file_changes {
        if fc.kind != "delete" && fc.blob_hash.is_none() {
            return Err(ERR_BAD_FILE_CHANGE.into());
        }
    }
    let mut object_hashes: BTreeSet<&'a str> = BTreeSet::new();
    for fc in file_changes {
        if let Some(h) = fc.blob_hash {
            object_hashes.insert(h);
        }
    }
    for h in object_links {
        object_hashes.insert(*h);
    }
    Ok((parent_ids, object_hashes))
}

/// Re-SELECT the winner after an insert-if-absent conflict — done on the pool
/// (post-rollback) so mysql's REPEATABLE READ snapshot sees the committed row.
async fn reselect_existing_changeset(
    pool: &DbPool,
    repo_id: &str,
    id: &str,
    idem_key: Option<&str>,
) -> Result<Option<String>, String> {
    // Idempotency key first (a resubmit may have minted a fresh id), then id.
    if let Some(idem) = idem_key {
        let found: Option<String> = match pool {
            DbPool::Postgres(p) => {
                sqlx::query_scalar(
                    "SELECT id FROM vcs_changesets WHERE repo_id = $1 AND idem_key = $2",
                )
                .bind(repo_id)
                .bind(idem)
                .fetch_optional(p)
                .await
            }
            DbPool::MySql(p) => {
                sqlx::query_scalar(
                    "SELECT id FROM vcs_changesets WHERE repo_id = ? AND idem_key = ?",
                )
                .bind(repo_id)
                .bind(idem)
                .fetch_optional(p)
                .await
            }
            DbPool::Sqlite(p) => {
                sqlx::query_scalar(
                    "SELECT id FROM vcs_changesets WHERE repo_id = ?1 AND idem_key = ?2",
                )
                .bind(repo_id)
                .bind(idem)
                .fetch_optional(p)
                .await
            }
        }
        .map_err(|e| format!("vcs create_tx reselect: {e}"))?;
        if found.is_some() {
            return Ok(found);
        }
    }
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query_scalar("SELECT id FROM vcs_changesets WHERE repo_id = $1 AND id = $2")
                .bind(repo_id)
                .bind(id)
                .fetch_optional(p)
                .await
        }
        DbPool::MySql(p) => {
            sqlx::query_scalar("SELECT id FROM vcs_changesets WHERE repo_id = ? AND id = ?")
                .bind(repo_id)
                .bind(id)
                .fetch_optional(p)
                .await
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar("SELECT id FROM vcs_changesets WHERE repo_id = ?1 AND id = ?2")
                .bind(repo_id)
                .bind(id)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|e| format!("vcs create_tx reselect: {e}"))
}

/// Single-transaction changeset write (BR2.6, WF2): idem lookup → parent +
/// object pre-validation → changeset insert-if-absent → file_changes →
/// object_links insert-if-absent → derived-index merge upserts → commit.
/// Any failure rolls the whole tx back — zero partial state. Returns the
/// changeset id (existing id on an idempotent resubmit — never an error).
pub async fn create_changeset_tx(
    pool: &DbPool,
    changeset: &VcsChangesetInsert<'_>,
    file_changes: &[VcsFileChangeInsert<'_>],
    object_links: &[&str],
    index_upserts: &[VcsDerivedIndexUpsert<'_>],
) -> Result<String, String> {
    let (parent_ids, object_hashes) = create_preflight(changeset, file_changes, object_links)?;
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("vcs create_tx begin: {e}"))?;
            if let Some(idem) = changeset.idem_key {
                let existing: Option<String> = sqlx::query_scalar(
                    "SELECT id FROM vcs_changesets WHERE repo_id = $1 AND idem_key = $2",
                )
                .bind(changeset.repo_id)
                .bind(idem)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| format!("vcs create_tx idem lookup: {e}"))?;
                if let Some(id) = existing {
                    let _ = tx.rollback().await;
                    return Ok(id);
                }
            }
            if !parent_ids.is_empty() {
                let in_list = in_placeholders(Dialect::Postgres, 2, parent_ids.len());
                let q = format!(
                    "SELECT id FROM vcs_changesets WHERE repo_id = $1 AND id IN ({in_list})"
                );
                let mut query = sqlx::query(sqlx::AssertSqlSafe(q)).bind(changeset.repo_id);
                for pid in &parent_ids {
                    query = query.bind(pid);
                }
                let found = query
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(|e| format!("vcs create_tx parent check: {e}"))?;
                if found.len() != parent_ids.len() {
                    let _ = tx.rollback().await;
                    return Err(ERR_NO_PARENT.into());
                }
            }
            if !object_hashes.is_empty() {
                let in_list = in_placeholders(Dialect::Postgres, 1, object_hashes.len());
                let q = format!("SELECT hash FROM vcs_objects WHERE hash IN ({in_list})");
                let mut query = sqlx::query(sqlx::AssertSqlSafe(q));
                for h in &object_hashes {
                    query = query.bind(*h);
                }
                let found = query
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(|e| format!("vcs create_tx object check: {e}"))?;
                if found.len() != object_hashes.len() {
                    let _ = tx.rollback().await;
                    return Err(ERR_NO_OBJECT.into());
                }
            }
            let inserted = sqlx::query(
                "INSERT INTO vcs_changesets
                   (id, repo_id, parents, author, message, committed_at, manifest_hash, idem_key)
                 VALUES ($1, $2, $3, $4, $5, $6::timestamptz, $7, $8)
                 ON CONFLICT DO NOTHING",
            )
            .bind(changeset.id)
            .bind(changeset.repo_id)
            .bind(changeset.parents)
            .bind(changeset.author)
            .bind(changeset.message)
            .bind(changeset.committed_at)
            .bind(changeset.manifest_hash)
            .bind(changeset.idem_key)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("vcs create_tx changeset insert: {e}"))?
            .rows_affected();
            if inserted == 0 {
                // Unique-race loser: roll back (nothing written) and return the
                // winner's id — idempotent resubmit resolves to the same id.
                let _ = tx.rollback().await;
                return reselect_existing_changeset(
                    pool,
                    changeset.repo_id,
                    changeset.id,
                    changeset.idem_key,
                )
                .await?
                .ok_or_else(|| {
                    "vcs create_tx: insert suppressed but no existing row found".to_string()
                });
            }
            for fc in file_changes {
                sqlx::query(
                    "INSERT INTO vcs_file_changes
                       (changeset_id, path, kind, mode, old_path, blob_hash)
                     VALUES ($1, $2, $3, $4, $5, $6)",
                )
                .bind(changeset.id)
                .bind(fc.path)
                .bind(fc.kind)
                .bind(fc.mode)
                .bind(fc.old_path)
                .bind(fc.blob_hash)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("vcs create_tx file_change: {e}"))?;
            }
            for hash in object_links {
                sqlx::query(
                    "INSERT INTO vcs_object_links (repo_id, hash) VALUES ($1, $2)
                     ON CONFLICT DO NOTHING",
                )
                .bind(changeset.repo_id)
                .bind(hash)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("vcs create_tx object_link: {e}"))?;
            }
            for u in index_upserts {
                sqlx::query(
                    "INSERT INTO vcs_derived_index
                       (repo_id, kind, index_key, payload, covering_changeset)
                     VALUES ($1, $2, $3, $4, $5)
                     ON CONFLICT (repo_id, kind, index_key) DO UPDATE SET
                       payload = excluded.payload,
                       covering_changeset = excluded.covering_changeset,
                       updated_at = now()",
                )
                .bind(changeset.repo_id)
                .bind(u.kind)
                .bind(u.index_key)
                .bind(u.payload)
                .bind(u.covering_changeset)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("vcs create_tx index upsert: {e}"))?;
            }
            tx.commit()
                .await
                .map_err(|e| format!("vcs create_tx commit: {e}"))?;
            Ok(changeset.id.to_string())
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("vcs create_tx begin: {e}"))?;
            if let Some(idem) = changeset.idem_key {
                let existing: Option<String> = sqlx::query_scalar(
                    "SELECT id FROM vcs_changesets WHERE repo_id = ? AND idem_key = ?",
                )
                .bind(changeset.repo_id)
                .bind(idem)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| format!("vcs create_tx idem lookup: {e}"))?;
                if let Some(id) = existing {
                    let _ = tx.rollback().await;
                    return Ok(id);
                }
            }
            if !parent_ids.is_empty() {
                let in_list = in_placeholders(Dialect::MySql, 2, parent_ids.len());
                let q = format!(
                    "SELECT id FROM vcs_changesets WHERE repo_id = ? AND id IN ({in_list})"
                );
                let mut query = sqlx::query(sqlx::AssertSqlSafe(q)).bind(changeset.repo_id);
                for pid in &parent_ids {
                    query = query.bind(pid);
                }
                let found = query
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(|e| format!("vcs create_tx parent check: {e}"))?;
                if found.len() != parent_ids.len() {
                    let _ = tx.rollback().await;
                    return Err(ERR_NO_PARENT.into());
                }
            }
            if !object_hashes.is_empty() {
                let in_list = in_placeholders(Dialect::MySql, 1, object_hashes.len());
                let q = format!("SELECT hash FROM vcs_objects WHERE hash IN ({in_list})");
                let mut query = sqlx::query(sqlx::AssertSqlSafe(q));
                for h in &object_hashes {
                    query = query.bind(*h);
                }
                let found = query
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(|e| format!("vcs create_tx object check: {e}"))?;
                if found.len() != object_hashes.len() {
                    let _ = tx.rollback().await;
                    return Err(ERR_NO_OBJECT.into());
                }
            }
            let inserted = sqlx::query(
                "INSERT IGNORE INTO vcs_changesets
                   (id, repo_id, parents, author, message, committed_at, manifest_hash, idem_key)
                 VALUES (?, ?, ?, ?, ?,
                         STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s'),
                         ?, ?)",
            )
            .bind(changeset.id)
            .bind(changeset.repo_id)
            .bind(changeset.parents)
            .bind(changeset.author)
            .bind(changeset.message)
            .bind(changeset.committed_at)
            .bind(changeset.manifest_hash)
            .bind(changeset.idem_key)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("vcs create_tx changeset insert: {e}"))?
            .rows_affected();
            if inserted == 0 {
                let _ = tx.rollback().await;
                return reselect_existing_changeset(
                    pool,
                    changeset.repo_id,
                    changeset.id,
                    changeset.idem_key,
                )
                .await?
                .ok_or_else(|| {
                    "vcs create_tx: insert suppressed but no existing row found".to_string()
                });
            }
            for fc in file_changes {
                sqlx::query(
                    "INSERT INTO vcs_file_changes
                       (changeset_id, path, kind, mode, old_path, blob_hash)
                     VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(changeset.id)
                .bind(fc.path)
                .bind(fc.kind)
                .bind(fc.mode)
                .bind(fc.old_path)
                .bind(fc.blob_hash)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("vcs create_tx file_change: {e}"))?;
            }
            for hash in object_links {
                sqlx::query("INSERT IGNORE INTO vcs_object_links (repo_id, hash) VALUES (?, ?)")
                    .bind(changeset.repo_id)
                    .bind(hash)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| format!("vcs create_tx object_link: {e}"))?;
            }
            for u in index_upserts {
                sqlx::query(
                    "INSERT INTO vcs_derived_index
                       (repo_id, kind, index_key, payload, covering_changeset)
                     VALUES (?, ?, ?, ?, ?)
                     ON DUPLICATE KEY UPDATE
                       payload = VALUES(payload),
                       covering_changeset = VALUES(covering_changeset),
                       updated_at = CURRENT_TIMESTAMP(6)",
                )
                .bind(changeset.repo_id)
                .bind(u.kind)
                .bind(u.index_key)
                .bind(u.payload)
                .bind(u.covering_changeset)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("vcs create_tx index upsert: {e}"))?;
            }
            tx.commit()
                .await
                .map_err(|e| format!("vcs create_tx commit: {e}"))?;
            Ok(changeset.id.to_string())
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("vcs create_tx begin: {e}"))?;
            if let Some(idem) = changeset.idem_key {
                let existing: Option<String> = sqlx::query_scalar(
                    "SELECT id FROM vcs_changesets WHERE repo_id = ?1 AND idem_key = ?2",
                )
                .bind(changeset.repo_id)
                .bind(idem)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| format!("vcs create_tx idem lookup: {e}"))?;
                if let Some(id) = existing {
                    let _ = tx.rollback().await;
                    return Ok(id);
                }
            }
            if !parent_ids.is_empty() {
                let in_list = in_placeholders(Dialect::Sqlite, 2, parent_ids.len());
                let q = format!(
                    "SELECT id FROM vcs_changesets WHERE repo_id = ?1 AND id IN ({in_list})"
                );
                let mut query = sqlx::query(sqlx::AssertSqlSafe(q)).bind(changeset.repo_id);
                for pid in &parent_ids {
                    query = query.bind(pid);
                }
                let found = query
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(|e| format!("vcs create_tx parent check: {e}"))?;
                if found.len() != parent_ids.len() {
                    let _ = tx.rollback().await;
                    return Err(ERR_NO_PARENT.into());
                }
            }
            if !object_hashes.is_empty() {
                let in_list = in_placeholders(Dialect::Sqlite, 1, object_hashes.len());
                let q = format!("SELECT hash FROM vcs_objects WHERE hash IN ({in_list})");
                let mut query = sqlx::query(sqlx::AssertSqlSafe(q));
                for h in &object_hashes {
                    query = query.bind(*h);
                }
                let found = query
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(|e| format!("vcs create_tx object check: {e}"))?;
                if found.len() != object_hashes.len() {
                    let _ = tx.rollback().await;
                    return Err(ERR_NO_OBJECT.into());
                }
            }
            let inserted = sqlx::query(
                "INSERT OR IGNORE INTO vcs_changesets
                   (id, repo_id, parents, author, message, committed_at, manifest_hash, idem_key)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .bind(changeset.id)
            .bind(changeset.repo_id)
            .bind(changeset.parents)
            .bind(changeset.author)
            .bind(changeset.message)
            .bind(changeset.committed_at)
            .bind(changeset.manifest_hash)
            .bind(changeset.idem_key)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("vcs create_tx changeset insert: {e}"))?
            .rows_affected();
            if inserted == 0 {
                let _ = tx.rollback().await;
                return reselect_existing_changeset(
                    pool,
                    changeset.repo_id,
                    changeset.id,
                    changeset.idem_key,
                )
                .await?
                .ok_or_else(|| {
                    "vcs create_tx: insert suppressed but no existing row found".to_string()
                });
            }
            for fc in file_changes {
                sqlx::query(
                    "INSERT INTO vcs_file_changes
                       (changeset_id, path, kind, mode, old_path, blob_hash)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )
                .bind(changeset.id)
                .bind(fc.path)
                .bind(fc.kind)
                .bind(fc.mode)
                .bind(fc.old_path)
                .bind(fc.blob_hash)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("vcs create_tx file_change: {e}"))?;
            }
            for hash in object_links {
                sqlx::query(
                    "INSERT OR IGNORE INTO vcs_object_links (repo_id, hash) VALUES (?1, ?2)",
                )
                .bind(changeset.repo_id)
                .bind(hash)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("vcs create_tx object_link: {e}"))?;
            }
            for u in index_upserts {
                sqlx::query(
                    "INSERT INTO vcs_derived_index
                       (repo_id, kind, index_key, payload, covering_changeset)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(repo_id, kind, index_key) DO UPDATE SET
                       payload = excluded.payload,
                       covering_changeset = excluded.covering_changeset,
                       updated_at = strftime('%Y-%m-%d %H:%M:%S','now')",
                )
                .bind(changeset.repo_id)
                .bind(u.kind)
                .bind(u.index_key)
                .bind(u.payload)
                .bind(u.covering_changeset)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("vcs create_tx index upsert: {e}"))?;
            }
            tx.commit()
                .await
                .map_err(|e| format!("vcs create_tx commit: {e}"))?;
            Ok(changeset.id.to_string())
        }
    }
}

const CHANGESET_COLS_PG: &str = "id, repo_id, parents, author, message,
       to_char(committed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS committed_at,
       manifest_hash, idem_key,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at";

const CHANGESET_COLS_MYSQL: &str = "id, repo_id, parents, author, message,
       DATE_FORMAT(committed_at, '%Y-%m-%dT%H:%i:%sZ') AS committed_at,
       manifest_hash, idem_key,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at";

const CHANGESET_COLS_SQLITE: &str = "id, repo_id, parents, author, message,
       strftime('%Y-%m-%dT%H:%M:%SZ', committed_at) AS committed_at,
       manifest_hash, idem_key,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at";

pub async fn get_changeset(
    pool: &DbPool,
    repo_id: &str,
    id: &str,
) -> Result<Option<VcsChangesetRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {CHANGESET_COLS_PG} FROM vcs_changesets WHERE repo_id = $1 AND id = $2"
            )))
            .bind(repo_id)
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("vcs get_changeset: {e}"))?;
            row.map(|r| Ok(map_changeset!(r))).transpose()
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {CHANGESET_COLS_MYSQL} FROM vcs_changesets WHERE repo_id = ? AND id = ?"
            )))
            .bind(repo_id)
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("vcs get_changeset: {e}"))?;
            row.map(|r| Ok(map_changeset!(r))).transpose()
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {CHANGESET_COLS_SQLITE} FROM vcs_changesets WHERE repo_id = ?1 AND id = ?2"
            )))
            .bind(repo_id)
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("vcs get_changeset: {e}"))?;
            row.map(|r| Ok(map_changeset!(r))).transpose()
        }
    }
}

/// Parent ids decoded from the stored JSON array — empty when the changeset
/// is absent or parentless (use `vcs_get_changeset` for existence checks).
pub async fn changeset_parents(
    pool: &DbPool,
    repo_id: &str,
    id: &str,
) -> Result<Vec<String>, String> {
    let raw: Option<String> = match pool {
        DbPool::Postgres(p) => {
            sqlx::query_scalar("SELECT parents FROM vcs_changesets WHERE repo_id = $1 AND id = $2")
                .bind(repo_id)
                .bind(id)
                .fetch_optional(p)
                .await
        }
        DbPool::MySql(p) => {
            sqlx::query_scalar("SELECT parents FROM vcs_changesets WHERE repo_id = ? AND id = ?")
                .bind(repo_id)
                .bind(id)
                .fetch_optional(p)
                .await
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar("SELECT parents FROM vcs_changesets WHERE repo_id = ?1 AND id = ?2")
                .bind(repo_id)
                .bind(id)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|e| format!("vcs changeset_parents: {e}"))?;
    match raw {
        None => Ok(Vec::new()),
        Some(json) => {
            serde_json::from_str(&json).map_err(|e| format!("vcs changeset_parents decode: {e}"))
        }
    }
}

/// Idempotency-key lookup — the resubmit short-circuit (BR2.8).
pub async fn find_changeset_by_idem(
    pool: &DbPool,
    repo_id: &str,
    idem_key: &str,
) -> Result<Option<VcsChangesetRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {CHANGESET_COLS_PG} FROM vcs_changesets WHERE repo_id = $1 AND idem_key = $2"
            )))
            .bind(repo_id)
            .bind(idem_key)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("vcs find_changeset_by_idem: {e}"))?;
            row.map(|r| Ok(map_changeset!(r))).transpose()
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {CHANGESET_COLS_MYSQL} FROM vcs_changesets WHERE repo_id = ? AND idem_key = ?"
            )))
            .bind(repo_id)
            .bind(idem_key)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("vcs find_changeset_by_idem: {e}"))?;
            row.map(|r| Ok(map_changeset!(r))).transpose()
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {CHANGESET_COLS_SQLITE} FROM vcs_changesets WHERE repo_id = ?1 AND idem_key = ?2"
            )))
            .bind(repo_id)
            .bind(idem_key)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("vcs find_changeset_by_idem: {e}"))?;
            row.map(|r| Ok(map_changeset!(r))).transpose()
        }
    }
}

/// `vcs.status.changeset_count` (C1 pin).
pub async fn count_changesets(pool: &DbPool, repo_id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM vcs_changesets WHERE repo_id = $1")
                .bind(repo_id)
                .fetch_one(p)
                .await
                .map_err(|e| format!("vcs count_changesets: {e}"))
        }
        DbPool::MySql(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM vcs_changesets WHERE repo_id = ?")
                .bind(repo_id)
                .fetch_one(p)
                .await
                .map_err(|e| format!("vcs count_changesets: {e}"))
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM vcs_changesets WHERE repo_id = ?1")
                .bind(repo_id)
                .fetch_one(p)
                .await
                .map_err(|e| format!("vcs count_changesets: {e}"))
        }
    }
}

/// Manifest rows for one changeset (the `manifest` block of
/// `vcs.changeset_get`), path-ordered.
pub async fn list_file_changes(
    pool: &DbPool,
    changeset_id: &str,
) -> Result<Vec<VcsFileChangeRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                // mode is INT4; VcsFileChangeRow.mode is i64 → widen in-query
                // (sqlx-pg refuses implicit INT4→INT8 decode).
                "SELECT changeset_id, path, kind, mode::bigint AS mode, old_path, blob_hash
                 FROM vcs_file_changes WHERE changeset_id = $1 ORDER BY path",
            )
            .bind(changeset_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("vcs list_file_changes: {e}"))?;
            rows.iter().map(|r| Ok(map_file_change!(r))).collect()
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT changeset_id, path, kind, mode, old_path, blob_hash
                 FROM vcs_file_changes WHERE changeset_id = ? ORDER BY path",
            )
            .bind(changeset_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("vcs list_file_changes: {e}"))?;
            rows.iter().map(|r| Ok(map_file_change!(r))).collect()
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT changeset_id, path, kind, mode, old_path, blob_hash
                 FROM vcs_file_changes WHERE changeset_id = ?1 ORDER BY path",
            )
            .bind(changeset_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("vcs list_file_changes: {e}"))?;
            rows.iter().map(|r| Ok(map_file_change!(r))).collect()
        }
    }
}

// ---------- refs (CAS-only writes) ----------

/// `vcs.ref_list` page: bare names ascending, keyset cursor on `name`
/// (`cursor` = exclusive lower bound). `limit` clamped to 1..=500.
pub async fn list_refs(
    pool: &DbPool,
    repo_id: &str,
    cursor: Option<&str>,
    limit: i64,
) -> Result<Vec<VcsRefRow>, String> {
    let limit = limit.clamp(1, 500);
    match pool {
        DbPool::Postgres(p) => {
            let cond = if cursor.is_some() {
                "AND name > $2"
            } else {
                ""
            };
            let limit_ph = if cursor.is_some() { "$3" } else { "$2" };
            let q = format!(
                "SELECT repo_id, name, changeset_id,
                        to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at
                 FROM vcs_refs WHERE repo_id = $1 {cond} ORDER BY name LIMIT {limit_ph}"
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(q)).bind(repo_id);
            if let Some(c) = cursor {
                query = query.bind(c);
            }
            let rows = query
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("vcs list_refs: {e}"))?;
            rows.iter().map(|r| Ok(map_ref!(r))).collect()
        }
        DbPool::MySql(p) => {
            let cond = if cursor.is_some() { "AND name > ?" } else { "" };
            let q = format!(
                "SELECT repo_id, name, changeset_id,
                        DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
                 FROM vcs_refs WHERE repo_id = ? {cond} ORDER BY name LIMIT ?"
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(q)).bind(repo_id);
            if let Some(c) = cursor {
                query = query.bind(c);
            }
            let rows = query
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("vcs list_refs: {e}"))?;
            rows.iter().map(|r| Ok(map_ref!(r))).collect()
        }
        DbPool::Sqlite(p) => {
            let (cond, limit_ph) = if cursor.is_some() {
                ("AND name > ?2", "?3")
            } else {
                ("", "?2")
            };
            let q = format!(
                "SELECT repo_id, name, changeset_id,
                        strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at
                 FROM vcs_refs WHERE repo_id = ?1 {cond} ORDER BY name LIMIT {limit_ph}"
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(q)).bind(repo_id);
            if let Some(c) = cursor {
                query = query.bind(c);
            }
            let rows = query
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("vcs list_refs: {e}"))?;
            rows.iter().map(|r| Ok(map_ref!(r))).collect()
        }
    }
}

/// Strict CAS on the one mutable entity (BR2.7, WF3). `expected_old`/`new` are
/// changeset ids or `None`; null-safe equality per dialect. `new = None` is
/// the delete path only (row DELETE — absent refs, never tombstones);
/// `expected_old = None` + absent row is the create path.
pub async fn cas_ref_update(
    pool: &DbPool,
    repo_id: &str,
    name: &str,
    expected_old: Option<&str>,
    new: Option<&str>,
) -> Result<CasOutcome, String> {
    match pool {
        DbPool::Postgres(p) => cas_ref_update_pg(p, repo_id, name, expected_old, new).await,
        DbPool::MySql(p) => cas_ref_update_mysql(p, repo_id, name, expected_old, new).await,
        DbPool::Sqlite(p) => cas_ref_update_sqlite(p, repo_id, name, expected_old, new).await,
    }
}

const REF_RETURNING_PG: &str = "RETURNING repo_id, name, changeset_id,
       to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at";

const REF_RETURNING_SQLITE: &str = "RETURNING repo_id, name, changeset_id,
       strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at";

async fn cas_ref_update_pg(
    p: &sqlx::PgPool,
    repo_id: &str,
    name: &str,
    expected_old: Option<&str>,
    new: Option<&str>,
) -> Result<CasOutcome, String> {
    match new {
        Some(n) => {
            // Update path: single statement, null-safe equality.
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE vcs_refs SET changeset_id = $3, updated_at = now()
                 WHERE repo_id = $1 AND name = $2
                   AND changeset_id IS NOT DISTINCT FROM $4
                 {REF_RETURNING_PG}"
            )))
            .bind(repo_id)
            .bind(name)
            .bind(n)
            .bind(expected_old)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("vcs cas_ref update: {e}"))?;
            if let Some(r) = row {
                return Ok(CasOutcome::Applied(map_ref!(r)));
            }
            if expected_old.is_none() {
                // Create-only path: insert when absent; on an existing row the
                // WHERE only applies when it still carries NULL (null-safe
                // equality with the NULL expectation).
                let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                    "INSERT INTO vcs_refs (repo_id, name, changeset_id)
                     VALUES ($1, $2, $3)
                     ON CONFLICT (repo_id, name) DO UPDATE SET
                       changeset_id = excluded.changeset_id,
                       updated_at = now()
                     WHERE vcs_refs.changeset_id IS NULL
                     {REF_RETURNING_PG}"
                )))
                .bind(repo_id)
                .bind(name)
                .bind(n)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("vcs cas_ref create: {e}"))?;
                if let Some(r) = row {
                    return Ok(CasOutcome::Applied(map_ref!(r)));
                }
            }
            let actual = ref_actual_pg(p, repo_id, name).await?;
            Ok(CasOutcome::Conflict { actual })
        }
        None => {
            // Delete path only — never inserts a NULL-changeset row.
            let row = sqlx::query(
                "DELETE FROM vcs_refs
                 WHERE repo_id = $1 AND name = $2
                   AND changeset_id IS NOT DISTINCT FROM $3
                 RETURNING to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at",
            )
            .bind(repo_id)
            .bind(name)
            .bind(expected_old)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("vcs cas_ref delete: {e}"))?;
            if let Some(r) = row {
                return Ok(CasOutcome::Applied(VcsRefRow {
                    repo_id: repo_id.to_string(),
                    name: name.to_string(),
                    changeset_id: None,
                    updated_at: r
                        .try_get("updated_at")
                        .map_err(|e| format!("vcs ref row: {e}"))?,
                }));
            }
            let actual = ref_actual_pg(p, repo_id, name).await?;
            Ok(CasOutcome::Conflict { actual })
        }
    }
}

async fn ref_actual_pg(
    p: &sqlx::PgPool,
    repo_id: &str,
    name: &str,
) -> Result<Option<String>, String> {
    let row: Option<Option<String>> = sqlx::query_scalar::<_, Option<String>>(
        "SELECT changeset_id FROM vcs_refs WHERE repo_id = $1 AND name = $2",
    )
    .bind(repo_id)
    .bind(name)
    .fetch_optional(p)
    .await
    .map_err(|e| format!("vcs cas_ref actual: {e}"))?;
    Ok(row.flatten())
}

async fn cas_ref_update_sqlite(
    p: &sqlx::SqlitePool,
    repo_id: &str,
    name: &str,
    expected_old: Option<&str>,
    new: Option<&str>,
) -> Result<CasOutcome, String> {
    match new {
        Some(n) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE vcs_refs SET changeset_id = ?3,
                        updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
                 WHERE repo_id = ?1 AND name = ?2
                   AND changeset_id IS NOT DISTINCT FROM ?4
                 {REF_RETURNING_SQLITE}"
            )))
            .bind(repo_id)
            .bind(name)
            .bind(n)
            .bind(expected_old)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("vcs cas_ref update: {e}"))?;
            if let Some(r) = row {
                return Ok(CasOutcome::Applied(map_ref!(r)));
            }
            if expected_old.is_none() {
                let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                    "INSERT INTO vcs_refs (repo_id, name, changeset_id)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(repo_id, name) DO UPDATE SET
                       changeset_id = excluded.changeset_id,
                       updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
                     WHERE vcs_refs.changeset_id IS NULL
                     {REF_RETURNING_SQLITE}"
                )))
                .bind(repo_id)
                .bind(name)
                .bind(n)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("vcs cas_ref create: {e}"))?;
                if let Some(r) = row {
                    return Ok(CasOutcome::Applied(map_ref!(r)));
                }
            }
            let actual = ref_actual_sqlite(p, repo_id, name).await?;
            Ok(CasOutcome::Conflict { actual })
        }
        None => {
            let row = sqlx::query(
                "DELETE FROM vcs_refs
                 WHERE repo_id = ?1 AND name = ?2
                   AND changeset_id IS NOT DISTINCT FROM ?3
                 RETURNING strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at",
            )
            .bind(repo_id)
            .bind(name)
            .bind(expected_old)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("vcs cas_ref delete: {e}"))?;
            if let Some(r) = row {
                return Ok(CasOutcome::Applied(VcsRefRow {
                    repo_id: repo_id.to_string(),
                    name: name.to_string(),
                    changeset_id: None,
                    updated_at: r
                        .try_get("updated_at")
                        .map_err(|e| format!("vcs ref row: {e}"))?,
                }));
            }
            let actual = ref_actual_sqlite(p, repo_id, name).await?;
            Ok(CasOutcome::Conflict { actual })
        }
    }
}

async fn ref_actual_sqlite(
    p: &sqlx::SqlitePool,
    repo_id: &str,
    name: &str,
) -> Result<Option<String>, String> {
    let row: Option<Option<String>> = sqlx::query_scalar::<_, Option<String>>(
        "SELECT changeset_id FROM vcs_refs WHERE repo_id = ?1 AND name = ?2",
    )
    .bind(repo_id)
    .bind(name)
    .fetch_optional(p)
    .await
    .map_err(|e| format!("vcs cas_ref actual: {e}"))?;
    Ok(row.flatten())
}

/// MySQL has no `RETURNING`, but `<=>` is its null-safe equality — the CAS
/// runs as the same single-statement conditional writes as pg/sqlite. The
/// previous `SELECT … FOR UPDATE` guard took a next-key/gap lock when the
/// ref was absent, spanning neighbouring index space and deadlocking
/// concurrent first-creates across repos (error 1213).
async fn cas_ref_update_mysql(
    p: &sqlx::MySqlPool,
    repo_id: &str,
    name: &str,
    expected_old: Option<&str>,
    new: Option<&str>,
) -> Result<CasOutcome, String> {
    match new {
        Some(n) => {
            let res = sqlx::query(
                "UPDATE vcs_refs SET changeset_id = ?, updated_at = CURRENT_TIMESTAMP(6)
                 WHERE repo_id = ? AND name = ? AND changeset_id <=> ?",
            )
            .bind(n)
            .bind(repo_id)
            .bind(name)
            .bind(expected_old)
            .execute(p)
            .await
            .map_err(|e| format!("vcs cas_ref update: {e}"))?;
            if res.rows_affected() == 1 {
                return Ok(CasOutcome::Applied(
                    ref_row_mysql(p, repo_id, name).await?,
                ));
            }
            if expected_old.is_none() {
                // Create path: the unique key arbitrates the insert — a
                // duplicate means a head already landed and becomes the
                // conflict actual (pg/sqlite's ON CONFLICT shape).
                let ins = sqlx::query(
                    "INSERT INTO vcs_refs (repo_id, name, changeset_id) VALUES (?, ?, ?)",
                )
                .bind(repo_id)
                .bind(name)
                .bind(n)
                .execute(p)
                .await;
                if let Err(e) = ins {
                    let actual = ref_actual_mysql(p, repo_id, name).await?;
                    return match actual {
                        Some(_) => Ok(CasOutcome::Conflict { actual }),
                        None => Err(format!("vcs cas_ref create: {e}")),
                    };
                }
                return Ok(CasOutcome::Applied(
                    ref_row_mysql(p, repo_id, name).await?,
                ));
            }
            let actual = ref_actual_mysql(p, repo_id, name).await?;
            Ok(CasOutcome::Conflict { actual })
        }
        None => {
            // No RETURNING: pre-read updated_at only for the Applied row —
            // the delete is guarded by the same `<=>` predicate, not this read.
            let updated_at: Option<String> = sqlx::query_scalar(
                "SELECT DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ')
                 FROM vcs_refs WHERE repo_id = ? AND name = ?",
            )
            .bind(repo_id)
            .bind(name)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("vcs cas_ref read: {e}"))?;
            let res = sqlx::query(
                "DELETE FROM vcs_refs
                 WHERE repo_id = ? AND name = ? AND changeset_id <=> ?",
            )
            .bind(repo_id)
            .bind(name)
            .bind(expected_old)
            .execute(p)
            .await
            .map_err(|e| format!("vcs cas_ref delete: {e}"))?;
            if res.rows_affected() == 1 {
                return Ok(CasOutcome::Applied(VcsRefRow {
                    repo_id: repo_id.to_string(),
                    name: name.to_string(),
                    changeset_id: None,
                    updated_at: updated_at.unwrap_or_default(),
                }));
            }
            let actual = ref_actual_mysql(p, repo_id, name).await?;
            Ok(CasOutcome::Conflict { actual })
        }
    }
}

async fn ref_actual_mysql(
    p: &sqlx::MySqlPool,
    repo_id: &str,
    name: &str,
) -> Result<Option<String>, String> {
    let row: Option<Option<String>> = sqlx::query_scalar::<_, Option<String>>(
        "SELECT changeset_id FROM vcs_refs WHERE repo_id = ? AND name = ?",
    )
    .bind(repo_id)
    .bind(name)
    .fetch_optional(p)
    .await
    .map_err(|e| format!("vcs cas_ref actual: {e}"))?;
    Ok(row.flatten())
}

async fn ref_row_mysql(
    p: &sqlx::MySqlPool,
    repo_id: &str,
    name: &str,
) -> Result<VcsRefRow, String> {
    let row = sqlx::query(
        "SELECT repo_id, name, changeset_id,
                DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
         FROM vcs_refs WHERE repo_id = ? AND name = ?",
    )
    .bind(repo_id)
    .bind(name)
    .fetch_one(p)
    .await
    .map_err(|e| format!("vcs cas_ref reread: {e}"))?;
    Ok(map_ref!(row))
}

/// Unconditional ref delete — returns whether a row was removed. CAS-guarded
/// deletes go through `cas_ref_update(.., expected, None)`.
pub async fn delete_ref(pool: &DbPool, repo_id: &str, name: &str) -> Result<bool, String> {
    let affected = match pool {
        DbPool::Postgres(p) => sqlx::query("DELETE FROM vcs_refs WHERE repo_id = $1 AND name = $2")
            .bind(repo_id)
            .bind(name)
            .execute(p)
            .await
            .map_err(|e| format!("vcs delete_ref: {e}"))?
            .rows_affected(),
        DbPool::MySql(p) => sqlx::query("DELETE FROM vcs_refs WHERE repo_id = ? AND name = ?")
            .bind(repo_id)
            .bind(name)
            .execute(p)
            .await
            .map_err(|e| format!("vcs delete_ref: {e}"))?
            .rows_affected(),
        DbPool::Sqlite(p) => sqlx::query("DELETE FROM vcs_refs WHERE repo_id = ?1 AND name = ?2")
            .bind(repo_id)
            .bind(name)
            .execute(p)
            .await
            .map_err(|e| format!("vcs delete_ref: {e}"))?
            .rows_affected(),
    };
    Ok(affected > 0)
}

// ---------- derived index ----------

/// Merge-safe upserts on `(repo_id, kind, index_key)` — concurrent same-parent
/// writers must not collide (BR2.9, AC2.3.1). Runs in one tx.
pub async fn upsert_derived_index_rows(
    pool: &DbPool,
    repo_id: &str,
    rows: &[VcsDerivedIndexUpsert<'_>],
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("vcs index_upsert begin: {e}"))?;
            for u in rows {
                sqlx::query(
                    "INSERT INTO vcs_derived_index
                       (repo_id, kind, index_key, payload, covering_changeset)
                     VALUES ($1, $2, $3, $4, $5)
                     ON CONFLICT (repo_id, kind, index_key) DO UPDATE SET
                       payload = excluded.payload,
                       covering_changeset = excluded.covering_changeset,
                       updated_at = now()",
                )
                .bind(repo_id)
                .bind(u.kind)
                .bind(u.index_key)
                .bind(u.payload)
                .bind(u.covering_changeset)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("vcs index_upsert: {e}"))?;
            }
            tx.commit()
                .await
                .map_err(|e| format!("vcs index_upsert commit: {e}"))?;
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("vcs index_upsert begin: {e}"))?;
            for u in rows {
                sqlx::query(
                    "INSERT INTO vcs_derived_index
                       (repo_id, kind, index_key, payload, covering_changeset)
                     VALUES (?, ?, ?, ?, ?)
                     ON DUPLICATE KEY UPDATE
                       payload = VALUES(payload),
                       covering_changeset = VALUES(covering_changeset),
                       updated_at = CURRENT_TIMESTAMP(6)",
                )
                .bind(repo_id)
                .bind(u.kind)
                .bind(u.index_key)
                .bind(u.payload)
                .bind(u.covering_changeset)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("vcs index_upsert: {e}"))?;
            }
            tx.commit()
                .await
                .map_err(|e| format!("vcs index_upsert commit: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("vcs index_upsert begin: {e}"))?;
            for u in rows {
                sqlx::query(
                    "INSERT INTO vcs_derived_index
                       (repo_id, kind, index_key, payload, covering_changeset)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(repo_id, kind, index_key) DO UPDATE SET
                       payload = excluded.payload,
                       covering_changeset = excluded.covering_changeset,
                       updated_at = strftime('%Y-%m-%d %H:%M:%S','now')",
                )
                .bind(repo_id)
                .bind(u.kind)
                .bind(u.index_key)
                .bind(u.payload)
                .bind(u.covering_changeset)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("vcs index_upsert: {e}"))?;
            }
            tx.commit()
                .await
                .map_err(|e| format!("vcs index_upsert commit: {e}"))?;
        }
    }
    Ok(())
}

/// Paginated read: `kind` equality + optional `index_key` equality + keyset
/// `cursor` (exclusive lower bound on `index_key`). Ordered by `index_key`;
/// `limit` clamped to 1..=500.
pub async fn query_derived_index(
    pool: &DbPool,
    repo_id: &str,
    kind: &str,
    index_key: Option<&str>,
    cursor: Option<&str>,
    limit: i64,
) -> Result<Vec<VcsDerivedIndexRow>, String> {
    let limit = limit.clamp(1, 500);
    match pool {
        DbPool::Postgres(p) => {
            let mut n = 3usize;
            let mut cond = String::new();
            if index_key.is_some() {
                cond.push_str(&format!("AND index_key = ${n} "));
                n += 1;
            }
            if cursor.is_some() {
                cond.push_str(&format!("AND index_key > ${n} "));
                n += 1;
            }
            let q = format!(
                "SELECT repo_id, kind, index_key, payload, covering_changeset,
                        to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at
                 FROM vcs_derived_index
                 WHERE repo_id = $1 AND kind = $2 {cond}
                 ORDER BY index_key LIMIT ${n}"
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(q)).bind(repo_id).bind(kind);
            if let Some(k) = index_key {
                query = query.bind(k);
            }
            if let Some(c) = cursor {
                query = query.bind(c);
            }
            let rows = query
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("vcs query_derived_index: {e}"))?;
            rows.iter().map(|r| Ok(map_index_row!(r))).collect()
        }
        DbPool::MySql(p) => {
            let mut cond = String::new();
            if index_key.is_some() {
                cond.push_str("AND index_key = ? ");
            }
            if cursor.is_some() {
                cond.push_str("AND index_key > ? ");
            }
            let q = format!(
                "SELECT repo_id, kind, index_key, payload, covering_changeset,
                        DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
                 FROM vcs_derived_index
                 WHERE repo_id = ? AND kind = ? {cond}
                 ORDER BY index_key LIMIT ?"
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(q)).bind(repo_id).bind(kind);
            if let Some(k) = index_key {
                query = query.bind(k);
            }
            if let Some(c) = cursor {
                query = query.bind(c);
            }
            let rows = query
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("vcs query_derived_index: {e}"))?;
            rows.iter().map(|r| Ok(map_index_row!(r))).collect()
        }
        DbPool::Sqlite(p) => {
            let mut n = 3usize;
            let mut cond = String::new();
            if index_key.is_some() {
                cond.push_str(&format!("AND index_key = ?{n} "));
                n += 1;
            }
            if cursor.is_some() {
                cond.push_str(&format!("AND index_key > ?{n} "));
                n += 1;
            }
            let q = format!(
                "SELECT repo_id, kind, index_key, payload, covering_changeset,
                        strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at
                 FROM vcs_derived_index
                 WHERE repo_id = ?1 AND kind = ?2 {cond}
                 ORDER BY index_key LIMIT ?{n}"
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(q)).bind(repo_id).bind(kind);
            if let Some(k) = index_key {
                query = query.bind(k);
            }
            if let Some(c) = cursor {
                query = query.bind(c);
            }
            let rows = query
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("vcs query_derived_index: {e}"))?;
            rows.iter().map(|r| Ok(map_index_row!(r))).collect()
        }
    }
}

// ---------- import links ----------

/// Insert-if-absent — an import crash-retry must not violate the PK (SD2.4).
/// Returns `true` when the row was inserted.
pub async fn insert_import_link_if_absent(
    pool: &DbPool,
    repo_id: &str,
    git_oid: &str,
    changeset_id: &str,
) -> Result<bool, String> {
    let affected = match pool {
        DbPool::Postgres(p) => sqlx::query(
            "INSERT INTO vcs_import_links (repo_id, git_oid, changeset_id)
                 VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(repo_id)
        .bind(git_oid)
        .bind(changeset_id)
        .execute(p)
        .await
        .map_err(|e| format!("vcs insert_import_link: {e}"))?
        .rows_affected(),
        DbPool::MySql(p) => sqlx::query(
            "INSERT IGNORE INTO vcs_import_links (repo_id, git_oid, changeset_id)
                 VALUES (?, ?, ?)",
        )
        .bind(repo_id)
        .bind(git_oid)
        .bind(changeset_id)
        .execute(p)
        .await
        .map_err(|e| format!("vcs insert_import_link: {e}"))?
        .rows_affected(),
        DbPool::Sqlite(p) => sqlx::query(
            "INSERT OR IGNORE INTO vcs_import_links (repo_id, git_oid, changeset_id)
                 VALUES (?1, ?2, ?3)",
        )
        .bind(repo_id)
        .bind(git_oid)
        .bind(changeset_id)
        .execute(p)
        .await
        .map_err(|e| format!("vcs insert_import_link: {e}"))?
        .rows_affected(),
    };
    Ok(affected > 0)
}

/// `git_oid → changeset_id` lookup.
pub async fn lookup_import_link(
    pool: &DbPool,
    repo_id: &str,
    git_oid: &str,
) -> Result<Option<String>, String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT changeset_id FROM vcs_import_links WHERE repo_id = $1 AND git_oid = $2",
        )
        .bind(repo_id)
        .bind(git_oid)
        .fetch_optional(p)
        .await
        .map_err(|e| format!("vcs lookup_import_link: {e}")),
        DbPool::MySql(p) => sqlx::query_scalar(
            "SELECT changeset_id FROM vcs_import_links WHERE repo_id = ? AND git_oid = ?",
        )
        .bind(repo_id)
        .bind(git_oid)
        .fetch_optional(p)
        .await
        .map_err(|e| format!("vcs lookup_import_link: {e}")),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT changeset_id FROM vcs_import_links WHERE repo_id = ?1 AND git_oid = ?2",
        )
        .bind(repo_id)
        .bind(git_oid)
        .fetch_optional(p)
        .await
        .map_err(|e| format!("vcs lookup_import_link: {e}")),
    }
}

/// Keyset-paginated link page for export walks — `git_oid` ascending,
/// `cursor` = exclusive lower bound; `limit` clamped to 1..=500.
pub async fn import_link_page(
    pool: &DbPool,
    repo_id: &str,
    cursor: Option<&str>,
    limit: i64,
) -> Result<Vec<VcsImportLinkRow>, String> {
    let limit = limit.clamp(1, 500);
    match pool {
        DbPool::Postgres(p) => {
            let (cond, limit_ph) = if cursor.is_some() {
                ("AND git_oid > $2", "$3")
            } else {
                ("", "$2")
            };
            let q = format!(
                "SELECT repo_id, git_oid, changeset_id,
                        to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
                 FROM vcs_import_links
                 WHERE repo_id = $1 {cond} ORDER BY git_oid LIMIT {limit_ph}"
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(q)).bind(repo_id);
            if let Some(c) = cursor {
                query = query.bind(c);
            }
            let rows = query
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("vcs import_link_page: {e}"))?;
            rows.iter().map(|r| Ok(map_import_link!(r))).collect()
        }
        DbPool::MySql(p) => {
            let cond = if cursor.is_some() {
                "AND git_oid > ?"
            } else {
                ""
            };
            let q = format!(
                "SELECT repo_id, git_oid, changeset_id,
                        DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
                 FROM vcs_import_links
                 WHERE repo_id = ? {cond} ORDER BY git_oid LIMIT ?"
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(q)).bind(repo_id);
            if let Some(c) = cursor {
                query = query.bind(c);
            }
            let rows = query
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("vcs import_link_page: {e}"))?;
            rows.iter().map(|r| Ok(map_import_link!(r))).collect()
        }
        DbPool::Sqlite(p) => {
            let (cond, limit_ph) = if cursor.is_some() {
                ("AND git_oid > ?2", "?3")
            } else {
                ("", "?2")
            };
            let q = format!(
                "SELECT repo_id, git_oid, changeset_id,
                        strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at
                 FROM vcs_import_links
                 WHERE repo_id = ?1 {cond} ORDER BY git_oid LIMIT {limit_ph}"
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(q)).bind(repo_id);
            if let Some(c) = cursor {
                query = query.bind(c);
            }
            let rows = query
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("vcs import_link_page: {e}"))?;
            rows.iter().map(|r| Ok(map_import_link!(r))).collect()
        }
    }
}

// ---------- Database façade ----------

impl Database {
    /// `None` = never-enabled; `Some(enabled:false)` = admin-disabled.
    pub async fn vcs_get_repo_surface_flag(
        &self,
        repo_id: &str,
    ) -> Result<Option<VcsRepoSurfaceFlagRow>, String> {
        get_repo_surface_flag(self.require_pool()?, repo_id).await
    }

    pub async fn vcs_upsert_repo_surface_flag(
        &self,
        repo_id: &str,
        enabled: bool,
        enabled_by: Option<&str>,
    ) -> Result<(), String> {
        upsert_repo_surface_flag(self.require_pool()?, repo_id, enabled, enabled_by).await
    }

    pub async fn vcs_get_object(&self, hash: &str) -> Result<Option<VcsObjectRow>, String> {
        get_object(self.require_pool()?, hash).await
    }

    /// `true` = stored, `false` = dedup (u3's `Stored | Dedup`).
    pub async fn vcs_insert_object_if_absent(&self, hash: &str, size: i64) -> Result<bool, String> {
        insert_object_if_absent(self.require_pool()?, hash, size).await
    }

    /// Global objects with no live links created before `grace_ts` (GC scan).
    pub async fn vcs_list_orphaned_objects_before(
        &self,
        grace_ts: &str,
    ) -> Result<Vec<VcsObjectRow>, String> {
        list_orphaned_objects_before(self.require_pool()?, grace_ts).await
    }

    pub async fn vcs_insert_object_link_if_absent(
        &self,
        repo_id: &str,
        hash: &str,
    ) -> Result<bool, String> {
        insert_object_link_if_absent(self.require_pool()?, repo_id, hash).await
    }

    /// `true` = a `(repo_id, hash)` link exists (u3 quota membership probe).
    pub async fn vcs_get_object_link(&self, repo_id: &str, hash: &str) -> Result<bool, String> {
        get_object_link(self.require_pool()?, repo_id, hash).await
    }

    /// Conditional object delete — `true` = row gone (caller may unlink the
    /// file); `false` = still referenced by a link or a manifest `blob_hash`.
    pub async fn vcs_delete_object_if_unlinked(&self, hash: &str) -> Result<bool, String> {
        delete_object_if_unlinked(self.require_pool()?, hash).await
    }

    /// `{object_count, storage_bytes}` for `vcs.status`.
    pub async fn vcs_repo_object_counts(
        &self,
        repo_id: &str,
    ) -> Result<VcsRepoObjectCounts, String> {
        repo_object_counts(self.require_pool()?, repo_id).await
    }

    pub async fn vcs_live_object_hashes(&self, repo_id: &str) -> Result<Vec<String>, String> {
        live_object_hashes(self.require_pool()?, repo_id).await
    }

    /// Atomic changeset write — returns the (new or idempotency-resolved) id.
    /// Fails closed on missing parents (`vcs.not_found:parent`), missing
    /// objects (`vcs.not_found:object`), malformed parents JSON
    /// (`vcs.invalid_input:parents`), or a non-delete change without
    /// `blob_hash` (`vcs.invalid_input:file_change`).
    pub async fn vcs_create_changeset_tx(
        &self,
        changeset: &VcsChangesetInsert<'_>,
        file_changes: &[VcsFileChangeInsert<'_>],
        object_links: &[&str],
        index_upserts: &[VcsDerivedIndexUpsert<'_>],
    ) -> Result<String, String> {
        create_changeset_tx(
            self.require_pool()?,
            changeset,
            file_changes,
            object_links,
            index_upserts,
        )
        .await
    }

    pub async fn vcs_get_changeset(
        &self,
        repo_id: &str,
        id: &str,
    ) -> Result<Option<VcsChangesetRow>, String> {
        get_changeset(self.require_pool()?, repo_id, id).await
    }

    /// Parent ids decoded from the stored JSON (empty on absent/parentless).
    pub async fn vcs_changeset_parents(
        &self,
        repo_id: &str,
        id: &str,
    ) -> Result<Vec<String>, String> {
        changeset_parents(self.require_pool()?, repo_id, id).await
    }

    pub async fn vcs_find_changeset_by_idem(
        &self,
        repo_id: &str,
        idem_key: &str,
    ) -> Result<Option<VcsChangesetRow>, String> {
        find_changeset_by_idem(self.require_pool()?, repo_id, idem_key).await
    }

    /// `vcs.status.changeset_count`.
    pub async fn vcs_count_changesets(&self, repo_id: &str) -> Result<i64, String> {
        count_changesets(self.require_pool()?, repo_id).await
    }

    /// Manifest rows of one changeset (`vcs.changeset_get.manifest`), path-ordered.
    pub async fn vcs_list_file_changes(
        &self,
        changeset_id: &str,
    ) -> Result<Vec<VcsFileChangeRow>, String> {
        list_file_changes(self.require_pool()?, changeset_id).await
    }

    /// Keyset page of bare-named refs (`vcs.ref_list`); `limit` → 1..=500.
    pub async fn vcs_list_refs(
        &self,
        repo_id: &str,
        cursor: Option<&str>,
        limit: i64,
    ) -> Result<Vec<VcsRefRow>, String> {
        list_refs(self.require_pool()?, repo_id, cursor, limit).await
    }

    /// Strict ref CAS — `Applied(row)` / `Conflict{actual}` (typed; the API
    /// maps `Conflict` to `vcs.ref_conflict{data:{actual}}`).
    /// `new = None` ⇒ delete path only; `expected_old = None` ⇒ create-only.
    pub async fn vcs_cas_ref_update(
        &self,
        repo_id: &str,
        name: &str,
        expected_old: Option<&str>,
        new: Option<&str>,
    ) -> Result<CasOutcome, String> {
        cas_ref_update(self.require_pool()?, repo_id, name, expected_old, new).await
    }

    /// Unconditional ref delete (CAS-guarded deletes use `vcs_cas_ref_update`).
    pub async fn vcs_delete_ref(&self, repo_id: &str, name: &str) -> Result<bool, String> {
        delete_ref(self.require_pool()?, repo_id, name).await
    }

    /// Merge-safe derived-index upserts (concurrent same-parent safe).
    pub async fn vcs_upsert_derived_index_rows(
        &self,
        repo_id: &str,
        rows: &[VcsDerivedIndexUpsert<'_>],
    ) -> Result<(), String> {
        upsert_derived_index_rows(self.require_pool()?, repo_id, rows).await
    }

    /// `kind` + optional `index_key` equality, keyset `cursor`, `limit` → 1..=500.
    pub async fn vcs_query_derived_index(
        &self,
        repo_id: &str,
        kind: &str,
        index_key: Option<&str>,
        cursor: Option<&str>,
        limit: i64,
    ) -> Result<Vec<VcsDerivedIndexRow>, String> {
        query_derived_index(
            self.require_pool()?,
            repo_id,
            kind,
            index_key,
            cursor,
            limit,
        )
        .await
    }

    /// Import link insert-if-absent — `true` when inserted.
    pub async fn vcs_insert_import_link_if_absent(
        &self,
        repo_id: &str,
        git_oid: &str,
        changeset_id: &str,
    ) -> Result<bool, String> {
        insert_import_link_if_absent(self.require_pool()?, repo_id, git_oid, changeset_id).await
    }

    /// `git_oid → changeset_id`.
    pub async fn vcs_lookup_import_link(
        &self,
        repo_id: &str,
        git_oid: &str,
    ) -> Result<Option<String>, String> {
        lookup_import_link(self.require_pool()?, repo_id, git_oid).await
    }

    /// Keyset page of import links for export walks; `limit` → 1..=500.
    pub async fn vcs_import_link_page(
        &self,
        repo_id: &str,
        cursor: Option<&str>,
        limit: i64,
    ) -> Result<Vec<VcsImportLinkRow>, String> {
        import_link_page(self.require_pool()?, repo_id, cursor, limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CS_A: &str = "cs_01H8V4JGZ0K2B9Q7X3F5M6NPQR";
    const CS_B: &str = "cs_01H8V4JGZ0K2B9Q7X3F5M6NPQS";
    const CS_C: &str = "cs_01H8V4JGZ0K2B9Q7X3F5M6NPQT";
    const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const HASH_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    const OID_A: &str = "dddddddddddddddddddddddddddddddddddddddd";
    const OID_B: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";

    async fn test_db() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite:{}", dir.path().join("vcs.db").display());
        let db = Database::connect(&url).await.unwrap();
        db.migrate().await.unwrap();
        (dir, db)
    }

    async fn seed_repo(db: &Database) -> (String, String) {
        let uid = "user-1".to_string();
        db.create_user(
            &uid,
            "vcs-u1@example.com",
            "vcsu1",
            None,
            "Vcs U1",
            "",
            None,
            oxidean_core::Role::User,
        )
        .await
        .unwrap();
        let rid = "repo-1".to_string();
        db.insert_repository(&rid, &uid, "user", "vcsrepo", "private", "", "main")
            .await
            .unwrap();
        (uid, rid)
    }

    fn cs<'a>(
        id: &'a str,
        repo_id: &'a str,
        parents: &'a str,
        idem: Option<&'a str>,
    ) -> VcsChangesetInsert<'a> {
        VcsChangesetInsert {
            id,
            repo_id,
            parents,
            author: "Vcs U1 <vcs-u1@example.com>",
            message: "test changeset",
            committed_at: "2026-01-02T03:04:05Z",
            manifest_hash: HASH_A,
            idem_key: idem,
        }
    }

    fn add_change<'a>(path: &'a str, blob: &'a str) -> VcsFileChangeInsert<'a> {
        VcsFileChangeInsert {
            path,
            kind: "add",
            mode: 0o100644,
            old_path: None,
            blob_hash: Some(blob),
        }
    }

    #[tokio::test]
    async fn skipped_db_fails_closed() {
        let db = Database::skipped();
        let e = db.vcs_get_repo_surface_flag("r").await.unwrap_err();
        assert_eq!(e, "database not configured");
        assert!(db.vcs_get_object(HASH_A).await.is_err());
        assert!(db.vcs_insert_object_if_absent(HASH_A, 1).await.is_err());
        assert!(db.vcs_list_orphaned_objects_before("x").await.is_err());
        assert!(db
            .vcs_insert_object_link_if_absent("r", HASH_A)
            .await
            .is_err());
        assert!(db.vcs_get_object_link("r", HASH_A).await.is_err());
        assert!(db.vcs_delete_object_if_unlinked(HASH_A).await.is_err());
        assert!(db.vcs_repo_object_counts("r").await.is_err());
        assert!(db.vcs_live_object_hashes("r").await.is_err());
        assert!(db.vcs_get_changeset("r", CS_A).await.is_err());
        assert!(db.vcs_changeset_parents("r", CS_A).await.is_err());
        assert!(db.vcs_find_changeset_by_idem("r", "k").await.is_err());
        assert!(db.vcs_count_changesets("r").await.is_err());
        assert!(db.vcs_list_file_changes(CS_A).await.is_err());
        assert!(db.vcs_list_refs("r", None, 10).await.is_err());
        assert!(db
            .vcs_cas_ref_update("r", "main", None, Some(CS_A))
            .await
            .is_err());
        assert!(db.vcs_delete_ref("r", "main").await.is_err());
        assert!(db.vcs_upsert_derived_index_rows("r", &[]).await.is_err());
        assert!(db
            .vcs_query_derived_index("r", "k", None, None, 10)
            .await
            .is_err());
        assert!(db
            .vcs_insert_import_link_if_absent("r", OID_A, CS_A)
            .await
            .is_err());
        assert!(db.vcs_lookup_import_link("r", OID_A).await.is_err());
        assert!(db.vcs_import_link_page("r", None, 10).await.is_err());
        let changeset = cs(CS_A, "r", "[]", None);
        assert!(db
            .vcs_create_changeset_tx(&changeset, &[], &[], &[])
            .await
            .is_err());
    }

    #[tokio::test]
    async fn surface_flag_get_and_upsert() {
        let (_d, db) = test_db().await;
        let (uid, rid) = seed_repo(&db).await;
        // Absent row = never-enabled.
        assert!(db.vcs_get_repo_surface_flag(&rid).await.unwrap().is_none());
        db.vcs_upsert_repo_surface_flag(&rid, true, Some(&uid))
            .await
            .unwrap();
        let flag = db.vcs_get_repo_surface_flag(&rid).await.unwrap().unwrap();
        assert!(flag.enabled);
        assert_eq!(flag.enabled_by.as_deref(), Some(uid.as_str()));
        assert!(!flag.enabled_at.is_empty());
        // Re-enable toggles the same row (upsert, not a second row).
        db.vcs_upsert_repo_surface_flag(&rid, false, None)
            .await
            .unwrap();
        let flag = db.vcs_get_repo_surface_flag(&rid).await.unwrap().unwrap();
        assert!(!flag.enabled);
        assert!(flag.enabled_by.is_none());
    }

    #[tokio::test]
    async fn object_insert_if_absent_and_counts() {
        let (_d, db) = test_db().await;
        let (_uid, rid) = seed_repo(&db).await;
        assert!(db.vcs_insert_object_if_absent(HASH_A, 10).await.unwrap());
        assert!(!db.vcs_insert_object_if_absent(HASH_A, 10).await.unwrap());
        assert!(db.vcs_insert_object_if_absent(HASH_B, 20).await.unwrap());
        let obj = db.vcs_get_object(HASH_A).await.unwrap().unwrap();
        assert_eq!(obj.size, 10);
        // Links: first insert stored, re-link dedups (no PK collision).
        assert!(db
            .vcs_insert_object_link_if_absent(&rid, HASH_A)
            .await
            .unwrap());
        assert!(!db
            .vcs_insert_object_link_if_absent(&rid, HASH_A)
            .await
            .unwrap());
        assert!(db
            .vcs_insert_object_link_if_absent(&rid, HASH_B)
            .await
            .unwrap());
        let counts = db.vcs_repo_object_counts(&rid).await.unwrap();
        assert_eq!(counts.object_count, 2);
        assert_eq!(counts.storage_bytes, 30);
        let mut live = db.vcs_live_object_hashes(&rid).await.unwrap();
        live.sort();
        assert_eq!(live, vec![HASH_A, HASH_B]);
        // Orphan scan: HASH_C has no link → visible to a far-future grace bound;
        // linked objects are not.
        assert!(db.vcs_insert_object_if_absent(HASH_C, 5).await.unwrap());
        let orphans = db
            .vcs_list_orphaned_objects_before("2999-01-01T00:00:00Z")
            .await
            .unwrap();
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].hash, HASH_C);
        let none = db
            .vcs_list_orphaned_objects_before("1970-01-01T00:00:00Z")
            .await
            .unwrap();
        assert!(none.is_empty());
    }

    /// u3 quota gate probe — `true` iff the (repo, hash) link exists.
    #[tokio::test]
    async fn vcs_get_object_link_bool() {
        let (_d, db) = test_db().await;
        let (_uid, rid) = seed_repo(&db).await;

        // Absent link → false.
        assert!(!db.vcs_get_object_link(&rid, HASH_A).await.unwrap());
        db.vcs_insert_object_if_absent(HASH_A, 10).await.unwrap();
        db.vcs_insert_object_link_if_absent(&rid, HASH_A)
            .await
            .unwrap();
        assert!(db.vcs_get_object_link(&rid, HASH_A).await.unwrap());
        // Different repo / different hash stay false.
        assert!(!db.vcs_get_object_link("other-repo", HASH_A).await.unwrap());
        assert!(!db.vcs_get_object_link(&rid, HASH_B).await.unwrap());
    }

    /// u3 conditional delete — GC's row-first unlink gate.
    #[tokio::test]
    async fn vcs_delete_object_if_unlinked() {
        let (_d, db) = test_db().await;
        let (_uid, rid) = seed_repo(&db).await;

        // Unlinked row → deleted, row gone; absent hash → false.
        db.vcs_insert_object_if_absent(HASH_B, 20).await.unwrap();
        assert!(db.vcs_delete_object_if_unlinked(HASH_B).await.unwrap());
        assert!(db.vcs_get_object(HASH_B).await.unwrap().is_none());
        assert!(!db.vcs_delete_object_if_unlinked(HASH_B).await.unwrap());

        // Linked row → false, row kept.
        db.vcs_insert_object_if_absent(HASH_A, 10).await.unwrap();
        db.vcs_insert_object_link_if_absent(&rid, HASH_A)
            .await
            .unwrap();
        assert!(!db.vcs_delete_object_if_unlinked(HASH_A).await.unwrap());
        assert!(db.vcs_get_object(HASH_A).await.unwrap().is_some());

        // Referenced by a manifest `blob_hash` (no link — the U4-invariant
        // edge) → false, row kept.
        db.vcs_insert_object_if_absent(HASH_C, 5).await.unwrap();
        let changeset = cs(CS_A, &rid, "[]", None);
        let files = [add_change("a.txt", HASH_C)];
        db.vcs_create_changeset_tx(&changeset, &files, &[], &[])
            .await
            .unwrap();
        assert!(!db.vcs_delete_object_if_unlinked(HASH_C).await.unwrap());
        assert!(db.vcs_get_object(HASH_C).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn create_tx_commits_and_reads_back() {
        let (_d, db) = test_db().await;
        let (_uid, rid) = seed_repo(&db).await;
        db.vcs_insert_object_if_absent(HASH_A, 10).await.unwrap();
        db.vcs_insert_object_if_absent(HASH_B, 20).await.unwrap();
        let root = cs(CS_A, &rid, "[]", Some("idem-1"));
        let files = [
            add_change("a.txt", HASH_A),
            VcsFileChangeInsert {
                path: "gone.txt",
                kind: "delete",
                mode: 0o100644,
                old_path: None,
                blob_hash: None,
            },
        ];
        let upserts = [VcsDerivedIndexUpsert {
            kind: "path_history",
            index_key: "a.txt",
            payload: "{\"seen\":1}",
            covering_changeset: Some(CS_A),
        }];
        let id = db
            .vcs_create_changeset_tx(&root, &files, &[HASH_A], &upserts)
            .await
            .unwrap();
        assert_eq!(id, CS_A);
        let row = db.vcs_get_changeset(&rid, CS_A).await.unwrap().unwrap();
        assert_eq!(row.author, "Vcs U1 <vcs-u1@example.com>");
        assert_eq!(row.manifest_hash, HASH_A);
        assert_eq!(row.idem_key.as_deref(), Some("idem-1"));
        assert_eq!(
            db.vcs_changeset_parents(&rid, CS_A).await.unwrap(),
            Vec::<String>::new()
        );
        let manifest = db.vcs_list_file_changes(CS_A).await.unwrap();
        assert_eq!(manifest.len(), 2);
        assert_eq!(manifest[0].path, "a.txt");
        assert_eq!(manifest[1].kind, "delete");
        assert!(manifest[1].blob_hash.is_none());
        // The link write inside the tx lands.
        let counts = db.vcs_repo_object_counts(&rid).await.unwrap();
        assert_eq!(counts.object_count, 1);
        // Derived row upserted in the same tx.
        let derived = db
            .vcs_query_derived_index(&rid, "path_history", None, None, 10)
            .await
            .unwrap();
        assert_eq!(derived.len(), 1);
        assert_eq!(derived[0].index_key, "a.txt");
        // Child changeset pointing at the root.
        let child_parents = format!("[\"{CS_A}\"]");
        let child = cs(CS_B, &rid, &child_parents, None);
        let child_files = [add_change("b.txt", HASH_B)];
        let id = db
            .vcs_create_changeset_tx(&child, &child_files, &[HASH_B], &[])
            .await
            .unwrap();
        assert_eq!(id, CS_B);
        assert_eq!(
            db.vcs_changeset_parents(&rid, CS_B).await.unwrap(),
            vec![CS_A.to_string()]
        );
        assert_eq!(db.vcs_count_changesets(&rid).await.unwrap(), 2);
        // Re-referencing an already-linked blob does not collide.
        let third_parents = format!("[\"{CS_B}\"]");
        let third = cs(CS_C, &rid, &third_parents, None);
        let third_files = [add_change("a-again.txt", HASH_A)];
        db.vcs_create_changeset_tx(&third, &third_files, &[HASH_A], &[])
            .await
            .unwrap();
        let counts = db.vcs_repo_object_counts(&rid).await.unwrap();
        assert_eq!(counts.object_count, 2);
    }

    #[tokio::test]
    async fn create_tx_idempotent_resubmit() {
        let (_d, db) = test_db().await;
        let (_uid, rid) = seed_repo(&db).await;
        db.vcs_insert_object_if_absent(HASH_A, 10).await.unwrap();
        let first = cs(CS_A, &rid, "[]", Some("k-same"));
        let files = [add_change("a.txt", HASH_A)];
        let id1 = db
            .vcs_create_changeset_tx(&first, &files, &[HASH_A], &[])
            .await
            .unwrap();
        // Byte-identical resubmit → same id.
        let id2 = db
            .vcs_create_changeset_tx(&first, &files, &[HASH_A], &[])
            .await
            .unwrap();
        assert_eq!(id1, id2);
        // Resubmit with the same idem key but a fresh id → resolves to the
        // first row's id, and no second row exists.
        let resubmit = cs(CS_B, &rid, "[]", Some("k-same"));
        let id3 = db
            .vcs_create_changeset_tx(&resubmit, &files, &[HASH_A], &[])
            .await
            .unwrap();
        assert_eq!(id3, CS_A);
        assert_eq!(db.vcs_count_changesets(&rid).await.unwrap(), 1);
        let found = db
            .vcs_find_changeset_by_idem(&rid, "k-same")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.id, CS_A);
    }

    #[tokio::test]
    async fn create_tx_missing_parent_aborts_atomically() {
        let (_d, db) = test_db().await;
        let (_uid, rid) = seed_repo(&db).await;
        db.vcs_insert_object_if_absent(HASH_A, 10).await.unwrap();
        let bad = cs(CS_A, &rid, "[\"cs_00000000000000000000000000\"]", None);
        let files = [add_change("a.txt", HASH_A)];
        let upserts = [VcsDerivedIndexUpsert {
            kind: "k",
            index_key: "i",
            payload: "{}",
            covering_changeset: Some(CS_A),
        }];
        let err = db
            .vcs_create_changeset_tx(&bad, &files, &[HASH_A], &upserts)
            .await
            .unwrap_err();
        assert_eq!(err, ERR_NO_PARENT);
        // Zero partial state: no changeset, file_change, link, or index row.
        assert_eq!(db.vcs_count_changesets(&rid).await.unwrap(), 0);
        assert_eq!(
            db.vcs_repo_object_counts(&rid).await.unwrap().object_count,
            0
        );
        assert!(db
            .vcs_query_derived_index(&rid, "k", None, None, 10)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn create_tx_missing_blob_aborts_atomically() {
        let (_d, db) = test_db().await;
        let (_uid, rid) = seed_repo(&db).await;
        db.vcs_insert_object_if_absent(HASH_A, 10).await.unwrap();
        // File change references an object that was never stored.
        let bad = cs(CS_A, &rid, "[]", None);
        let files = [add_change("a.txt", HASH_B)];
        let err = db
            .vcs_create_changeset_tx(&bad, &files, &[HASH_A], &[])
            .await
            .unwrap_err();
        assert_eq!(err, ERR_NO_OBJECT);
        assert_eq!(db.vcs_count_changesets(&rid).await.unwrap(), 0);
        assert_eq!(
            db.vcs_repo_object_counts(&rid).await.unwrap().object_count,
            0
        );
        // A link hash for a missing object also trips the object sentinel.
        let bad_link = cs(CS_B, &rid, "[]", None);
        let err = db
            .vcs_create_changeset_tx(&bad_link, &[], &[HASH_B], &[])
            .await
            .unwrap_err();
        assert_eq!(err, ERR_NO_OBJECT);
    }

    #[tokio::test]
    async fn create_tx_rejects_delete_shape_violations() {
        let (_d, db) = test_db().await;
        let (_uid, rid) = seed_repo(&db).await;
        db.vcs_insert_object_if_absent(HASH_A, 10).await.unwrap();
        // Non-delete change without blob_hash is refused before any write.
        let bad = cs(CS_A, &rid, "[]", None);
        let files = [VcsFileChangeInsert {
            path: "a.txt",
            kind: "add",
            mode: 0o100644,
            old_path: None,
            blob_hash: None,
        }];
        let err = db
            .vcs_create_changeset_tx(&bad, &files, &[], &[])
            .await
            .unwrap_err();
        assert_eq!(err, ERR_BAD_FILE_CHANGE);
        // Malformed parents JSON fails before any write.
        let bad_parents = cs(CS_B, &rid, "not-json", None);
        let err = db
            .vcs_create_changeset_tx(&bad_parents, &[], &[], &[])
            .await
            .unwrap_err();
        assert_eq!(err, ERR_BAD_PARENTS);
        assert_eq!(db.vcs_count_changesets(&rid).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn cas_ref_lifecycle() {
        let (_d, db) = test_db().await;
        let (_uid, rid) = seed_repo(&db).await;
        db.vcs_insert_object_if_absent(HASH_A, 10).await.unwrap();
        db.vcs_create_changeset_tx(&cs(CS_A, &rid, "[]", None), &[], &[HASH_A], &[])
            .await
            .unwrap();
        db.vcs_create_changeset_tx(
            &cs(CS_B, &rid, &format!("[\"{CS_A}\"]"), None),
            &[],
            &[],
            &[],
        )
        .await
        .unwrap();

        // Create: expected NULL + absent row.
        let out = db
            .vcs_cas_ref_update(&rid, "main", None, Some(CS_A))
            .await
            .unwrap();
        match out {
            CasOutcome::Applied(row) => {
                assert_eq!(row.name, "main");
                assert_eq!(row.changeset_id.as_deref(), Some(CS_A));
            }
            CasOutcome::Conflict { actual } => panic!("unexpected conflict: {actual:?}"),
        }
        // Create-only on an existing ref conflicts with the stored head.
        let out = db
            .vcs_cas_ref_update(&rid, "main", None, Some(CS_B))
            .await
            .unwrap();
        match out {
            CasOutcome::Conflict { actual } => {
                assert_eq!(actual.as_deref(), Some(CS_A));
            }
            CasOutcome::Applied(_) => panic!("expected conflict"),
        }
        // Wrong expected → Conflict carrying the actual head.
        let out = db
            .vcs_cas_ref_update(&rid, "main", Some(CS_B), Some(CS_B))
            .await
            .unwrap();
        match out {
            CasOutcome::Conflict { actual } => {
                assert_eq!(actual.as_deref(), Some(CS_A));
            }
            CasOutcome::Applied(_) => panic!("expected conflict"),
        }
        // Correct expected → Applied.
        let out = db
            .vcs_cas_ref_update(&rid, "main", Some(CS_A), Some(CS_B))
            .await
            .unwrap();
        match out {
            CasOutcome::Applied(row) => assert_eq!(row.changeset_id.as_deref(), Some(CS_B)),
            CasOutcome::Conflict { actual } => panic!("unexpected conflict: {actual:?}"),
        }
        // Delete with stale expectation conflicts.
        let out = db
            .vcs_cas_ref_update(&rid, "main", Some(CS_A), None)
            .await
            .unwrap();
        assert!(matches!(out, CasOutcome::Conflict { .. }));
        // Delete with correct expectation → Applied, row gone (no tombstone).
        let out = db
            .vcs_cas_ref_update(&rid, "main", Some(CS_B), None)
            .await
            .unwrap();
        match out {
            CasOutcome::Applied(row) => assert!(row.changeset_id.is_none()),
            CasOutcome::Conflict { actual } => panic!("unexpected conflict: {actual:?}"),
        }
        assert!(db.vcs_list_refs(&rid, None, 10).await.unwrap().is_empty());
        // Delete of an absent ref conflicts (absent refs, AC5.2.1).
        let out = db
            .vcs_cas_ref_update(&rid, "main", None, None)
            .await
            .unwrap();
        match out {
            CasOutcome::Conflict { actual } => assert!(actual.is_none()),
            CasOutcome::Applied(_) => panic!("expected conflict"),
        }
    }

    #[tokio::test]
    async fn refs_list_and_delete() {
        let (_d, db) = test_db().await;
        let (_uid, rid) = seed_repo(&db).await;
        db.vcs_insert_object_if_absent(HASH_A, 1).await.unwrap();
        db.vcs_create_changeset_tx(&cs(CS_A, &rid, "[]", None), &[], &[HASH_A], &[])
            .await
            .unwrap();
        for name in ["agent/task-1", "main"] {
            let out = db
                .vcs_cas_ref_update(&rid, name, None, Some(CS_A))
                .await
                .unwrap();
            assert!(matches!(out, CasOutcome::Applied(_)));
        }
        let page = db.vcs_list_refs(&rid, None, 10).await.unwrap();
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].name, "agent/task-1"); // name asc
                                                  // Keyset cursor.
        let page = db
            .vcs_list_refs(&rid, Some("agent/task-1"), 10)
            .await
            .unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].name, "main");
        assert!(db.vcs_delete_ref(&rid, "agent/task-1").await.unwrap());
        assert!(!db.vcs_delete_ref(&rid, "agent/task-1").await.unwrap());
    }

    #[tokio::test]
    async fn derived_index_merge_upserts() {
        let (_d, db) = test_db().await;
        let (_uid, rid) = seed_repo(&db).await;
        db.vcs_insert_object_if_absent(HASH_A, 1).await.unwrap();
        db.vcs_create_changeset_tx(&cs(CS_A, &rid, "[]", None), &[], &[HASH_A], &[])
            .await
            .unwrap();
        let rows = [
            VcsDerivedIndexUpsert {
                kind: "path_history",
                index_key: "a.txt",
                payload: "{\"v\":1}",
                covering_changeset: Some(CS_A),
            },
            VcsDerivedIndexUpsert {
                kind: "path_history",
                index_key: "b.txt",
                payload: "{\"v\":1}",
                covering_changeset: Some(CS_A),
            },
        ];
        db.vcs_upsert_derived_index_rows(&rid, &rows).await.unwrap();
        // Merge upsert on the same PK rewrites payload, not a second row.
        let merge = [VcsDerivedIndexUpsert {
            kind: "path_history",
            index_key: "a.txt",
            payload: "{\"v\":2}",
            covering_changeset: Some(CS_A),
        }];
        db.vcs_upsert_derived_index_rows(&rid, &merge)
            .await
            .unwrap();
        let all = db
            .vcs_query_derived_index(&rid, "path_history", None, None, 10)
            .await
            .unwrap();
        assert_eq!(all.len(), 2);
        let a = db
            .vcs_query_derived_index(&rid, "path_history", Some("a.txt"), None, 10)
            .await
            .unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].payload, "{\"v\":2}");
        // Keyset cursor skips the first row.
        let rest = db
            .vcs_query_derived_index(&rid, "path_history", None, Some("a.txt"), 10)
            .await
            .unwrap();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].index_key, "b.txt");
    }

    #[tokio::test]
    async fn import_links_lookup_and_page() {
        let (_d, db) = test_db().await;
        let (_uid, rid) = seed_repo(&db).await;
        db.vcs_insert_object_if_absent(HASH_A, 1).await.unwrap();
        db.vcs_create_changeset_tx(&cs(CS_A, &rid, "[]", None), &[], &[HASH_A], &[])
            .await
            .unwrap();
        db.vcs_create_changeset_tx(
            &cs(CS_B, &rid, &format!("[\"{CS_A}\"]"), None),
            &[],
            &[],
            &[],
        )
        .await
        .unwrap();
        // Insert-if-absent: crash-retry does not violate the PK.
        assert!(db
            .vcs_insert_import_link_if_absent(&rid, OID_A, CS_A)
            .await
            .unwrap());
        assert!(!db
            .vcs_insert_import_link_if_absent(&rid, OID_A, CS_A)
            .await
            .unwrap());
        assert!(db
            .vcs_insert_import_link_if_absent(&rid, OID_B, CS_B)
            .await
            .unwrap());
        assert_eq!(
            db.vcs_lookup_import_link(&rid, OID_A)
                .await
                .unwrap()
                .as_deref(),
            Some(CS_A)
        );
        assert!(db
            .vcs_lookup_import_link(&rid, "ff")
            .await
            .unwrap()
            .is_none());
        let page = db.vcs_import_link_page(&rid, None, 10).await.unwrap();
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].git_oid, OID_A);
        let page = db
            .vcs_import_link_page(&rid, Some(OID_A), 10)
            .await
            .unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].git_oid, OID_B);
    }

    #[tokio::test]
    async fn repo_delete_cascades_all_repo_scoped_rows() {
        let (_d, db) = test_db().await;
        let (_uid, rid) = seed_repo(&db).await;
        db.vcs_upsert_repo_surface_flag(&rid, true, None)
            .await
            .unwrap();
        db.vcs_insert_object_if_absent(HASH_A, 10).await.unwrap();
        db.vcs_create_changeset_tx(
            &cs(CS_A, &rid, "[]", Some("cascade-k")),
            &[add_change("a.txt", HASH_A)],
            &[HASH_A],
            &[VcsDerivedIndexUpsert {
                kind: "walk",
                index_key: "k1",
                payload: "{}",
                covering_changeset: Some(CS_A),
            }],
        )
        .await
        .unwrap();
        db.vcs_cas_ref_update(&rid, "main", None, Some(CS_A))
            .await
            .unwrap();
        db.vcs_insert_import_link_if_absent(&rid, OID_A, CS_A)
            .await
            .unwrap();

        db.hard_delete_repository(&rid).await.unwrap();

        assert!(db.vcs_get_repo_surface_flag(&rid).await.unwrap().is_none());
        assert_eq!(db.vcs_count_changesets(&rid).await.unwrap(), 0);
        assert!(db.vcs_get_changeset(&rid, CS_A).await.unwrap().is_none());
        assert!(db.vcs_list_file_changes(CS_A).await.unwrap().is_empty());
        assert!(db.vcs_list_refs(&rid, None, 10).await.unwrap().is_empty());
        assert_eq!(
            db.vcs_repo_object_counts(&rid).await.unwrap().object_count,
            0
        );
        assert!(db.vcs_live_object_hashes(&rid).await.unwrap().is_empty());
        assert!(db
            .vcs_query_derived_index(&rid, "walk", None, None, 10)
            .await
            .unwrap()
            .is_empty());
        assert!(db
            .vcs_lookup_import_link(&rid, OID_A)
            .await
            .unwrap()
            .is_none());
        // The global CAS row survives — objects are not repo-scoped.
        assert!(db.vcs_get_object(HASH_A).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn pagination_limits_clamp() {
        let (_d, db) = test_db().await;
        let (_uid, rid) = seed_repo(&db).await;
        db.vcs_insert_object_if_absent(HASH_A, 1).await.unwrap();
        db.vcs_create_changeset_tx(&cs(CS_A, &rid, "[]", None), &[], &[HASH_A], &[])
            .await
            .unwrap();
        db.vcs_cas_ref_update(&rid, "main", None, Some(CS_A))
            .await
            .unwrap();
        db.vcs_insert_import_link_if_absent(&rid, OID_A, CS_A)
            .await
            .unwrap();
        db.vcs_upsert_derived_index_rows(
            &rid,
            &[VcsDerivedIndexUpsert {
                kind: "walk",
                index_key: "k1",
                payload: "{}",
                covering_changeset: Some(CS_A),
            }],
        )
        .await
        .unwrap();
        // limit 0 clamps to 1 — returns the single row rather than erroring.
        assert_eq!(db.vcs_list_refs(&rid, None, 0).await.unwrap().len(), 1);
        assert_eq!(
            db.vcs_query_derived_index(&rid, "walk", None, None, 0)
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            db.vcs_import_link_page(&rid, None, 0).await.unwrap().len(),
            1
        );
        // Over-cap clamps down, still serves.
        assert_eq!(db.vcs_list_refs(&rid, None, 99999).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn ref_free_activity_row_roundtrips() {
        let (_d, db) = test_db().await;
        let (uid, rid) = seed_repo(&db).await;
        // C4: vcs.* kinds write ref-free rows — all four push columns NULL.
        db.insert_repo_activity(
            "act-1",
            &rid,
            &uid,
            "vcs.repo_enable",
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
        // A legacy push row with Some(...) still works.
        db.insert_repo_activity(
            "act-2",
            &rid,
            &uid,
            "push",
            Some("refs/heads/main"),
            Some("0000000000000000000000000000000000000000"),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            Some(2),
            Some("msg"),
            None,
        )
        .await
        .unwrap();
        let (rows, total) = db
            .list_repo_activity(&rid, None, None, 0, 10)
            .await
            .unwrap();
        assert_eq!(total, 2);
        let ref_free = rows.iter().find(|r| r.id == "act-1").unwrap();
        assert!(ref_free.ref_name.is_none());
        assert!(ref_free.before_oid.is_none());
        assert!(ref_free.after_oid.is_none());
        assert!(ref_free.commits_count.is_none());
        let push = rows.iter().find(|r| r.id == "act-2").unwrap();
        assert_eq!(push.ref_name.as_deref(), Some("refs/heads/main"));
        assert_eq!(push.commits_count, Some(2));
    }
}
