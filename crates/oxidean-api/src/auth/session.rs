//! Opaque HttpOnly session cookies + server-side session store (D-11–D-13).
//!
//! Cookie value is a CSPRNG token; only SHA-256(token) is stored in the DB.
//! Do not use `tower-sessions` / `tower-sessions-sqlx-store`.

use std::time::Duration;

use chrono::{DateTime, Utc};
use cookie::{Cookie, SameSite};
use oxidean_db::Database;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

/// Cookie name for the opaque session id (discretion lock).
pub const SESSION_COOKIE_NAME: &str = "oxidean_session";

/// Non-HttpOnly presence flag so the SPA can pick signed-in vs landing chrome
/// before `auth.me` resolves. Value is always `1` — never a secret.
pub const SESSION_PRESENCE_COOKIE_NAME: &str = "oxidean_signed_in";

/// Default idle session TTL (24h). Refresh `expires_at` on resolve for non-remember sessions.
pub const SESSION_IDLE: Duration = Duration::from_secs(24 * 3600);

/// Remember-me absolute TTL from create (30d). `last_seen` still updates; expiry does not slide.
pub const SESSION_REMEMBER: Duration = Duration::from_secs(30 * 24 * 3600);

const TOKEN_BYTES: usize = 32;

/// Secure cookie flag: off only for local HTTP (`development` / `dev`).
pub fn secure_cookies(env_name: &str) -> bool {
    env_name != "development" && env_name != "dev"
}

/// Resolved authenticated session after cookie lookup.
#[derive(Debug, Clone)]
pub struct ResolvedSession {
    pub session_id: String,
    pub user_id: String,
    pub remember_me: bool,
    pub expires_at: DateTime<Utc>,
}

/// Session service errors (mapped to RPC codes by callers later).
#[derive(Debug, Error)]
pub enum AuthError {
    #[error("database not configured")]
    NotConfigured,
    #[error("session store error: {0}")]
    Store(String),
}

impl AuthError {
    fn from_db(e: String) -> Self {
        if e == "database not configured" {
            Self::NotConfigured
        } else {
            Self::Store(e)
        }
    }
}

/// Opaque-cookie session lifecycle over `oxidean-db` sessions.
#[derive(Debug, Clone)]
pub struct SessionService {
    env_name: String,
}

impl SessionService {
    pub fn new(env_name: impl Into<String>) -> Self {
        Self {
            env_name: env_name.into(),
        }
    }

    /// Mint a new session: CSPRNG token → cookie; SHA-256 hex → DB.
    /// Always creates a fresh session id (session fixation mitigation T-04-08).
    /// `ip_address`/`user_agent` are stored as the session's client details.
    pub async fn create(
        &self,
        db: &Database,
        user_id: &str,
        remember_me: bool,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<(String, Cookie<'static>), AuthError> {
        let ttl = if remember_me {
            SESSION_REMEMBER
        } else {
            SESSION_IDLE
        };
        let now = Utc::now();
        let expires_at = now + chrono::Duration::from_std(ttl).map_err(|e| AuthError::Store(e.to_string()))?;
        let expires_at_str = expires_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

        let mut token_bytes = [0u8; TOKEN_BYTES];
        rand::fill(&mut token_bytes);
        let raw_token = bytes_to_hex(&token_bytes);
        let token_hash = sha256_hex(raw_token.as_bytes());

        let session_id = Uuid::new_v4().to_string();
        db.create_session(
            &session_id,
            user_id,
            &token_hash,
            &expires_at_str,
            remember_me,
            ip_address,
            user_agent,
        )
        .await
        .map_err(AuthError::from_db)?;

        let cookie = build_session_cookie(&raw_token, ttl, &self.env_name);
        Ok((raw_token, cookie))
    }

    /// Resolve cookie token → session. Expired rows are deleted. Idle sessions slide expiry.
    /// When `ip_address`/`user_agent` are provided they refresh the session's
    /// last-known client details alongside `last_seen`.
    pub async fn resolve(
        &self,
        db: &Database,
        raw_token: &str,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<Option<ResolvedSession>, AuthError> {
        if raw_token.is_empty() {
            return Ok(None);
        }
        let token_hash = sha256_hex(raw_token.as_bytes());
        let Some(row) = db
            .find_session_by_token_hash(&token_hash)
            .await
            .map_err(AuthError::from_db)?
        else {
            return Ok(None);
        };

        let expires_at = parse_rfc3339(&row.expires_at).map_err(AuthError::Store)?;
        let now = Utc::now();
        if expires_at <= now {
            db.delete_session(&row.id)
                .await
                .map_err(AuthError::from_db)?;
            return Ok(None);
        }

        // Owner gone (user row deleted) — drop the dangling session.
        if row.joined_user_id.is_none() {
            let _ = db.delete_session(&row.id).await;
            return Ok(None);
        }

        // Soft-ban: treat session as absent (PATs gated separately at resolve).
        if row.user_banned_at.is_some() {
            let _ = db.delete_session(&row.id).await;
            return Ok(None);
        }

        let last_seen = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let new_expires = if row.remember_me {
            // Absolute expiry from create; only refresh last_seen.
            expires_at
        } else {
            now + chrono::Duration::from_std(SESSION_IDLE)
                .map_err(|e| AuthError::Store(e.to_string()))?
        };
        let new_expires_str = new_expires.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        db.touch_session(&row.id, &new_expires_str, &last_seen, ip_address, user_agent)
            .await
            .map_err(AuthError::from_db)?;

        Ok(Some(ResolvedSession {
            session_id: row.id,
            user_id: row.user_id,
            remember_me: row.remember_me,
            expires_at: new_expires,
        }))
    }

    /// Revoke a single session (this device / logout).
    pub async fn revoke(&self, db: &Database, session_id: &str) -> Result<(), AuthError> {
        db.delete_session(session_id)
            .await
            .map_err(AuthError::from_db)
    }

    /// Revoke all sessions for a user (logout all devices — D-13).
    pub async fn revoke_all(&self, db: &Database, user_id: &str) -> Result<u64, AuthError> {
        db.delete_sessions_for_user(user_id)
            .await
            .map_err(AuthError::from_db)
    }
}

/// Empty cookie with Max-Age=0 to clear the session (logout).
pub fn clear_session_cookie(env_name: &str) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE_NAME, ""))
        .http_only(true)
        .path("/")
        .same_site(SameSite::Lax)
        .max_age(cookie::time::Duration::ZERO)
        .secure(secure_cookies(env_name))
        .build()
}

/// Readable companion cookie: same Path/SameSite/Secure/Max-Age as the session.
pub fn build_session_presence_cookie(ttl: Duration, env_name: &str) -> Cookie<'static> {
    let max_age = cookie::time::Duration::seconds(ttl.as_secs() as i64);
    Cookie::build((SESSION_PRESENCE_COOKIE_NAME, "1"))
        .http_only(false)
        .path("/")
        .same_site(SameSite::Lax)
        .max_age(max_age)
        .secure(secure_cookies(env_name))
        .build()
}

/// Clear the SPA presence hint (pair with [`clear_session_cookie`]).
pub fn clear_session_presence_cookie(env_name: &str) -> Cookie<'static> {
    Cookie::build((SESSION_PRESENCE_COOKIE_NAME, ""))
        .http_only(false)
        .path("/")
        .same_site(SameSite::Lax)
        .max_age(cookie::time::Duration::ZERO)
        .secure(secure_cookies(env_name))
        .build()
}

fn build_session_cookie(raw_token: &str, ttl: Duration, env_name: &str) -> Cookie<'static> {
    let max_age = cookie::time::Duration::seconds(ttl.as_secs() as i64);
    Cookie::build((SESSION_COOKIE_NAME, raw_token.to_owned()))
        .http_only(true)
        .path("/")
        .same_site(SameSite::Lax)
        .max_age(max_age)
        .secure(secure_cookies(env_name))
        .build()
}

/// SHA-256 hex digest (sessions + PAT hash-at-rest).
pub fn sha256_hex(data: &[u8]) -> String {
    bytes_to_hex(&Sha256::digest(data))
}

/// Lowercase hex encode (CSPRNG token / PAT secret material).
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

fn parse_rfc3339(s: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| format!("invalid expires_at: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cookie::time::Duration as CookieDuration;

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().expect("tempdir");
        let url = format!("sqlite:{}", dir.path().join("session.db").display());
        let db = Database::connect(&url).await.expect("connect");
        db.migrate().await.expect("migrate");
        // Keep tempdir alive for the pool — leak for test process lifetime.
        std::mem::forget(dir);
        db
    }

    async fn seed_user(db: &Database) -> String {
        let id = Uuid::new_v4().to_string();
        db.create_user(
            &id,
            &format!("{id}@example.com"),
            &format!("u{}", &id[..8]),
            Some("$argon2id$test"),
            "Session Tester",
            "",
            None,
            oxidean_core::Role::User,
        )
        .await
        .expect("create user");
        id
    }

    #[test]
    fn secure_cookies_dev_false() {
        assert!(!secure_cookies("development"));
        assert!(!secure_cookies("dev"));
        assert!(secure_cookies("production"));
        assert!(secure_cookies("staging"));
    }

    #[tokio::test]
    async fn create_resolve_some() {
        let db = test_db().await;
        let user_id = seed_user(&db).await;
        let svc = SessionService::new("development");

        let (token, cookie) = svc
            .create(&db, &user_id, false, Some("203.0.113.7"), Some("test-ua"))
            .await
            .expect("create");
        assert_eq!(cookie.name(), SESSION_COOKIE_NAME);
        assert!(cookie.http_only().unwrap_or(false));
        assert_eq!(cookie.path(), Some("/"));
        assert_eq!(cookie.same_site(), Some(SameSite::Lax));
        assert!(!cookie.secure().unwrap_or(true));
        assert!(
            cookie.max_age().unwrap() >= CookieDuration::seconds(SESSION_IDLE.as_secs() as i64 - 1)
        );

        let resolved = svc
            .resolve(&db, &token, None, None)
            .await
            .expect("resolve");
        let session = resolved.expect("some");
        assert_eq!(session.user_id, user_id);
        assert!(!session.remember_me);
    }

    #[tokio::test]
    async fn revoke_then_resolve_none() {
        let db = test_db().await;
        let user_id = seed_user(&db).await;
        let svc = SessionService::new("development");

        let (token, _) = svc
            .create(&db, &user_id, false, None, None)
            .await
            .expect("create");
        let session = svc
            .resolve(&db, &token, None, None)
            .await
            .expect("resolve")
            .expect("present");
        svc.revoke(&db, &session.session_id).await.expect("revoke");
        let gone = svc
            .resolve(&db, &token, None, None)
            .await
            .expect("resolve after revoke");
        assert!(gone.is_none());
    }

    #[tokio::test]
    async fn remember_me_cookie_max_age_30d() {
        let db = test_db().await;
        let user_id = seed_user(&db).await;
        let svc = SessionService::new("production");

        let (_token, cookie) = svc
            .create(&db, &user_id, true, None, None)
            .await
            .expect("create");
        let min = CookieDuration::seconds(SESSION_REMEMBER.as_secs() as i64);
        assert!(
            cookie.max_age().unwrap() >= min,
            "remember-me max-age {:?} < {:?}",
            cookie.max_age(),
            min
        );
        assert!(cookie.secure().unwrap_or(false));
        assert_eq!(cookie.same_site(), Some(SameSite::Lax));
    }

    #[test]
    fn clear_cookie_max_age_zero() {
        let c = clear_session_cookie("development");
        assert_eq!(c.name(), SESSION_COOKIE_NAME);
        assert_eq!(c.value(), "");
        assert_eq!(c.max_age(), Some(CookieDuration::ZERO));
        assert!(c.http_only().unwrap_or(false));
        assert_eq!(c.same_site(), Some(SameSite::Lax));
    }

    #[test]
    fn presence_cookie_is_readable_and_pairs_with_clear() {
        let set = build_session_presence_cookie(SESSION_IDLE, "development");
        assert_eq!(set.name(), SESSION_PRESENCE_COOKIE_NAME);
        assert_eq!(set.value(), "1");
        assert!(!set.http_only().unwrap_or(true));
        assert_eq!(set.path(), Some("/"));
        assert!(!set.secure().unwrap_or(true));

        let clear = clear_session_presence_cookie("development");
        assert_eq!(clear.name(), SESSION_PRESENCE_COOKIE_NAME);
        assert_eq!(clear.max_age(), Some(CookieDuration::ZERO));
    }
}
