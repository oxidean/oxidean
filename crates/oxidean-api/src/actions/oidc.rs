//! OIDC issuer for `id-token: write` jobs — GitHub's `ACTIONS_ID_TOKEN_*`
//! surface. Workflows fetch signed JWTs to federate into cloud providers
//! (AWS/GCP/Vault) without stored credentials.
//!
//! Flow (GitHub parity):
//! 1. Claim: `id-token: write` mints `ACTIONS_ID_TOKEN_REQUEST_URL` +
//!    `ACTIONS_ID_TOKEN_REQUEST_TOKEN` into the job env (hash-bound to the
//!    job's token row — see [`crate::actions::job_tokens`]).
//! 2. Request: `GET {origin}/api/actions/oidc/token?audience=<aud>` with
//!    `Authorization: Bearer <request token>` → `{"value": "<jwt>"}`.
//! 3. Verify: consumers fetch `/.well-known/openid-configuration` for the
//!    `jwks_uri`, then validate the Ed25519 signature + `iss`/`aud`/`exp`.
//!
//! The signing key lives in `action_oidc_keys` (PKCS#8 hex, API-side
//! generation); `list_action_oidc_public_jwks` keeps rotated keys verifiable
//! while in-flight JWTs age out.

use std::collections::BTreeMap;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use ed25519_dalek::pkcs8::EncodePrivateKey;
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::auth::session::{bytes_to_hex, sha256_hex};

/// The job-env request endpoint (`ACTIONS_ID_TOKEN_REQUEST_URL`).
pub const TOKEN_REQUEST_PATH: &str = "/api/actions/oidc/token";

/// OIDC discovery document — consumers resolve `jwks_uri` from `iss`.
pub const DISCOVERY_PATH: &str = "/.well-known/actions-oidc/openid-configuration";

/// Issuer claim — the instance's public origin names the issuer (matching
/// GitHub's `https://token.actions.githubusercontent.com`).
fn issuer() -> String {
    crate::public_origin::resolve_public_origin()
}

// ── keys ──────────────────────────────────────────────────────────────────

/// JWK JSON for an Ed25519 verifying key (`{kty,crv,x,kid,alg,use}`).
fn public_jwk(kid: &str, public_bytes: &[u8; 32]) -> String {
    use base64::Engine;
    let x = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(public_bytes);
    serde_json::json!({
        "kty": "OKP",
        "crv": "Ed25519",
        "x": x,
        "kid": kid,
        "alg": "EdDSA",
        "use": "sig",
    })
    .to_string()
}

/// Generate a fresh Ed25519 keypair — returns (kid, private_pkcs8_hex, jwk).
/// `kid` is the first 16 hex of sha256(public key) — stable and collision-
/// safe enough for a single-issuer key set.
fn generate_keypair() -> (String, String, String) {
    let mut seed = [0u8; 32];
    rand::fill(&mut seed);
    let signing = ed25519_dalek::SigningKey::from_bytes(&seed);
    let public = signing.verifying_key().to_bytes();
    let kid = sha256_hex(&public)[..16].to_string();
    let der = signing
        .to_pkcs8_der()
        .map(|d| d.as_bytes().to_vec())
        .unwrap_or_default();
    let public_der_hex = bytes_to_hex(&der);
    let jwk = public_jwk(&kid, &public);
    (kid, public_der_hex, jwk)
}

/// The active signing key — lazily generated on first OIDC use so fresh
/// instances self-provision without an admin step.
async fn signing_key(db: &oxidean_db::Database) -> Result<oxidean_db::ActionOidcKeyRow, String> {
    if let Some(k) = db.find_active_action_oidc_key().await? {
        return Ok(k);
    }
    let (kid, private_der, jwk) = generate_keypair();
    db.insert_action_oidc_key(&kid, &private_der, &jwk).await?;
    db.find_active_action_oidc_key()
        .await?
        .ok_or_else(|| "oidc key insert vanished".to_string())
}

// ── discovery endpoints (public, no auth) ─────────────────────────────────

/// `GET /.well-known/actions-oidc/openid-configuration` — RFC-8414-shaped
/// discovery document pointing at our JWKS.
pub async fn openid_configuration() -> Json<serde_json::Value> {
    let iss = issuer();
    Json(serde_json::json!({
        "issuer": iss,
        "jwks_uri": format!("{iss}/api/actions/oidc/jwks"),
        "response_types_supported": ["id_token"],
        "id_token_signing_alg_values_supported": ["EdDSA"],
        "subject_types_supported": ["public"],
    }))
}

/// `GET /api/actions/oidc/jwks` — every stored public key (rotation-safe).
pub async fn jwks(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let keys = state
        .db
        .list_action_oidc_public_jwks()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let parsed: Vec<serde_json::Value> = keys
        .iter()
        .filter_map(|s| serde_json::from_str(s).ok())
        .collect();
    Ok(Json(serde_json::json!({ "keys": parsed })))
}

// ── token endpoint ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TokenQuery {
    /// GitHub's toolkit sends `audience`; `aud` accepted as an alias.
    pub audience: Option<String>,
    pub aud: Option<String>,
}

#[derive(Debug, Serialize)]
struct OidcClaims {
    iss: String,
    sub: String,
    aud: String,
    iat: i64,
    nbf: i64,
    exp: i64,
    jti: String,
    // GitHub parity claims
    repository: String,
    repository_owner: String,
    actor: String,
    #[serde(rename = "ref")]
    git_ref: String,
    sha: String,
    event_name: String,
    ref_type: String,
    run_id: String,
    run_number: i64,
    run_attempt: i64,
    workflow: String,
    job_workflow_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    environment: Option<String>,
    runner_environment: String,
}

/// `GET /api/actions/oidc/token?audience=…` — bearer-authed by the per-job
/// request token (`ACTIONS_ID_TOKEN_REQUEST_TOKEN`); returns `{"value":jwt}`
/// per GitHub's toolkit contract.
pub async fn request_token(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(q): Query<TokenQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let fail = |code: StatusCode, msg: &str| (code, Json(serde_json::json!({ "message": msg })));
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .unwrap_or_default();
    if raw.is_empty() {
        return Err(fail(StatusCode::UNAUTHORIZED, "missing request token"));
    }
    let row = state
        .db
        .find_job_token_by_oidc_hash(&sha256_hex(raw.as_bytes()))
        .await
        .map_err(|_| fail(StatusCode::INTERNAL_SERVER_ERROR, "lookup failed"))?
        .ok_or_else(|| fail(StatusCode::UNAUTHORIZED, "invalid request token"))?;
    if row.revoked_at.is_some() || crate::oauth::rfc3339_expired(&row.expires_at) {
        return Err(fail(
            StatusCode::UNAUTHORIZED,
            "request token expired or revoked",
        ));
    }
    // The bound job must be live — a terminal job can't fetch more tokens.
    let job = state
        .db
        .find_action_job_by_id(&row.job_id)
        .await
        .map_err(|_| fail(StatusCode::INTERNAL_SERVER_ERROR, "job lookup failed"))?
        .ok_or_else(|| fail(StatusCode::NOT_FOUND, "job not found"))?;
    if !matches!(job.status.as_str(), "queued" | "in_progress") {
        return Err(fail(
            StatusCode::FORBIDDEN,
            "job is not running — token requests closed",
        ));
    }
    // `id-token: write` must be present in the minted grant (defense in
    // depth: the request token only exists when it was).
    let grant_ok = row
        .perms_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<BTreeMap<String, String>>(s).ok())
        .and_then(|m| m.get("id-token").cloned())
        .is_some_and(|l| l == "write");
    if !grant_ok {
        return Err(fail(
            StatusCode::FORBIDDEN,
            "id-token: write not granted to this job",
        ));
    }

    let run = state
        .db
        .find_action_run_by_id(&row.run_id)
        .await
        .map_err(|_| fail(StatusCode::INTERNAL_SERVER_ERROR, "run lookup failed"))?
        .ok_or_else(|| fail(StatusCode::NOT_FOUND, "run not found"))?;
    let repo = state
        .db
        .find_repository_by_id(&row.repository_id)
        .await
        .map_err(|_| fail(StatusCode::INTERNAL_SERVER_ERROR, "repo lookup failed"))?
        .ok_or_else(|| fail(StatusCode::NOT_FOUND, "repository not found"))?;
    let owner_ref = crate::repo::owner_ref_for_repo(&state.db, &repo)
        .await
        .map_err(|_| fail(StatusCode::INTERNAL_SERVER_ERROR, "owner lookup failed"))?
        .ok_or_else(|| fail(StatusCode::NOT_FOUND, "repository owner not found"))?;
    let owner_slug = owner_ref.slug().to_string();
    let actor = match row.user_id.as_str() {
        "" => String::new(),
        uid => state
            .db
            .find_user_by_id(uid)
            .await
            .ok()
            .flatten()
            .map(|u| u.username)
            .unwrap_or_default(),
    };

    let key = signing_key(&state.db)
        .await
        .map_err(|_| fail(StatusCode::INTERNAL_SERVER_ERROR, "signing key unavailable"))?;
    let der = hex::decode(&key.private_der)
        .map_err(|_| fail(StatusCode::INTERNAL_SERVER_ERROR, "corrupt signing key"))?;
    let encoding = jsonwebtoken::EncodingKey::from_ed_der(&der);

    let aud = q.audience.or(q.aud).unwrap_or_else(issuer);
    let now = chrono::Utc::now().timestamp();
    let ref_type = if run.head_ref.starts_with("refs/tags/") {
        "tag"
    } else {
        "branch"
    };
    let full_repo = format!("{owner_slug}/{}", repo.name);
    let payload = state
        .db
        .find_action_payload(job.payload_sha.as_deref().unwrap_or_default())
        .await
        .ok()
        .flatten()
        .and_then(|p| serde_json::from_str::<serde_json::Value>(&p.payload_json).ok());
    let environment = payload
        .as_ref()
        .and_then(|p| p.get("environment"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let claims = OidcClaims {
        iss: issuer(),
        sub: format!("repo:{full_repo}:ref:{}", run.head_ref),
        aud,
        iat: now,
        nbf: now - 60,
        exp: now + 600,
        jti: uuid::Uuid::new_v4().to_string(),
        repository: full_repo.clone(),
        repository_owner: owner_slug.clone(),
        actor,
        git_ref: run.head_ref.clone(),
        sha: run.head_sha.clone(),
        event_name: run.event.clone(),
        ref_type: ref_type.into(),
        run_id: run.id.clone(),
        run_number: run.run_number,
        run_attempt: run.attempt,
        workflow: run.workflow_name.clone(),
        job_workflow_ref: format!("{full_repo}/{}@{}", run.workflow_path, run.head_ref),
        environment,
        runner_environment: "self-hosted".into(),
    };
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::EdDSA);
    header.kid = Some(key.kid.clone());
    let jwt = jsonwebtoken::encode(&header, &claims, &encoding)
        .map_err(|_| fail(StatusCode::INTERNAL_SERVER_ERROR, "token signing failed"))?;
    Ok(Json(serde_json::json!({ "value": jwt })))
}

/// Mount on the app router — all three paths are unauthenticated at the HTTP
/// layer (the request token is itself the credential).
pub fn router() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/token", axum::routing::get(request_token))
        .route("/jwks", axum::routing::get(jwks))
}
