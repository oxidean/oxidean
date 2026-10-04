//! `ox` — companion CLI for Oxidean forge instances (roadmap CLI-01).
//!
//! Talks to `POST {instance}/api/rpc` using the same `{procedure, input}`
//! envelope and `Oxidean-RPC-Version` header as `@oxidean/api-client`, with
//! `Authorization: Bearer <pat>` credentials. See `docs/CLI.md`.

pub mod args;
pub mod commands;
pub mod config;
pub mod manifest;
pub mod output;
pub mod rpc;
