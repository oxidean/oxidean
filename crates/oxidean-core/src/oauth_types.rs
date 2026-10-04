//! OAuth2 provider DTOs and constants (API-03).
//!
//! Oxidean acts as an OAuth2 authorization server: third-party "OAuth apps"
//! registered by users drive the authorization-code flow through
//! `GET /oauth/authorize` (browser + consent screen) and `POST /oauth/token`
//! (server-to-server exchange). Plaintext secrets/codes/tokens are only ever
//! returned in create/exchange responses — list rows carry display prefixes.

use serde::{Deserialize, Serialize};

/// `client_id` minted for registered apps (brand prefix, D-08 style).
pub const OAUTH_CLIENT_ID_PREFIX: &str = "oxidean_oc_";
/// One-time `client_secret` plaintext prefix; only the SHA-256 hash is stored.
pub const OAUTH_CLIENT_SECRET_PREFIX: &str = "oxidean_osec_";
/// Authorization code plaintext prefix; only the SHA-256 hash is stored.
pub const OAUTH_CODE_PREFIX: &str = "oxidean_oac_";
/// Access token plaintext prefix; only the SHA-256 hash is stored. Tokens with
/// this prefix authenticate like PATs on surfaces that accept token auth
/// (git smart HTTP, package registries, `/oauth/userinfo`).
pub const OAUTH_ACCESS_TOKEN_PREFIX: &str = "oxidean_oat_";

/// Authorization code lifetime (10 minutes, RFC 6749 §4.1.2 guidance).
pub const OAUTH_CODE_TTL_SECS: i64 = 600;
/// Access token lifetime returned as `expires_in` (8 hours).
pub const OAUTH_TOKEN_TTL_SECS: i64 = 8 * 3600;

/// Scopes an OAuth grant may carry. Space-delimited on the wire and at rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OAuthScope {
    /// Identity: read the user's profile via `/oauth/userinfo`.
    #[serde(rename = "read:user")]
    ReadUser,
    /// Identity: reveal the user's primary email via `/oauth/userinfo`.
    #[serde(rename = "user:email")]
    UserEmail,
    /// Git smart HTTP read/write — equivalent to a classic PAT `repo` scope.
    #[serde(rename = "repo")]
    Repo,
    /// Package registry pull — equivalent to classic PAT `package:read`.
    #[serde(rename = "package:read")]
    PackageRead,
    /// Package registry push/delete — equivalent to classic PAT `package:write`.
    #[serde(rename = "package:write")]
    PackageWrite,
}

impl OAuthScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadUser => "read:user",
            Self::UserEmail => "user:email",
            Self::Repo => "repo",
            Self::PackageRead => "package:read",
            Self::PackageWrite => "package:write",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "read:user" => Ok(Self::ReadUser),
            "user:email" => Ok(Self::UserEmail),
            "repo" => Ok(Self::Repo),
            "package:read" => Ok(Self::PackageRead),
            "package:write" => Ok(Self::PackageWrite),
            other => Err(format!("invalid scope: {other}")),
        }
    }

    pub const ALL: [OAuthScope; 5] = [
        Self::ReadUser,
        Self::UserEmail,
        Self::Repo,
        Self::PackageRead,
        Self::PackageWrite,
    ];
}

/// Scope set used when the client omits `scope` (identity-only default).
pub const OAUTH_DEFAULT_SCOPES: &[OAuthScope] = &[OAuthScope::ReadUser];

/// Parse a space-delimited scope string into a deduplicated scope list.
pub fn parse_oauth_scopes(raw: &str) -> Result<Vec<OAuthScope>, String> {
    let mut out: Vec<OAuthScope> = Vec::new();
    for part in raw.split_whitespace() {
        let scope = OAuthScope::parse(part)?;
        if !out.contains(&scope) {
            out.push(scope);
        }
    }
    Ok(out)
}

/// Serialize scopes back to the space-delimited wire form.
pub fn format_oauth_scopes(scopes: &[OAuthScope]) -> String {
    scopes
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse the stored/requested scope string, defaulting to
/// [`OAUTH_DEFAULT_SCOPES`] when empty.
pub fn oauth_scopes_or_default(raw: Option<&str>) -> Result<Vec<OAuthScope>, String> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        Some(v) => parse_oauth_scopes(v),
        None => Ok(OAUTH_DEFAULT_SCOPES.to_vec()),
    }
}

/// Registered app as shown to its owner — `client_secret` is never included.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthAppPublic {
    pub id: String,
    pub name: String,
    pub client_id: String,
    /// Display fingerprint: secret prefix + first 8 hex — never the secret.
    pub client_secret_prefix: String,
    pub redirect_uris: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// `oauthApp.create` input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOAuthAppRequest {
    pub name: String,
    pub redirect_uris: Vec<String>,
}

/// `oauthApp.create` / `oauthApp.regenerateSecret` response — the only places a
/// plaintext `client_secret` ever appears.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOAuthAppResponse {
    pub app: OAuthAppPublic,
    pub client_secret: String,
}

/// `oauthApp.update` input — omitted fields stay unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateOAuthAppRequest {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub redirect_uris: Option<Vec<String>>,
}

/// `oauthApp.delete` / `oauthApp.regenerateSecret` / `oauthApp.revoke` input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthAppIdRequest {
    pub id: String,
}

/// One row of `oauthApp.listGrants` — a user's grant to an application
/// (grouped across the app's live tokens for that user).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthGrantPublic {
    /// `oauth_applications.id` — pass to `oauthApp.revoke`.
    pub application_id: String,
    pub app_name: String,
    pub client_id: String,
    pub scopes: Vec<String>,
    pub granted_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
}

/// `oauthApp.authorizeInfo` input — mirrors the `/oauth/authorize` query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthAuthorizeInfoRequest {
    pub client_id: String,
    #[serde(default)]
    pub redirect_uri: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

/// What the consent screen renders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthAuthorizeInfo {
    pub app_name: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    /// Username of the app owner ("developed by X").
    pub owner_username: String,
}

/// `oauthApp.authorize` input — the consent decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthAuthorizeRequest {
    pub client_id: String,
    pub redirect_uri: String,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    pub approve: bool,
}

/// `oauthApp.authorize` response — the browser should navigate here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthAuthorizeResponse {
    pub redirect_to: String,
}

/// `POST /oauth/token` success body (RFC 6749 §5.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub scope: String,
}

/// `GET /oauth/userinfo` body — Bearer-gated identity surface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthUserInfoResponse {
    pub id: String,
    pub username: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    /// Present only when the token carries `user:email`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_round_trip() {
        let scopes = parse_oauth_scopes("repo read:user repo").unwrap();
        assert_eq!(scopes, vec![OAuthScope::Repo, OAuthScope::ReadUser]);
        assert_eq!(format_oauth_scopes(&scopes), "repo read:user");
        assert!(parse_oauth_scopes("admin").is_err());
        assert_eq!(
            oauth_scopes_or_default(None).unwrap(),
            vec![OAuthScope::ReadUser]
        );
        assert_eq!(
            oauth_scopes_or_default(Some("  ")).unwrap(),
            vec![OAuthScope::ReadUser]
        );
    }
}
