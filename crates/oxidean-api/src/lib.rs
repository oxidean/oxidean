// The typed-RPC dispatch graph compiles to very deep async-future types;
// proving `Send` on them exceeds the default trait-resolution recursion
// limit (the future IS Send — the proof just needs more depth). Without
// this, `clippy -D warnings` (dead-code-check) hard-errors on the overflow.
#![recursion_limit = "256"]

pub mod actions;
pub mod admin;
pub mod app;
pub mod audit;
pub mod auth;
pub mod check;
pub mod cors;
pub mod crypto;
pub mod deploy_keys;
pub mod deployment;
pub mod email;
pub mod emails;
pub mod git;
pub mod gpg_keys;
pub mod invites;
pub mod issue;
pub mod jobs;
pub mod label;
pub mod lfs;
pub mod mcp;
pub mod milestone;
pub mod mirror;
pub mod notification;
pub mod notify;
pub mod oauth;
pub mod org;
pub mod packages;
pub mod pat;
pub mod project;
pub mod protection;
pub mod public_origin;
pub mod pull;
pub mod release;
pub mod repo;
pub mod rest;
pub mod routes;
pub mod rpc;
pub mod search;
pub mod ssh;
pub mod ssh_keys;
pub mod templates;
pub mod user;
pub mod version;
pub mod webhook;
pub mod wiki;

pub use app::{router, router_with_state, AppState};
pub use cors::build_cors;
pub use public_origin::resolve_public_origin;
