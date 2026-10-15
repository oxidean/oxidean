//! Two-factor credential storage via `DbPool` match — TOTP secrets (encrypted),
//! recovery-code hashes, and WebAuthn/passkey credentials (#130).

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct TotpRow {
    pub user_id: String,
    /// AES-256-GCM ciphertext (nonce||ct hex) of the base32 TOTP secret.
    pub secret_enc: String,
    /// `Some` once the user proved a code — pending enrollment otherwise.
    pub confirmed_at: Option<String>,
    pub last_used_at: Option<String>,
    pub created_at: String,
}

impl TotpRow {
    pub fn is_confirmed(&self) -> bool {
        self.confirmed_at.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct PasskeyRow {
    pub id: String,
    pub user_id: String,
    /// base64url credential id (unique).
    pub credential_id: String,
    pub nickname: String,
    /// Serialized `webauthn-rs` Passkey (public key + metadata) — JSON text.
    pub passkey_json: String,
    pub transports: String,
    pub sign_count: i64,
    pub last_used_at: Option<String>,
    pub created_at: String,
}

macro_rules! map_totp {
    ($row:expr) => {{
        let row = $row;
        TotpRow {
            user_id: row
                .try_get("user_id")
                .map_err(|e| format!("totp row: {e}"))?,
            secret_enc: row
                .try_get("secret_enc")
                .map_err(|e| format!("totp row: {e}"))?,
            confirmed_at: row
                .try_get::<Option<String>, _>("confirmed_at")
                .unwrap_or(None),
            last_used_at: row
                .try_get::<Option<String>, _>("last_used_at")
                .unwrap_or(None),
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("totp row: {e}"))?,
        }
    }};
}

macro_rules! map_passkey {
    ($row:expr) => {{
        let row = $row;
        PasskeyRow {
            id: row.try_get("id").map_err(|e| format!("passkey row: {e}"))?,
            user_id: row
                .try_get("user_id")
                .map_err(|e| format!("passkey row: {e}"))?,
            credential_id: row
                .try_get("credential_id")
                .map_err(|e| format!("passkey row: {e}"))?,
            nickname: row
                .try_get("nickname")
                .map_err(|e| format!("passkey row: {e}"))?,
            passkey_json: row
                .try_get("passkey_json")
                .map_err(|e| format!("passkey row: {e}"))?,
            transports: row
                .try_get::<Option<String>, _>("transports")
                .unwrap_or(None)
                .unwrap_or_default(),
            sign_count: row
                .try_get::<i64, _>("sign_count")
                .unwrap_or(0),
            last_used_at: row
                .try_get::<Option<String>, _>("last_used_at")
                .unwrap_or(None),
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("passkey row: {e}"))?,
        }
    }};
}

const TOTP_SELECT_PG: &str = "SELECT user_id, secret_enc,
       to_char(confirmed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS confirmed_at,
       to_char(last_used_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS last_used_at,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM user_totp_secrets";

const TOTP_SELECT_MYSQL: &str = "SELECT user_id, secret_enc,
       DATE_FORMAT(confirmed_at, '%Y-%m-%dT%H:%i:%sZ') AS confirmed_at,
       DATE_FORMAT(last_used_at, '%Y-%m-%dT%H:%i:%sZ') AS last_used_at,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM user_totp_secrets";

const TOTP_SELECT_SQLITE: &str = "SELECT user_id, secret_enc,
       strftime('%Y-%m-%dT%H:%M:%SZ', confirmed_at) AS confirmed_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', last_used_at) AS last_used_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at
FROM user_totp_secrets";

const PASSKEY_SELECT_PG: &str = "SELECT id, user_id, credential_id, nickname, passkey_json,
       transports, sign_count,
       to_char(last_used_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS last_used_at,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM webauthn_credentials";

const PASSKEY_SELECT_MYSQL: &str = "SELECT id, user_id, credential_id, nickname, passkey_json,
       transports, sign_count,
       DATE_FORMAT(last_used_at, '%Y-%m-%dT%H:%i:%sZ') AS last_used_at,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM webauthn_credentials";

const PASSKEY_SELECT_SQLITE: &str = "SELECT id, user_id, credential_id, nickname, passkey_json,
       transports, sign_count,
       strftime('%Y-%m-%dT%H:%M:%SZ', last_used_at) AS last_used_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at
FROM webauthn_credentials";

// ---------- TOTP ----------

/// Insert or replace the user's TOTP secret. A replaced secret resets to
/// pending (`confirmed_at NULL`) until the new code is verified.
pub async fn totp_upsert(pool: &DbPool, user_id: &str, secret_enc: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO user_totp_secrets (user_id, secret_enc)
VALUES ($1, $2)
ON CONFLICT (user_id) DO UPDATE
SET secret_enc = EXCLUDED.secret_enc, confirmed_at = NULL, last_used_at = NULL, updated_at = now()",
            )
            .bind(user_id)
            .bind(secret_enc)
            .execute(p)
            .await
            .map(|_| ())
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO user_totp_secrets (user_id, secret_enc)
VALUES (?, ?)
ON DUPLICATE KEY UPDATE
  secret_enc = VALUES(secret_enc), confirmed_at = NULL, last_used_at = NULL, updated_at = NOW()",
            )
            .bind(user_id)
            .bind(secret_enc)
            .execute(p)
            .await
            .map(|_| ())
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO user_totp_secrets (user_id, secret_enc)
VALUES (?1, ?2)
ON CONFLICT (user_id) DO UPDATE
SET secret_enc = excluded.secret_enc, confirmed_at = NULL, last_used_at = NULL,
    updated_at = strftime('%Y-%m-%d %H:%M:%S','now')",
            )
            .bind(user_id)
            .bind(secret_enc)
            .execute(p)
            .await
            .map(|_| ())
        }
    }
    .map_err(|e| format!("totp upsert failed: {e}"))?;
    Ok(())
}

pub async fn totp_find(pool: &DbPool, user_id: &str) -> Result<Option<TotpRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TOTP_SELECT_PG} WHERE user_id = $1"
            )))
            .bind(user_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("totp find failed: {e}"))?;
            Ok(match row { Some(r) => Some(map_totp!(&r)), None => None })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TOTP_SELECT_MYSQL} WHERE user_id = ?"
            )))
            .bind(user_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("totp find failed: {e}"))?;
            Ok(match row { Some(r) => Some(map_totp!(&r)), None => None })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TOTP_SELECT_SQLITE} WHERE user_id = ?1"
            )))
            .bind(user_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("totp find failed: {e}"))?;
            Ok(match row { Some(r) => Some(map_totp!(&r)), None => None })
        }
    }
}

/// Mark a pending TOTP secret confirmed.
pub async fn totp_confirm(pool: &DbPool, user_id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE user_totp_secrets SET confirmed_at = now(), updated_at = now() WHERE user_id = $1")
                .bind(user_id)
                .execute(p)
                .await
                .map(|_| ())
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE user_totp_secrets SET confirmed_at = NOW(), updated_at = NOW() WHERE user_id = ?")
                .bind(user_id)
                .execute(p)
                .await
                .map(|_| ())
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE user_totp_secrets
SET confirmed_at = strftime('%Y-%m-%d %H:%M:%S','now'),
    updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
WHERE user_id = ?1",
            )
            .bind(user_id)
            .execute(p)
            .await
            .map(|_| ())
        }
    }
    .map_err(|e| format!("totp confirm failed: {e}"))?;
    Ok(())
}

pub async fn totp_touch_last_used(pool: &DbPool, user_id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE user_totp_secrets SET last_used_at = now() WHERE user_id = $1")
                .bind(user_id)
                .execute(p)
                .await
                .map(|_| ())
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE user_totp_secrets SET last_used_at = NOW() WHERE user_id = ?")
                .bind(user_id)
                .execute(p)
                .await
                .map(|_| ())
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE user_totp_secrets SET last_used_at = strftime('%Y-%m-%d %H:%M:%S','now') WHERE user_id = ?1",
            )
            .bind(user_id)
            .execute(p)
            .await
            .map(|_| ())
        }
    }
    .map_err(|e| format!("totp last_used update failed: {e}"))?;
    Ok(())
}

pub async fn totp_delete(pool: &DbPool, user_id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query("DELETE FROM user_totp_secrets WHERE user_id = $1")
            .bind(user_id)
            .execute(p)
            .await
            .map(|_| ()),
        DbPool::MySql(p) => sqlx::query("DELETE FROM user_totp_secrets WHERE user_id = ?")
            .bind(user_id)
            .execute(p)
            .await
            .map(|_| ()),
        DbPool::Sqlite(p) => sqlx::query("DELETE FROM user_totp_secrets WHERE user_id = ?1")
            .bind(user_id)
            .execute(p)
            .await
            .map(|_| ()),
    }
    .map_err(|e| format!("totp delete failed: {e}"))?;
    Ok(())
}

// ---------- recovery codes ----------

/// Replace all recovery codes for a user (issue-time regenerate).
/// `codes` is `(row id, code_hash)` pairs — hashes only, never plaintext.
pub async fn recovery_replace(
    pool: &DbPool,
    user_id: &str,
    codes: &[(String, String)],
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM user_recovery_codes WHERE user_id = $1")
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("recovery clear failed: {e}"))?;
            for (id, hash) in codes {
                sqlx::query(
                    "INSERT INTO user_recovery_codes (id, user_id, code_hash) VALUES ($1, $2, $3)",
                )
                .bind(id)
                .bind(user_id)
                .bind(hash)
                .execute(p)
                .await
                .map_err(|e| format!("recovery insert failed: {e}"))?;
            }
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM user_recovery_codes WHERE user_id = ?")
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("recovery clear failed: {e}"))?;
            for (id, hash) in codes {
                sqlx::query(
                    "INSERT INTO user_recovery_codes (id, user_id, code_hash) VALUES (?, ?, ?)",
                )
                .bind(id)
                .bind(user_id)
                .bind(hash)
                .execute(p)
                .await
                .map_err(|e| format!("recovery insert failed: {e}"))?;
            }
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM user_recovery_codes WHERE user_id = ?1")
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("recovery clear failed: {e}"))?;
            for (id, hash) in codes {
                sqlx::query(
                    "INSERT INTO user_recovery_codes (id, user_id, code_hash) VALUES (?1, ?2, ?3)",
                )
                .bind(id)
                .bind(user_id)
                .bind(hash)
                .execute(p)
                .await
                .map_err(|e| format!("recovery insert failed: {e}"))?;
            }
        }
    }
    Ok(())
}

/// Atomically consume a recovery code: mark `used_at` only when still unused.
/// Returns true when this call was the one that consumed it.
pub async fn recovery_consume(
    pool: &DbPool,
    user_id: &str,
    code_hash: &str,
) -> Result<bool, String> {
    let affected = match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE user_recovery_codes SET used_at = now()
WHERE user_id = $1 AND code_hash = $2 AND used_at IS NULL",
            )
            .bind(user_id)
            .bind(code_hash)
            .execute(p)
            .await
            .map(|r| r.rows_affected())
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE user_recovery_codes SET used_at = NOW()
WHERE user_id = ? AND code_hash = ? AND used_at IS NULL",
            )
            .bind(user_id)
            .bind(code_hash)
            .execute(p)
            .await
            .map(|r| r.rows_affected())
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE user_recovery_codes
SET used_at = strftime('%Y-%m-%d %H:%M:%S','now')
WHERE user_id = ?1 AND code_hash = ?2 AND used_at IS NULL",
            )
            .bind(user_id)
            .bind(code_hash)
            .execute(p)
            .await
            .map(|r| r.rows_affected())
        }
    }
    .map_err(|e| format!("recovery consume failed: {e}"))?;
    Ok(affected > 0)
}

/// Remaining (unused) recovery codes for a user.
pub async fn recovery_count_remaining(pool: &DbPool, user_id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM user_recovery_codes WHERE user_id = $1 AND used_at IS NULL",
            )
            .bind(user_id)
            .fetch_one(p)
            .await
        }
        DbPool::MySql(p) => {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM user_recovery_codes WHERE user_id = ? AND used_at IS NULL",
            )
            .bind(user_id)
            .fetch_one(p)
            .await
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM user_recovery_codes WHERE user_id = ?1 AND used_at IS NULL",
            )
            .bind(user_id)
            .fetch_one(p)
            .await
        }
    }
    .map_err(|e| format!("recovery count failed: {e}"))
}

pub async fn recovery_delete_all(pool: &DbPool, user_id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query("DELETE FROM user_recovery_codes WHERE user_id = $1")
            .bind(user_id)
            .execute(p)
            .await
            .map(|_| ()),
        DbPool::MySql(p) => sqlx::query("DELETE FROM user_recovery_codes WHERE user_id = ?")
            .bind(user_id)
            .execute(p)
            .await
            .map(|_| ()),
        DbPool::Sqlite(p) => sqlx::query("DELETE FROM user_recovery_codes WHERE user_id = ?1")
            .bind(user_id)
            .execute(p)
            .await
            .map(|_| ()),
    }
    .map_err(|e| format!("recovery delete failed: {e}"))?;
    Ok(())
}

// ---------- WebAuthn / passkeys ----------

#[allow(clippy::too_many_arguments)]
pub async fn passkey_insert(
    pool: &DbPool,
    id: &str,
    user_id: &str,
    credential_id: &str,
    nickname: &str,
    passkey_json: &str,
    transports: &str,
    sign_count: i64,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO webauthn_credentials
(id, user_id, credential_id, nickname, passkey_json, transports, sign_count)
VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(id)
            .bind(user_id)
            .bind(credential_id)
            .bind(nickname)
            .bind(passkey_json)
            .bind(transports)
            .bind(sign_count)
            .execute(p)
            .await
            .map(|_| ())
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO webauthn_credentials
(id, user_id, credential_id, nickname, passkey_json, transports, sign_count)
VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(user_id)
            .bind(credential_id)
            .bind(nickname)
            .bind(passkey_json)
            .bind(transports)
            .bind(sign_count)
            .execute(p)
            .await
            .map(|_| ())
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO webauthn_credentials
(id, user_id, credential_id, nickname, passkey_json, transports, sign_count)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .bind(id)
            .bind(user_id)
            .bind(credential_id)
            .bind(nickname)
            .bind(passkey_json)
            .bind(transports)
            .bind(sign_count)
            .execute(p)
            .await
            .map(|_| ())
        }
    }
    .map_err(|e| format!("passkey insert failed: {e}"))?;
    Ok(())
}

pub async fn passkey_list_for_user(
    pool: &DbPool,
    user_id: &str,
) -> Result<Vec<PasskeyRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{PASSKEY_SELECT_PG} WHERE user_id = $1 ORDER BY created_at"
            )))
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("passkey list failed: {e}"))?;
            rows.iter().map(|r| Ok(map_passkey!(r))).collect()
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{PASSKEY_SELECT_MYSQL} WHERE user_id = ? ORDER BY created_at"
            )))
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("passkey list failed: {e}"))?;
            rows.iter().map(|r| Ok(map_passkey!(r))).collect()
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{PASSKEY_SELECT_SQLITE} WHERE user_id = ?1 ORDER BY created_at"
            )))
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("passkey list failed: {e}"))?;
            rows.iter().map(|r| Ok(map_passkey!(r))).collect()
        }
    }
}

pub async fn passkey_find_for_user(
    pool: &DbPool,
    user_id: &str,
    id: &str,
) -> Result<Option<PasskeyRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{PASSKEY_SELECT_PG} WHERE user_id = $1 AND id = $2"
            )))
            .bind(user_id)
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("passkey find failed: {e}"))?;
            Ok(match row { Some(r) => Some(map_passkey!(&r)), None => None })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{PASSKEY_SELECT_MYSQL} WHERE user_id = ? AND id = ?"
            )))
            .bind(user_id)
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("passkey find failed: {e}"))?;
            Ok(match row { Some(r) => Some(map_passkey!(&r)), None => None })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{PASSKEY_SELECT_SQLITE} WHERE user_id = ?1 AND id = ?2"
            )))
            .bind(user_id)
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("passkey find failed: {e}"))?;
            Ok(match row { Some(r) => Some(map_passkey!(&r)), None => None })
        }
    }
}

/// Rename a credential. Returns false when the row was not the user's.
pub async fn passkey_update_nickname(
    pool: &DbPool,
    user_id: &str,
    id: &str,
    nickname: &str,
) -> Result<bool, String> {
    let affected = match pool {
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE webauthn_credentials SET nickname = $3 WHERE id = $1 AND user_id = $2")
                .bind(id)
                .bind(user_id)
                .bind(nickname)
                .execute(p)
                .await
                .map(|r| r.rows_affected())
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE webauthn_credentials SET nickname = ? WHERE id = ? AND user_id = ?")
                .bind(nickname)
                .bind(id)
                .bind(user_id)
                .execute(p)
                .await
                .map(|r| r.rows_affected())
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE webauthn_credentials SET nickname = ?3 WHERE id = ?1 AND user_id = ?2",
            )
            .bind(id)
            .bind(user_id)
            .bind(nickname)
            .execute(p)
            .await
            .map(|r| r.rows_affected())
        }
    }
    .map_err(|e| format!("passkey rename failed: {e}"))?;
    Ok(affected > 0)
}

/// Persist the post-assertion credential state (counter, JSON, last_used).
pub async fn passkey_after_assertion(
    pool: &DbPool,
    id: &str,
    sign_count: i64,
    passkey_json: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE webauthn_credentials
SET sign_count = $2, passkey_json = $3, last_used_at = now()
WHERE id = $1",
            )
            .bind(id)
            .bind(sign_count)
            .bind(passkey_json)
            .execute(p)
            .await
            .map(|_| ())
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE webauthn_credentials
SET sign_count = ?, passkey_json = ?, last_used_at = NOW()
WHERE id = ?",
            )
            .bind(sign_count)
            .bind(passkey_json)
            .bind(id)
            .execute(p)
            .await
            .map(|_| ())
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE webauthn_credentials
SET sign_count = ?2, passkey_json = ?3,
    last_used_at = strftime('%Y-%m-%d %H:%M:%S','now')
WHERE id = ?1",
            )
            .bind(id)
            .bind(sign_count)
            .bind(passkey_json)
            .execute(p)
            .await
            .map(|_| ())
        }
    }
    .map_err(|e| format!("passkey assertion update failed: {e}"))?;
    Ok(())
}

/// Delete one credential owned by the user. Returns false when not found.
pub async fn passkey_delete(pool: &DbPool, user_id: &str, id: &str) -> Result<bool, String> {
    let affected = match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM webauthn_credentials WHERE id = $1 AND user_id = $2")
                .bind(id)
                .bind(user_id)
                .execute(p)
                .await
                .map(|r| r.rows_affected())
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM webauthn_credentials WHERE id = ? AND user_id = ?")
                .bind(id)
                .bind(user_id)
                .execute(p)
                .await
                .map(|r| r.rows_affected())
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM webauthn_credentials WHERE id = ?1 AND user_id = ?2")
                .bind(id)
                .bind(user_id)
                .execute(p)
                .await
                .map(|r| r.rows_affected())
        }
    }
    .map_err(|e| format!("passkey delete failed: {e}"))?;
    Ok(affected > 0)
}

pub async fn passkey_delete_all_for_user(pool: &DbPool, user_id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query("DELETE FROM webauthn_credentials WHERE user_id = $1")
            .bind(user_id)
            .execute(p)
            .await
            .map(|_| ()),
        DbPool::MySql(p) => sqlx::query("DELETE FROM webauthn_credentials WHERE user_id = ?")
            .bind(user_id)
            .execute(p)
            .await
            .map(|_| ()),
        DbPool::Sqlite(p) => sqlx::query("DELETE FROM webauthn_credentials WHERE user_id = ?1")
            .bind(user_id)
            .execute(p)
            .await
            .map(|_| ()),
    }
    .map_err(|e| format!("passkey delete-all failed: {e}"))?;
    Ok(())
}

// ---------- policy helpers ----------

/// True when the user has at least one active factor: confirmed TOTP or any
/// registered passkey.
pub async fn user_has_confirmed_factor(pool: &DbPool, user_id: &str) -> Result<bool, String> {
    let n: i64 = match pool {
        DbPool::Postgres(p) => {
            sqlx::query_scalar::<_, i64>(
                "SELECT (SELECT COUNT(*) FROM user_totp_secrets WHERE user_id = $1 AND confirmed_at IS NOT NULL)
              + (SELECT COUNT(*) FROM webauthn_credentials WHERE user_id = $1)",
            )
            .bind(user_id)
            .fetch_one(p)
            .await
        }
        DbPool::MySql(p) => {
            sqlx::query_scalar::<_, i64>(
                "SELECT (SELECT COUNT(*) FROM user_totp_secrets WHERE user_id = ? AND confirmed_at IS NOT NULL)
              + (SELECT COUNT(*) FROM webauthn_credentials WHERE user_id = ?)",
            )
            .bind(user_id)
            .bind(user_id)
            .fetch_one(p)
            .await
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar::<_, i64>(
                "SELECT (SELECT COUNT(*) FROM user_totp_secrets WHERE user_id = ?1 AND confirmed_at IS NOT NULL)
              + (SELECT COUNT(*) FROM webauthn_credentials WHERE user_id = ?1)",
            )
            .bind(user_id)
            .fetch_one(p)
            .await
        }
    }
    .map_err(|e| format!("factor presence check failed: {e}"))?;
    Ok(n > 0)
}

/// True when the user belongs to at least one organization that requires 2FA.
pub async fn user_in_org_requiring_two_factor(
    pool: &DbPool,
    user_id: &str,
) -> Result<bool, String> {
    let n: i64 = match pool {
        DbPool::Postgres(p) => {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM organizations o
JOIN organization_members m ON m.org_id = o.id
WHERE m.user_id = $1 AND o.require_two_factor",
            )
            .bind(user_id)
            .fetch_one(p)
            .await
        }
        DbPool::MySql(p) => {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM organizations o
JOIN organization_members m ON m.org_id = o.id
WHERE m.user_id = ? AND o.require_two_factor <> 0",
            )
            .bind(user_id)
            .fetch_one(p)
            .await
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM organizations o
JOIN organization_members m ON m.org_id = o.id
WHERE m.user_id = ?1 AND o.require_two_factor != 0",
            )
            .bind(user_id)
            .fetch_one(p)
            .await
        }
    }
    .map_err(|e| format!("org 2fa policy check failed: {e}"))?;
    Ok(n > 0)
}
