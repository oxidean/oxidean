//! Oxidean agentic VCS domain engine.
//!
//! The `server` feature carries the domain internals: the content-addressed
//! object store (u3), the changeset/ref/read-index stores (u4), and the
//! IO-free line-diff module shared with the u5 merge engine. The `vcs.*` RPC
//! surface wires through `oxidean-api` in u7. The `client` feature (default)
//! carries the `ox vcs` CliSurface + working copy over an injected transport
//! (u8). Wire DTOs live in `oxidean_core::vcs_types` and must never be
//! duplicated here.

/// Content-addressed object store — gated so `client` (default) builds link
/// zero `oxidean-db`/tokio/fs machinery (ADR-011, BR3.1).
#[cfg(feature = "server")]
pub mod object_store;

/// Changeset identity, validation, and the single-transaction create path.
#[cfg(feature = "server")]
pub mod changeset_store;

/// CAS-only ref registry — absent refs, strict expected-head updates.
#[cfg(feature = "server")]
pub mod ref_registry;

/// Derived-index backed read walks (log/tree/diff/blame/path-history/
/// merge-base) over the u2 facade.
#[cfg(feature = "server")]
pub mod read_index;

/// Myers line diff on byte-slice lines — shared with the u5 merge engine.
#[cfg(feature = "server")]
pub mod text_diff;

/// Three-way merge + first-parent rebase engine — deterministic two-pass
/// writes through `ObjectStore::put` + `ChangesetStore::create`, zero refs.
#[cfg(feature = "server")]
pub mod merge_engine;

/// Git ↔ changeset interop — deterministic import (explicit + lazy, paged
/// and resumable) and two-phase export (mint objects, check, ONE hook-gated
/// ref move). `oxidean-git` supplies the plumbing; the host supplies the
/// `ProtectionChecker` impl at dispatch time.
#[cfg(feature = "server")]
pub mod git_interop;

/// Operation-granular `repo_activity` recorder for mutating `vcs.*` ops
/// (contract C4 nullable-column mapping).
#[cfg(feature = "server")]
pub mod activity;

/// `vcs.*` procedure orchestration — the two-case flag gate, the 17 C1
/// methods, the lazy-import retry on git-oid read misses, and the typed
/// error bridge onto the frozen C1 taxonomy.
#[cfg(feature = "server")]
pub mod api;

/// Client-side surface — `VcsTransport` seam, `CliSurface` command impls,
/// working copy, and the bounded object cache (u8).
#[cfg(feature = "client")]
pub mod client;
