//! Shared SSO identity linking + safe post-auth redirects (D-05, D-07, D-08, D-15).

use oxidean_core::{is_reserved_username, validate_username};
use oxidean_db::{Database, UserRow};
use thiserror::Error;
use uuid::Uuid;

/// Identity returned by an external IdP after successful code exchange.
#[derive(Debug, Clone)]
pub struct ExternalIdentity {
    pub provider: String,
    pub provider_subject: String,
    pub email: String,
    pub display_name: Option<String>,
    /// IdP-trust: when true, mark `users.email_verified_at` on link/create (D-03, D-15).
    pub email_verified: bool,
}

#[derive(Debug, Error)]
pub enum ExternalAuthError {
    #[error("auth provider not configured")]
    NotConfigured,
    #[error("auth provider mode mismatch")]
    ProviderMismatch,
    #[error("invalid or expired OAuth state")]
    InvalidState,
    #[error("SSO failed: {0}")]
    Failed(String),
    #[error("database not configured")]
    DbNotConfigured,
    #[error("store error: {0}")]
    Store(String),
}

impl ExternalAuthError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotConfigured => "auth.not_configured",
            Self::ProviderMismatch => "auth.provider_mismatch",
            Self::InvalidState => "auth.invalid_state",
            Self::Failed(_) => "auth.sso_failed",
            Self::DbNotConfigured => "db.not_configured",
            Self::Store(_) => "auth.internal",
        }
    }

    pub fn from_db(e: String) -> Self {
        if e == "database not configured" {
            Self::DbNotConfigured
        } else {
            Self::Store(e)
        }
    }
}

/// Placeholder usernames created when email local-part is invalid/reserved (`u` + 8 hex).
pub fn is_placeholder_username(username: &str) -> bool {
    let b = username.as_bytes();
    b.len() == 9
        && b[0] == b'u'
        && b[1..].iter().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

/// Sanitize email local-part into a candidate username.
fn candidate_from_email(email: &str) -> String {
    let local = email.split('@').next().unwrap_or("user");
    let mut out = String::new();
    for c in local.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if (c == '-' || c == '_' || c == '.') && !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.len() >= 39 {
            break;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    while out.starts_with('-') {
        out.remove(0);
    }
    if out.is_empty() {
        "user".into()
    } else {
        out
    }
}

fn placeholder_username() -> String {
    let id = Uuid::new_v4().to_string().replace('-', "");
    format!("u{}", &id[..8])
}

/// Allocate a unique username from email; returns `(username, profile_incomplete)`.
pub async fn allocate_username(
    db: &Database,
    email: &str,
) -> Result<(String, bool), ExternalAuthError> {
    let base = candidate_from_email(email);
    let try_name = |name: &str| -> bool {
        validate_username(name).is_ok() && !is_reserved_username(name)
    };

    if try_name(&base) {
        match db.find_user_by_username(&base).await.map_err(ExternalAuthError::from_db)? {
            None
                if db
                    .find_organization_by_slug(&base)
                    .await
                    .map_err(ExternalAuthError::from_db)?
                    .is_none() =>
            {
                return Ok((base, false));
            }
            _ => {
                for i in 2..100 {
                    let suffix = format!("-{i}");
                    let max = 39usize.saturating_sub(suffix.len());
                    let mut candidate = base.clone();
                    if candidate.len() > max {
                        candidate.truncate(max);
                    }
                    while candidate.ends_with('-') {
                        candidate.pop();
                    }
                    candidate.push_str(&suffix);
                    if try_name(&candidate)
                        && db
                            .find_user_by_username(&candidate)
                            .await
                            .map_err(ExternalAuthError::from_db)?
                            .is_none()
                        && db
                            .find_organization_by_slug(&candidate)
                            .await
                            .map_err(ExternalAuthError::from_db)?
                            .is_none()
                    {
                        return Ok((candidate, false));
                    }
                }
            }
        }
    }

    // Invalid/reserved or exhausted suffixes → placeholder (forces profile completion).
    for _ in 0..8 {
        let ph = placeholder_username();
        if db
            .find_user_by_username(&ph)
            .await
            .map_err(ExternalAuthError::from_db)?
            .is_none()
            && db
                .find_organization_by_slug(&ph)
                .await
                .map_err(ExternalAuthError::from_db)?
                .is_none()
        {
            return Ok((ph, true));
        }
    }
    Err(ExternalAuthError::Failed(
        "could not allocate unique username".into(),
    ))
}

/// Find existing identity or create user + link `auth_identities`.
/// No welcome email (D-20 — local signup only).
/// When `identity.email_verified`, sets `email_verified_at` (D-03, D-15 IdP-trust).
pub async fn link_or_create_user(
    db: &Database,
    identity: &ExternalIdentity,
) -> Result<(UserRow, bool), ExternalAuthError> {
    let email = identity.email.trim().to_ascii_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(ExternalAuthError::Failed("missing email claim".into()));
    }

    if let Some(link) = db
        .find_auth_identity(&identity.provider, &identity.provider_subject)
        .await
        .map_err(ExternalAuthError::from_db)?
    {
        let user = db
            .find_user_by_id(&link.user_id)
            .await
            .map_err(ExternalAuthError::from_db)?
            .ok_or_else(|| ExternalAuthError::Failed("linked user missing".into()))?;
        let _ = db
            .upsert_auth_identity(
                &link.id,
                &user.id,
                &identity.provider,
                &identity.provider_subject,
                Some(&email),
            )
            .await
            .map_err(ExternalAuthError::from_db)?;
        let incomplete = is_placeholder_username(&user.username);
        let user = apply_idp_email_verified(db, user, identity.email_verified).await?;
        return Ok((user, incomplete));
    }

    let (user, incomplete) = if let Some(existing) = db
        .find_user_by_email(&email)
        .await
        .map_err(ExternalAuthError::from_db)?
    {
        let incomplete = is_placeholder_username(&existing.username);
        (existing, incomplete)
    } else {
        let (username, incomplete) = allocate_username(db, &email).await?;
        let id = Uuid::new_v4().to_string();
        let display = identity
            .display_name
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| username.clone());
        let row = db
            .create_user(
                &id,
                &email,
                &username,
                None, // SSO-only — no local password
                &display,
                "",
                None,
                oxidean_core::Role::User,
            )
            .await
            .map_err(ExternalAuthError::from_db)?;
        (row, incomplete)
    };

    let identity_id = Uuid::new_v4().to_string();
    db.upsert_auth_identity(
        &identity_id,
        &user.id,
        &identity.provider,
        &identity.provider_subject,
        Some(&email),
    )
    .await
    .map_err(ExternalAuthError::from_db)?;

    let user = apply_idp_email_verified(db, user, identity.email_verified).await?;
    Ok((user, incomplete))
}

/// IdP-trust: mark verified when the provider asserts a verified email (D-03, D-15).
async fn apply_idp_email_verified(
    db: &Database,
    user: UserRow,
    email_verified: bool,
) -> Result<UserRow, ExternalAuthError> {
    if !email_verified || user.email_verified_at.is_some() {
        return Ok(user);
    }
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user.id, &now)
        .await
        .map_err(ExternalAuthError::from_db)
}

/// Safe relative return path (D-15). Reject open redirects.
/// Default home is `/` (signed-in shell lives there). Legacy `/dashboard` → `/`.
pub fn sanitize_return_to(raw: Option<&str>) -> String {
    let Some(s) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return "/".into();
    };
    if !s.starts_with('/') || s.starts_with("//") || s.contains('\\') || s.contains('\n') {
        return "/".into();
    }
    if s == "/dashboard" || s.starts_with("/dashboard?") {
        return "/".into();
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().expect("tempdir");
        let url = format!("sqlite:{}", dir.path().join("external.db").display());
        let db = Database::connect(&url).await.expect("connect");
        db.migrate().await.expect("migrate");
        std::mem::forget(dir);
        db
    }

    #[test]
    fn sanitize_rejects_open_redirect() {
        assert_eq!(sanitize_return_to(Some("https://evil.example")), "/");
        assert_eq!(sanitize_return_to(Some("//evil.example")), "/");
        assert_eq!(sanitize_return_to(Some("/settings/profile")), "/settings/profile");
        assert_eq!(sanitize_return_to(Some("/")), "/");
        assert_eq!(sanitize_return_to(Some("/dashboard")), "/");
        assert_eq!(sanitize_return_to(None), "/");
    }

    #[test]
    fn placeholder_detection() {
        assert!(is_placeholder_username("u1a2b3c4d"));
        assert!(!is_placeholder_username("alice"));
        assert!(!is_placeholder_username("uABCDEFGH")); // uppercase not used
    }

    #[test]
    fn candidate_sanitizes_local_part() {
        assert_eq!(candidate_from_email("Alice.Bob_tag@ex.com"), "alice-bob-tag");
    }

    /// D-03/D-15: IdP-asserted verified email marks users.email_verified_at on SSO link/create.
    #[tokio::test]
    async fn link_or_create_marks_verified_when_idp_asserts() {
        let db = test_db().await;
        let identity = ExternalIdentity {
            provider: "workos".into(),
            provider_subject: "user_verified_1".into(),
            email: "verified-sso@ex.com".into(),
            display_name: Some("Verified SSO".into()),
            email_verified: true,
        };
        let (user, _) = link_or_create_user(&db, &identity)
            .await
            .expect("link_or_create");
        assert!(
            user.email_verified_at.is_some(),
            "IdP email_verified=true must set email_verified_at"
        );
    }

    /// D-15 edge: absent/false IdP verification leaves local verify flows required.
    #[tokio::test]
    async fn link_or_create_leaves_unverified_when_idp_does_not_assert() {
        let db = test_db().await;
        let identity = ExternalIdentity {
            provider: "oidc".into(),
            provider_subject: "sub_unverified_1".into(),
            email: "unverified-sso@ex.com".into(),
            display_name: None,
            email_verified: false,
        };
        let (user, _) = link_or_create_user(&db, &identity)
            .await
            .expect("link_or_create");
        assert!(
            user.email_verified_at.is_none(),
            "IdP email_verified=false must leave email_verified_at null"
        );
    }
}
