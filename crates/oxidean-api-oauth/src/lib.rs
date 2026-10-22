//! `oxidean-api-oauth` — `oauthApp.*` and `oauth.device*` RPC handlers
//! extracted from the monolithic `oxidean-api` per issue #162. The OAuth HTTP
//! endpoints (`authorize`/`token`/`userinfo`/`device_code`/metadata) and the
//! token-authentication machinery stay in `oxidean-api-core` — routes mount
//! them and `actions`/`packages` authenticate through them.

mod dispatch;
mod handlers;

pub use dispatch::dispatch;

/// Procedures this crate owns — aggregated by the shell's `PROCEDURES`.
pub const PROCEDURES: &[&str] = &[
    "oauth.deviceAuthorize",
    "oauth.deviceInfo",
    "oauthApp.authorize",
    "oauthApp.authorizeInfo",
    "oauthApp.create",
    "oauthApp.delete",
    "oauthApp.list",
    "oauthApp.listGrants",
    "oauthApp.regenerateSecret",
    "oauthApp.revoke",
    "oauthApp.update",
];
