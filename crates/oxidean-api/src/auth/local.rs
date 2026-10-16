//! Local email/password signup and login (D-01, D-02, D-10, D-20).

use cookie::Cookie;
use oxidean_core::{
    is_reserved_username, validate_username, AppError, LoginBeginRequest, LoginBeginResponse,
    LoginRequest, ProviderMode, Role, SignupRequest, UserPublic,
};
use oxidean_db::{Database, UserRow};
use uuid::Uuid;

use crate::auth::external::is_placeholder_username;
use crate::auth::password::{hash_password_str, verify_password, PasswordError, MIN_PASSWORD_LEN};
use crate::auth::session::clear_session_cookie;
use crate::auth::verify_reset;
use crate::email::OutboundEmail;
use crate::rpc::{CookieChange, RpcCtx};

/// Convert a DB user row to the public RPC DTO (never includes password_hash).
///
/// `two_factor_*` flags reflect current enrollment + policy (#130): a session
/// user's required-state is computed from the org/instance policy plus whether
/// the account has a local password (SSO-only accounts follow the external
/// policy). Errors fail closed to `false` so a blip never claims "enrolled".
pub async fn user_to_public(db: &Database, row: &UserRow) -> UserPublic {
    let (two_factor_enabled, two_factor_enroll_required) =
        crate::auth::two_factor::flags::for_user(db, row).await;
    UserPublic {
        id: row.id.clone(),
        email: row.email.clone(),
        username: row.username.clone(),
        display_name: row.display_name.clone(),
        bio: row.bio.clone(),
        avatar_url: row.avatar_path.clone(),
        role: row.role,
        profile_incomplete: is_placeholder_username(&row.username),
        email_verified: row.email_verified_at.is_some(),
        must_change_credentials: row.must_change_credentials,
        default_branch: if row.default_branch.trim().is_empty() {
            "main".into()
        } else {
            row.default_branch.clone()
        },
        two_factor_enabled,
        two_factor_enroll_required,
    }
}

fn require_local(mode: ProviderMode) -> Result<(), AppError> {
    if mode != ProviderMode::Local {
        return Err(AppError::new(
            "auth.provider_mismatch",
            "local signup/login is disabled for this instance",
        ));
    }
    Ok(())
}

fn map_username_err(msg: String) -> AppError {
    if msg.contains("reserved") {
        AppError::new("auth.reserved_username", "username is reserved")
    } else {
        AppError::new("auth.invalid_username", msg)
    }
}

pub(crate) fn normalize_email(raw: &str) -> Result<String, AppError> {
    let email = raw.trim().to_ascii_lowercase();
    if email.is_empty() || !email.contains('@') || email.starts_with('@') || email.ends_with('@') {
        return Err(AppError::new("auth.invalid_email", "invalid email address"));
    }
    Ok(email)
}

/// Resolve a login `identifier` — email when it contains `@`, else username.
async fn resolve_login_identifier(
    db: &Database,
    identifier: &str,
) -> Result<Option<UserRow>, AppError> {
    if identifier.contains('@') {
        let email = identifier.to_ascii_lowercase();
        db.find_user_by_email(&email).await.map_err(db_err)
    } else {
        db.find_user_by_username(identifier).await.map_err(db_err)
    }
}

fn invalid_credentials() -> AppError {
    AppError::new(
        "auth.invalid_credentials",
        "Incorrect email/username or password",
    )
}

async fn resolve_provider_mode(ctx: &RpcCtx) -> Result<ProviderMode, AppError> {
    match ctx.db.get_auth_settings().await {
        Ok(row) => match row.provider_mode.as_str() {
            "local" => Ok(ProviderMode::Local),
            "workos" => Ok(ProviderMode::Workos),
            "oidc" => Ok(ProviderMode::Oidc),
            other => {
                tracing::warn!(mode = %other, "unknown provider_mode; treating as local");
                Ok(ProviderMode::Local)
            }
        },
        Err(e) if e == "database not configured" => Err(AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )),
        Err(e) => {
            tracing::error!("get auth settings failed: {e}");
            Err(AppError::new(
                "auth.settings_failed",
                "failed to load auth settings",
            ))
        }
    }
}

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else if e.contains("UNIQUE") || e.contains("unique") || e.contains("Duplicate") {
        AppError::new("auth.taken", "email or username already taken")
    } else {
        tracing::error!("auth db error: {e}");
        AppError::new("auth.internal", "authentication failed")
    }
}

fn session_err(e: crate::auth::session::AuthError) -> AppError {
    match e {
        crate::auth::session::AuthError::NotConfigured => AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        ),
        crate::auth::session::AuthError::Store(msg) => {
            tracing::error!("session store error: {msg}");
            AppError::new("auth.session_failed", "session operation failed")
        }
    }
}

pub(crate) async fn issue_session(
    ctx: &mut RpcCtx,
    user_id: &str,
    remember_me: bool,
) -> Result<Cookie<'static>, AppError> {
    let (_token, cookie) = ctx
        .sessions
        .create(
            &ctx.db,
            user_id,
            remember_me,
            ctx.client.ip_address.as_deref(),
            ctx.client.user_agent.as_deref(),
        )
        .await
        .map_err(session_err)?;
    Ok(cookie)
}

/// Local signup: create user, session cookie, welcome email (D-20).
pub async fn signup(ctx: &mut RpcCtx, input: serde_json::Value) -> Result<UserPublic, AppError> {
    let mode = resolve_provider_mode(ctx).await?;
    require_local(mode)?;

    // Empty instance without ENV seed must use `/setup` (AUTH-07) — never open signup.
    if crate::auth::bootstrap::needs_setup(&ctx.db).await? {
        return Err(AppError::new(
            "auth.setup_required",
            "Complete instance setup before signing up.",
        ));
    }

    // Post-bootstrap: allow_signup governs local registration (D-05/D-07); fail closed.
    let allow_signup = match ctx.db.get_auth_settings().await {
        Ok(s) => s.allow_signup,
        Err(_) => false,
    };
    if !allow_signup {
        return Err(AppError::new(
            "auth.signup_closed",
            "Sign-up is closed for this instance.",
        ));
    }

    let req: SignupRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid signup input: {e}")))?;

    let email = normalize_email(&req.email)?;
    validate_username(&req.username).map_err(map_username_err)?;
    let username = req.username.trim().to_string();

    if is_reserved_username(&username) {
        return Err(AppError::new(
            "auth.reserved_username",
            "username is reserved",
        ));
    }

    let password_hash = hash_password_str(&req.password).map_err(|e| match e {
        PasswordError::TooShort => AppError::new(
            "auth.weak_password",
            format!("password must be at least {MIN_PASSWORD_LEN} characters"),
        ),
        PasswordError::Hash(_) => {
            tracing::error!("password hash failed");
            AppError::new("auth.internal", "authentication failed")
        }
    })?;

    if ctx
        .db
        .find_user_by_email(&email)
        .await
        .map_err(db_err)?
        .is_some()
        || crate::repo::login_slug_taken(&ctx.db, &username)
            .await
            .map_err(db_err)?
    {
        return Err(AppError::new(
            "auth.taken",
            "email or username already taken",
        ));
    }

    let id = Uuid::new_v4().to_string();
    let display_name = username.clone();
    let row = ctx
        .db
        .create_user(
            &id,
            &email,
            &username,
            Some(&password_hash),
            &display_name,
            "",
            None,
            Role::User,
        )
        .await
        .map_err(db_err)?;

    crate::audit::record(
        ctx,
        Some((&row.id, &row.username)),
        "auth.signup",
        Some(("user", &row.id)),
        None,
    )
    .await;

    let welcome = OutboundEmail {
        to: email.clone(),
        subject: "Welcome to Oxidean".into(),
        text: format!(
            "Welcome to Oxidean, {username}!\n\nYour account is ready. Sign in anytime to get started.\n"
        ),
        html: None,
    };
    if let Err(e) = ctx.email.send(welcome).await {
        tracing::error!(error = %e, "welcome email failed");
    }

    // Auto-send verify email on local signup (D-23); mail/issue errors must not fail signup (T-05-08).
    if let Err(e) = verify_reset::issue_and_send_verify(ctx, &row.id, &email, &username).await {
        tracing::error!(code = %e.code, "signup verify email issue failed");
    }

    // Two-factor gate (#130): signup is a sign-in — when policy requires a
    // factor, the account gets an enroll challenge instead of a session.
    let gate = crate::auth::two_factor::flags::login_gate(
        &ctx.db,
        &row,
        crate::auth::two_factor::LoginChannel::Local,
    )
    .await?;
    if gate.has_factor || gate.enroll_required {
        return Err(crate::auth::two_factor::open_challenge(
            ctx, &gate, &row.id, false, true,
        ));
    }

    let cookie = issue_session(ctx, &row.id, false).await?;
    ctx.set_cookie = Some(CookieChange::Set(cookie));

    Ok(user_to_public(&ctx.db, &row).await)
}

/// `auth.login_begin` — identifier-first sign-in (#130): resolve the
/// email/username, open an `oxidean_2fa` challenge, and report the next step
/// (`"factor"` or `"password"`). Unknown, banned, or passwordless (SSO-only)
/// identifiers get an indistinguishable password-step response bound to a
/// dead-end challenge — the account reveals nothing.
pub async fn login_begin(
    ctx: &mut RpcCtx,
    input: serde_json::Value,
) -> Result<LoginBeginResponse, AppError> {
    let mode = resolve_provider_mode(ctx).await?;
    require_local(mode)?;

    let req: LoginBeginRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid login_begin input: {e}")))?;
    let identifier = req.identifier.trim();
    if identifier.is_empty() {
        return Err(AppError::new("rpc.bad_input", "identifier is required"));
    }

    // Each challenge resets the factor-attempt budget and can trigger OTP
    // mail — bound challenge churn per client IP and per identifier.
    {
        let ip = ctx.client.ip_address.as_deref().unwrap_or("unknown");
        let mut lim = ctx
            .login_begin_limiter
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if !lim.check_and_record(ip, &identifier.to_ascii_lowercase()) {
            return Err(AppError::new(
                "auth.rate_limited",
                "too many sign-in attempts — try again later",
            ));
        }
    }

    let user = resolve_login_identifier(&ctx.db, identifier).await?;
    let usable = user.filter(|u| u.password_hash.is_some() && u.banned_at.is_none());
    let Some(user) = usable else {
        // Phantom challenge: a dead-end password step, indistinguishable
        // from a real password-only account.
        let id = crate::auth::two_factor::open_and_cookie(
            ctx,
            crate::auth::two_factor::OpenChallenge {
                user_id: String::new(),
                remember_me: req.remember_me,
                needs_factor: false,
                needs_enroll: false,
                needs_password: true,
                methods: vec![],
            },
        );
        return Ok(LoginBeginResponse {
            challenge: id,
            next: "password".into(),
            kind: None,
            methods: vec![],
        });
    };

    let gate = crate::auth::two_factor::flags::login_gate(
        &ctx.db,
        &user,
        crate::auth::two_factor::LoginChannel::Local,
    )
    .await?;

    if gate.has_factor {
        let id = crate::auth::two_factor::open_and_cookie(
            ctx,
            crate::auth::two_factor::OpenChallenge {
                user_id: user.id.clone(),
                remember_me: req.remember_me,
                needs_factor: true,
                needs_enroll: false,
                needs_password: true,
                methods: gate.methods.clone(),
            },
        );
        return Ok(LoginBeginResponse {
            challenge: id,
            next: "factor".into(),
            kind: Some("verify".into()),
            methods: gate.methods,
        });
    }

    // Enroll-required and password-only accounts both run the password step
    // first — a factor is never enrolled pre-auth.
    let id = crate::auth::two_factor::open_and_cookie(
        ctx,
        crate::auth::two_factor::OpenChallenge {
            user_id: user.id.clone(),
            remember_me: req.remember_me,
            needs_factor: false,
            needs_enroll: gate.enroll_required,
            needs_password: true,
            methods: if gate.enroll_required {
                vec!["totp".into(), "passkey".into()]
            } else {
                vec![]
            },
        },
    );
    Ok(LoginBeginResponse {
        challenge: id,
        next: "password".into(),
        kind: gate.enroll_required.then(|| "enroll".to_string()),
        methods: vec![],
    })
}

/// Local login by email or username + password (D-02); generic invalid_credentials (T-04-11).
///
/// Two shapes: `challenge` absent runs the one-shot credential check
/// (machine clients, legacy path) and opens the post-password 2FA gate as
/// before; `challenge` set — or the `oxidean_2fa` cookie present — runs the
/// password leg of the identifier-first flow. The challenge binds the
/// account, so `identifier` may be omitted on resume; when supplied it must
/// resolve to the same user.
pub async fn login(ctx: &mut RpcCtx, input: serde_json::Value) -> Result<UserPublic, AppError> {
    let mode = resolve_provider_mode(ctx).await?;
    require_local(mode)?;

    let req: LoginRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid login input: {e}")))?;

    let identifier = req.identifier.trim();

    // Identifier-first path: an explicit challenge (or the `oxidean_2fa`
    // cookie) binds this call to the `auth.login_begin` challenge.
    let challenge = req
        .challenge
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| ctx.two_factor.challenge_id.clone());
    if let Some(id) = challenge {
        match ctx.two_factor.store.peek(&id) {
            Some(p) => {
                // The challenge binds the account: resolve it directly when
                // no identifier was sent (reload-resume), else require the
                // identifier to agree with the binding. Phantom challenges
                // carry no user and always dead-end here.
                let bound = if identifier.is_empty() {
                    if p.user_id.is_empty() {
                        None
                    } else {
                        ctx.db.find_user_by_id(&p.user_id).await.map_err(db_err)?
                    }
                } else {
                    match resolve_login_identifier(&ctx.db, identifier).await? {
                        Some(u) if u.id == p.user_id => Some(u),
                        _ => None,
                    }
                };
                match bound {
                    Some(u) => {
                        return crate::auth::two_factor::password_step(
                            ctx,
                            &id,
                            &u,
                            &req.password,
                            req.remember_me,
                        )
                        .await;
                    }
                    None => {
                        if req.challenge.is_some() {
                            // Bound to a different account (or a phantom) —
                            // generic failure, same as a bad password.
                            return Err(invalid_credentials());
                        }
                        // Cookie-only mismatch: stale challenge — ignore it.
                    }
                }
            }
            None => {
                if req.challenge.is_some() {
                    return Err(crate::auth::two_factor::expired(
                        "sign-in challenge expired — start over",
                    ));
                }
            }
        }
    }

    if identifier.is_empty() {
        return Err(invalid_credentials());
    }
    let user = resolve_login_identifier(&ctx.db, identifier).await?;

    let Some(user) = user else {
        return Err(invalid_credentials());
    };

    let Some(ref hash) = user.password_hash else {
        return Err(invalid_credentials());
    };

    if !verify_password(req.password.as_bytes(), hash) {
        return Err(invalid_credentials());
    }

    if user.banned_at.is_some() {
        // Same generic message as bad password (no ban-status leak).
        return Err(invalid_credentials());
    }

    // Two-factor gate (issue #130): a confirmed factor requires proof, and
    // instance/org policy can require factor-less users to enroll — either way
    // the session is only minted after `auth.two_factor.*` completes.
    let gate = crate::auth::two_factor::flags::login_gate(
        &ctx.db,
        &user,
        crate::auth::two_factor::LoginChannel::Local,
    )
    .await?;
    if gate.has_factor || gate.enroll_required {
        return Err(crate::auth::two_factor::open_challenge(
            ctx,
            &gate,
            &user.id,
            req.remember_me,
            true,
        ));
    }

    let cookie = issue_session(ctx, &user.id, req.remember_me).await?;
    ctx.set_cookie = Some(CookieChange::Set(cookie));
    crate::audit::record(
        ctx,
        Some((&user.id, &user.username)),
        "auth.login",
        Some(("user", &user.id)),
        None,
    )
    .await;
    Ok(user_to_public(&ctx.db, &user).await)
}

/// Log out current session only (D-13).
pub async fn logout(ctx: &mut RpcCtx) -> Result<(), AppError> {
    let Some(session) = ctx.session.clone() else {
        return Err(AppError::new("auth.unauthenticated", "not authenticated"));
    };
    ctx.sessions
        .revoke(&ctx.db, &session.session_id)
        .await
        .map_err(session_err)?;
    let user_id = session.user_id.clone();
    ctx.set_cookie = Some(CookieChange::Clear);
    ctx.session = None;
    let username = ctx
        .db
        .find_user_by_id(&user_id)
        .await
        .ok()
        .flatten()
        .map(|u| u.username)
        .unwrap_or_default();
    crate::audit::record(
        ctx,
        Some((&user_id, &username)),
        "auth.logout",
        Some(("user", &user_id)),
        None,
    )
    .await;
    Ok(())
}

/// Revoke all sessions for the current user (D-13).
pub async fn logout_all(ctx: &mut RpcCtx) -> Result<(), AppError> {
    let Some(session) = ctx.session.clone() else {
        return Err(AppError::new("auth.unauthenticated", "not authenticated"));
    };
    ctx.sessions
        .revoke_all(&ctx.db, &session.user_id)
        .await
        .map_err(session_err)?;
    ctx.set_cookie = Some(CookieChange::Clear);
    ctx.session = None;
    Ok(())
}

/// Current authenticated user, or `auth.unauthenticated`.
pub async fn me(ctx: &RpcCtx) -> Result<UserPublic, AppError> {
    let Some(session) = &ctx.session else {
        return Err(AppError::new("auth.unauthenticated", "not authenticated"));
    };
    let user = ctx
        .db
        .find_user_by_id(&session.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))?;
    if user.banned_at.is_some() {
        return Err(AppError::new(
            "auth.banned",
            "This account has been suspended.",
        ));
    }
    Ok(user_to_public(&ctx.db, &user).await)
}

/// Public provider mode for UI (D-14–D-16); no auth required.
pub async fn provider_config(ctx: &RpcCtx) -> Result<oxidean_core::ProviderConfigPublic, AppError> {
    let mode = resolve_provider_mode(ctx)
        .await
        .unwrap_or(ProviderMode::Local);
    // Fail closed: missing/error settings → allow_signup false (D-07 / T-06-01).
    let allow_signup = match ctx.db.get_auth_settings().await {
        Ok(s) => s.allow_signup,
        Err(_) => false,
    };
    Ok(oxidean_core::ProviderConfigPublic { mode, allow_signup })
}

/// Clear-cookie helper for HTTP layer when CookieChange::Clear is set.
pub fn clear_cookie_for_env(env_name: &str) -> Cookie<'static> {
    clear_session_cookie(env_name)
}
