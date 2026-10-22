// The typed-RPC dispatch graph compiles to very deep async-future types;
// proving `Send` on them exceeds the default trait-resolution recursion
// limit (the future IS Send — the proof just needs more depth). Without
// this, `clippy -D warnings` (dead-code-check) hard-errors on the overflow.
#![recursion_limit = "256"]

pub mod app;
pub mod cors;
pub mod mcp;
pub use oxidean_api_rest as rest;
pub mod routes;
pub mod ssh;

// Domain + shared-service modules live in `oxidean-api-core` (issue #162);
// re-export them under the original `oxidean_api::<mod>` paths so existing
// call sites, tests, and binaries keep resolving during the transition.
pub use oxidean_api_core::actions;
pub use oxidean_api_core::admin;
pub use oxidean_api_core::audit;
pub use oxidean_api_core::auth;
pub use oxidean_api_core::check;
pub use oxidean_api_core::crypto;
pub use oxidean_api_core::discussion;
pub use oxidean_api_core::email;
pub use oxidean_api_core::git;
pub use oxidean_api_core::invites;
pub use oxidean_api_core::issue;
pub use oxidean_api_core::jobs;
pub use oxidean_api_core::label;
pub use oxidean_api_core::lfs;
pub use oxidean_api_core::merge_queue;
pub use oxidean_api_core::milestone;
pub use oxidean_api_core::mirror;
pub use oxidean_api_core::notify;
pub use oxidean_api_core::oauth;
pub use oxidean_api_core::org;
pub use oxidean_api_core::packages;
pub use oxidean_api_core::pages;
pub use oxidean_api_core::pat;
pub use oxidean_api_core::project;
pub use oxidean_api_core::protection;
pub use oxidean_api_core::public_origin;
pub use oxidean_api_core::pull;
pub use oxidean_api_core::release;
pub use oxidean_api_core::repo;
pub mod rpc;
pub use oxidean_api_core::search;
pub use oxidean_api_core::templates;
pub use oxidean_api_core::user;
pub use oxidean_api_core::vcs;
pub use oxidean_api_core::vcs_protection;
pub use oxidean_api_core::vcs_routes;
pub use oxidean_api_core::version;
pub use oxidean_api_core::webhook;

pub use app::{router, router_with_state, AppState};
pub use cors::build_cors;
pub use public_origin::resolve_public_origin;
