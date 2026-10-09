//! Generic OIDC auth-code + PKCE adapter (D-08).

use std::net::IpAddr;
use std::time::Instant;

use openidconnect::core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata};
use openidconnect::reqwest;
use openidconnect::{
    AuthType, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet,
    EndpointNotSet, EndpointSet, IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, Scope, TokenResponse,
};
use url::Url;

use crate::auth::external::{ExternalAuthError, ExternalIdentity};
use crate::auth::pending::{PendingAuth, PendingAuthStore};

pub const PROVIDER: &str = "oidc";

/// Map OIDC `email_verified` claim — only `Some(true)` is trusted (D-03, D-15).
pub(crate) fn map_oidc_email_verified(claim: Option<bool>) -> bool {
    claim == Some(true)
}

/// Client after discovery: auth URL set; token/userinfo maybe set from metadata.
type DiscoveredClient = CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

#[derive(Debug, Clone)]
pub struct OidcConfig {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
}

impl OidcConfig {
    pub fn from_env() -> Option<Self> {
        let issuer = std::env::var("OXIDEAN_OIDC_ISSUER").ok()?.trim().to_string();
        let client_id = std::env::var("OXIDEAN_OIDC_CLIENT_ID")
            .ok()?
            .trim()
            .to_string();
        let client_secret = std::env::var("OXIDEAN_OIDC_CLIENT_SECRET")
            .ok()?
            .trim()
            .to_string();
        if issuer.is_empty() || client_id.is_empty() || client_secret.is_empty() {
            return None;
        }
        Some(Self {
            issuer,
            client_id,
            client_secret,
        })
    }
}

/// SSRF mitigation for OIDC issuer discovery (T-04-16).
///
/// Local mock IdPs (`docs/dev-auth.md`) may set `OXIDEAN_OIDC_ALLOW_INSECURE=1` when
/// `OXIDEAN_ENV` is `development` / `dev` / `compose` so http:// and loopback issuers work.
pub fn validate_issuer_url(issuer: &str) -> Result<Url, ExternalAuthError> {
    let url = Url::parse(issuer.trim())
        .map_err(|e| ExternalAuthError::Failed(format!("invalid issuer URL: {e}")))?;
    let allow_insecure = oidc_allow_insecure_issuer();
    if !allow_insecure && url.scheme() != "https" {
        return Err(ExternalAuthError::Failed(
            "OIDC issuer must use https://".into(),
        ));
    }
    if allow_insecure && url.scheme() != "https" && url.scheme() != "http" {
        return Err(ExternalAuthError::Failed(
            "OIDC issuer must use http:// or https://".into(),
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| ExternalAuthError::Failed("OIDC issuer missing host".into()))?;
    let host_l = host.to_ascii_lowercase();
    if !allow_insecure
        && (host_l == "localhost"
            || host_l == "metadata"
            || host_l.ends_with(".localhost")
            || host_l == "metadata.google.internal")
    {
        return Err(ExternalAuthError::Failed(
            "OIDC issuer host is not allowed".into(),
        ));
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        match ip {
            IpAddr::V4(v4) => {
                let o = v4.octets();
                // Reject loopback, link-local (169.254/16), and 10/8 per T-04-16.
                if !allow_insecure && (o[0] == 127 || o[0] == 10 || (o[0] == 169 && o[1] == 254)) {
                    return Err(ExternalAuthError::Failed(
                        "OIDC issuer host is not allowed".into(),
                    ));
                }
            }
            IpAddr::V6(v6)
                if (v6.is_loopback() || v6.is_unicast_link_local()) && !allow_insecure =>
            {
                return Err(ExternalAuthError::Failed(
                    "OIDC issuer host is not allowed".into(),
                ));
            }
            _ => {}
        }
    }
    Ok(url)
}

fn oidc_allow_insecure_issuer() -> bool {
    let flag = std::env::var("OXIDEAN_OIDC_ALLOW_INSECURE")
        .map(|v| matches!(v.trim(), "1" | "true" | "yes"))
        .unwrap_or(false);
    if !flag {
        return false;
    }
    let env_name = std::env::var("OXIDEAN_ENV").unwrap_or_else(|_| "development".into());
    matches!(env_name.as_str(), "development" | "dev" | "compose")
}

fn http_client() -> Result<reqwest::Client, ExternalAuthError> {
    use std::time::Duration;
    reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        // Fail fast when the IdP/mock is down or mis-routed (Compose 127.0.0.1 trap).
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| ExternalAuthError::Failed(e.to_string()))
}

async fn build_core_client(
    cfg: &OidcConfig,
    redirect_uri: &str,
) -> Result<DiscoveredClient, ExternalAuthError> {
    validate_issuer_url(&cfg.issuer)?;
    let http = http_client()?;
    let issuer = IssuerUrl::new(cfg.issuer.clone())
        .map_err(|e| ExternalAuthError::Failed(e.to_string()))?;
    let metadata = CoreProviderMetadata::discover_async(issuer, &http)
        .await
        .map_err(|e| ExternalAuthError::Failed(format!("OIDC discovery failed: {e}")))?;
    let redirect = RedirectUrl::new(redirect_uri.to_string())
        .map_err(|e| ExternalAuthError::Failed(e.to_string()))?;
    Ok(CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(cfg.client_id.clone()),
        Some(ClientSecret::new(cfg.client_secret.clone())),
    )
    // Prefer form client_id/secret so IdP tokenCallbacks that key on body
    // params (e.g. navikt mock-oauth2-server) still match.
    .set_auth_type(AuthType::RequestBody)
    .set_redirect_uri(redirect))
}

/// Begin OIDC authorize with PKCE + nonce; stash verifier/state/nonce.
pub async fn start(
    pending: &PendingAuthStore,
    cfg: &OidcConfig,
    redirect_uri: &str,
    return_to: &str,
) -> Result<Url, ExternalAuthError> {
    let client = build_core_client(cfg, redirect_uri).await?;
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let (auth_url, csrf_token, nonce) = client
        .authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        // openid is implied by AuthorizationCode flow — do not add it again.
        .add_scope(Scope::new("email".into()))
        .add_scope(Scope::new("profile".into()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    pending.insert(
        csrf_token.secret().clone(),
        PendingAuth {
            provider: PROVIDER.into(),
            code_verifier: pkce_verifier.secret().clone(),
            nonce: Some(nonce.secret().clone()),
            return_to: return_to.to_string(),
            redirect_uri: redirect_uri.to_string(),
            created_at: Instant::now(),
        },
    );

    Url::parse(auth_url.as_str()).map_err(|e| ExternalAuthError::Failed(e.to_string()))
}

/// Exchange code, verify ID token (nonce), return identity + return_to.
pub async fn finish(
    pending: &PendingAuthStore,
    cfg: &OidcConfig,
    code: &str,
    state: &str,
) -> Result<(ExternalIdentity, String), ExternalAuthError> {
    let pending_auth = pending
        .take(state)
        .filter(|p| p.provider == PROVIDER)
        .ok_or(ExternalAuthError::InvalidState)?;

    let nonce_secret = pending_auth
        .nonce
        .clone()
        .ok_or_else(|| ExternalAuthError::Failed("missing nonce".into()))?;

    let client = build_core_client(cfg, &pending_auth.redirect_uri).await?;
    let http = http_client()?;
    let pkce_verifier = PkceCodeVerifier::new(pending_auth.code_verifier);
    let token_response = client
        .exchange_code(AuthorizationCode::new(code.to_string()))
        .map_err(|e| ExternalAuthError::Failed(e.to_string()))?
        .set_pkce_verifier(pkce_verifier)
        .request_async(&http)
        .await
        .map_err(|e| ExternalAuthError::Failed(format!("token exchange failed: {e}")))?;

    let id_token = token_response
        .id_token()
        .ok_or_else(|| ExternalAuthError::Failed("no ID token in response".into()))?;
    let nonce = Nonce::new(nonce_secret);
    let claims = id_token
        .claims(&client.id_token_verifier(), &nonce)
        .map_err(|e| ExternalAuthError::Failed(format!("ID token verify failed: {e}")))?;

    let email = claims
        .email()
        .map(|e| e.as_str().to_string())
        .filter(|e| !e.is_empty())
        .ok_or_else(|| ExternalAuthError::Failed("missing email claim".into()))?;

    let display_name = claims
        .name()
        .and_then(|n| n.get(None))
        .map(|n| n.to_string())
        .or_else(|| {
            let given = claims
                .given_name()
                .and_then(|n| n.get(None))
                .map(|s| s.to_string());
            let family = claims
                .family_name()
                .and_then(|n| n.get(None))
                .map(|s| s.to_string());
            match (given, family) {
                (Some(g), Some(f)) => Some(format!("{g} {f}")),
                (Some(g), None) => Some(g),
                (None, Some(f)) => Some(f),
                _ => None,
            }
        });

    Ok((
        ExternalIdentity {
            provider: PROVIDER.into(),
            provider_subject: claims.subject().as_str().to_string(),
            email,
            display_name,
            email_verified: map_oidc_email_verified(claims.email_verified()),
        },
        pending_auth.return_to,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_http_issuer() {
        let err = validate_issuer_url("http://accounts.example.com").unwrap_err();
        assert!(err.to_string().contains("https"), "{err}");
    }

    #[test]
    fn reject_link_local_host() {
        let err = validate_issuer_url("https://169.254.169.254/latest").unwrap_err();
        assert!(err.to_string().contains("not allowed"), "{err}");
        let err = validate_issuer_url("https://10.0.0.5/oidc").unwrap_err();
        assert!(err.to_string().contains("not allowed"), "{err}");
        let err = validate_issuer_url("https://localhost/oidc").unwrap_err();
        assert!(err.to_string().contains("not allowed"), "{err}");
        let err = validate_issuer_url("https://127.0.0.1/oidc").unwrap_err();
        assert!(err.to_string().contains("not allowed"), "{err}");
        let err = validate_issuer_url("https://metadata/oidc").unwrap_err();
        assert!(err.to_string().contains("not allowed"), "{err}");
    }

    #[test]
    fn allows_public_https_issuer() {
        assert!(validate_issuer_url("https://accounts.example.com").is_ok());
    }

    #[test]
    fn allow_insecure_flag_permits_loopback_http() {
        // SAFETY: test-only env mutation; single-threaded test binary for this module.
        unsafe {
            std::env::set_var("OXIDEAN_ENV", "development");
            std::env::set_var("OXIDEAN_OIDC_ALLOW_INSECURE", "1");
        }
        let ok = validate_issuer_url("http://127.0.0.1:9090/default");
        unsafe {
            std::env::remove_var("OXIDEAN_OIDC_ALLOW_INSECURE");
        }
        assert!(ok.is_ok(), "{ok:?}");
    }

    #[test]
    fn finish_rejects_state_mismatch() {
        let pending = PendingAuthStore::new();
        let cfg = OidcConfig {
            issuer: "https://accounts.example.com".into(),
            client_id: "cid".into(),
            client_secret: "secret".into(),
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt
            .block_on(finish(&pending, &cfg, "code", "bad-state"))
            .unwrap_err();
        assert!(matches!(err, ExternalAuthError::InvalidState));
        assert_eq!(err.code(), "auth.invalid_state");
    }

    #[test]
    fn pkce_challenge_generates() {
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        assert!(!challenge.as_str().is_empty());
        assert!(!verifier.secret().is_empty());
    }

    /// D-03/D-15: OIDC email_verified claim trusts only Some(true).
    #[test]
    fn maps_oidc_email_verified_claim_some_true_only() {
        assert!(map_oidc_email_verified(Some(true)));
        assert!(!map_oidc_email_verified(Some(false)));
        assert!(!map_oidc_email_verified(None));
    }
}
