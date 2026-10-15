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
use oxidean_core::{
    AdminResetTwoFactorRequest, AppError, PasskeyCompleteRequest, PasskeyDeleteRequest,
    PasskeyPublic, PasskeyRegisterCompleteRequest, PasskeyUpdateRequest, RecoveryCodesPublic,
    TotpEnrollmentPublic, TwoFactorChallengeRequest, TwoFactorCompletePublic,
    TwoFactorCompleteRequest, TwoFactorStatusPublic, UserPublic,
};
use oxidean_db::{Database, UserRow};
use url::Url;
use uuid::Uuid;
use webauthn_rs::prelude::*;

use crate::audit;
use crate::auth::local::{issue_session, user_to_public};
use crate::auth::session::sha256_hex;
use crate::email::OutboundEmail;
use crate::rpc::{CookieChange, RpcCtx};

pub use flags::LoginChannel;
pub use store::{OpenChallenge, PendingKind, TwoFactorStore};

/// Second-factor state cookie — HttpOnly, scoped to the challenge TTL.
pub const TWO_FACTOR_COOKIE_NAME: &str = "oxidean_2fa";
const OTP_EMAIL_TTL_MINS: u64 = 30;
/// OTP e-mail resend cap per challenge.
const MAX_EMAIL_SENDS: u32 = 5;
/// Email-token redeem attempts cap (mirrors `verify_reset::MAX_REDEEM_ATTEMPTS`).
const MAX_OTP_ATTEMPTS: i32 = 10;
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

fn db_err(e: String) -> AppError {
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

fn invalid(msg: &str) -> AppError {
    AppError::new("auth.two_factor_invalid", msg)
}

pub(crate) fn expired(msg: &str) -> AppError {
    AppError::new("auth.two_factor_expired", msg)
}

fn bad_input(e: serde_json::Error) -> AppError {
    AppError::new("rpc.bad_input", format!("invalid two-factor input: {e}"))
}

fn encrypt_str(plaintext: &str) -> Result<String, AppError> {
    crate::crypto::encrypt(TOTP_KEY_ENVS, plaintext).map_err(|e| {
        tracing::error!("totp secret encrypt failed: {e}");
        AppError::new("internal.error", "secret encryption unavailable")
    })
}

fn decrypt_str(blob: &str) -> Result<String, AppError> {
    crate::crypto::decrypt(TOTP_KEY_ENVS, blob).map_err(|e| {
        tracing::error!("totp secret decrypt failed: {e}");
        AppError::new("internal.error", "stored secret is unreadable")
    })
}

// ---------------------------------------------------------------------------
// Cookie helpers
// ---------------------------------------------------------------------------

fn challenge_cookie(env_name: &str, id: &str) -> cookie::Cookie<'static> {
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

// ---------------------------------------------------------------------------
// Challenge plumbing — shared by `auth.login` and the SSO callbacks.
// ---------------------------------------------------------------------------

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
fn challenge_id(ctx: &RpcCtx, explicit: Option<&str>) -> Option<String> {
    explicit
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| ctx.two_factor.challenge_id.clone())
}

/// Load live challenge metadata; `auth.two_factor_expired` when stale/unknown.
fn pending_or_err(ctx: &RpcCtx, id: &str) -> Result<store::PendingInfo, AppError> {
    ctx.two_factor
        .store
        .peek(id)
        .ok_or_else(|| expired("two-factor challenge expired — sign in again"))
}

/// Count a failed attempt; drop the challenge when the cap is hit.
fn bump_attempts(ctx: &RpcCtx, id: &str) -> Result<(), AppError> {
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
async fn enroll_actor(
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
async fn mint_session(
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
pub(crate) async fn password_step(
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

// ---------------------------------------------------------------------------
// WebAuthn helpers
// ---------------------------------------------------------------------------

fn make_webauthn() -> Result<Webauthn, AppError> {
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
fn cred_id_str(id: &CredentialID) -> Result<String, AppError> {
    serde_json::to_value(id)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .ok_or_else(|| AppError::new("internal.error", "credential id encoding failed"))
}

async fn load_passkeys(
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

fn nickname_ok(name: &str) -> bool {
    !name.trim().is_empty() && name.chars().count() <= MAX_NICKNAME_LEN
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

/// `auth.two_factor.status` — factor inventory for the signed-in user, or
/// challenge metadata for a pending one.
pub async fn status(ctx: &mut RpcCtx) -> Result<TwoFactorStatusPublic, AppError> {
    let db = &ctx.db;

    if let Some(id) = challenge_id(ctx, None) {
        if let Ok(pending) = pending_or_err(ctx, &id) {
            return Ok(TwoFactorStatusPublic {
                totp_enabled: false,
                recovery_codes_remaining: 0,
                passkeys: vec![],
                challenge_pending: true,
                challenge_kind: Some(pending.kind().as_str().to_string()),
                challenge_methods: pending.methods,
                challenge_next: Some(
                    if pending.needs_factor {
                        "factor"
                    } else if pending.needs_password {
                        "password"
                    } else {
                        "enroll"
                    }
                    .to_string(),
                ),
                enroll_required: pending.needs_enroll,
            });
        }
    }

    let user = crate::auth::gate::require_verified(ctx).await?;
    let totp = db.totp_find(&user.id).await.map_err(db_err)?;
    let passkeys = db.passkey_list_for_user(&user.id).await.map_err(db_err)?;
    let remaining = db
        .recovery_count_remaining(&user.id)
        .await
        .map_err(db_err)?;
    let totp_enabled = totp.as_ref().map(|t| t.is_confirmed()).unwrap_or(false);
    let enrolled = totp_enabled || !passkeys.is_empty();
    let enroll_required = if enrolled {
        false
    } else {
        let channel = if user.password_hash.is_some() {
            LoginChannel::Local
        } else {
            LoginChannel::External
        };
        flags::login_gate(db, &user, channel)
            .await
            .map(|g| g.enroll_required)
            .unwrap_or(false)
    };
    Ok(TwoFactorStatusPublic {
        totp_enabled,
        recovery_codes_remaining: remaining,
        passkeys: passkeys
            .into_iter()
            .map(|r| PasskeyPublic {
                id: r.id,
                nickname: r.nickname,
                created_at: r.created_at,
                last_used_at: r.last_used_at,
            })
            .collect(),
        challenge_pending: false,
        challenge_kind: None,
        challenge_methods: vec![],
        challenge_next: None,
        enroll_required,
    })
}

// ---------------------------------------------------------------------------
// TOTP enrollment
// ---------------------------------------------------------------------------

/// `auth.two_factor.totp_begin` — create a pending secret; returns the
/// `otpauth://` URI + base32 manual key. Never returns a confirmed secret.
pub async fn totp_begin(
    ctx: &mut RpcCtx,
    input: serde_json::Value,
) -> Result<TotpEnrollmentPublic, AppError> {
    let db = &ctx.db;
    let req: TwoFactorChallengeRequest = if input.is_null() {
        TwoFactorChallengeRequest { challenge: None }
    } else {
        serde_json::from_value(input).map_err(bad_input)?
    };
    let (user_id, _) = enroll_actor(ctx, req.challenge.as_deref()).await?;
    if let Some(t) = db.totp_find(&user_id).await.map_err(db_err)? {
        if t.is_confirmed() {
            return Err(AppError::new(
                "auth.two_factor_exists",
                "an authenticator app is already enrolled",
            ));
        }
    }
    let secret = totp::generate_secret_b32();
    let encrypted = encrypt_str(&secret)?;
    db.totp_upsert(&user_id, &encrypted).await.map_err(db_err)?;
    let user = db
        .find_user_by_id(&user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.internal", "authentication failed"))?;
    let host = Url::parse(&crate::public_origin::resolve_public_origin())
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_else(|| "localhost".to_string());
    let issuer = format!("Oxidean {host}");
    Ok(TotpEnrollmentPublic {
        secret: secret.clone(),
        otpauth_uri: totp::otpauth_uri(&issuer, &user.username, &secret),
    })
}

/// Replace all recovery codes for `user_id` with a freshly generated set;
/// returns the plaintext codes (shown once).
async fn fresh_recovery_codes(db: &Database, user_id: &str) -> Result<Vec<String>, AppError> {
    let codes = recovery::generate();
    let pairs: Vec<(String, String)> = codes
        .iter()
        .map(|c| (Uuid::new_v4().to_string(), recovery::hash(c)))
        .collect();
    db.recovery_replace(user_id, &pairs).await.map_err(db_err)?;
    Ok(codes)
}

/// `auth.two_factor.totp_confirm` — verify the first code against the pending
/// secret, confirm it, and mint the recovery-code set. Under an enroll-kind
/// challenge this also completes the login (session minted).
pub async fn totp_confirm(
    ctx: &mut RpcCtx,
    input: serde_json::Value,
) -> Result<RecoveryCodesPublic, AppError> {
    let req: TwoFactorCompleteRequest = serde_json::from_value(input).map_err(bad_input)?;
    let (user_id, challenge) = enroll_actor(ctx, req.challenge.as_deref()).await?;
    let db = &ctx.db;
    let secret_enc = db
        .totp_find(&user_id)
        .await
        .map_err(db_err)?
        .filter(|t| !t.is_confirmed())
        .map(|t| t.secret_enc)
        .ok_or_else(|| {
            AppError::new(
                "auth.two_factor_pending_secret",
                "no authenticator enrollment in progress",
            )
        })?;
    let secret = decrypt_str(&secret_enc)?;
    let now = now_unix() as u64;
    if !totp::verify_code(&secret, &req.code, now) {
        if let Some(id) = &challenge {
            bump_attempts(ctx, id)?;
        }
        return Err(invalid("that code did not match — try again"));
    }
    db.totp_confirm(&user_id).await.map_err(db_err)?;
    let recovery_codes = fresh_recovery_codes(db, &user_id).await?;
    let username = db
        .find_user_by_id(&user_id)
        .await
        .ok()
        .flatten()
        .map(|u| u.username)
        .unwrap_or_default();
    audit::record(
        ctx,
        Some((&user_id, &username)),
        "auth.two_factor_enroll",
        Some(("user", &user_id)),
        Some("{\"method\":\"totp\"}".to_string()),
    )
    .await;
    if let Some(id) = challenge {
        let pending = ctx
            .two_factor
            .store
            .take(&id)
            .ok_or_else(|| expired("two-factor challenge expired — sign in again"))?;
        mint_session(ctx, pending, "totp_enroll").await?;
    }
    Ok(RecoveryCodesPublic { recovery_codes })
}

/// `auth.two_factor.totp_disable` — remove the authenticator app factor
/// (signed-in users only; recovery codes and passkeys are untouched).
pub async fn totp_disable(ctx: &mut RpcCtx) -> Result<serde_json::Value, AppError> {
    let user = crate::auth::gate::require_verified(ctx).await?;
    ctx.db.totp_delete(&user.id).await.map_err(db_err)?;
    audit::record(
        ctx,
        Some((&user.id, &user.username)),
        "auth.two_factor_disable",
        Some(("user", &user.id)),
        Some("{\"method\":\"totp\"}".to_string()),
    )
    .await;
    Ok(serde_json::json!({"ok": true}))
}

// ---------------------------------------------------------------------------
// Recovery codes
// ---------------------------------------------------------------------------

/// `auth.two_factor.recovery_regenerate` — burn all existing codes and mint a
/// new set. Requires an existing factor (signed-in only).
pub async fn recovery_regenerate(ctx: &mut RpcCtx) -> Result<RecoveryCodesPublic, AppError> {
    let user = crate::auth::gate::require_verified(ctx).await?;
    if !ctx
        .db
        .user_has_confirmed_factor(&user.id)
        .await
        .map_err(db_err)?
    {
        return Err(AppError::new(
            "auth.two_factor_no_factor",
            "enroll an authenticator app or passkey first",
        ));
    }
    let recovery_codes = fresh_recovery_codes(&ctx.db, &user.id).await?;
    audit::record(
        ctx,
        Some((&user.id, &user.username)),
        "auth.two_factor_recovery_regen",
        Some(("user", &user.id)),
        None,
    )
    .await;
    Ok(RecoveryCodesPublic { recovery_codes })
}

// ---------------------------------------------------------------------------
// Challenge completion: TOTP / email OTP / recovery code
// ---------------------------------------------------------------------------

/// A verify-kind factor leg succeeded. When the challenge still owes the
/// password step (identifier-first login) the challenge stays open and the
/// client advances to it; otherwise the challenge is consumed and the
/// session minted.
async fn finish_factor(
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

/// `auth.two_factor.complete` — satisfy the factor leg of a verify challenge
/// with a `code`: 6 digits → TOTP, 8 digits → email OTP, otherwise a
/// recovery code. Returns `next: "password"` when the identifier-first flow
/// still owes the password step, `"done"` + session when it does not.
pub async fn complete(
    ctx: &mut RpcCtx,
    input: serde_json::Value,
) -> Result<TwoFactorCompletePublic, AppError> {
    let req: TwoFactorCompleteRequest = serde_json::from_value(input).map_err(bad_input)?;
    let id = challenge_id(ctx, req.challenge.as_deref()).ok_or_else(|| {
        AppError::new(
            "auth.unauthenticated",
            "no two-factor challenge in progress",
        )
    })?;
    let pending = pending_or_err(ctx, &id)?;
    if !pending.needs_factor {
        return Err(AppError::new(
            "auth.two_factor_unavailable",
            "this challenge is not waiting for a verification code",
        ));
    }
    let digits: String = req.code.chars().filter(|c| !c.is_whitespace()).collect();
    let has = |m: &str| pending.methods.iter().any(|x| x == m);

    // --- TOTP (6 digits) ---
    if digits.len() == 6 && digits.chars().all(|c| c.is_ascii_digit()) {
        if !has("totp") {
            return Err(invalid("authenticator app is not enrolled"));
        }
        let secret_enc = ctx
            .db
            .totp_find(&pending.user_id)
            .await
            .map_err(db_err)?
            .filter(|t| t.is_confirmed())
            .map(|t| t.secret_enc)
            .ok_or_else(|| invalid("authenticator app is not enrolled"))?;
        let secret = decrypt_str(&secret_enc)?;
        if !totp::verify_code(&secret, &digits, now_unix() as u64) {
            bump_attempts(ctx, &id)?;
            return Err(invalid("that code did not match — try again"));
        }
        let _ = ctx.db.totp_touch_last_used(&pending.user_id).await;
        return finish_factor(ctx, &id, "totp").await;
    }

    // --- Email OTP (8 digits) ---
    if digits.len() == 8 && digits.chars().all(|c| c.is_ascii_digit()) {
        if !has("email") {
            return Err(invalid("email sign-in codes are not available"));
        }
        let user = ctx
            .db
            .find_user_by_id(&pending.user_id)
            .await
            .map_err(db_err)?
            .ok_or_else(|| AppError::new("auth.internal", "authentication failed"))?;
        let row = ctx
            .db
            .find_email_token_by_user_purpose_target(
                &pending.user_id,
                crate::auth::verify_reset::PURPOSE_LOGIN_2FA,
                &user.email,
            )
            .await
            .map_err(db_err)?
            .ok_or_else(|| expired("no sign-in code was sent — request one and try again"))?;
        let expiry = chrono::DateTime::parse_from_rfc3339(&row.expires_at)
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());
        let ok = expiry > chrono::Utc::now()
            && row.attempt_count < MAX_OTP_ATTEMPTS
            && row.otp_hash == sha256_hex(digits.as_bytes());
        if !ok {
            let _ = ctx.db.increment_email_token_attempts(&row.id).await;
            bump_attempts(ctx, &id)?;
            return Err(invalid("that sign-in code is wrong or expired"));
        }
        ctx.db.delete_email_token(&row.id).await.map_err(db_err)?;
        return finish_factor(ctx, &id, "email").await;
    }

    // --- Recovery code ---
    if recovery::looks_like_recovery_code(&req.code) {
        if !has("recovery") {
            return Err(invalid("recovery codes are not available"));
        }
        if !ctx
            .db
            .recovery_consume(&pending.user_id, &recovery::hash(&req.code))
            .await
            .map_err(db_err)?
        {
            bump_attempts(ctx, &id)?;
            return Err(invalid("that recovery code is wrong or already used"));
        }
        return finish_factor(ctx, &id, "recovery").await;
    }

    Err(invalid(
        "enter a 6-digit app code, an 8-digit email code, or a recovery code",
    ))
}

// ---------------------------------------------------------------------------
// Passkey sign-in (challenge) and registration (session/enroll-challenge)
// ---------------------------------------------------------------------------

/// `auth.two_factor.passkey_begin` — start a WebAuthn assertion against the
/// user's stored passkeys; returns `{publicKey}` for `startAuthentication`.
pub async fn passkey_begin(
    ctx: &mut RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let req: TwoFactorChallengeRequest = serde_json::from_value(input).map_err(bad_input)?;
    let id = challenge_id(ctx, req.challenge.as_deref()).ok_or_else(|| {
        AppError::new(
            "auth.unauthenticated",
            "no two-factor challenge in progress",
        )
    })?;
    let pending = pending_or_err(ctx, &id)?;
    if !pending.needs_factor {
        return Err(AppError::new(
            "auth.two_factor_unavailable",
            "this challenge is not waiting for a verification",
        ));
    }
    if !pending.methods.iter().any(|m| m == "passkey") {
        return Err(invalid("no passkey is enrolled on this account"));
    }
    let passkeys: Vec<Passkey> = load_passkeys(&ctx.db, &pending.user_id)
        .await?
        .into_iter()
        .map(|(_, p, _)| p)
        .collect();
    let (rcr, state) = make_webauthn()?
        .start_passkey_authentication(&passkeys)
        .map_err(|e| {
            tracing::error!("webauthn assertion start failed: {e:?}");
            AppError::new("internal.error", "passkey challenge failed")
        })?;
    let stashed = ctx
        .two_factor
        .store
        .update(&id, |p| {
            p.webauthn_assertion = Some(state);
        })
        .is_some();
    if !stashed {
        return Err(expired("two-factor challenge expired — sign in again"));
    }
    serde_json::to_value(&rcr)
        .map_err(|_| AppError::new("internal.error", "passkey challenge encode failed"))
}

/// `auth.two_factor.passkey_complete` — finish a WebAuthn assertion; mints the
/// session.
pub async fn passkey_complete(
    ctx: &mut RpcCtx,
    input: serde_json::Value,
) -> Result<UserPublic, AppError> {
    let req: PasskeyCompleteRequest = serde_json::from_value(input).map_err(bad_input)?;
    let id = challenge_id(ctx, req.challenge.as_deref()).ok_or_else(|| {
        AppError::new(
            "auth.unauthenticated",
            "no two-factor challenge in progress",
        )
    })?;
    let pending = pending_or_err(ctx, &id)?;
    if !pending.needs_factor {
        return Err(AppError::new(
            "auth.two_factor_unavailable",
            "this challenge is not waiting for a verification",
        ));
    }
    let assertion: PublicKeyCredential =
        serde_json::from_value(req.assertion).map_err(|_| invalid("malformed passkey response"))?;
    let state = ctx
        .two_factor
        .store
        .update(&id, |p| p.webauthn_assertion.take())
        .flatten()
        .ok_or_else(|| expired("no passkey ceremony in progress — request a new challenge"))?;
    let result = match make_webauthn()?.finish_passkey_authentication(&assertion, &state) {
        Ok(r) => r,
        Err(e) => {
            tracing::info!("passkey assertion rejected: {e:?}");
            bump_attempts(ctx, &id)?;
            return Err(invalid("passkey verification failed"));
        }
    };
    if !result.user_verified() {
        // A passkey assertion without UV is treated as a failed factor.
        bump_attempts(ctx, &id)?;
        return Err(invalid("passkey did not verify the user"));
    }
    // Update counter/backup flags + last_used on the matching credential.
    let cred_id = cred_id_str(result.cred_id())?;
    let mut passkeys = load_passkeys(&ctx.db, &pending.user_id).await?;
    if let Some((row_id, pk, _)) = passkeys.iter_mut().find(|(_, p, _)| {
        cred_id_str(p.cred_id())
            .map(|s| s == cred_id)
            .unwrap_or(false)
    }) {
        pk.update_credential(&result);
        let json = serde_json::to_string(pk)
            .map_err(|_| AppError::new("internal.error", "passkey encode failed"))?;
        ctx.db
            .passkey_after_assertion(row_id, result.counter() as i64, &json)
            .await
            .map_err(db_err)?;
    }
    // A user-verified passkey assertion is multi-factor in one gesture — it
    // clears the password step too (identifier-first flow, #130).
    ctx.two_factor.store.update(&id, |p| {
        p.needs_factor = false;
        p.needs_password = false;
    });
    let Some(pending) = ctx.two_factor.store.take(&id) else {
        return Err(expired("two-factor challenge expired — sign in again"));
    };
    mint_session(ctx, pending, "passkey").await
}

/// `auth.two_factor.passkey_register_begin` — start `credentials.create`;
/// returns `{publicKey}` options. Signed-in or enroll-challenge.
pub async fn passkey_register_begin(
    ctx: &mut RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let req: TwoFactorChallengeRequest = serde_json::from_value(input).map_err(bad_input)?;
    let (user_id, challenge) = enroll_actor(ctx, req.challenge.as_deref()).await?;
    let user = ctx
        .db
        .find_user_by_id(&user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.internal", "authentication failed"))?;
    let uuid = Uuid::parse_str(&user.id)
        .map_err(|_| AppError::new("internal.error", "user id is not a uuid"))?;
    let existing = load_passkeys(&ctx.db, &user_id).await?;
    let exclude: Vec<CredentialID> = existing
        .iter()
        .map(|(_, p, _)| p.cred_id().clone())
        .collect();
    let (ccr, state) = make_webauthn()?
        .start_passkey_registration(uuid, &user.username, &user.display_name, Some(exclude))
        .map_err(|e| {
            tracing::error!("webauthn registration start failed: {e:?}");
            AppError::new("internal.error", "passkey registration failed")
        })?;
    if let Some(id) = &challenge {
        let ok = ctx
            .two_factor
            .store
            .update(id, |p| {
                p.passkey_registration = Some(state);
            })
            .is_some();
        if !ok {
            return Err(expired("two-factor challenge expired — sign in again"));
        }
    } else {
        ctx.two_factor.store.stash_session_passkey(&user_id, state);
    }
    serde_json::to_value(&ccr)
        .map_err(|_| AppError::new("internal.error", "passkey challenge encode failed"))
}

/// `auth.two_factor.passkey_register_complete` — persist the new passkey.
/// Under an enroll-kind challenge this completes login (session minted).
pub async fn passkey_register_complete(
    ctx: &mut RpcCtx,
    input: serde_json::Value,
) -> Result<PasskeyPublic, AppError> {
    let req: PasskeyRegisterCompleteRequest = serde_json::from_value(input).map_err(bad_input)?;
    let (user_id, challenge) = enroll_actor(ctx, req.challenge.as_deref()).await?;
    let nickname = {
        let n = req.nickname.trim();
        if n.is_empty() {
            "Passkey".to_string()
        } else {
            n.to_string()
        }
    };
    if !nickname_ok(&nickname) {
        return Err(AppError::new(
            "validation.failed",
            "nickname must be 1-64 characters",
        ));
    }
    let attestation: RegisterPublicKeyCredential = serde_json::from_value(req.attestation)
        .map_err(|_| invalid("malformed passkey response"))?;
    let state = match &challenge {
        Some(id) => ctx
            .two_factor
            .store
            .update(id, |p| p.passkey_registration.take())
            .flatten()
            .ok_or_else(|| expired("no passkey ceremony in progress — request a new challenge"))?,
        None => ctx
            .two_factor
            .store
            .take_session_passkey(&user_id)
            .ok_or_else(|| expired("passkey enrollment expired — start over"))?,
    };
    let passkey = make_webauthn()?
        .finish_passkey_registration(&attestation, &state)
        .map_err(|e| {
            tracing::info!("passkey registration rejected: {e:?}");
            invalid("passkey registration failed")
        })?;
    let cred_id = cred_id_str(passkey.cred_id())?;
    let json = serde_json::to_string(&passkey)
        .map_err(|_| AppError::new("internal.error", "passkey encode failed"))?;
    let row_id = Uuid::new_v4().to_string();
    ctx.db
        .passkey_insert(&row_id, &user_id, &cred_id, &nickname, &json, "", 0)
        .await
        .map_err(db_err)?;
    let username = ctx
        .db
        .find_user_by_id(&user_id)
        .await
        .ok()
        .flatten()
        .map(|u| u.username)
        .unwrap_or_default();
    audit::record(
        ctx,
        Some((&user_id, &username)),
        "auth.two_factor_passkey_register",
        Some(("user", &user_id)),
        None,
    )
    .await;
    if let Some(id) = challenge {
        let pending = ctx
            .two_factor
            .store
            .take(&id)
            .ok_or_else(|| expired("two-factor challenge expired — sign in again"))?;
        mint_session(ctx, pending, "passkey_enroll").await?;
    }
    let row = ctx
        .db
        .passkey_find_for_user(&user_id, &row_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("internal.error", "passkey persistence failed"))?;
    Ok(PasskeyPublic {
        id: row.id,
        nickname: row.nickname,
        created_at: row.created_at,
        last_used_at: row.last_used_at,
    })
}

/// `auth.two_factor.passkey_update` — rename a passkey (owner only).
pub async fn passkey_update(
    ctx: &mut RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let req: PasskeyUpdateRequest = serde_json::from_value(input).map_err(bad_input)?;
    let user = crate::auth::gate::require_verified(ctx).await?;
    if !nickname_ok(&req.nickname) {
        return Err(AppError::new(
            "validation.failed",
            "nickname must be 1-64 characters",
        ));
    }
    if !ctx
        .db
        .passkey_update_nickname(&user.id, &req.id, req.nickname.trim())
        .await
        .map_err(db_err)?
    {
        return Err(AppError::new("not_found", "passkey not found"));
    }
    Ok(serde_json::json!({"ok": true}))
}

/// `auth.two_factor.passkey_delete` — remove a passkey (owner only).
pub async fn passkey_delete(
    ctx: &mut RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let req: PasskeyDeleteRequest = serde_json::from_value(input).map_err(bad_input)?;
    let user = crate::auth::gate::require_verified(ctx).await?;
    if !ctx
        .db
        .passkey_delete(&user.id, &req.id)
        .await
        .map_err(db_err)?
    {
        return Err(AppError::new("not_found", "passkey not found"));
    }
    audit::record(
        ctx,
        Some((&user.id, &user.username)),
        "auth.two_factor_passkey_delete",
        Some(("user", &user.id)),
        None,
    )
    .await;
    Ok(serde_json::json!({"ok": true}))
}

// ---------------------------------------------------------------------------
// Email OTP
// ---------------------------------------------------------------------------

/// `auth.two_factor.email_send` — issue an 8-digit sign-in OTP to the pending
/// user's email. Challenge-scoped and resend-limited.
pub async fn email_send(
    ctx: &mut RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let req: TwoFactorChallengeRequest = serde_json::from_value(input).map_err(bad_input)?;
    let id = challenge_id(ctx, req.challenge.as_deref()).ok_or_else(|| {
        AppError::new(
            "auth.unauthenticated",
            "no two-factor challenge in progress",
        )
    })?;
    let pending = pending_or_err(ctx, &id)?;
    if !pending.needs_factor {
        return Err(AppError::new(
            "auth.two_factor_unavailable",
            "this challenge is not waiting for a verification",
        ));
    }
    if !pending.methods.iter().any(|m| m == "email") {
        return Err(invalid("email sign-in codes are not available"));
    }
    let sends = ctx
        .two_factor
        .store
        .update(&id, |p| {
            p.email_sends += 1;
            p.email_sends
        })
        .unwrap_or(0);
    if sends == 0 {
        return Err(expired("two-factor challenge expired — sign in again"));
    }
    if sends > MAX_EMAIL_SENDS {
        return Err(AppError::new(
            "auth.rate_limited",
            "too many sign-in codes requested — try again later",
        ));
    }
    let user = ctx
        .db
        .find_user_by_id(&pending.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.internal", "authentication failed"))?;
    let secrets =
        crate::auth::verify_reset::issue_2fa_token(&ctx.db, &user.id, &user.email).await?;
    let text = format!(
        "Hi {name},\n\nYour Oxidean sign-in code is: {otp}\n\nIt expires in \
         {mins} minutes. If you did not try to sign in, you can ignore this email.\n",
        name = user.display_name,
        otp = secrets.otp,
        mins = OTP_EMAIL_TTL_MINS,
    );
    let msg = OutboundEmail {
        to: user.email.clone(),
        subject: "Oxidean sign-in code".into(),
        text,
        html: None,
    };
    ctx.email.send(msg).await.map_err(|e| {
        tracing::error!("two-factor email send failed: {e}");
        AppError::new("email.send_failed", "failed to send email")
    })?;
    Ok(serde_json::json!({"ok": true}))
}

// ---------------------------------------------------------------------------
// Admin
// ---------------------------------------------------------------------------

/// `admin.users.resetTwoFactor` — system admin clears every factor + recovery
/// code for a user (account-recovery path).
pub async fn admin_reset(
    ctx: &mut RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let req: AdminResetTwoFactorRequest = serde_json::from_value(input).map_err(bad_input)?;
    crate::auth::admin::require_admin(ctx).await?;
    let target = ctx
        .db
        .find_user_by_id(&req.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("not_found", "user not found"))?;
    ctx.db.totp_delete(&target.id).await.map_err(db_err)?;
    ctx.db
        .passkey_delete_all_for_user(&target.id)
        .await
        .map_err(db_err)?;
    ctx.db
        .recovery_delete_all(&target.id)
        .await
        .map_err(db_err)?;
    let actor_id = ctx
        .session
        .as_ref()
        .map(|s| s.user_id.clone())
        .unwrap_or_default();
    let actor_name = ctx
        .db
        .find_user_by_id(&actor_id)
        .await
        .ok()
        .flatten()
        .map(|u| u.username)
        .unwrap_or_default();
    audit::record(
        ctx,
        Some((&actor_id, &actor_name)),
        "auth.two_factor_admin_reset",
        Some(("user", &target.id)),
        None,
    )
    .await;
    Ok(serde_json::json!({"ok": true}))
}

// ---------------------------------------------------------------------------

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
