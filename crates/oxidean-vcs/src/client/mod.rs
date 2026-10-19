//! Client-side VCS surface (`client` feature, default).
//!
//! `oxidean-vcs` owns the `ox vcs` logic; the host (`oxidean-cli`) injects a
//! [`crate::client::transport::VcsTransport`] over its existing HTTP clients
//! and maps argv into [`crate::client::cli::VcsCommand`]. No socket, no
//! `oxidean-db`/`oxidean-git` — the
//! feature compiles dependency-light by contract (ADR-011).

pub mod cli;
pub mod mcp;
pub mod object_cache;
pub mod transport;
pub mod working_copy;
