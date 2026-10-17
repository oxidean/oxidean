//! OIDC issuer for `id-token: write` jobs — GitHub's `ACTIONS_ID_TOKEN_*`
//! surface. Workflows fetch signed JWTs to federate into cloud providers
//! (AWS/GCP/Vault) without stored credentials.
//!
//! Flow (GitHub parity):
//! 1. Claim: `id-token: write` mints `ACTIONS_ID_TOKEN_REQUEST_URL` +
//!    `ACTIONS_ID_TOKEN_REQUEST_TOKEN` into the job env (hash-bound to the
//!    job's token row — see [`crate::actions::job_tokens`]). The URL already
//!    carries `?api-version=2.0` so `@actions/core`'s `&audience=` append
//!    produces a valid query string.
//! 2. Request: `GET {iss}/token?api-version=2.0&audience=<aud>` with
//!    `Authorization: Bearer <request token>` → `{"value": "<jwt>"}`.
//! 3. Verify: consumers fetch `{iss}/.well-known/openid-configuration` for the
//!    `jwks_uri`, then validate the RS256 signature + `iss`/`aud`/`exp`.
//!
//! The issuer is a *path issuer* — `{public_origin}/api/actions/oidc` (like
//! GHES's `/_services/token`) — so discovery naturally resolves under the API
//! surface `oxidean-web` already proxies.
//!
//! Signing: RSA-2048/RS256 — GCP STS accepts only RS256/ES256 and AWS STS
//! RS256/384/512 (EdDSA is supported by neither). Every private-key operation
//! (keygen, signing) goes through `aws-lc-rs`, which is constant-time; the
//! `rsa` crate is never used for private-key material (RUSTSEC-2023-0071 is a
//! timing side channel). Keys live in `action_oidc_keys`: the private half is
//! AES-256-GCM ciphertext under the same secrets key as repo secrets
//! (`private_enc`), so a configured secrets key is required to mint.

use std::collections::BTreeMap;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::auth::session::{bytes_to_hex, sha256_hex};

/// The OIDC issuer path — tokens carry `iss = {public_origin}{ISSUER_PATH}`.
pub const ISSUER_PATH: &str = "/api/actions/oidc";

/// The job-env request endpoint (`ACTIONS_ID_TOKEN_REQUEST_URL`), including
/// the pre-queried `?api-version=2.0` `@actions/core` expects before appending
/// `&audience=…`.
pub const TOKEN_REQUEST_PATH: &str = "/api/actions/oidc/token?api-version=2.0";

/// Signing algorithm — only RS256 is minted and listed in the JWKS.
const SIGNING_ALG: &str = "RS256";

/// Issuer claim — `{public_origin}/api/actions/oidc` (a path issuer, like
/// GHES's `/_services/token`; consumers append `/.well-known/…` to it).
fn issuer() -> String {
    format!(
        "{}{}",
        crate::public_origin::resolve_public_origin(),
        ISSUER_PATH
    )
}

// ── keys ──────────────────────────────────────────────────────────────────

fn b64u(data: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

/// JWK JSON for an RSA verifying key (`{kty,n,e,kid,alg,use}`).
fn public_jwk(kid: &str, n: &[u8], e: &[u8]) -> String {
    serde_json::json!({
        "kty": "RSA",
        "n": b64u(n),
        "e": b64u(e),
        "kid": kid,
        "alg": SIGNING_ALG,
        "use": "sig",
    })
    .to_string()
}

/// Read one DER TLV of the expected tag; returns (value, total TLV length).
/// Definite-form lengths only — enough for `RSAPublicKey`.
fn der_tlv(input: &[u8], tag: u8) -> Result<(&[u8], usize), String> {
    if input.len() < 2 || input[0] != tag {
        return Err("rsa public key der: unexpected tag".into());
    }
    let (len, header_len) = match input[1] {
        l if l & 0x80 == 0 => (l as usize, 2usize),
        0x81 => (
            *input.get(2).ok_or("rsa der: truncated length")? as usize,
            3,
        ),
        0x82 => {
            let b = input.get(2..4).ok_or("rsa der: truncated length")?;
            (u16::from_be_bytes([b[0], b[1]]) as usize, 4)
        }
        _ => return Err("rsa der: unsupported length form".into()),
    };
    let end = header_len + len;
    if end > input.len() {
        return Err("rsa der: truncated value".into());
    }
    Ok((&input[header_len..end], end))
}

/// RSAPublicKey ::= SEQUENCE { modulus INTEGER, publicExponent INTEGER }.
/// Strips the DER sign-padding zero byte so `n`/`e` are JWK-ready.
fn rsa_public_components(pkcs1_der: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let (seq, _) = der_tlv(pkcs1_der, 0x30)?;
    let (n_raw, n_len) = der_tlv(seq, 0x02)?;
    let (e_raw, _) = der_tlv(&seq[n_len..], 0x02)?;
    fn minimal(v: &[u8]) -> &[u8] {
        if v.len() > 1 && v[0] == 0 {
            &v[1..]
        } else {
            v
        }
    }
    Ok((minimal(n_raw).to_vec(), minimal(e_raw).to_vec()))
}

/// Generate a fresh RSA-2048 keypair — returns (kid, pkcs8_der, jwk). `kid` is
/// the first 16 hex of sha256(public key) — stable and collision-safe enough
/// for a single-issuer key set.
fn generate_keypair() -> Result<(String, Vec<u8>, String), String> {
    use aws_lc_rs::encoding::AsDer;
    use aws_lc_rs::signature::KeyPair;
    let kp = aws_lc_rs::rsa::KeyPair::generate(aws_lc_rs::rsa::KeySize::Rsa2048)
        .map_err(|_| "rsa-2048 key generation failed".to_string())?;
    let pkcs8 = kp
        .as_der()
        .map_err(|_| "rsa pkcs8 encoding failed".to_string())?;
    let pkcs1 = kp.public_key().as_ref().to_vec();
    let (n, e) = rsa_public_components(&pkcs1)?;
    let kid = sha256_hex(&pkcs1)[..16].to_string();
    let jwk = public_jwk(&kid, &n, &e);
    Ok((kid, pkcs8.as_ref().to_vec(), jwk))
}

/// The active RS256 signing key — lazily generated on first OIDC use so fresh
/// instances self-provision without an admin step. The private half is stored
/// as secrets ciphertext (`private_enc`); a missing secrets key surfaces as a
/// clear error instead of minting.
async fn signing_key(db: &oxidean_db::Database) -> Result<(String, Vec<u8>), String> {
    if let Some(k) = db.find_active_action_oidc_key(SIGNING_ALG).await? {
        let cipher = k.private_enc.as_deref().unwrap_or(&k.private_der);
        let der_hex = crate::actions::secrets::decrypt_secret(cipher).map_err(|e| {
            format!("OIDC signing key cannot be decrypted — is OXIDEAN_ACTIONS_SECRETS_KEY or OXIDEAN_SESSION_SECRET configured? {e}")
        })?;
        let der = hex::decode(&der_hex).map_err(|_| "corrupt OIDC signing key".to_string())?;
        return Ok((k.kid, der));
    }
    let (kid, pkcs8, jwk) = generate_keypair()?;
    let cipher = crate::actions::secrets::encrypt_secret(&bytes_to_hex(&pkcs8)).map_err(|e| {
        format!("OIDC signing key cannot be encrypted — is OXIDEAN_ACTIONS_SECRETS_KEY or OXIDEAN_SESSION_SECRET configured? {e}")
    })?;
    db.insert_action_oidc_key(&kid, SIGNING_ALG, "", Some(&cipher), &jwk)
        .await?;
    Ok((kid, pkcs8))
}

/// RS256-sign a compact JWT (`header.payload.signature`) with the PKCS#8 RSA
/// private key — aws-lc-rs signs (constant-time); `rsa`-crate code paths are
/// never touched for private-key operations.
fn encode_rs256(claims: &OidcClaims, kid: &str, pkcs8_der: &[u8]) -> Result<String, String> {
    let header = serde_json::json!({ "alg": SIGNING_ALG, "typ": "JWT", "kid": kid }).to_string();
    let payload = serde_json::to_vec(claims).map_err(|e| e.to_string())?;
    let signing_input = format!("{}.{}", b64u(header.as_bytes()), b64u(&payload));
    let key_pair = aws_lc_rs::rsa::KeyPair::from_pkcs8(pkcs8_der)
        .map_err(|_| "oidc signing key is not a valid RSA PKCS#8 key".to_string())?;
    let mut signature = vec![0u8; key_pair.public_modulus_len()];
    key_pair
        .sign(
            &aws_lc_rs::signature::RSA_PKCS1_SHA256,
            &aws_lc_rs::rand::SystemRandom::new(),
            signing_input.as_bytes(),
            &mut signature,
        )
        .map_err(|_| "oidc token signing failed".to_string())?;
    Ok(format!("{signing_input}.{}", b64u(&signature)))
}

// ── discovery endpoints (public, no auth) ─────────────────────────────────

/// `GET /api/actions/oidc/.well-known/openid-configuration` — RFC-8414-shaped
/// discovery document at the issuer-relative location consumers fetch.
pub async fn openid_configuration() -> Json<serde_json::Value> {
    let iss = issuer();
    Json(serde_json::json!({
        "issuer": iss,
        "jwks_uri": format!("{iss}/jwks"),
        "response_types_supported": ["id_token"],
        "subject_types_supported": ["public", "pairwise"],
        "id_token_signing_alg_values_supported": [SIGNING_ALG],
        "scopes_supported": ["openid"],
        "claims_supported": [
            "iss", "sub", "aud", "exp", "iat", "nbf", "jti",
            "actor", "actor_id", "base_ref", "environment", "event_name",
            "head_ref", "job_workflow_ref", "job_workflow_sha", "ref",
            "ref_protected", "ref_type", "repository", "repository_id",
            "repository_owner", "repository_owner_id",
            "repository_visibility", "run_attempt", "run_id", "run_number",
            "runner_environment", "sha", "workflow", "workflow_ref",
            "workflow_sha",
        ],
    }))
}

/// `GET /api/actions/oidc/jwks` — every stored RS256 public key
/// (rotation-safe; legacy Ed25519 rows never sign or list).
pub async fn jwks(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let keys = state
        .db
        .list_action_oidc_public_jwks(SIGNING_ALG)
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

/// GitHub's documented OIDC claim set (plus the standard JWT claims). PR-only
/// `base_ref`/`head_ref` serialize as `""` outside pull_request events,
/// matching GitHub's own tokens; `environment` is omitted when the job has
/// none.
#[derive(Debug, Serialize)]
struct OidcClaims {
    iss: String,
    sub: String,
    aud: String,
    iat: i64,
    nbf: i64,
    exp: i64,
    jti: String,
    actor: String,
    actor_id: String,
    base_ref: String,
    event_name: String,
    head_ref: String,
    job_workflow_ref: String,
    job_workflow_sha: String,
    #[serde(rename = "ref")]
    git_ref: String,
    ref_protected: bool,
    ref_type: String,
    repository: String,
    repository_id: String,
    repository_owner: String,
    repository_owner_id: String,
    repository_visibility: String,
    run_attempt: i64,
    run_id: String,
    run_number: i64,
    runner_environment: String,
    sha: String,
    workflow: String,
    workflow_ref: String,
    workflow_sha: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    environment: Option<String>,
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
    // GitHub's documented curl uses lowercase `bearer` — accept the scheme
    // case-insensitively.
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|v| {
            v.strip_prefix("Bearer ")
                .or_else(|| v.strip_prefix("bearer "))
                .unwrap_or(v)
        })
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
    let (actor, actor_id) = match row.user_id.as_str() {
        "" => (String::new(), String::new()),
        uid => (
            state
                .db
                .find_user_by_id(uid)
                .await
                .ok()
                .flatten()
                .map(|u| u.username)
                .unwrap_or_default(),
            uid.to_string(),
        ),
    };

    let (kid, pkcs8) = signing_key(&state.db)
        .await
        .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, &e))?;

    let full_repo = format!("{owner_slug}/{}", repo.name);
    // GitHub defaults `aud` to the repository owner's URL.
    let aud = q.audience.or(q.aud).unwrap_or_else(|| {
        format!(
            "{}/{owner_slug}",
            crate::public_origin::resolve_public_origin()
        )
    });
    let now = chrono::Utc::now().timestamp();
    let (ref_type, base_ref, head_ref_pr) = if run.head_ref.starts_with("refs/tags/") {
        ("tag", String::new(), String::new())
    } else {
        let event = run
            .event_payload_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());
        let pr_field = |key: &str| {
            event
                .as_ref()
                .and_then(|e| e.get("pull_request"))
                .and_then(|p| p.get(key))
                .and_then(|r| r.get("ref"))
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_default()
        };
        ("branch", pr_field("base"), pr_field("head"))
    };
    let ref_protected = if let Some(tag) = crate::protection::tag_from_ref(&run.head_ref) {
        crate::protection::effective_for_tag(&state.db, &repo.id, tag)
            .await
            .map(|e| e.matched)
            .unwrap_or(false)
    } else if let Some(branch) = crate::protection::branch_from_ref(&run.head_ref) {
        crate::protection::effective_for_branch(&state.db, &repo.id, branch)
            .await
            .map(|e| e.matched)
            .unwrap_or(false)
    } else {
        false
    };
    let payload = state
        .db
        .find_action_payload(job.payload_sha.as_deref().unwrap_or_default())
        .await
        .ok()
        .flatten()
        .and_then(|p| serde_json::from_str::<serde_json::Value>(&p.payload_json).ok());
    // `environment:` accepts a bare name or `{ name, url }` — the claim and
    // subject use the name either way.
    let environment = payload
        .as_ref()
        .and_then(|p| p.get("environment"))
        .and_then(|v| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Object(_) => {
                v.get("name").and_then(|n| n.as_str()).map(String::from)
            }
            _ => None,
        });
    // Subject formats (GitHub parity): environment wins, then pull_request,
    // then the triggering ref.
    let sub = match &environment {
        Some(env) => format!("repo:{full_repo}:environment:{env}"),
        None if run.event.starts_with("pull_request") => {
            format!("repo:{full_repo}:pull_request")
        }
        None => format!("repo:{full_repo}:ref:{}", run.head_ref),
    };
    let workflow_ref = format!("{full_repo}/{}@{}", run.workflow_path, run.head_ref);
    let claims = OidcClaims {
        iss: issuer(),
        sub,
        aud,
        iat: now,
        nbf: now - 60,
        exp: now + 600,
        jti: uuid::Uuid::new_v4().to_string(),
        actor,
        actor_id,
        base_ref,
        event_name: run.event.clone(),
        head_ref: head_ref_pr,
        // Our jobs never call reusable workflows in another repo, so the
        // called workflow is the run's own.
        job_workflow_ref: workflow_ref.clone(),
        job_workflow_sha: run.head_sha.clone(),
        git_ref: run.head_ref.clone(),
        ref_protected,
        ref_type: ref_type.into(),
        repository: full_repo.clone(),
        repository_id: repo.id.clone(),
        repository_owner: owner_slug.clone(),
        repository_owner_id: repo.owner_id.clone(),
        repository_visibility: repo.visibility.clone(),
        run_attempt: run.attempt,
        run_id: run.id.clone(),
        run_number: run.run_number,
        runner_environment: "self-hosted".into(),
        sha: run.head_sha.clone(),
        workflow: run.workflow_name.clone(),
        workflow_ref,
        workflow_sha: run.head_sha.clone(),
        environment,
    };
    let jwt = encode_rs256(&claims, &kid, &pkcs8)
        .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, &e))?;
    Ok(Json(serde_json::json!({ "value": jwt })))
}

/// Mount on the app router — all paths are unauthenticated at the HTTP layer
/// (the request token is itself the credential). Discovery sits at the
/// issuer-relative `/.well-known/openid-configuration` under the API prefix.
pub fn router() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/token", axum::routing::get(request_token))
        .route("/jwks", axum::routing::get(jwks))
        .route(
            "/.well-known/openid-configuration",
            axum::routing::get(openid_configuration),
        )
}
