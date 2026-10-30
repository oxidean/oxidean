//! Singleton `oxpilot_settings` row (id = 1) — U4 sparse provider/settings
//! overrides over `OXIDEAN_OXPILOT_*` env.
//!
//! Every override column is nullable: NULL = "follow the env default", a
//! set value wins field-by-field (the `admin.mcp.*` env-default/override
//! split restated for Oxpilot). `key_ciphertext` + `key_last4` are the
//! write-only custody pair — they move together inside every write
//! statement (NFR1.5) and reads return them verbatim for the API layer to
//! *not* surface.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OxpilotSettingsRow {
    /// NULL = follow `OXIDEAN_OXPILOT_ENABLED`.
    pub enabled_override: Option<bool>,
    /// NULL = follow `OXIDEAN_OXPILOT_PROVIDER`.
    pub provider_id: Option<String>,
    /// NULL = follow `OXIDEAN_OXPILOT_MODEL` / provider default.
    pub model: Option<String>,
    /// NULL = follow `OXIDEAN_OXPILOT_BASE_URL` / adapter default.
    pub base_url: Option<String>,
    /// AES-256-GCM `hex(nonce || ciphertext)` — always moves with
    /// `key_last4` (NFR1.5); never emitted on any read surface.
    pub key_ciphertext: Option<String>,
    /// Last 4 chars of the stored key — the only key material a read may
    /// surface.
    pub key_last4: Option<String>,
    /// NULL = follow `OXIDEAN_OXPILOT_USER_BUDGET`.
    pub user_budget: Option<i64>,
    /// NULL = follow `OXIDEAN_OXPILOT_INSTANCE_BUDGET`.
    pub instance_budget: Option<i64>,
    /// Server-set on every write; absent when the row was never read.
    pub updated_at: String,
}

/// Dialect-tolerant decode: Postgres BOOLEAN/BIGINT, MySQL TINYINT(1)/BIGINT,
/// SQLite INTEGER — same fallback chain as `mcp_settings`.
macro_rules! map_settings {
    ($row:expr) => {{
        let row = $row;
        let bool_at = |name: &str| -> Result<Option<bool>, String> {
            row.try_get::<Option<bool>, _>(name)
                .or_else(|_| {
                    row.try_get::<Option<i64>, _>(name)
                        .map(|v| v.map(|n| n != 0))
                })
                .or_else(|_| {
                    row.try_get::<Option<i8>, _>(name)
                        .map(|v| v.map(|n| n != 0))
                })
                .map_err(|e| format!("oxpilot settings row {name}: {e}"))
        };
        let int_at = |name: &str| -> Result<Option<i64>, String> {
            row.try_get::<Option<i64>, _>(name)
                .or_else(|_| {
                    row.try_get::<Option<i32>, _>(name)
                        .map(|v| v.map(i64::from))
                })
                .or_else(|_| {
                    row.try_get::<Option<i16>, _>(name)
                        .map(|v| v.map(i64::from))
                })
                .or_else(|_| row.try_get::<Option<i8>, _>(name).map(|v| v.map(i64::from)))
                .map_err(|e| format!("oxpilot settings row {name}: {e}"))
        };
        OxpilotSettingsRow {
            enabled_override: bool_at("enabled_override")?,
            provider_id: row
                .try_get("provider_id")
                .map_err(|e| format!("oxpilot settings row provider_id: {e}"))?,
            model: row
                .try_get("model")
                .map_err(|e| format!("oxpilot settings row model: {e}"))?,
            base_url: row
                .try_get("base_url")
                .map_err(|e| format!("oxpilot settings row base_url: {e}"))?,
            key_ciphertext: row
                .try_get("key_ciphertext")
                .map_err(|e| format!("oxpilot settings row key_ciphertext: {e}"))?,
            key_last4: row
                .try_get("key_last4")
                .map_err(|e| format!("oxpilot settings row key_last4: {e}"))?,
            user_budget: int_at("user_budget")?,
            instance_budget: int_at("instance_budget")?,
            updated_at: row
                .try_get::<String, _>("updated_at")
                .or_else(|_| {
                    // MySQL/PG timestamp → TEXT fallback.
                    row.try_get::<chrono::NaiveDateTime, _>("updated_at")
                        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
                })
                .or_else(|_| {
                    row.try_get::<chrono::DateTime<chrono::Utc>, _>("updated_at")
                        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
                })
                .map_err(|e| format!("oxpilot settings row updated_at: {e}"))?,
        }
    }};
}

const SELECT: &str = "SELECT enabled_override, provider_id, model, base_url, \
     key_ciphertext, key_last4, user_budget, instance_budget, updated_at \
     FROM oxpilot_settings WHERE id = 1";

/// Read the singleton row. `Ok(None)` when the row is absent — the
/// migration seeds it, but a pre-migration or hand-cleared database reads
/// as "all overrides unset" rather than an error (the boot rebuild's
/// degraded path maps read *failures* to env-only anyway).
pub async fn get_row(pool: &DbPool) -> Result<Option<OxpilotSettingsRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(SELECT)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("get_oxpilot_settings: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_settings!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(SELECT)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("get_oxpilot_settings: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_settings!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(SELECT)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("get_oxpilot_settings: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_settings!(&r)),
                None => None,
            })
        }
    }
}

/// Write the whole override row in ONE statement — the key pair moves
/// together by construction (NFR1.5), `updated_at` is server-set, and an
/// absent singleton row is re-created (dialect-native upsert).
pub async fn upsert_row(
    pool: &DbPool,
    row: &OxpilotSettingsRow,
) -> Result<OxpilotSettingsRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO oxpilot_settings \
                 (id, enabled_override, provider_id, model, base_url, \
                  key_ciphertext, key_last4, user_budget, instance_budget, updated_at) \
                 VALUES (1, $1, $2, $3, $4, $5, $6, $7, $8, NOW()) \
                 ON CONFLICT (id) DO UPDATE SET \
                   enabled_override = EXCLUDED.enabled_override, \
                   provider_id = EXCLUDED.provider_id, \
                   model = EXCLUDED.model, \
                   base_url = EXCLUDED.base_url, \
                   key_ciphertext = EXCLUDED.key_ciphertext, \
                   key_last4 = EXCLUDED.key_last4, \
                   user_budget = EXCLUDED.user_budget, \
                   instance_budget = EXCLUDED.instance_budget, \
                   updated_at = NOW()",
            )
            .bind(row.enabled_override)
            .bind(&row.provider_id)
            .bind(&row.model)
            .bind(&row.base_url)
            .bind(&row.key_ciphertext)
            .bind(&row.key_last4)
            .bind(row.user_budget)
            .bind(row.instance_budget)
            .execute(p)
            .await
            .map_err(|e| format!("upsert_oxpilot_settings: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO oxpilot_settings \
                 (id, enabled_override, provider_id, model, base_url, \
                  key_ciphertext, key_last4, user_budget, instance_budget, updated_at) \
                 VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP(3)) \
                 ON DUPLICATE KEY UPDATE \
                   enabled_override = VALUES(enabled_override), \
                   provider_id = VALUES(provider_id), \
                   model = VALUES(model), \
                   base_url = VALUES(base_url), \
                   key_ciphertext = VALUES(key_ciphertext), \
                   key_last4 = VALUES(key_last4), \
                   user_budget = VALUES(user_budget), \
                   instance_budget = VALUES(instance_budget), \
                   updated_at = CURRENT_TIMESTAMP(3)",
            )
            .bind(row.enabled_override)
            .bind(&row.provider_id)
            .bind(&row.model)
            .bind(&row.base_url)
            .bind(&row.key_ciphertext)
            .bind(&row.key_last4)
            .bind(row.user_budget)
            .bind(row.instance_budget)
            .execute(p)
            .await
            .map_err(|e| format!("upsert_oxpilot_settings: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO oxpilot_settings \
                 (id, enabled_override, provider_id, model, base_url, \
                  key_ciphertext, key_last4, user_budget, instance_budget, updated_at) \
                 VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, strftime('%Y-%m-%d %H:%M:%S','now')) \
                 ON CONFLICT(id) DO UPDATE SET \
                   enabled_override = excluded.enabled_override, \
                   provider_id = excluded.provider_id, \
                   model = excluded.model, \
                   base_url = excluded.base_url, \
                   key_ciphertext = excluded.key_ciphertext, \
                   key_last4 = excluded.key_last4, \
                   user_budget = excluded.user_budget, \
                   instance_budget = excluded.instance_budget, \
                   updated_at = strftime('%Y-%m-%d %H:%M:%S','now')",
            )
            .bind(row.enabled_override)
            .bind(&row.provider_id)
            .bind(&row.model)
            .bind(&row.base_url)
            .bind(&row.key_ciphertext)
            .bind(&row.key_last4)
            .bind(row.user_budget)
            .bind(row.instance_budget)
            .execute(p)
            .await
            .map_err(|e| format!("upsert_oxpilot_settings: {e}"))?;
        }
    }
    get_row(pool)
        .await?
        .ok_or_else(|| "upsert_oxpilot_settings: singleton row vanished after write".to_string())
}

/// NULL every override column in ONE statement — `clear_overrides` /
/// `clear_key` semantics (BR3.4); the singleton row itself persists.
/// Same "key pair moves together" guarantee: both key columns NULL out in
/// this one UPDATE.
pub async fn clear_row(pool: &DbPool) -> Result<(), String> {
    let (sql_pg, sql_my, sql_lite) = (
        "UPDATE oxpilot_settings SET enabled_override = NULL, provider_id = NULL, \
         model = NULL, base_url = NULL, key_ciphertext = NULL, key_last4 = NULL, \
         user_budget = NULL, instance_budget = NULL, updated_at = NOW() WHERE id = 1",
        "UPDATE oxpilot_settings SET enabled_override = NULL, provider_id = NULL, \
         model = NULL, base_url = NULL, key_ciphertext = NULL, key_last4 = NULL, \
         user_budget = NULL, instance_budget = NULL, updated_at = CURRENT_TIMESTAMP(3) WHERE id = 1",
        "UPDATE oxpilot_settings SET enabled_override = NULL, provider_id = NULL, \
         model = NULL, base_url = NULL, key_ciphertext = NULL, key_last4 = NULL, \
         user_budget = NULL, instance_budget = NULL, \
         updated_at = strftime('%Y-%m-%d %H:%M:%S','now') WHERE id = 1",
    );
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(sql_pg)
                .execute(p)
                .await
                .map_err(|e| format!("clear_oxpilot_settings: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(sql_my)
                .execute(p)
                .await
                .map_err(|e| format!("clear_oxpilot_settings: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(sql_lite)
                .execute(p)
                .await
                .map_err(|e| format!("clear_oxpilot_settings: {e}"))?;
        }
    };
    Ok(())
}
