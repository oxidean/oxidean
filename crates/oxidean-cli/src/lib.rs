//! `ox` — companion CLI for Oxidean forge instances (roadmap CLI-01).
//!
//! Talks to `POST {instance}/api/rpc` using the same `{procedure, input}`
//! envelope and `Oxidean-RPC-Version` header as `@oxidean/api-client`, with
//! `Authorization: Bearer <pat>` credentials. See `docs/CLI.md`.

pub mod args;
pub mod commands;
pub mod completion;
pub mod config;
pub mod device;
pub mod harness;
pub mod http;
pub mod jq;
pub mod keystore;
pub mod manifest;
pub mod markdown;
pub mod mcp;
pub mod output;
pub mod pager;
pub mod pilot;
pub mod prompt;
pub mod rpc;
pub mod self_update;
pub mod skill;
pub mod spinner;
pub mod term;
