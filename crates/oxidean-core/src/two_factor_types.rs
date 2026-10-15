//! Two-factor auth RPC types (#130) — enrollment, challenges, and policy flags.

use serde::{Deserialize, Serialize};

/// One registered WebAuthn/passkey credential in status responses.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PasskeyPublic {
    pub id: String,
    pub nickname: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

/// `auth.two_factor.status` — signed-in view of the caller's factors.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TwoFactorStatusPublic {
    /// Confirmed TOTP authenticator enrolled.
    pub totp_enabled: bool,
    /// Unused recovery codes remaining (0 when TOTP disabled).
    pub recovery_codes_remaining: i64,
    pub passkeys: Vec<PasskeyPublic>,
    /// A pre-session 2FA challenge exists for this request (cookie or input id).
    pub challenge_pending: bool,
    /// `"verify"` (prove a factor) or `"enroll"` (must add one) when pending.
    pub challenge_kind: Option<String>,
    /// Methods the pending challenge accepts (`totp`, `passkey`, `email`, `recovery`).
    pub challenge_methods: Vec<String>,
    /// Instance/org policy requires this user to enroll a factor.
    pub enroll_required: bool,
}

/// `auth.two_factor.totp_begin` — new pending TOTP secret for QR/manual entry.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TotpEnrollmentPublic {
    /// Base32 secret for manual authenticator entry (grouped for readability).
    pub secret: String,
    /// `otpauth://totp/...` URI for QR rendering.
    pub otpauth_uri: String,
}

/// `auth.two_factor.totp_confirm` / `recovery_regenerate` — plaintext codes,
/// returned exactly once; only hashes persist server-side.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RecoveryCodesPublic {
    pub recovery_codes: Vec<String>,
}

/// `auth.two_factor.complete` input — TOTP (6 digits), email OTP (8 digits),
/// or a recovery code (`xxxxx-xxxxx`). `challenge` is only needed for API
/// clients without the `oxidean_2fa` cookie.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TwoFactorCompleteRequest {
    pub code: String,
    #[serde(default)]
    pub challenge: Option<String>,
}

/// `auth.two_factor.passkey_complete` input — browser assertion JSON.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PasskeyCompleteRequest {
    pub assertion: serde_json::Value,
    #[serde(default)]
    pub challenge: Option<String>,
}

/// `auth.two_factor.passkey_register_complete` input — browser attestation JSON.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PasskeyRegisterCompleteRequest {
    pub nickname: String,
    pub attestation: serde_json::Value,
    #[serde(default)]
    pub challenge: Option<String>,
}

/// `auth.two_factor.passkey_update` input — rename a credential.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PasskeyUpdateRequest {
    pub id: String,
    pub nickname: String,
}

/// `auth.two_factor.passkey_delete` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PasskeyDeleteRequest {
    pub id: String,
}

/// `auth.two_factor.*` inputs carrying an optional explicit challenge id.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TwoFactorChallengeRequest {
    #[serde(default)]
    pub challenge: Option<String>,
}

/// `admin.users.resetTwoFactor` input — clear all factors for a user (lockout
/// recovery; sys-admin only).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminResetTwoFactorRequest {
    pub user_id: String,
}
