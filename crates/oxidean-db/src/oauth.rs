//! OAuth2 provider persistence (API-03) — registered third-party applications,
//! single-use authorization codes, and access tokens. All secrets, codes, and
//! tokens are stored as SHA-256 hex only (hash-at-rest, same posture as PATs).

use sqlx::Row;

use crate::pool::DbPool;

/// Registered OAuth application (developer-owned).
#[derive(Debug, Clone)]
pub struct OAuthAppRow {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub client_id: String,
    pub client_secret_hash: String,
    pub client_secret_prefix: String,
    /// JSON array of absolute redirect URIs (exact-match at authorize time).
    pub redirect_uris_json: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Single-use authorization code (hashed at rest).
#[derive(Debug, Clone)]
pub struct OAuthCodeRow {
    pub id: String,
    pub application_id: String,
    pub user_id: String,
    pub redirect_uri: String,
    /// Space-delimited granted scopes (OAuth convention).
    pub scopes: String,
    pub expires_at: String,
    pub used_at: Option<String>,
    pub created_at: String,
}

/// Minted OAuth access token (hashed at rest).
#[derive(Debug, Clone)]
pub struct OAuthTokenRow {
    pub id: String,
    pub application_id: String,
    pub user_id: String,
    pub token_prefix: String,
    pub token_hash: String,
    /// Space-delimited granted scopes (OAuth convention).
    pub scopes: String,
    pub expires_at: String,
    pub revoked_at: Option<String>,
    pub last_used_at: Option<String>,
    pub last_used_ip: Option<String>,
    pub created_at: String,
}

macro_rules! map_app {
    ($row:expr) => {{
        let row = $row;
        OAuthAppRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("oauth app row: {e}"))?,
            owner_id: row
                .try_get("owner_id")
                .map_err(|e| format!("oauth app row: {e}"))?,
            name: row
                .try_get("name")
                .map_err(|e| format!("oauth app row: {e}"))?,
            client_id: row
                .try_get("client_id")
                .map_err(|e| format!("oauth app row: {e}"))?,
            client_secret_hash: row
                .try_get("client_secret_hash")
                .map_err(|e| format!("oauth app row: {e}"))?,
            client_secret_prefix: row
                .try_get("client_secret_prefix")
                .map_err(|e| format!("oauth app row: {e}"))?,
            redirect_uris_json: row
                .try_get("redirect_uris_json")
                .map_err(|e| format!("oauth app row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("oauth app row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("oauth app row: {e}"))?,
        }
    }};
}

macro_rules! map_code {
    ($row:expr) => {{
        let row = $row;
        OAuthCodeRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("oauth code row: {e}"))?,
            application_id: row
                .try_get("application_id")
                .map_err(|e| format!("oauth code row: {e}"))?,
            user_id: row
                .try_get("user_id")
                .map_err(|e| format!("oauth code row: {e}"))?,
            redirect_uri: row
                .try_get("redirect_uri")
                .map_err(|e| format!("oauth code row: {e}"))?,
            scopes: row
                .try_get("scopes")
                .map_err(|e| format!("oauth code row: {e}"))?,
            expires_at: row
                .try_get("expires_at")
                .map_err(|e| format!("oauth code row: {e}"))?,
            used_at: row
                .try_get("used_at")
                .map_err(|e| format!("oauth code row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("oauth code row: {e}"))?,
        }
    }};
}

macro_rules! map_token {
    ($row:expr) => {{
        let row = $row;
        OAuthTokenRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("oauth token row: {e}"))?,
            application_id: row
                .try_get("application_id")
                .map_err(|e| format!("oauth token row: {e}"))?,
            user_id: row
                .try_get("user_id")
                .map_err(|e| format!("oauth token row: {e}"))?,
            token_prefix: row
                .try_get("token_prefix")
                .map_err(|e| format!("oauth token row: {e}"))?,
            token_hash: row
                .try_get("token_hash")
                .map_err(|e| format!("oauth token row: {e}"))?,
            scopes: row
                .try_get("scopes")
                .map_err(|e| format!("oauth token row: {e}"))?,
            expires_at: row
                .try_get("expires_at")
                .map_err(|e| format!("oauth token row: {e}"))?,
            revoked_at: row
                .try_get("revoked_at")
                .map_err(|e| format!("oauth token row: {e}"))?,
            last_used_at: row
                .try_get("last_used_at")
                .map_err(|e| format!("oauth token row: {e}"))?,
            last_used_ip: row
                .try_get("last_used_ip")
                .map_err(|e| format!("oauth token row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("oauth token row: {e}"))?,
        }
    }};
}

const APP_SELECT_PG: &str = "SELECT id, owner_id, name, client_id, client_secret_hash,
       client_secret_prefix, redirect_uris_json,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at
FROM oauth_applications";

const APP_SELECT_MYSQL: &str = "SELECT id, owner_id, name, client_id, client_secret_hash,
       client_secret_prefix, redirect_uris_json,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
FROM oauth_applications";

const APP_SELECT_SQLITE: &str = "SELECT id, owner_id, name, client_id, client_secret_hash,
       client_secret_prefix, redirect_uris_json,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at
FROM oauth_applications";

const CODE_SELECT_PG: &str = "SELECT id, application_id, user_id, redirect_uri, scopes,
       to_char(expires_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS expires_at,
       CASE WHEN used_at IS NULL THEN NULL
            ELSE to_char(used_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') END AS used_at,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM oauth_authorization_codes";

const CODE_SELECT_MYSQL: &str = "SELECT id, application_id, user_id, redirect_uri, scopes,
       DATE_FORMAT(expires_at, '%Y-%m-%dT%H:%i:%sZ') AS expires_at,
       CASE WHEN used_at IS NULL THEN NULL
            ELSE DATE_FORMAT(used_at, '%Y-%m-%dT%H:%i:%sZ') END AS used_at,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM oauth_authorization_codes";

const CODE_SELECT_SQLITE: &str = "SELECT id, application_id, user_id, redirect_uri, scopes,
       strftime('%Y-%m-%dT%H:%M:%SZ', expires_at) AS expires_at,
       CASE WHEN used_at IS NULL THEN NULL
            ELSE strftime('%Y-%m-%dT%H:%M:%SZ', used_at) END AS used_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at
FROM oauth_authorization_codes";

const TOKEN_SELECT_PG: &str = "SELECT id, application_id, user_id, token_prefix, token_hash, scopes,
       to_char(expires_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS expires_at,
       CASE WHEN revoked_at IS NULL THEN NULL
            ELSE to_char(revoked_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') END AS revoked_at,
       CASE WHEN last_used_at IS NULL THEN NULL
            ELSE to_char(last_used_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') END AS last_used_at,
       last_used_ip,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM oauth_access_tokens";

const TOKEN_SELECT_MYSQL: &str =
    "SELECT id, application_id, user_id, token_prefix, token_hash, scopes,
       DATE_FORMAT(expires_at, '%Y-%m-%dT%H:%i:%sZ') AS expires_at,
       CASE WHEN revoked_at IS NULL THEN NULL
            ELSE DATE_FORMAT(revoked_at, '%Y-%m-%dT%H:%i:%sZ') END AS revoked_at,
       CASE WHEN last_used_at IS NULL THEN NULL
            ELSE DATE_FORMAT(last_used_at, '%Y-%m-%dT%H:%i:%sZ') END AS last_used_at,
       last_used_ip,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM oauth_access_tokens";

const TOKEN_SELECT_SQLITE: &str =
    "SELECT id, application_id, user_id, token_prefix, token_hash, scopes,
       strftime('%Y-%m-%dT%H:%M:%SZ', expires_at) AS expires_at,
       CASE WHEN revoked_at IS NULL THEN NULL
            ELSE strftime('%Y-%m-%dT%H:%M:%SZ', revoked_at) END AS revoked_at,
       CASE WHEN last_used_at IS NULL THEN NULL
            ELSE strftime('%Y-%m-%dT%H:%M:%SZ', last_used_at) END AS last_used_at,
       last_used_ip,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at
FROM oauth_access_tokens";

// --- applications ---

#[allow(clippy::too_many_arguments)]
pub async fn insert_app(
    pool: &DbPool,
    id: &str,
    owner_id: &str,
    name: &str,
    client_id: &str,
    client_secret_hash: &str,
    client_secret_prefix: &str,
    redirect_uris_json: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO oauth_applications
(id, owner_id, name, client_id, client_secret_hash, client_secret_prefix, redirect_uris_json)
VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(id)
            .bind(owner_id)
            .bind(name)
            .bind(client_id)
            .bind(client_secret_hash)
            .bind(client_secret_prefix)
            .bind(redirect_uris_json)
            .execute(p)
            .await
            .map_err(|e| format!("insert oauth app failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO oauth_applications
(id, owner_id, name, client_id, client_secret_hash, client_secret_prefix, redirect_uris_json)
VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(owner_id)
            .bind(name)
            .bind(client_id)
            .bind(client_secret_hash)
            .bind(client_secret_prefix)
            .bind(redirect_uris_json)
            .execute(p)
            .await
            .map_err(|e| format!("insert oauth app failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO oauth_applications
(id, owner_id, name, client_id, client_secret_hash, client_secret_prefix, redirect_uris_json)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .bind(id)
            .bind(owner_id)
            .bind(name)
            .bind(client_id)
            .bind(client_secret_hash)
            .bind(client_secret_prefix)
            .bind(redirect_uris_json)
            .execute(p)
            .await
            .map_err(|e| format!("insert oauth app failed: {e}"))?;
        }
    }
    Ok(())
}

pub async fn find_app_by_id(pool: &DbPool, id: &str) -> Result<Option<OAuthAppRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{APP_SELECT_PG} WHERE id = $1"
            )))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find oauth app failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_app!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{APP_SELECT_MYSQL} WHERE id = ?"
            )))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find oauth app failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_app!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{APP_SELECT_SQLITE} WHERE id = ?1"
            )))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find oauth app failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_app!(&r))),
                None => Ok(None),
            }
        }
    }
}

pub async fn find_app_by_client_id(
    pool: &DbPool,
    client_id: &str,
) -> Result<Option<OAuthAppRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{APP_SELECT_PG} WHERE client_id = $1"
            )))
            .bind(client_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find oauth app failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_app!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{APP_SELECT_MYSQL} WHERE client_id = ?"
            )))
            .bind(client_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find oauth app failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_app!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{APP_SELECT_SQLITE} WHERE client_id = ?1"
            )))
            .bind(client_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find oauth app failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_app!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// Apps owned by a user, newest first.
pub async fn list_apps_for_owner(
    pool: &DbPool,
    owner_id: &str,
) -> Result<Vec<OAuthAppRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{APP_SELECT_PG} WHERE owner_id = $1 ORDER BY created_at DESC, id DESC"
            )))
            .bind(owner_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list oauth apps failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_app!(&r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{APP_SELECT_MYSQL} WHERE owner_id = ? ORDER BY created_at DESC, id DESC"
            )))
            .bind(owner_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list oauth apps failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_app!(&r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{APP_SELECT_SQLITE} WHERE owner_id = ?1 ORDER BY created_at DESC, id DESC"
            )))
            .bind(owner_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list oauth apps failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_app!(&r));
            }
            Ok(out)
        }
    }
}

/// Update display name and/or redirect URI set; bumps `updated_at`.
pub async fn update_app(
    pool: &DbPool,
    id: &str,
    name: &str,
    redirect_uris_json: &str,
    updated_at: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE oauth_applications
SET name = $2, redirect_uris_json = $3, updated_at = $4::timestamptz
WHERE id = $1",
            )
            .bind(id)
            .bind(name)
            .bind(redirect_uris_json)
            .bind(updated_at)
            .execute(p)
            .await
            .map_err(|e| format!("update oauth app failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE oauth_applications
SET name = ?, redirect_uris_json = ?, updated_at = ?
WHERE id = ?",
            )
            .bind(name)
            .bind(redirect_uris_json)
            .bind(updated_at)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("update oauth app failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE oauth_applications
SET name = ?2, redirect_uris_json = ?3, updated_at = ?4
WHERE id = ?1",
            )
            .bind(id)
            .bind(name)
            .bind(redirect_uris_json)
            .bind(updated_at)
            .execute(p)
            .await
            .map_err(|e| format!("update oauth app failed: {e}"))?;
        }
    }
    Ok(())
}

/// Rotate the client secret (hash + display prefix only — never plaintext).
pub async fn update_app_secret(
    pool: &DbPool,
    id: &str,
    client_secret_hash: &str,
    client_secret_prefix: &str,
    updated_at: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE oauth_applications
SET client_secret_hash = $2, client_secret_prefix = $3, updated_at = $4::timestamptz
WHERE id = $1",
            )
            .bind(id)
            .bind(client_secret_hash)
            .bind(client_secret_prefix)
            .bind(updated_at)
            .execute(p)
            .await
            .map_err(|e| format!("update oauth app secret failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE oauth_applications
SET client_secret_hash = ?, client_secret_prefix = ?, updated_at = ?
WHERE id = ?",
            )
            .bind(client_secret_hash)
            .bind(client_secret_prefix)
            .bind(updated_at)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("update oauth app secret failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE oauth_applications
SET client_secret_hash = ?2, client_secret_prefix = ?3, updated_at = ?4
WHERE id = ?1",
            )
            .bind(id)
            .bind(client_secret_hash)
            .bind(client_secret_prefix)
            .bind(updated_at)
            .execute(p)
            .await
            .map_err(|e| format!("update oauth app secret failed: {e}"))?;
        }
    }
    Ok(())
}

/// Hard delete — cascades authorization codes and access tokens via FK.
pub async fn delete_app(pool: &DbPool, id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM oauth_applications WHERE id = $1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete oauth app failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM oauth_applications WHERE id = ?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete oauth app failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM oauth_applications WHERE id = ?1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete oauth app failed: {e}"))?;
        }
    }
    Ok(())
}

// --- authorization codes ---

#[allow(clippy::too_many_arguments)]
pub async fn insert_code(
    pool: &DbPool,
    id: &str,
    code_hash: &str,
    application_id: &str,
    user_id: &str,
    redirect_uri: &str,
    scopes: &str,
    expires_at: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO oauth_authorization_codes
(id, code_hash, application_id, user_id, redirect_uri, scopes, expires_at)
VALUES ($1, $2, $3, $4, $5, $6, $7::timestamptz)",
            )
            .bind(id)
            .bind(code_hash)
            .bind(application_id)
            .bind(user_id)
            .bind(redirect_uri)
            .bind(scopes)
            .bind(expires_at)
            .execute(p)
            .await
            .map_err(|e| format!("insert oauth code failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO oauth_authorization_codes
(id, code_hash, application_id, user_id, redirect_uri, scopes, expires_at)
VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(code_hash)
            .bind(application_id)
            .bind(user_id)
            .bind(redirect_uri)
            .bind(scopes)
            .bind(expires_at)
            .execute(p)
            .await
            .map_err(|e| format!("insert oauth code failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO oauth_authorization_codes
(id, code_hash, application_id, user_id, redirect_uri, scopes, expires_at)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .bind(id)
            .bind(code_hash)
            .bind(application_id)
            .bind(user_id)
            .bind(redirect_uri)
            .bind(scopes)
            .bind(expires_at)
            .execute(p)
            .await
            .map_err(|e| format!("insert oauth code failed: {e}"))?;
        }
    }
    Ok(())
}

/// Atomically mark a code used and return its row. Returns `Ok(None)` when the
/// hash is unknown **or** the code was already consumed (single-use guarantee —
/// the `used_at IS NULL` predicate makes a second exchange a no-op).
pub async fn consume_code(
    pool: &DbPool,
    code_hash: &str,
    used_at: &str,
) -> Result<Option<OAuthCodeRow>, String> {
    let affected = match pool {
        DbPool::Postgres(p) => sqlx::query(
            "UPDATE oauth_authorization_codes SET used_at = $2::timestamptz
WHERE code_hash = $1 AND used_at IS NULL",
        )
        .bind(code_hash)
        .bind(used_at)
        .execute(p)
        .await
        .map_err(|e| format!("consume oauth code failed: {e}"))?
        .rows_affected(),
        DbPool::MySql(p) => sqlx::query(
            "UPDATE oauth_authorization_codes SET used_at = ?
WHERE code_hash = ? AND used_at IS NULL",
        )
        .bind(used_at)
        .bind(code_hash)
        .execute(p)
        .await
        .map_err(|e| format!("consume oauth code failed: {e}"))?
        .rows_affected(),
        DbPool::Sqlite(p) => sqlx::query(
            "UPDATE oauth_authorization_codes SET used_at = ?2
WHERE code_hash = ?1 AND used_at IS NULL",
        )
        .bind(code_hash)
        .bind(used_at)
        .execute(p)
        .await
        .map_err(|e| format!("consume oauth code failed: {e}"))?
        .rows_affected(),
    };
    if affected == 0 {
        return Ok(None);
    }
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{CODE_SELECT_PG} WHERE code_hash = $1"
            )))
            .bind(code_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("fetch oauth code failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_code!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{CODE_SELECT_MYSQL} WHERE code_hash = ?"
            )))
            .bind(code_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("fetch oauth code failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_code!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{CODE_SELECT_SQLITE} WHERE code_hash = ?1"
            )))
            .bind(code_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("fetch oauth code failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_code!(&r))),
                None => Ok(None),
            }
        }
    }
}

// --- access tokens ---

#[allow(clippy::too_many_arguments)]
pub async fn insert_token(
    pool: &DbPool,
    id: &str,
    application_id: &str,
    user_id: &str,
    token_prefix: &str,
    token_hash: &str,
    scopes: &str,
    expires_at: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO oauth_access_tokens
(id, application_id, user_id, token_prefix, token_hash, scopes, expires_at)
VALUES ($1, $2, $3, $4, $5, $6, $7::timestamptz)",
            )
            .bind(id)
            .bind(application_id)
            .bind(user_id)
            .bind(token_prefix)
            .bind(token_hash)
            .bind(scopes)
            .bind(expires_at)
            .execute(p)
            .await
            .map_err(|e| format!("insert oauth token failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO oauth_access_tokens
(id, application_id, user_id, token_prefix, token_hash, scopes, expires_at)
VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(application_id)
            .bind(user_id)
            .bind(token_prefix)
            .bind(token_hash)
            .bind(scopes)
            .bind(expires_at)
            .execute(p)
            .await
            .map_err(|e| format!("insert oauth token failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO oauth_access_tokens
(id, application_id, user_id, token_prefix, token_hash, scopes, expires_at)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .bind(id)
            .bind(application_id)
            .bind(user_id)
            .bind(token_prefix)
            .bind(token_hash)
            .bind(scopes)
            .bind(expires_at)
            .execute(p)
            .await
            .map_err(|e| format!("insert oauth token failed: {e}"))?;
        }
    }
    Ok(())
}

/// Lookup by SHA-256 hex. Returns `None` when missing or revoked
/// (expiry is enforced by the caller — `expires_at` is returned for the check).
pub async fn find_token_by_hash(
    pool: &DbPool,
    token_hash: &str,
) -> Result<Option<OAuthTokenRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TOKEN_SELECT_PG} WHERE token_hash = $1 AND revoked_at IS NULL"
            )))
            .bind(token_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find oauth token failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_token!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TOKEN_SELECT_MYSQL} WHERE token_hash = ? AND revoked_at IS NULL"
            )))
            .bind(token_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find oauth token failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_token!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TOKEN_SELECT_SQLITE} WHERE token_hash = ?1 AND revoked_at IS NULL"
            )))
            .bind(token_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find oauth token failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_token!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// Active (non-revoked) tokens for a user — grant listing groups by app.
pub async fn list_active_tokens_for_user(
    pool: &DbPool,
    user_id: &str,
) -> Result<Vec<OAuthTokenRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TOKEN_SELECT_PG} WHERE user_id = $1 AND revoked_at IS NULL
ORDER BY created_at DESC, id DESC"
            )))
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list oauth tokens failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_token!(&r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TOKEN_SELECT_MYSQL} WHERE user_id = ? AND revoked_at IS NULL
ORDER BY created_at DESC, id DESC"
            )))
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list oauth tokens failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_token!(&r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TOKEN_SELECT_SQLITE} WHERE user_id = ?1 AND revoked_at IS NULL
ORDER BY created_at DESC, id DESC"
            )))
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list oauth tokens failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_token!(&r));
            }
            Ok(out)
        }
    }
}

/// Soft-revoke every active token a user holds for one application
/// (the "revoke grant" path). Returns the number of tokens revoked.
pub async fn revoke_tokens_for_user_app(
    pool: &DbPool,
    user_id: &str,
    application_id: &str,
    revoked_at: &str,
) -> Result<u64, String> {
    let affected = match pool {
        DbPool::Postgres(p) => sqlx::query(
            "UPDATE oauth_access_tokens SET revoked_at = $3::timestamptz
WHERE user_id = $1 AND application_id = $2 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .bind(application_id)
        .bind(revoked_at)
        .execute(p)
        .await
        .map_err(|e| format!("revoke oauth tokens failed: {e}"))?
        .rows_affected(),
        DbPool::MySql(p) => sqlx::query(
            "UPDATE oauth_access_tokens SET revoked_at = ?
WHERE user_id = ? AND application_id = ? AND revoked_at IS NULL",
        )
        .bind(revoked_at)
        .bind(user_id)
        .bind(application_id)
        .execute(p)
        .await
        .map_err(|e| format!("revoke oauth tokens failed: {e}"))?
        .rows_affected(),
        DbPool::Sqlite(p) => sqlx::query(
            "UPDATE oauth_access_tokens SET revoked_at = ?3
WHERE user_id = ?1 AND application_id = ?2 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .bind(application_id)
        .bind(revoked_at)
        .execute(p)
        .await
        .map_err(|e| format!("revoke oauth tokens failed: {e}"))?
        .rows_affected(),
    };
    Ok(affected)
}

/// Update last-used timestamp and optional client IP after token auth.
pub async fn touch_token_last_used(
    pool: &DbPool,
    id: &str,
    last_used_at: &str,
    last_used_ip: Option<&str>,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE oauth_access_tokens
SET last_used_at = $2::timestamptz, last_used_ip = $3
WHERE id = $1 AND revoked_at IS NULL",
            )
            .bind(id)
            .bind(last_used_at)
            .bind(last_used_ip)
            .execute(p)
            .await
            .map_err(|e| format!("touch oauth token failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE oauth_access_tokens SET last_used_at = ?, last_used_ip = ?
WHERE id = ? AND revoked_at IS NULL",
            )
            .bind(last_used_at)
            .bind(last_used_ip)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("touch oauth token failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE oauth_access_tokens SET last_used_at = ?2, last_used_ip = ?3
WHERE id = ?1 AND revoked_at IS NULL",
            )
            .bind(id)
            .bind(last_used_at)
            .bind(last_used_ip)
            .execute(p)
            .await
            .map_err(|e| format!("touch oauth token failed: {e}"))?;
        }
    }
    Ok(())
}
