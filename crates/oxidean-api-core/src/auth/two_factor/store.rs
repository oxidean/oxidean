//! In-memory two-factor challenge + enrollment-state store (#130).
//!
//! Same shape as `auth::pending` (OAuth PKCE): process-local `HashMap` entries
//! with a hard TTL, consumed on success. A pending 2FA login must survive a
//! browser round trip but never across restarts — an API restart just makes
//! the user re-enter their password, matching `PendingAuthStore` semantics.
//!
//! WebAuthn registration/assertion server state MUST stay server-side
//! (replay protection); it lives inside these entries, never in the DB.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use webauthn_rs::prelude::{PasskeyAuthentication, PasskeyRegistration};

/// Challenge TTL — generous enough for an OTP email round trip.
pub const CHALLENGE_TTL: Duration = Duration::from_secs(10 * 60);
/// Failed second-factor attempts before a challenge is invalidated.
pub const MAX_ATTEMPTS: u32 = 8;
/// Session-scoped enrollment state TTL (start/finish round trips).
const ENROLL_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingKind {
    /// User has factor(s) and must prove one.
    Verify,
    /// Policy requires a factor and the user has none — must enroll.
    Enroll,
}

impl PendingKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Verify => "verify",
            Self::Enroll => "enroll",
        }
    }
}

/// One pending second-factor login/enrollment.
///
/// The `needs_*` flags are the outstanding sign-in requirements; the
/// challenge may only mint a session once all are cleared (`satisfied()`).
/// Identifier-first login (`auth.login_begin`) opens challenges with
/// `needs_password = true`; post-primary-auth callers (one-shot `auth.login`,
/// signup, reset, SSO callback) open them with `needs_password = false`.
/// `needs_factor` and `needs_enroll` are mutually exclusive.
pub struct PendingChallenge {
    pub user_id: String,
    pub remember_me: bool,
    /// Prove an enrolled factor (TOTP / email OTP / recovery / passkey).
    pub needs_factor: bool,
    /// Policy requires enrolling a factor. Enroll legs reject while
    /// `needs_password` is still set — a factor must never be plantable
    /// on an account pre-auth.
    pub needs_enroll: bool,
    /// The local-password step is still owed.
    pub needs_password: bool,
    /// Methods offered to the client (`totp`, `passkey`, `email`, `recovery`,
    /// and for enroll `totp`/`passkey`).
    pub methods: Vec<String>,
    pub created_at: Instant,
    pub attempts: u32,
    /// WebAuthn assertion state stashed by `passkey_begin`.
    pub webauthn_assertion: Option<PasskeyAuthentication>,
    /// WebAuthn registration state stashed by `passkey_register_begin`
    /// (enroll-kind challenges only).
    pub passkey_registration: Option<PasskeyRegistration>,
    /// OTP-email issue count within this challenge (resend is rate-limited).
    pub email_sends: u32,
}

impl PendingChallenge {
    /// `Enroll` while enrollment is still owed, else `Verify`.
    pub fn kind(&self) -> PendingKind {
        if self.needs_enroll {
            PendingKind::Enroll
        } else {
            PendingKind::Verify
        }
    }

    /// All requirements cleared — safe to mint the session.
    pub fn satisfied(&self) -> bool {
        !self.needs_factor && !self.needs_enroll && !self.needs_password
    }
}

/// Parameters for `TwoFactorStore::open` — the outstanding requirements the
/// challenge starts with. `needs_factor` and `needs_enroll` are mutually
/// exclusive; `needs_password` is `true` only for identifier-first
/// `auth.login_begin` challenges (post-primary-auth callers pass `false`).
pub struct OpenChallenge {
    pub user_id: String,
    pub remember_me: bool,
    /// Prove an enrolled factor.
    pub needs_factor: bool,
    /// Enroll a factor — enrollment legs reject while `needs_password` holds.
    pub needs_enroll: bool,
    /// The local-password step is still owed.
    pub needs_password: bool,
    pub methods: Vec<String>,
}

/// Lightweight view for handlers (the heavy WebAuthn states stay inside).
#[derive(Debug, Clone)]
pub struct PendingInfo {
    pub user_id: String,
    pub remember_me: bool,
    pub needs_factor: bool,
    pub needs_enroll: bool,
    pub needs_password: bool,
    pub methods: Vec<String>,
}

impl PendingInfo {
    /// `Enroll` while enrollment is still owed, else `Verify`.
    pub fn kind(&self) -> PendingKind {
        if self.needs_enroll {
            PendingKind::Enroll
        } else {
            PendingKind::Verify
        }
    }
}

/// Session-scoped (signed-in user) enrollment state — passkey registration
/// pending `navigator.credentials.create` completion, keyed by user id.
struct SessionEnroll {
    passkey_registration: PasskeyRegistration,
    created_at: Instant,
}

#[derive(Clone)]
pub struct TwoFactorStore {
    inner: Arc<Mutex<StoreInner>>,
}

#[derive(Default)]
struct StoreInner {
    challenges: HashMap<String, PendingChallenge>,
    session_enrolls: HashMap<String, SessionEnroll>,
}

impl Default for TwoFactorStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TwoFactorStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(StoreInner::default())),
        }
    }

    /// Open a new challenge; returns the opaque challenge id (32-byte CSPRNG hex).
    pub fn open(&self, params: OpenChallenge) -> String {
        let mut bytes = [0u8; 32];
        rand::fill(&mut bytes);
        let id = crate::auth::session::bytes_to_hex(&bytes);
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        sweep_locked(&mut inner);
        inner.challenges.insert(
            id.clone(),
            PendingChallenge {
                user_id: params.user_id,
                remember_me: params.remember_me,
                needs_factor: params.needs_factor,
                needs_enroll: params.needs_enroll,
                needs_password: params.needs_password,
                methods: params.methods,
                created_at: Instant::now(),
                attempts: 0,
                webauthn_assertion: None,
                passkey_registration: None,
                email_sends: 0,
            },
        );
        id
    }

    /// Read challenge metadata without consuming it.
    pub fn peek(&self, id: &str) -> Option<PendingInfo> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        sweep_locked(&mut inner);
        inner.challenges.get(id).map(|p| PendingInfo {
            user_id: p.user_id.clone(),
            remember_me: p.remember_me,
            needs_factor: p.needs_factor,
            needs_enroll: p.needs_enroll,
            needs_password: p.needs_password,
            methods: p.methods.clone(),
        })
    }

    /// Mutate a live challenge in place (attempt counter, WebAuthn state).
    /// Closures run under the mutex — never `.await` inside.
    pub fn update<R>(&self, id: &str, f: impl FnOnce(&mut PendingChallenge) -> R) -> Option<R> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.challenges.get_mut(id).map(f)
    }

    /// Consume and remove a challenge (successful completion or abandonment).
    pub fn take(&self, id: &str) -> Option<PendingChallenge> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        sweep_locked(&mut inner);
        inner.challenges.remove(id)
    }

    /// Drop a challenge after too many failed attempts.
    pub fn drop_challenge(&self, id: &str) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.challenges.remove(id);
    }

    /// Stash a session-scoped passkey registration (signed-in enroll flow).
    pub fn stash_session_passkey(&self, user_id: &str, state: PasskeyRegistration) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        sweep_locked(&mut inner);
        inner.session_enrolls.insert(
            user_id.to_string(),
            SessionEnroll {
                passkey_registration: state,
                created_at: Instant::now(),
            },
        );
    }

    /// Take a session-scoped passkey registration for `finish`.
    pub fn take_session_passkey(&self, user_id: &str) -> Option<PasskeyRegistration> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        sweep_locked(&mut inner);
        let live = inner
            .session_enrolls
            .get(user_id)
            .map(|e| e.created_at.elapsed() <= ENROLL_TTL)
            .unwrap_or(false);
        if live {
            inner
                .session_enrolls
                .remove(user_id)
                .map(|e| e.passkey_registration)
        } else {
            inner.session_enrolls.remove(user_id);
            None
        }
    }
}

fn sweep_locked(inner: &mut StoreInner) {
    inner
        .challenges
        .retain(|_, p| p.created_at.elapsed() <= CHALLENGE_TTL);
    inner
        .session_enrolls
        .retain(|_, e| e.created_at.elapsed() <= ENROLL_TTL);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_peek_take_roundtrip() {
        let store = TwoFactorStore::new();
        let id = store.open(OpenChallenge {
            user_id: "u1".into(),
            remember_me: true,
            needs_factor: true,
            needs_enroll: false,
            needs_password: false,
            methods: vec!["totp".into()],
        });
        let info = store.peek(&id).expect("peek");
        assert_eq!(info.user_id, "u1");
        assert!(info.remember_me);
        assert_eq!(info.kind(), PendingKind::Verify);
        assert!(info.needs_factor);
        assert!(!info.needs_password);
        assert_eq!(info.methods, vec!["totp".to_string()]);
        assert!(store.take(&id).is_some());
        assert!(store.peek(&id).is_none(), "consumed challenge must be gone");
    }

    #[test]
    fn update_mutates_attempts() {
        let store = TwoFactorStore::new();
        let id = store.open(OpenChallenge {
            user_id: "u1".into(),
            remember_me: false,
            needs_factor: false,
            needs_enroll: true,
            needs_password: false,
            methods: vec![],
        });
        let n = store.update(&id, |p| {
            p.attempts += 1;
            p.attempts
        });
        assert_eq!(n, Some(1));
    }
}
