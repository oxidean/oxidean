//! Oxidean agentic VCS domain engine.
//!
//! Registered skeleton — the domain internals (object store, changeset core,
//! merge engine, git interop) land in the u3–u6 units; the `vcs.*` RPC surface
//! wires through `oxidean-api` in u7. Wire DTOs live in
//! `oxidean_core::vcs_types` and must never be duplicated here.

/// Content-addressed object store — gated so `client` (default) builds link
/// zero `oxidean-db`/tokio/fs machinery (ADR-011, BR3.1).
#[cfg(feature = "server")]
pub mod object_store;
