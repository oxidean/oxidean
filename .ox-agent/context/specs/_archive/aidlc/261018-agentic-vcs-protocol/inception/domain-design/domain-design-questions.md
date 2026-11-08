# Domain Design — Questions & Answers

Consumes `requirements.md` (FR1–FR16), `stories.md` (US1–US8), codekb `architecture.md` + `component-inventory.md`, `team-practices.md`. Brownfield: new `oxidean-vcs` crate inside the existing Cargo workspace.

## Q1. Component decomposition inside the crate — one engine or bounded parts?

`oxidean-vcs` holds server procedures, CLI-side logic, and MCP descriptors (pinned architecture). Inside it, do we ship one `VcsEngine` component or decompose?

**Options:**
- A. Single `VcsEngine` component owning everything (simplest; but blends storage, lineage, merge, interop, and client concerns with different change rates)
- B. Decompose into bounded components by data ownership + change rate: object store (CAS bytes+refcounts), changeset store (create/lookup/lineage), ref registry (CAS heads), read/derived-data (log/diff/tree/blame/history), merge engine (merge-base/diff3/rebase), git interop (import/export), client working-copy (sparse fetch/materialize/cache), plus wire-surface adapters (CLI commands, MCP descriptors, activity recording)

[Answer]: B — the concerns have distinct lifecycles and change rates (storage is stable, merge/interop evolve, client logic moves with the CLI). Eight bounded components + thin adapters keeps each deep-module seam testable and matches the "one owning component per entity" rule cleanly. The adapters (CLI/MCP/activity) stay thin: they translate, never own data.

## Q2. Where do wire types and the proc_classes entries live?

**Options:**
- A. `oxidean-vcs` exports wire types; `oxidean-core` imports them (would invert the core←leaf dependency rule — core currently depends on nothing)
- B. Wire types (changeset/ref/object DTOs, `RpcRequest` arg/result shapes) live in `oxidean-core`; `proc_classes` gains `vcs.*` entries there; `oxidean-vcs` consumes them

[Answer]: B — `oxidean-core` is the highest-fan-out leaf (api/cli/db/web all depend on it); component-inventory already names it the home of new wire types. Putting them in the crate would create a core→vcs edge that violates the acyclic dep graph.

## Q3. Server topology — in-app mount or aux listener?

**Options:**
- A. Spawn a dedicated listener from `main` (SSH-sidecar pattern) serving the `vcs.*` surface on its own port
- B. Mount inside `router_with_state` — RPC procedures join the `PROCEDURES`/dispatch table; object routes nest under `/api/vcs/{o}/{r}/objects`

[Answer]: B — the ratified transport decision keeps everything under `/api/` (already in `API_PREFIXES`, zero edge-parity work). A second listener would need its own auth/CORS/wire-version plumbing for no benefit; per-request auth (PAT/proc_classes) is already centralized at dispatch.

## Q4. Persistence split — what goes in DB vs filesystem CAS?

The article's model: compressed content-addressed blobs + DB-side changeset/ref metadata. Oxidean has two precedents: `oxidean-db` façade tables ×3 dialects, and on-disk content dirs (`OXIDEAN_LFS_DIR`, `OXIDEAN_PACKAGES_DIR`) with refcount + GC.

**Options:**
- A. Everything in DB tables (blobs as BYTEA/BLOB) — single backup story, but large-object churn inside DB pages and no streaming reads
- B. Metadata/lineage/refs/indexes in `oxidean-db` tables; blob bytes in a content-addressed dir (`OXIDEAN_VCS_OBJECTS_DIR`, sha-sharded like LFS) with refcount + grace-windowed GC job

[Answer]: B — clones the proven LFS/packages pattern (env-dir, `{ab}/{cd}/{hash}` sharding, refcount + GC job via `jobs/mod.rs`), keeps DB rows small, and want-based GETs stream from disk without dialect SQL on the hot path. Derived-data indexes (blame/path-history) are DB tables — they're small, relational, and updated transactionally with the changeset row.

## Q5. Entity ownership — what does the crate own?

Drafting ownership (one owner each): `Changeset`, `FileChange`/`ManifestEntry`, `ObjectEntry`(CAS byte record + refcount), `Ref` (name→changeset CAS), `ImportLink` (git_oid↔changeset_id), `DerivedIndex` rows (blame/path-history), `CheckoutCache` (client-side). Repo itself stays owned by the existing repo domain.

[Answer]: As drafted. `CheckoutCache` is owned by the client working-copy component (it never crosses the wire — local disk state). `repo_activity` rows are NOT owned here: the ActivityRecorder adapter calls `Database::insert_repo_activity` (oxidean-db façade) — the crate calls the façade, the row type stays in `oxidean-db` per the dependency-correct path (FR16 fix).

## Q6. git_export's protection-verdict path — how does the crate reach it without depending on oxidean-api?

`git_export` must reuse the same protection-verdict path as `git push` (pinned requirement), but `oxidean-api` hosts the protection hook (`oxidean-protection-hook` bin) and `oxidean-vcs` must not depend on its host.

**Options:**
- A. `oxidean-vcs` defines a `ProtectionChecker` trait (port); `oxidean-api`'s dispatch arm supplies the real implementation (existing protection hook) — trait+adapter idiom already in team practices
- B. Export logic lives in `oxidean-api` calling into `oxidean-vcs` for conversion — but then the "export" procedure's domain logic splits across crates, and the CLI can't share it

[Answer]: A — the trait is defined in `oxidean-vcs` (dependency direction host→crate preserved); `oxidean-api` wires the real checker at dispatch time, and tests can stub it. `oxidean-vcs` also takes `Arc<dyn GitBackend>`-style input — but from `oxidean-git` (sibling crate, allowed dep) for the object read/ref write during import/export.

## Q7. Derived-data computation — on-write, on-read, or job-based?

Article: blame/path-history/commit-graph are derived, computed incrementally, independently scalable.

**Options:**
- A. Purely on-read (no index tables) — simplest, but blame/history over deep history walks the whole chain per request
- B. On-write incremental: changeset create transactionally updates `blame`/`path_history` index rows from parent state; reads hit index tables; GC/compaction is a background job
- C. Async job queue populates indexes post-commit — adds "not yet indexed" states the API must expose

[Answer]: B — incremental-on-write inside the same transaction as the changeset row (writes are already WAL-ordered; the delta from parent is exactly what the changeset carries). Reads stay O(index), no async consistency window to document. If hot-path latency later suffers, derivation can move to a job without changing the wire contract (index rows are internal).

## Q8. Client working-copy scope — does checkout logic live in the crate?

Requirement: server + CLI + MCP logic together in the crate. Sparse checkout/fetch is client-side (manifest filter → want-fetch blobs → materialize).

[Answer]: Yes — `WorkingCopy` component owns manifest filtering, the local object cache (dedup across checkouts), and materialization (mode/symlink fidelity). `oxidean-cli` gets thin argv glue calling into it; the dependency `oxidean-cli → oxidean-vcs` is added (cli already depends on `oxidean-core`; this is the same pattern). This also gives `ox vcs import --lookup` etc. without duplicating logic.
