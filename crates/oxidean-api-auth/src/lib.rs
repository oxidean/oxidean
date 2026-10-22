//! `oxidean-api-auth` — authentication RPC handlers (`auth.*`, plus
//! `admin.users.resetTwoFactor` which is a two_factor operation) extracted
//! from the monolithic `oxidean-api` per issue #162. Auth *machinery*
//! (session store, challenge store, password hashing, OAuth providers,
//! `issue_*` token plumbing, `require_admin`) stays in `oxidean-api-core` —
//! every entry point and several domains consume it.

mod bootstrap;
mod dispatch;
mod local;
mod two_factor;
mod verify_reset;

pub use dispatch::dispatch;

/// Procedures this crate owns — aggregated by the shell's `PROCEDURES`.
pub const PROCEDURES: &[&str] = &[
    "admin.users.resetTwoFactor",
    "auth.bootstrap_setup",
    "auth.bootstrap_status",
    "auth.confirm_admin_credentials",
    "auth.dev.privileged_ping",
    "auth.login",
    "auth.login_begin",
    "auth.logout",
    "auth.logout_all",
    "auth.me",
    "auth.provider_config",
    "auth.request_password_reset",
    "auth.request_verify",
    "auth.resend_verify",
    "auth.reset_password",
    "auth.signup",
    "auth.two_factor.complete",
    "auth.two_factor.email_send",
    "auth.two_factor.passkey_begin",
    "auth.two_factor.passkey_complete",
    "auth.two_factor.passkey_delete",
    "auth.two_factor.passkey_register_begin",
    "auth.two_factor.passkey_register_complete",
    "auth.two_factor.passkey_update",
    "auth.two_factor.recovery_regenerate",
    "auth.two_factor.status",
    "auth.two_factor.totp_begin",
    "auth.two_factor.totp_confirm",
    "auth.two_factor.totp_disable",
    "auth.verify",
];
