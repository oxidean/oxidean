//! Auth module: passwords, sessions, local + WorkOS + OIDC providers.

pub mod admin;
pub mod bootstrap;
pub mod external;
pub mod gate;
pub mod local;
pub mod oidc;
pub mod password;
pub mod pending;
pub mod profile;
pub mod seed;
pub mod session;
pub mod verify_reset;
pub mod workos;

pub use password::{
    hash_password, hash_password_str, verify_password, PasswordError, MIN_PASSWORD_LEN,
};
pub use session::{
    build_session_presence_cookie, clear_session_cookie, clear_session_presence_cookie,
    secure_cookies, AuthError, ResolvedSession, SessionService, SESSION_COOKIE_NAME, SESSION_IDLE,
    SESSION_PRESENCE_COOKIE_NAME, SESSION_REMEMBER,
};
