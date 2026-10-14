//! OAuth2 provider DTOs and constants (API-03).
//!
//! Oxidean acts as an OAuth2 authorization server: third-party "OAuth apps"
//! registered by users drive the authorization-code flow through
//! `GET /oauth/authorize` (browser + consent screen) and `POST /oauth/token`
//! (server-to-server exchange). Plaintext secrets/codes/tokens are only ever
//! returned in create/exchange responses — list rows carry display prefixes.

use serde::{Deserialize, Serialize};

use crate::pat_types::ClassicPatScope;

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

/// Scopes an OAuth grant may carry, space-delimited on the wire and at rest.
/// OAuth scopes share the classic-PAT catalog (`crate::scope_catalog`) —
/// `OAuthScope` is an alias so existing call sites keep reading naturally.
pub type OAuthScope = ClassicPatScope;

/// Scope set used when the client omits `scope` — the shared first-party set
/// (`crate::scope_catalog::FIRST_PARTY_SCOPES`). The consent screen is where
/// the grant gets narrowed, so the *resolved* default is broad and the user
/// trims before approving.
pub fn oauth_default_scopes() -> Vec<OAuthScope> {
    crate::first_party_scopes()
}

/// Parse a space-delimited scope string into a deduplicated scope list.
/// Every classic catalog name is a valid OAuth scope.
pub fn parse_oauth_scopes(raw: &str) -> Result<Vec<OAuthScope>, String> {
    let mut out: Vec<OAuthScope> = Vec::new();
    for part in raw.split_whitespace() {
        let scope = ClassicPatScope::parse(part).map_err(|e| e.replace("classic ", ""))?;
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
/// [`oauth_default_scopes`] when empty.
pub fn oauth_scopes_or_default(raw: Option<&str>) -> Result<Vec<OAuthScope>, String> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        Some(v) => parse_oauth_scopes(v),
        None => Ok(oauth_default_scopes()),
    }
}

/// Registered app as shown to its owner — `client_secret` is never included.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OAuthAppPublic {
    pub id: String,
    pub name: String,
    pub client_id: String,
    /// Masked marker only (`oxidean_osec_`) — no secret material is exposed.
    pub client_secret_prefix: String,
    pub redirect_uris: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// `oauthApp.create` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateOAuthAppRequest {
    pub name: String,
    pub redirect_uris: Vec<String>,
}

/// `oauthApp.create` / `oauthApp.regenerateSecret` response — the only places a
/// plaintext `client_secret` ever appears.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateOAuthAppResponse {
    pub app: OAuthAppPublic,
    pub client_secret: String,
}

/// `oauthApp.update` input — omitted fields stay unchanged.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateOAuthAppRequest {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub redirect_uris: Option<Vec<String>>,
}

/// `oauthApp.delete` / `oauthApp.regenerateSecret` / `oauthApp.revoke` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OAuthAppIdRequest {
    pub id: String,
}

/// One row of `oauthApp.listGrants` — a user's grant to an application
/// (grouped across the app's live tokens for that user).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OAuthAuthorizeInfoRequest {
    pub client_id: String,
    #[serde(default)]
    pub redirect_uri: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

/// What the consent screen renders.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OAuthAuthorizeInfo {
    pub app_name: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    /// Catalog descriptions per scope (added post-v1 — absent on older servers).
    #[serde(default)]
    pub scope_details: Vec<crate::scope_catalog::OAuthScopeDetail>,
    /// Username of the app owner ("developed by X").
    pub owner_username: String,
}

/// `oauthApp.authorize` input — the consent decision.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OAuthAuthorizeRequest {
    pub client_id: String,
    pub redirect_uri: String,
    /// Requested scope set (from the authorize URL) — resolves defaults.
    #[serde(default)]
    pub scope: Option<String>,
    /// Consent-screen narrowing: the scopes the user actually kept checked.
    /// Must be a non-empty subset of the resolved requested set.
    #[serde(default)]
    pub granted_scope: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    pub approve: bool,
}

/// `oauthApp.authorize` response — the browser should navigate here.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OAuthAuthorizeResponse {
    pub redirect_to: String,
}

/// `POST /oauth/token` success body (RFC 6749 §5.1).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OAuthTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub scope: String,
}

/// `GET /oauth/userinfo` body — Bearer-gated identity surface.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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

// ---------------------------------------------------------------------------
// Device authorization grant (RFC 8628) — `ox auth login --web`
// ---------------------------------------------------------------------------

/// `device_code` plaintext prefix; only the SHA-256 hash is stored.
pub const OAUTH_DEVICE_CODE_PREFIX: &str = "oxidean_odc_";

/// Device grant lifetime — 15 minutes (RFC 8628 §3.2 `expires_in` guidance).
pub const OAUTH_DEVICE_CODE_TTL_SECS: i64 = 900;

/// Minimum poll interval the server advertises (seconds).
pub const OAUTH_DEVICE_POLL_INTERVAL_SECS: i64 = 5;

/// `POST /oauth/device/code` success body (RFC 8628 §3.2).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OAuthDeviceCodeResponse {
    pub device_code: String,
    /// Human-typable `XXXX-XXXX` code the user enters on the verify page.
    pub user_code: String,
    pub verification_uri: String,
    /// verification_uri with `user_code` prefilled — what the CLI opens/prints.
    pub verification_uri_complete: String,
    pub expires_in: i64,
    pub interval: i64,
}

/// `oauth.deviceInfo` input — the approval page's lookup.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OAuthDeviceInfoRequest {
    /// User code as typed (dashes/case-insensitive — normalized server-side).
    pub user_code: String,
}

/// What the approval page renders for a pending code.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OAuthDeviceInfo {
    /// Canonical `XXXX-XXXX` form.
    pub user_code: String,
    /// Classic-PAT scope names the resulting token will carry.
    pub scopes: Vec<String>,
    /// Catalog descriptions per scope (added post-v1 — absent on older servers).
    #[serde(default)]
    pub scope_details: Vec<crate::scope_catalog::OAuthScopeDetail>,
    pub expires_at: String,
}

/// `oauth.deviceAuthorize` input — the approval decision.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OAuthDeviceAuthorizeRequest {
    pub user_code: String,
    pub approve: bool,
    /// Approval-page narrowing: scopes the user kept checked. Must be a
    /// non-empty subset of the grant's requested set.
    #[serde(default)]
    pub granted_scope: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_round_trip() {
        let scopes = parse_oauth_scopes("repo read:user repo").unwrap();
        assert_eq!(scopes, vec![OAuthScope::Repo, OAuthScope::ReadUser]);
        assert_eq!(format_oauth_scopes(&scopes), "repo read:user");
        assert!(parse_oauth_scopes("bogus").is_err());
        // The unified catalog accepts every classic scope name.
        assert_eq!(
            parse_oauth_scopes("admin:repo_hook delete_repo").unwrap(),
            vec![OAuthScope::AdminRepoHook, OAuthScope::DeleteRepo]
        );
        // Omitted scope resolves to the first-party set — the consent screen
        // is where the grant gets narrowed.
        assert_eq!(
            oauth_scopes_or_default(None).unwrap(),
            crate::first_party_scopes()
        );
        assert_eq!(
            oauth_scopes_or_default(Some("  ")).unwrap(),
            crate::first_party_scopes()
        );
    }
}
