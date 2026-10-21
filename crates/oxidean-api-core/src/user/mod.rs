//! User-domain RPC handlers moved to `oxidean-api-user` (#162). `rate_limit`
//! stays: `app.rs` and `auth::verify_reset` share the lookup limiter.

pub mod rate_limit;
