//! `oxidean-api-user` — `user.*` RPC handlers (profile, lookup, follows,
//! starred/watched lists) extracted from the monolithic `oxidean-api` per
//! issue #162. `user::rate_limit` stays in core — `app.rs` and
//! `auth::verify_reset` share the lookup limiter.

mod dispatch;
mod follows;
mod list_starred;
mod lookup;
mod profile;

pub use dispatch::dispatch;

/// Procedures this crate owns — aggregated by the shell's `PROCEDURES`.
pub const PROCEDURES: &[&str] = &[
    "user.follow",
    "user.followers.list",
    "user.following.list",
    "user.getPublicProfile",
    "user.get_profile",
    "user.listStarred",
    "user.listWatched",
    "user.lookup",
    "user.unfollow",
    "user.update_profile",
];
