//! Oxidean agentic VCS domain engine.
//!
//! Registered skeleton — the domain internals (object store, changeset core,
//! merge engine, git interop) land in the u3–u6 units; the `vcs.*` RPC surface
//! wires through `oxidean-api` in u7. Wire DTOs live in
//! `oxidean_core::vcs_types` and must never be duplicated here.
