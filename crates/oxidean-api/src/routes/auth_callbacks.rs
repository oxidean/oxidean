//! HTTP start/callback routes for WorkOS and OIDC (mint Oxidean session cookie).

use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

use crate::app::AppState;
use crate::auth::bootstrap;
use crate::auth::external::{link_or_create_user, sanitize_return_to, ExternalAuthError};
use crate::auth::session::{
    build_session_presence_cookie, SESSION_COOKIE_NAME, SESSION_IDLE, SESSION_PRESENCE_COOKIE_NAME,
};
use crate::auth::{oidc, workos};

#[derive(Debug, Deserialize)]
pub struct StartQuery {
    /// Preferred snake_case (stack e2e / docs).
    #[serde(alias = "returnTo")]
    pub return_to: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

fn public_origin(headers: &HeaderMap) -> String {
    let resolved = crate::public_origin::resolve_public_origin();
    // Prefer Railway / env resolution when it is not the local default.
    if resolved != "http://localhost:8080" {
        return resolved;
    }
    if let Ok(origin) = std::env::var("OXIDEAN_PUBLIC_ORIGIN") {
        let o = origin.trim().trim_end_matches('/');
        if !o.is_empty() {
            return o.to_string();
        }
    }
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http");
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(header::HOST))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost:8080");
    format!("{proto}://{host}")
}

async fn current_mode(state: &AppState) -> Result<String, ExternalAuthError> {
    match state.db.get_auth_settings().await {
        Ok(row) => Ok(row.provider_mode),
        Err(e) if e == "database not configured" => Err(ExternalAuthError::DbNotConfigured),
        Err(e) => Err(ExternalAuthError::from_db(e)),
    }
}

fn sso_error_redirect() -> Response {
    Redirect::temporary("/login?error=sso").into_response()
}

/// Empty instance without ENV seed must complete `/setup` before SSO can create users.
async fn reject_if_setup_required(state: &AppState) -> Option<Response> {
    match bootstrap::needs_setup(&state.db).await {
        Ok(true) => Some(Redirect::temporary("/setup").into_response()),
        Ok(false) => None,
        Err(e) => {
            tracing::error!(error = %e.message, "bootstrap needs_setup check failed");
            Some(sso_error_redirect())
        }
    }
}

fn redirect_with_cookies(return_to: &str, cookies: &[String]) -> Response {
    let mut res = Redirect::temporary(return_to).into_response();
    for cookie in cookies {
        if let Ok(hv) = HeaderValue::from_str(cookie) {
            res.headers_mut().append(header::SET_COOKIE, hv);
        }
    }
    res
}

fn log_sso_err(err: &ExternalAuthError) {
    tracing::error!(code = err.code(), error = %err, "external auth failed");
}

/// GET `/api/auth/workos/start`
pub async fn workos_start(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<StartQuery>,
) -> Response {
    if let Some(res) = reject_if_setup_required(&state).await {
        return res;
    }
    let mode = match current_mode(&state).await {
        Ok(m) => m,
        Err(e) => {
            log_sso_err(&e);
            return sso_error_redirect();
        }
    };
    if mode != "workos" {
        log_sso_err(&ExternalAuthError::ProviderMismatch);
        return sso_error_redirect();
    }

    let Some(cfg) = workos::WorkOsConfig::from_env() else {
        tracing::warn!("WORKOS_API_KEY / WORKOS_CLIENT_ID missing");
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({
                "ok": false,
                "error": {
                    "code": "auth.not_configured",
                    "message": "WorkOS is not configured"
                }
            })),
        )
            .into_response();
    };

    let return_to = sanitize_return_to(q.return_to.as_deref());
    let origin = public_origin(&headers);
    let redirect_uri = format!("{origin}/api/auth/workos/callback");

    match workos::start(&state.pending, &cfg, &redirect_uri, &return_to) {
        Ok(url) => Redirect::temporary(url.as_str()).into_response(),
        Err(e) => {
            log_sso_err(&e);
            sso_error_redirect()
        }
    }
}

/// GET `/api/auth/workos/callback`
pub async fn workos_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Response {
    if q.error.is_some() {
        tracing::warn!(error = ?q.error, "WorkOS IdP returned error");
        return sso_error_redirect();
    }
    let (Some(code), Some(state_param)) = (q.code.as_deref(), q.state.as_deref()) else {
        return sso_error_redirect();
    };

    let Some(cfg) = workos::WorkOsConfig::from_env() else {
        return sso_error_redirect();
    };

    let (identity, return_to) =
        match workos::finish(&state.pending, &cfg, code, state_param).await {
            Ok(v) => v,
            Err(e) => {
                log_sso_err(&e);
                return sso_error_redirect();
            }
        };

    mint_session_and_redirect(&state, &identity, &return_to, &headers).await
}

/// GET `/api/auth/oidc/start`
pub async fn oidc_start(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<StartQuery>,
) -> Response {
    if let Some(res) = reject_if_setup_required(&state).await {
        return res;
    }
    let mode = match current_mode(&state).await {
        Ok(m) => m,
        Err(e) => {
            log_sso_err(&e);
            return sso_error_redirect();
        }
    };
    if mode != "oidc" {
        log_sso_err(&ExternalAuthError::ProviderMismatch);
        return sso_error_redirect();
    }

    let Some(cfg) = oidc::OidcConfig::from_env() else {
        tracing::warn!("OIDC env vars missing");
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({
                "ok": false,
                "error": {
                    "code": "auth.not_configured",
                    "message": "OIDC is not configured"
                }
            })),
        )
            .into_response();
    };

    let return_to = sanitize_return_to(q.return_to.as_deref());
    let origin = public_origin(&headers);
    let redirect_uri = format!("{origin}/api/auth/oidc/callback");

    match oidc::start(&state.pending, &cfg, &redirect_uri, &return_to).await {
        Ok(url) => Redirect::temporary(url.as_str()).into_response(),
        Err(e) => {
            log_sso_err(&e);
            sso_error_redirect()
        }
    }
}

/// GET `/api/auth/oidc/callback`
pub async fn oidc_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Response {
    if q.error.is_some() {
        tracing::warn!(error = ?q.error, "OIDC IdP returned error");
        return sso_error_redirect();
    }
    let (Some(code), Some(state_param)) = (q.code.as_deref(), q.state.as_deref()) else {
        return sso_error_redirect();
    };

    let Some(cfg) = oidc::OidcConfig::from_env() else {
        return sso_error_redirect();
    };

    let (identity, return_to) = match oidc::finish(&state.pending, &cfg, code, state_param).await {
        Ok(v) => v,
        Err(e) => {
            log_sso_err(&e);
            return sso_error_redirect();
        }
    };

    mint_session_and_redirect(&state, &identity, &return_to, &headers).await
}

async fn mint_session_and_redirect(
    state: &AppState,
    identity: &crate::auth::external::ExternalIdentity,
    return_to: &str,
    headers: &HeaderMap,
) -> Response {
    if let Some(res) = reject_if_setup_required(state).await {
        return res;
    }
    let (user, _incomplete) = match link_or_create_user(&state.db, identity).await {
        Ok(v) => v,
        Err(e) => {
            log_sso_err(&e);
            return sso_error_redirect();
        }
    };

    let client = crate::rpc::ClientMeta::from_headers(headers);
    // SSO sessions: remember_me=false. Never use WorkOS sealed cookies (T-04-17).
    let (_token, cookie) = match state
        .sessions
        .create(
            &state.db,
            &user.id,
            false,
            client.ip_address.as_deref(),
            client.user_agent.as_deref(),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "session mint failed after SSO");
            return sso_error_redirect();
        }
    };

    crate::audit::record_with(
        &state.db,
        &client,
        Some((&user.id, &user.username)),
        "auth.sso_login",
        Some(("user", &user.id)),
        None,
    )
    .await;

    let path = sanitize_return_to(Some(return_to));
    let session_header = cookie.to_string();
    let presence =
        build_session_presence_cookie(SESSION_IDLE, &state.env_name).to_string();
    debug_assert!(session_header.contains(SESSION_COOKIE_NAME));
    debug_assert!(presence.contains(SESSION_PRESENCE_COOKIE_NAME));
    redirect_with_cookies(&path, &[session_header, presence])
}
