//! `oxidean-api-core` — the shared foundation every `oxidean-api-*` domain
//! crate builds on: `RpcCtx`/`AppState`, session+auth plumbing, RPC dispatch,
//! and the cross-domain services (repo ACL, git/LFS transports, protection,
//! webhook and notify fan-out, actions engine, mirror engine, PAT bearer).
//!
//! Dependency direction is fixed: `oxidean-api-core` ← `oxidean-api-{domain}`
//! ← `oxidean-api` (shell). Domain crates never depend on each other; anything
//! two domains need lives here. See issue #162.

pub mod actions;
pub mod admin;
pub mod app;
pub mod audit;
pub mod auth;
pub mod check;
pub mod crypto;
pub mod discussion;
pub mod email;
pub mod git;
pub mod invites;
pub mod issue;
pub mod jobs;
pub mod label;
pub mod lfs;
pub mod merge_queue;
pub mod milestone;
pub mod mirror;
pub mod notify;
pub mod oauth;
pub mod org;
pub mod packages;
pub mod pages;
pub mod pat;
pub mod project;
pub mod protection;
pub mod public_origin;
pub mod pull;
pub mod release;
pub mod repo;
pub mod rpc;
pub mod search;
pub mod ssh;
pub mod templates;
pub mod user;
pub mod vcs;
pub mod vcs_protection;
pub mod vcs_routes;
pub mod version;
pub mod webhook;
