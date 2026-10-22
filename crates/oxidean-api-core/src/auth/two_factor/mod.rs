//! Two-factor authentication (issue #130).
//!
//! Methods: TOTP authenticator apps, WebAuthn/passkeys (platform authenticators
//! such as Face ID, Touch ID, fingerprint readers, and Windows Hello — plus
//! roaming keys), single-use recovery codes, and a pre-session email OTP.
//!
//! Flow: sign-in is identifier-first — `auth.login_begin` resolves the
//! email/username and opens a pending `TwoFactorStore` challenge (its id set
//! as an `oxidean_2fa` HttpOnly cookie). The challenge carries the
//! outstanding requirements: prove a factor (`needs_factor`), enroll one
//! (`needs_enroll`), and/or submit the account password (`needs_password`).
//! `auth.two_factor.complete` / `passkey_*` / `email_send` satisfy the factor
//! leg, `auth.login` with the challenge id satisfies the password leg, and a
//! user-verified passkey assertion satisfies both at once. Only when nothing
//! remains is a real session minted via `issue_session` + `CookieChange::Set`.
//! One-shot `auth.login` (identifier + password, no challenge) keeps working
//! for machine clients: it opens the same challenge post-password.
//!
//! Enrollment legs never run while `needs_password` is still set — a factor
//! must not be plantable on an account pre-auth — so enroll-required users
//! always take the password step first.
//!
//! Policy: `auth_settings.require_two_factor_local` / `_external` (instance)
//! and `organizations.require_two_factor` (any membership) decide whether a
//! factor-less user must enroll (`flags::login_gate`). External SSO logins
//! follow the external policy — deployments that trust their IdP's MFA leave
//! it off.

pub mod flags;
pub mod recovery;
pub mod store;
pub mod totp;

use cookie::time::Duration as TimeDuration;
use oxidean_core::{AppError, TwoFactorCompletePublic, UserPublic};
use oxidean_db::{Database, UserRow};
use url::Url;
use uuid::Uuid;
use webauthn_rs::prelude::*;

use crate::audit;
use crate::auth::local::{issue_session, user_to_public};
use crate::rpc::{CookieChange, RpcCtx};

pub use flags::LoginChannel;
pub use store::{OpenChallenge, PendingKind, TwoFactorStore};

/// Second-factor state cookie — HttpOnly, scoped to the challenge TTL.
pub const TWO_FACTOR_COOKIE_NAME: &str = "oxidean_2fa";
pub const OTP_EMAIL_TTL_MINS: u64 = 30;
/// OTP e-mail resend cap per challenge.
pub const MAX_EMAIL_SENDS: u32 = 5;
/// Email-token redeem attempts cap (mirrors `verify_reset::MAX_REDEEM_ATTEMPTS`).
pub const MAX_OTP_ATTEMPTS: i32 = 10;
const MAX_NICKNAME_LEN: usize = 64;
/// Env vars searched (in order) for the TOTP secret encryption key.
const TOTP_KEY_ENVS: &[&str] = &[
    "OXIDEAN_SECRETS_KEY",
    "OXIDEAN_ACTIONS_SECRETS_KEY",
    "OXIDEAN_SESSION_SECRET",
];

/// Shared challenge store + parsed `oxidean_2fa` cookie, populated at the HTTP
/// edge. `challenge_id` is `None` for plain session-only calls.
#[derive(Clone, Default)]
pub struct TwoFactorCtx {
    pub store: TwoFactorStore,
    pub challenge_id: Option<String>,
}

pub fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("two-factor db error: {e}");
        AppError::new("auth.internal", "authentication failed")
    }
}

pub fn invalid(msg: &str) -> AppError {
    AppError::new("auth.two_factor_invalid", msg)
}

pub fn expired(msg: &str) -> AppError {
    AppError::new("auth.two_factor_expired", msg)
}

pub fn bad_input(e: serde_json::Error) -> AppError {
    AppError::new("rpc.bad_input", format!("invalid two-factor input: {e}"))
}

pub fn encrypt_str(plaintext: &str) -> Result<String, AppError> {
    crate::crypto::encrypt(TOTP_KEY_ENVS, plaintext).map_err(|e| {
        tracing::error!("totp secret encrypt failed: {e}");
        AppError::new("internal.error", "secret encryption unavailable")
    })
}

pub fn decrypt_str(blob: &str) -> Result<String, AppError> {
    crate::crypto::decrypt(TOTP_KEY_ENVS, blob).map_err(|e| {
        tracing::error!("totp secret decrypt failed: {e}");
        AppError::new("internal.error", "stored secret is unreadable")
    })
}
pub fn challenge_cookie(env_name: &str, id: &str) -> cookie::Cookie<'static> {
    let mut cookie = cookie::Cookie::new(TWO_FACTOR_COOKIE_NAME, id.to_string());
    cookie.set_http_only(true);
    cookie.set_same_site(cookie::SameSite::Lax);
    cookie.set_path("/");
    cookie.set_max_age(TimeDuration::seconds(store::CHALLENGE_TTL.as_secs() as i64));
    if env_name != "development" {
        cookie.set_secure(true);
    }
    cookie
}

/// `Set-Cookie` header value for a newly opened challenge — used by the SSO
/// callback route, which sets cookies on its redirect response directly.
pub fn challenge_cookie_header(env_name: &str, id: &str) -> String {
    challenge_cookie(env_name, id).to_string()
}

/// Clear cookie value (appended alongside `CookieChange::Set`/session mint).
pub fn cleared_challenge_cookie(env_name: &str) -> cookie::Cookie<'static> {
    let mut cookie = cookie::Cookie::new(TWO_FACTOR_COOKIE_NAME, "");
    cookie.set_http_only(true);
    cookie.set_same_site(cookie::SameSite::Lax);
    cookie.set_path("/");
    cookie.set_max_age(TimeDuration::seconds(0));
    if env_name != "development" {
        cookie.set_secure(true);
    }
    cookie
}
/// Open a pending challenge and queue its `oxidean_2fa` cookie on the ctx;
/// returns the challenge id. (The SSO callback sets the cookie on its
/// redirect response directly via `challenge_cookie_header`.)
pub fn open_and_cookie(ctx: &mut RpcCtx, params: OpenChallenge) -> String {
    let id = ctx.two_factor.store.open(params);
    ctx.set_cookie = Some(CookieChange::SetTwoFactor(challenge_cookie(
        &ctx.env_name,
        &id,
    )));
    id
}

/// `auth.two_factor_required` payload for a challenge that is still open —
/// used by the stepped `auth.login` password leg when factor/enroll remains.
pub fn pending_required_error(id: &str, pending: &store::PendingInfo) -> AppError {
    AppError::new(
        "auth.two_factor_required",
        "two-factor authentication is required",
    )
    .with_data(serde_json::json!({
        "challenge": id,
        "kind": pending.kind().as_str(),
        "methods": pending.methods,
    }))
}

/// Open a pending challenge for `user` and return the
/// `auth.two_factor_required` error `auth.login` / SSO callbacks surface.
/// Also queues the `oxidean_2fa` cookie on the ctx (RPC path) — the SSO
/// callback sets it on its redirect response directly via `challenge_cookie`.
/// `password_satisfied` is true when a password-equivalent primary credential
/// already passed (one-shot login, signup, reset, SSO identity).
pub fn required_error(
    ctx: &mut RpcCtx,
    user_id: &str,
    remember_me: bool,
    kind: PendingKind,
    methods: Vec<String>,
    password_satisfied: bool,
) -> AppError {
    let id = open_and_cookie(
        ctx,
        OpenChallenge {
            user_id: user_id.to_string(),
            remember_me,
            needs_factor: kind == PendingKind::Verify,
            needs_enroll: kind == PendingKind::Enroll,
            needs_password: !password_satisfied,
            methods: methods.clone(),
        },
    );
    AppError::new(
        "auth.two_factor_required",
        "two-factor authentication is required",
    )
    .with_data(serde_json::json!({
        "challenge": id,
        "kind": kind.as_str(),
        "methods": methods,
    }))
}

/// The effective challenge id: explicit input beats the cookie.
pub fn challenge_id(ctx: &RpcCtx, explicit: Option<&str>) -> Option<String> {
    explicit
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| ctx.two_factor.challenge_id.clone())
}

/// Load live challenge metadata; `auth.two_factor_expired` when stale/unknown.
pub fn pending_or_err(ctx: &RpcCtx, id: &str) -> Result<store::PendingInfo, AppError> {
    ctx.two_factor
        .store
        .peek(id)
        .ok_or_else(|| expired("two-factor challenge expired — sign in again"))
}

/// Count a failed attempt; drop the challenge when the cap is hit.
pub fn bump_attempts(ctx: &RpcCtx, id: &str) -> Result<(), AppError> {
    let attempts = ctx
        .two_factor
        .store
        .update(id, |p| {
            p.attempts += 1;
            p.attempts
        })
        .ok_or_else(|| expired("two-factor challenge expired — sign in again"))?;
    if attempts >= store::MAX_ATTEMPTS {
        ctx.two_factor.store.drop_challenge(id);
        return Err(AppError::new(
            "auth.rate_limited",
            "too many failed attempts — sign in again",
        ));
    }
    Ok(())
}

/// Enrollment actor: `(user_id, Option<challenge_id>)` — a verified signed-in
/// user, or a user inside an *enroll*-kind pending challenge.
pub async fn enroll_actor(
    ctx: &RpcCtx,
    explicit_challenge: Option<&str>,
) -> Result<(String, Option<String>), AppError> {
    if ctx.session.is_some() {
        let user = crate::auth::gate::require_verified(ctx).await?;
        return Ok((user.id, None));
    }
    if let Some(id) = challenge_id(ctx, explicit_challenge) {
        let pending = pending_or_err(ctx, &id)?;
        if !pending.needs_enroll {
            return Err(AppError::new(
                "auth.two_factor_unavailable",
                "this challenge expects verification, not enrollment",
            ));
        }
        if pending.needs_password {
            // Enroll legs never run pre-password — a factor must not be
            // plantable on an account its owner has not authenticated into.
            return Err(AppError::new(
                "auth.two_factor_unavailable",
                "finish the password step before enrolling a factor",
            ));
        }
        return Ok((pending.user_id, Some(id)));
    }
    Err(AppError::new(
        "auth.unauthenticated",
        "authentication required",
    ))
}

/// Mint the real session after the challenge's requirement is satisfied.
pub async fn mint_session(
    ctx: &mut RpcCtx,
    pending: store::PendingChallenge,
    method_detail: &str,
) -> Result<UserPublic, AppError> {
    let cookie = issue_session(ctx, &pending.user_id, pending.remember_me).await?;
    ctx.set_cookie = Some(CookieChange::Set(cookie));
    let user = ctx
        .db
        .find_user_by_id(&pending.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.internal", "authentication failed"))?;
    audit::record(
        ctx,
        Some((&user.id, &user.username)),
        "auth.login",
        Some(("user", &user.id)),
        Some(format!("{{\"method\":{method_detail:?}}}")),
    )
    .await;
    Ok(user_to_public(&ctx.db, &user).await)
}

/// Password leg of an identifier-first challenge (called by `auth.login`
/// with `challenge` set): verify `password` against the challenge-bound
/// `user`, clear `needs_password`, and mint only once every outstanding
/// requirement is satisfied. Remaining factor/enroll debt surfaces as the
/// same `auth.two_factor_required` payload the challenge opened with.
pub async fn password_step(
    ctx: &mut RpcCtx,
    id: &str,
    user: &UserRow,
    password: &str,
    remember_me: bool,
) -> Result<UserPublic, AppError> {
    let valid = match user.password_hash.as_deref() {
        Some(hash) => crate::auth::password::verify_password(password.as_bytes(), hash),
        None => false,
    };
    if !valid || user.banned_at.is_some() {
        // Wrong passwords burn the same attempt budget as factor codes —
        // when the cap trips, the challenge is dropped and that error wins.
        bump_attempts(ctx, id)?;
        return Err(AppError::new(
            "auth.invalid_credentials",
            "Incorrect email/username or password",
        ));
    }
    let remaining = ctx.two_factor.store.update(id, |p| {
        p.needs_password = false;
        p.remember_me = remember_me;
        (p.needs_factor, p.needs_enroll)
    });
    let Some((needs_factor, needs_enroll)) = remaining else {
        return Err(expired("two-factor challenge expired — sign in again"));
    };
    if needs_factor || needs_enroll {
        let pending = pending_or_err(ctx, id)?;
        return Err(pending_required_error(id, &pending));
    }
    let Some(pending) = ctx.two_factor.store.take(id) else {
        return Err(expired("two-factor challenge expired — sign in again"));
    };
    mint_session(ctx, pending, "password").await
}

/// Open a challenge + queue its cookie — used by `auth.login` and callers
/// whose primary credential already passed (`password_satisfied: true`).
pub fn open_challenge(
    ctx: &mut RpcCtx,
    gate: &flags::LoginGate,
    user_id: &str,
    remember_me: bool,
    password_satisfied: bool,
) -> AppError {
    let kind = if gate.enroll_required && !gate.has_factor {
        PendingKind::Enroll
    } else {
        PendingKind::Verify
    };
    required_error(
        ctx,
        user_id,
        remember_me,
        kind,
        gate.methods.clone(),
        password_satisfied,
    )
}
pub fn make_webauthn() -> Result<Webauthn, AppError> {
    let origin = crate::public_origin::resolve_public_origin();
    let url = Url::parse(&origin)
        .map_err(|_| AppError::new("internal.error", "invalid OXIDEAN_PUBLIC_ORIGIN"))?;
    let host = url
        .host_str()
        .ok_or_else(|| AppError::new("internal.error", "invalid OXIDEAN_PUBLIC_ORIGIN"))?
        .to_string();
    WebauthnBuilder::new(&host, &url)
        .and_then(|b| b.rp_name("Oxidean").build())
        .map_err(|e| {
            tracing::error!("webauthn init failed: {e:?}");
            AppError::new("internal.error", "passkey support unavailable")
        })
}

/// `CredentialID` → base64url string (storage form).
pub fn cred_id_str(id: &CredentialID) -> Result<String, AppError> {
    serde_json::to_value(id)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .ok_or_else(|| AppError::new("internal.error", "credential id encoding failed"))
}

pub async fn load_passkeys(
    db: &Database,
    user_id: &str,
) -> Result<Vec<(String /*row id*/, Passkey, String /*nickname*/)>, AppError> {
    let rows = db.passkey_list_for_user(user_id).await.map_err(db_err)?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let pk: Passkey = serde_json::from_str(&row.passkey_json).map_err(|e| {
            tracing::error!("passkey deserialize failed for {}: {e}", row.id);
            AppError::new("internal.error", "stored passkey is corrupt")
        })?;
        out.push((row.id, pk, row.nickname));
    }
    Ok(out)
}

pub fn nickname_ok(name: &str) -> bool {
    !name.trim().is_empty() && name.chars().count() <= MAX_NICKNAME_LEN
}
/// Replace all recovery codes for `user_id` with a freshly generated set;
/// returns the plaintext codes (shown once).
pub async fn fresh_recovery_codes(db: &Database, user_id: &str) -> Result<Vec<String>, AppError> {
    let codes = recovery::generate();
    let pairs: Vec<(String, String)> = codes
        .iter()
        .map(|c| (Uuid::new_v4().to_string(), recovery::hash(c)))
        .collect();
    db.recovery_replace(user_id, &pairs).await.map_err(db_err)?;
    Ok(codes)
}
/// A verify-kind factor leg succeeded. When the challenge still owes the
/// password step (identifier-first login) the challenge stays open and the
/// client advances to it; otherwise the challenge is consumed and the
/// session minted.
pub async fn finish_factor(
    ctx: &mut RpcCtx,
    id: &str,
    method_detail: &str,
) -> Result<TwoFactorCompletePublic, AppError> {
    let needs_password = ctx
        .two_factor
        .store
        .update(id, |p| {
            p.needs_factor = false;
            p.needs_password
        })
        .ok_or_else(|| expired("two-factor challenge expired — sign in again"))?;
    if needs_password {
        return Ok(TwoFactorCompletePublic {
            next: "password".into(),
            user: None,
        });
    }
    let Some(pending) = ctx.two_factor.store.take(id) else {
        return Err(expired("two-factor challenge expired — sign in again"));
    };
    let user = mint_session(ctx, pending, method_detail).await?;
    Ok(TwoFactorCompletePublic {
        next: "done".into(),
        user: Some(user),
    })
}
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
