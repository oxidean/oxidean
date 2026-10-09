//! Repository two-way mirrors persistence (GIT-V2-01).

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct RepositoryMirrorRow {
    pub id: String,
    pub repository_id: String,
    pub remote_url: String,
    pub auth_kind: String,
    pub username: String,
    pub secret_ciphertext: String,
    pub ssh_public_key: String,
    pub known_hosts: String,
    pub webhook_secret_ciphertext: String,
    pub poll_interval_secs: i64,
    pub enabled: bool,
    pub sync_mode: String,
    pub last_ref_snapshot: String,
    pub last_synced_at: Option<String>,
    pub last_status: String,
    pub last_error: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct RepositoryMirrorRefResultRow {
    pub id: String,
    pub mirror_id: String,
    pub refname: String,
    pub outcome: String,
    pub local_oid: String,
    pub remote_oid: String,
    pub detail: String,
    pub updated_at: String,
}

macro_rules! map_mirror {
    ($row:expr) => {{
        let row = $row;
        let enabled_i: i64 = row
            .try_get::<i32, _>("enabled")
            .map(|v| i64::from(v))
            .or_else(|_| row.try_get::<i64, _>("enabled"))
            .or_else(|_| {
                row.try_get::<bool, _>("enabled")
                    .map(|b| if b { 1 } else { 0 })
            })
            .map_err(|e| format!("enabled: {e}"))?;
        let poll: i64 = row
            .try_get::<i64, _>("poll_interval_secs")
            .or_else(|_| {
                row.try_get::<i32, _>("poll_interval_secs")
                    .map(|v| i64::from(v))
            })
            .map_err(|e| format!("poll_interval_secs: {e}"))?;
        RepositoryMirrorRow {
            id: row.try_get("id").map_err(|e| format!("id: {e}"))?,
            repository_id: row
                .try_get("repository_id")
                .map_err(|e| format!("repository_id: {e}"))?,
            remote_url: row
                .try_get("remote_url")
                .map_err(|e| format!("remote_url: {e}"))?,
            auth_kind: row
                .try_get("auth_kind")
                .map_err(|e| format!("auth_kind: {e}"))?,
            username: row
                .try_get("username")
                .map_err(|e| format!("username: {e}"))?,
            secret_ciphertext: row
                .try_get("secret_ciphertext")
                .map_err(|e| format!("secret_ciphertext: {e}"))?,
            ssh_public_key: row
                .try_get("ssh_public_key")
                .map_err(|e| format!("ssh_public_key: {e}"))?,
            known_hosts: row
                .try_get("known_hosts")
                .map_err(|e| format!("known_hosts: {e}"))?,
            webhook_secret_ciphertext: row
                .try_get("webhook_secret_ciphertext")
                .map_err(|e| format!("webhook_secret_ciphertext: {e}"))?,
            poll_interval_secs: poll,
            enabled: enabled_i != 0,
            sync_mode: row
                .try_get("sync_mode")
                .unwrap_or_else(|_| "merge".to_string()),
            last_ref_snapshot: row
                .try_get("last_ref_snapshot")
                .unwrap_or_else(|_| "{}".to_string()),
            last_synced_at: row.try_get("last_synced_at").ok(),
            last_status: row
                .try_get("last_status")
                .map_err(|e| format!("last_status: {e}"))?,
            last_error: row
                .try_get("last_error")
                .map_err(|e| format!("last_error: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("created_at: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("updated_at: {e}"))?,
        }
    }};
}

macro_rules! map_ref_result {
    ($row:expr) => {{
        let row = $row;
        RepositoryMirrorRefResultRow {
            id: row.try_get("id").map_err(|e| format!("id: {e}"))?,
            mirror_id: row
                .try_get("mirror_id")
                .map_err(|e| format!("mirror_id: {e}"))?,
            refname: row
                .try_get("refname")
                .map_err(|e| format!("refname: {e}"))?,
            outcome: row
                .try_get("outcome")
                .map_err(|e| format!("outcome: {e}"))?,
            local_oid: row
                .try_get("local_oid")
                .map_err(|e| format!("local_oid: {e}"))?,
            remote_oid: row
                .try_get("remote_oid")
                .map_err(|e| format!("remote_oid: {e}"))?,
            detail: row.try_get("detail").map_err(|e| format!("detail: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("updated_at: {e}"))?,
        }
    }};
}

pub async fn get_mirror_by_repo(
    pool: &DbPool,
    repository_id: &str,
) -> Result<Option<RepositoryMirrorRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                r#"SELECT id, repository_id, remote_url, auth_kind, username, secret_ciphertext,
                          ssh_public_key, known_hosts, webhook_secret_ciphertext, poll_interval_secs,
                          enabled, sync_mode, last_ref_snapshot, last_synced_at::text, last_status, last_error,
                          created_at::text, updated_at::text
                   FROM repository_mirrors WHERE repository_id = $1"#,
            )
            .bind(repository_id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            match row { Some(r) => Ok(Some(map_mirror!(r))), None => Ok(None) }
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                r#"SELECT id, repository_id, remote_url, auth_kind, username, secret_ciphertext,
                          ssh_public_key, known_hosts, webhook_secret_ciphertext, poll_interval_secs,
                          enabled, sync_mode, last_ref_snapshot, last_synced_at, last_status, last_error, created_at, updated_at
                   FROM repository_mirrors WHERE repository_id = ?"#,
            )
            .bind(repository_id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            match row { Some(r) => Ok(Some(map_mirror!(r))), None => Ok(None) }
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                r#"SELECT id, repository_id, remote_url, auth_kind, username, secret_ciphertext,
                          ssh_public_key, known_hosts, webhook_secret_ciphertext, poll_interval_secs,
                          enabled, sync_mode, last_ref_snapshot, last_synced_at, last_status, last_error, created_at, updated_at
                   FROM repository_mirrors WHERE repository_id = ?"#,
            )
            .bind(repository_id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            match row { Some(r) => Ok(Some(map_mirror!(r))), None => Ok(None) }
        }
    }
}

pub async fn get_mirror_by_id(
    pool: &DbPool,
    id: &str,
) -> Result<Option<RepositoryMirrorRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                r#"SELECT id, repository_id, remote_url, auth_kind, username, secret_ciphertext,
                          ssh_public_key, known_hosts, webhook_secret_ciphertext, poll_interval_secs,
                          enabled, sync_mode, last_ref_snapshot, last_synced_at::text, last_status, last_error,
                          created_at::text, updated_at::text
                   FROM repository_mirrors WHERE id = $1"#,
            )
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            match row { Some(r) => Ok(Some(map_mirror!(r))), None => Ok(None) }
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                r#"SELECT id, repository_id, remote_url, auth_kind, username, secret_ciphertext,
                          ssh_public_key, known_hosts, webhook_secret_ciphertext, poll_interval_secs,
                          enabled, sync_mode, last_ref_snapshot, last_synced_at, last_status, last_error, created_at, updated_at
                   FROM repository_mirrors WHERE id = ?"#,
            )
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            match row { Some(r) => Ok(Some(map_mirror!(r))), None => Ok(None) }
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                r#"SELECT id, repository_id, remote_url, auth_kind, username, secret_ciphertext,
                          ssh_public_key, known_hosts, webhook_secret_ciphertext, poll_interval_secs,
                          enabled, sync_mode, last_ref_snapshot, last_synced_at, last_status, last_error, created_at, updated_at
                   FROM repository_mirrors WHERE id = ?"#,
            )
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            match row { Some(r) => Ok(Some(map_mirror!(r))), None => Ok(None) }
        }
    }
}

pub async fn list_enabled_mirrors(pool: &DbPool) -> Result<Vec<RepositoryMirrorRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                r#"SELECT id, repository_id, remote_url, auth_kind, username, secret_ciphertext,
                          ssh_public_key, known_hosts, webhook_secret_ciphertext, poll_interval_secs,
                          enabled, sync_mode, last_ref_snapshot, last_synced_at::text, last_status, last_error,
                          created_at::text, updated_at::text
                   FROM repository_mirrors WHERE enabled = TRUE"#,
            )
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            rows.into_iter().map(|r| Ok(map_mirror!(r))).collect()
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                r#"SELECT id, repository_id, remote_url, auth_kind, username, secret_ciphertext,
                          ssh_public_key, known_hosts, webhook_secret_ciphertext, poll_interval_secs,
                          enabled, sync_mode, last_ref_snapshot, last_synced_at, last_status, last_error, created_at, updated_at
                   FROM repository_mirrors WHERE enabled = 1"#,
            )
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            rows.into_iter().map(|r| Ok(map_mirror!(r))).collect()
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                r#"SELECT id, repository_id, remote_url, auth_kind, username, secret_ciphertext,
                          ssh_public_key, known_hosts, webhook_secret_ciphertext, poll_interval_secs,
                          enabled, sync_mode, last_ref_snapshot, last_synced_at, last_status, last_error, created_at, updated_at
                   FROM repository_mirrors WHERE enabled = 1"#,
            )
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            rows.into_iter().map(|r| Ok(map_mirror!(r))).collect()
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_mirror(
    pool: &DbPool,
    id: &str,
    repository_id: &str,
    remote_url: &str,
    auth_kind: &str,
    username: &str,
    secret_ciphertext: &str,
    ssh_public_key: &str,
    known_hosts: &str,
    webhook_secret_ciphertext: &str,
    poll_interval_secs: i64,
    enabled: bool,
    sync_mode: &str,
    clear_ref_snapshot: bool,
) -> Result<RepositoryMirrorRow, String> {
    let enabled_i: i32 = if enabled { 1 } else { 0 };
    let snapshot_value = if clear_ref_snapshot { "{}" } else { "" };
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"INSERT INTO repository_mirrors (
                     id, repository_id, remote_url, auth_kind, username, secret_ciphertext,
                     ssh_public_key, known_hosts, webhook_secret_ciphertext, poll_interval_secs,
                     enabled, sync_mode
                   ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
                   ON CONFLICT (repository_id) DO UPDATE SET
                     remote_url = EXCLUDED.remote_url,
                     auth_kind = EXCLUDED.auth_kind,
                     username = EXCLUDED.username,
                     secret_ciphertext = CASE
                       WHEN EXCLUDED.secret_ciphertext = '' THEN repository_mirrors.secret_ciphertext
                       ELSE EXCLUDED.secret_ciphertext END,
                     ssh_public_key = CASE
                       WHEN EXCLUDED.ssh_public_key = '' THEN repository_mirrors.ssh_public_key
                       ELSE EXCLUDED.ssh_public_key END,
                     known_hosts = CASE
                       WHEN EXCLUDED.known_hosts = '' THEN repository_mirrors.known_hosts
                       ELSE EXCLUDED.known_hosts END,
                     webhook_secret_ciphertext = CASE
                       WHEN EXCLUDED.webhook_secret_ciphertext = '' THEN repository_mirrors.webhook_secret_ciphertext
                       ELSE EXCLUDED.webhook_secret_ciphertext END,
                     poll_interval_secs = EXCLUDED.poll_interval_secs,
                     enabled = EXCLUDED.enabled,
                     sync_mode = EXCLUDED.sync_mode,
                     last_ref_snapshot = CASE
                       WHEN $13 <> '' THEN $13
                       ELSE repository_mirrors.last_ref_snapshot END,
                     updated_at = NOW()"#,
            )
            .bind(id)
            .bind(repository_id)
            .bind(remote_url)
            .bind(auth_kind)
            .bind(username)
            .bind(secret_ciphertext)
            .bind(ssh_public_key)
            .bind(known_hosts)
            .bind(webhook_secret_ciphertext)
            .bind(poll_interval_secs)
            .bind(enabled)
            .bind(sync_mode)
            .bind(snapshot_value)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                r#"INSERT INTO repository_mirrors (
                     id, repository_id, remote_url, auth_kind, username, secret_ciphertext,
                     ssh_public_key, known_hosts, webhook_secret_ciphertext, poll_interval_secs,
                     enabled, sync_mode, last_error
                   ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?, '')
                   ON DUPLICATE KEY UPDATE
                     remote_url = VALUES(remote_url),
                     auth_kind = VALUES(auth_kind),
                     username = VALUES(username),
                     secret_ciphertext = IF(VALUES(secret_ciphertext) = '', secret_ciphertext, VALUES(secret_ciphertext)),
                     ssh_public_key = IF(VALUES(ssh_public_key) = '', ssh_public_key, VALUES(ssh_public_key)),
                     known_hosts = IF(VALUES(known_hosts) = '', known_hosts, VALUES(known_hosts)),
                     webhook_secret_ciphertext = IF(VALUES(webhook_secret_ciphertext) = '', webhook_secret_ciphertext, VALUES(webhook_secret_ciphertext)),
                     poll_interval_secs = VALUES(poll_interval_secs),
                     enabled = VALUES(enabled),
                     sync_mode = VALUES(sync_mode),
                     last_ref_snapshot = IF(? <> '', ?, last_ref_snapshot),
                     updated_at = CURRENT_TIMESTAMP"#,
            )
            .bind(id)
            .bind(repository_id)
            .bind(remote_url)
            .bind(auth_kind)
            .bind(username)
            .bind(secret_ciphertext)
            .bind(ssh_public_key)
            .bind(known_hosts)
            .bind(webhook_secret_ciphertext)
            .bind(poll_interval_secs)
            .bind(enabled_i)
            .bind(sync_mode)
            .bind(snapshot_value)
            .bind(snapshot_value)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::Sqlite(p) => {
            // Prefer update-if-exists by repository_id for stable ids.
            let existing = get_mirror_by_repo(pool, repository_id).await?;
            if let Some(ex) = existing {
                let secret = if secret_ciphertext.is_empty() {
                    ex.secret_ciphertext.as_str()
                } else {
                    secret_ciphertext
                };
                let ssh_pub = if ssh_public_key.is_empty() {
                    ex.ssh_public_key.as_str()
                } else {
                    ssh_public_key
                };
                let kh = if known_hosts.is_empty() {
                    ex.known_hosts.as_str()
                } else {
                    known_hosts
                };
                let wh = if webhook_secret_ciphertext.is_empty() {
                    ex.webhook_secret_ciphertext.as_str()
                } else {
                    webhook_secret_ciphertext
                };
                let snap = if clear_ref_snapshot {
                    "{}"
                } else {
                    ex.last_ref_snapshot.as_str()
                };
                sqlx::query(
                    r#"UPDATE repository_mirrors SET
                         remote_url = ?, auth_kind = ?, username = ?,
                         secret_ciphertext = ?, ssh_public_key = ?, known_hosts = ?,
                         webhook_secret_ciphertext = ?, poll_interval_secs = ?, enabled = ?,
                         sync_mode = ?, last_ref_snapshot = ?,
                         updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
                       WHERE repository_id = ?"#,
                )
                .bind(remote_url)
                .bind(auth_kind)
                .bind(username)
                .bind(secret)
                .bind(ssh_pub)
                .bind(kh)
                .bind(wh)
                .bind(poll_interval_secs)
                .bind(enabled_i)
                .bind(sync_mode)
                .bind(snap)
                .bind(repository_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            } else {
                sqlx::query(
                    r#"INSERT INTO repository_mirrors (
                         id, repository_id, remote_url, auth_kind, username, secret_ciphertext,
                         ssh_public_key, known_hosts, webhook_secret_ciphertext, poll_interval_secs,
                         enabled, sync_mode, last_error
                       ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?, '')"#,
                )
                .bind(id)
                .bind(repository_id)
                .bind(remote_url)
                .bind(auth_kind)
                .bind(username)
                .bind(secret_ciphertext)
                .bind(ssh_public_key)
                .bind(known_hosts)
                .bind(webhook_secret_ciphertext)
                .bind(poll_interval_secs)
                .bind(enabled_i)
                .bind(sync_mode)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            }
        }
    }
    get_mirror_by_repo(pool, repository_id)
        .await?
        .ok_or_else(|| "mirror upsert failed".into())
}

pub async fn delete_mirror_by_repo(pool: &DbPool, repository_id: &str) -> Result<bool, String> {
    let n = match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM repository_mirrors WHERE repository_id = $1")
                .bind(repository_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?
                .rows_affected()
        }
        DbPool::MySql(p) => sqlx::query("DELETE FROM repository_mirrors WHERE repository_id = ?")
            .bind(repository_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?
            .rows_affected(),
        DbPool::Sqlite(p) => sqlx::query("DELETE FROM repository_mirrors WHERE repository_id = ?")
            .bind(repository_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?
            .rows_affected(),
    };
    Ok(n > 0)
}

pub async fn update_mirror_status(
    pool: &DbPool,
    mirror_id: &str,
    status: &str,
    last_error: &str,
    synced: bool,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            if synced {
                sqlx::query(
                    r#"UPDATE repository_mirrors SET last_status = $2, last_error = $3,
                           last_synced_at = NOW(), updated_at = NOW() WHERE id = $1"#,
                )
                .bind(mirror_id)
                .bind(status)
                .bind(last_error)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            } else {
                sqlx::query(
                    r#"UPDATE repository_mirrors SET last_status = $2, last_error = $3,
                           updated_at = NOW() WHERE id = $1"#,
                )
                .bind(mirror_id)
                .bind(status)
                .bind(last_error)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            }
        }
        DbPool::MySql(p) => {
            if synced {
                sqlx::query(
                    r#"UPDATE repository_mirrors SET last_status = ?, last_error = ?,
                           last_synced_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE id = ?"#,
                )
                .bind(status)
                .bind(last_error)
                .bind(mirror_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            } else {
                sqlx::query(
                    r#"UPDATE repository_mirrors SET last_status = ?, last_error = ?,
                           updated_at = CURRENT_TIMESTAMP WHERE id = ?"#,
                )
                .bind(status)
                .bind(last_error)
                .bind(mirror_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            }
        }
        DbPool::Sqlite(p) => {
            if synced {
                sqlx::query(
                    r#"UPDATE repository_mirrors SET last_status = ?, last_error = ?,
                           last_synced_at = strftime('%Y-%m-%d %H:%M:%S','now'),
                           updated_at = strftime('%Y-%m-%d %H:%M:%S','now') WHERE id = ?"#,
                )
                .bind(status)
                .bind(last_error)
                .bind(mirror_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            } else {
                sqlx::query(
                    r#"UPDATE repository_mirrors SET last_status = ?, last_error = ?,
                           updated_at = strftime('%Y-%m-%d %H:%M:%S','now') WHERE id = ?"#,
                )
                .bind(status)
                .bind(last_error)
                .bind(mirror_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

pub async fn set_webhook_secret(
    pool: &DbPool,
    mirror_id: &str,
    ciphertext: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"UPDATE repository_mirrors SET webhook_secret_ciphertext = $2, updated_at = NOW()
                   WHERE id = $1"#,
            )
            .bind(mirror_id)
            .bind(ciphertext)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                r#"UPDATE repository_mirrors SET webhook_secret_ciphertext = ?,
                       updated_at = CURRENT_TIMESTAMP WHERE id = ?"#,
            )
            .bind(ciphertext)
            .bind(mirror_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                r#"UPDATE repository_mirrors SET webhook_secret_ciphertext = ?,
                       updated_at = strftime('%Y-%m-%d %H:%M:%S','now') WHERE id = ?"#,
            )
            .bind(ciphertext)
            .bind(mirror_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}


pub async fn update_mirror_ref_snapshot(
    pool: &DbPool,
    mirror_id: &str,
    snapshot_json: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"UPDATE repository_mirrors SET last_ref_snapshot = $2, updated_at = NOW()
                   WHERE id = $1"#,
            )
            .bind(mirror_id)
            .bind(snapshot_json)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                r#"UPDATE repository_mirrors SET last_ref_snapshot = ?,
                       updated_at = CURRENT_TIMESTAMP WHERE id = ?"#,
            )
            .bind(snapshot_json)
            .bind(mirror_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                r#"UPDATE repository_mirrors SET last_ref_snapshot = ?,
                       updated_at = strftime('%Y-%m-%d %H:%M:%S','now') WHERE id = ?"#,
            )
            .bind(snapshot_json)
            .bind(mirror_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_ref_result(
    pool: &DbPool,
    id: &str,
    mirror_id: &str,
    refname: &str,
    outcome: &str,
    local_oid: &str,
    remote_oid: &str,
    detail: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"INSERT INTO repository_mirror_ref_results
                     (id, mirror_id, refname, outcome, local_oid, remote_oid, detail)
                   VALUES ($1,$2,$3,$4,$5,$6,$7)
                   ON CONFLICT (mirror_id, refname) DO UPDATE SET
                     outcome = EXCLUDED.outcome,
                     local_oid = EXCLUDED.local_oid,
                     remote_oid = EXCLUDED.remote_oid,
                     detail = EXCLUDED.detail,
                     updated_at = NOW()"#,
            )
            .bind(id)
            .bind(mirror_id)
            .bind(refname)
            .bind(outcome)
            .bind(local_oid)
            .bind(remote_oid)
            .bind(detail)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                r#"INSERT INTO repository_mirror_ref_results
                     (id, mirror_id, refname, outcome, local_oid, remote_oid, detail)
                   VALUES (?,?,?,?,?,?,?)
                   ON DUPLICATE KEY UPDATE
                     outcome = VALUES(outcome),
                     local_oid = VALUES(local_oid),
                     remote_oid = VALUES(remote_oid),
                     detail = VALUES(detail),
                     updated_at = CURRENT_TIMESTAMP"#,
            )
            .bind(id)
            .bind(mirror_id)
            .bind(refname)
            .bind(outcome)
            .bind(local_oid)
            .bind(remote_oid)
            .bind(detail)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                r#"INSERT INTO repository_mirror_ref_results
                     (id, mirror_id, refname, outcome, local_oid, remote_oid, detail, updated_at)
                   VALUES (?,?,?,?,?,?,?, strftime('%Y-%m-%d %H:%M:%S','now'))
                   ON CONFLICT (mirror_id, refname) DO UPDATE SET
                     outcome = excluded.outcome,
                     local_oid = excluded.local_oid,
                     remote_oid = excluded.remote_oid,
                     detail = excluded.detail,
                     updated_at = strftime('%Y-%m-%d %H:%M:%S','now')"#,
            )
            .bind(id)
            .bind(mirror_id)
            .bind(refname)
            .bind(outcome)
            .bind(local_oid)
            .bind(remote_oid)
            .bind(detail)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub async fn list_ref_results(
    pool: &DbPool,
    mirror_id: &str,
) -> Result<Vec<RepositoryMirrorRefResultRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                r#"SELECT id, mirror_id, refname, outcome, local_oid, remote_oid, detail,
                          updated_at::text
                   FROM repository_mirror_ref_results WHERE mirror_id = $1
                   ORDER BY refname"#,
            )
            .bind(mirror_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            rows.into_iter().map(|r| Ok(map_ref_result!(r))).collect()
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                r#"SELECT id, mirror_id, refname, outcome, local_oid, remote_oid, detail, updated_at
                   FROM repository_mirror_ref_results WHERE mirror_id = ?
                   ORDER BY refname"#,
            )
            .bind(mirror_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            rows.into_iter().map(|r| Ok(map_ref_result!(r))).collect()
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                r#"SELECT id, mirror_id, refname, outcome, local_oid, remote_oid, detail, updated_at
                   FROM repository_mirror_ref_results WHERE mirror_id = ?
                   ORDER BY refname"#,
            )
            .bind(mirror_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            rows.into_iter().map(|r| Ok(map_ref_result!(r))).collect()
        }
    }
}
