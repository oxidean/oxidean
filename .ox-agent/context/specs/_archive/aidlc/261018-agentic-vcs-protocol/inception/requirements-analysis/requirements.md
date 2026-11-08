# Requirements — Side-by-Side Agentic Source-Control Protocol

## Sources

- Initial description (verbatim, `project-description.json`): "I want to create a change for adding a side-by-side source-control protocol (that doesnt use git, but runs alongside it on the server for interop) for improved agentic coding efficiency and throughput based on Anthropic's internal source control. The article here https://www.anthropic.com/engineering/redesigning-source-control explains a large amount of the details. The logic on our side should be built in a dedicated crate (the server, cli and mcp side logic should live together inside its own crate and become a dependency internally)"
- Reference document: "Redesigning source control at Anthropic" (Scion) — changeset/WAL model, derived data, want-based fetch, tiered architecture, API-first queries.
- Upstream artifacts consumed: `business-overview.md`, `architecture.md`, `code-structure.md` (code knowledge base), `team-practices.md` (affirmed practices), `requirements-analysis-questions.md` (answered decisions Q1–Q9).

## Intent Analysis

The goal is a second, non-Git source-control protocol that runs alongside Git on the same forge — designed for agentic workloads where clients are always online and write volume is machine-generated, not human. The measurable problem it solves: Git's packfile/ref/tree-pointer model makes every commit, checkout, and history query carry repository-scale cost; agents need cheap server-side writes, want-based object fetch, and API-first access to history that never requires a local clone.

The Scion article supplies the architecture: server-created **changesets** (WAL entries recording changed files relative to the first parent + content-addressed blob pointers), **derived data** computed recursively from parents (blame, path history, commit-graph traversal), a **want-based fetch protocol** (clients request the immutable objects they want, never negotiate what they have — so any cache can answer), **minimal CAS'd refs** kept off the commit path, **sparse-by-default working copies**, and **API-first operations** so jobs need no checkout at all.

Oxidean adapts this to a self-hostable forge: the same model, scaled to single-node Postgres/MySQL/SQLite + filesystem storage rather than Spanner — keeping horizontal-scalability *posture* (immutable data, stateless serving, no coordination) without needing multi-tier cache fleets yet.

## Functional Requirements

### FR1 — Repository enablement and identity

- FR1.1 A repository gains a per-repo flag enabling the side-by-side VCS surface; the flag is stored on the repo record and admin/owner-toggleable.
- FR1.2 VCS data is keyed to the existing repository identity (`owner/name` → repo id); a repo may be served by Git, VCS, or both simultaneously.
- FR1.3 Enabling VCS on a repo makes the VCS surface immediately usable (empty history); Git history is imported per FR9, lazily or on demand.
- FR1.4 Disabling the flag stops new writes; stored changesets and objects remain readable unless the data is explicitly deleted.
- FR1.5 Repository lifecycle is inherited: deleting a repository removes its VCS rows (changesets, refs, oid mappings) through the existing repo-deletion cascade; object files under `OXIDEAN_VCS_OBJECTS_DIR` are shared refcounted CAS and are NOT eagerly deleted — unreferenced objects are reclaimed by the FR3.3 GC job (the reconcile-purge path only covers `repos_dir`). Transfers preserve VCS data (all keys derive from repo id); archival follows the repo record's existing semantics with VCS reads continuing to work.

### FR2 — Changeset model

- FR2.1 The unit of work is a **changeset**: a server-created record containing author identity, message, timestamps, parent changeset ids (first parent primary), and a list of changed files relative to the first parent — each entry carrying path, change kind (add/modify/delete/rename), file mode, and a content-addressed blob pointer. A changeset never stores a full tree pointer.
- FR2.2 Changesets are immutable and content-addressed: the id is sha256 over a canonical encoding of exactly {ordered parent ids; the changed-file manifest entries (path, change kind, mode, blob hash) in canonical order; author identity as a canonical `Name <email>` string (resolved from the platform user record when the author is a platform user, carried verbatim for imported commits — deterministic mapping keeps FR9 import id-convergent); message}. Server-assigned fields (sequence number, wall-clock timestamp) are recorded on the row but excluded from the hash, so re-submitting identical content returns the existing id (idempotent creation).
- FR2.3 Changeset creation accepts parent ids supplied by the client and never requires a ref to be read or moved; concurrent changesets on the same parent always succeed (forking is normal).
- FR2.4 Merge changesets record ≥2 parents; the changed-file list is always relative to the first parent.
- FR2.5 File entries support binary contents, executable bit, and symlinks; renames record `(from_path, to_path)` explicitly.

### FR3 — Content-addressed object store

- FR3.1 File contents are stored once per content hash (sha256) in a sharded directory (`{aa}/{bb}/{hash}`) under a dedicated env-configured root (`OXIDEAN_VCS_OBJECTS_DIR`), following the LFS/packages precedent.
- FR3.2 Object PUT is deduplicating (existing hash → no-op) and size-limited by a configurable quota; oversized uploads fail with a typed error.
- FR3.3 Objects are reference-counted from changesets; a periodic GC job reclaims unreferenced objects (LFS-GC precedent).
- FR3.4 Object reads by hash are served from a dedicated route and carry immutable-cache semantics (long-lived cache headers allowed).

### FR4 — Want-based fetch protocol

- FR4.1 Clients fetch data exclusively by requesting objects they want — never by describing what they have (no have/want negotiation).
- FR4.2 From a changeset id, a client resolves its changed-file entries and blob hashes, checks its bounded local object cache, and requests missing objects by hash; every request targets immutable data any tier could answer.
- FR4.3 Fetch is resumable and per-object — there is no single multi-megabyte monolithic response equivalent to a packfile.
- FR4.4 Sparse fetch is the default: a client requests only the trees/paths in its declared filter; a "full checkout" is a degenerate filter, not a separate mode.

### FR5 — Derived data and history queries

- FR5.1 Derived data is computed through one recursive mechanism per type — a value for a changeset computed from its parents' values — following the article's model; each type may be computed eagerly at write time or lazily on first read and persisted.
- FR5.2 Required derived data in v1: per-path history (which changesets touched a path), commit-graph ancestry (merge-base / is-ancestor), and per-file line attribution (blame).
- FR5.3 Blame may be computed lazily from path history + diffs rather than stored eagerly; the query contract is what matters, not the materialization.
- FR5.4 A `tree` derived type materializes the full file listing for a changeset on demand (needed for checkout and Git export); results are cacheable by changeset id.

### FR6 — Refs

- FR6.1 A minimal refs namespace per repo (default head `main` plus caller-chosen named heads); refs point at changeset ids.
- FR6.2 Ref moves are compare-and-swap on `(repo, name, expected_old)` — never implicit during changeset creation; an explicit advance operation moves a ref atomically.
- FR6.3 Ref listing, creation, update, and deletion are API operations: creation and deletion ride `vcs.ref_update`'s CAS contract (expected-absent creates; a null target deletes), so no separate mutation procedures exist; diverged heads require an explicit merge changeset to reconcile.

### FR7 — Server-side write operations

- FR7.1 `changeset_create` — client supplies changed-file list + blob uploads (or inline contents); server writes objects, then the changeset row, then derived data per FR5 policy. The whole operation is lock-free with respect to other commits.
- FR7.2 `merge` — given two changeset ids, the server computes merge-base, performs a 3-way merge, and creates a merge changeset (with a machine-readable conflict list on failure; partial conflicts must never write a broken changeset).
- FR7.3 `rebase` — given a changeset and a new parent, the server replays its changed files onto the parent and creates the resulting changeset.
- FR7.4 Write operations are atomic per changeset: objects + row + derived data commit or none do; a half-written changeset must never be visible.

### FR8 — Sparse checkout materialization

- FR8.1 `ox vcs checkout` materializes a changeset's files to disk for declared path filters only; unrequested trees are never fetched.
- FR8.2 The client maintains a bounded local object cache keyed by content hash shared across checkouts/repos on the machine.
- FR8.3 Checkout materialization supports standard file modes, executables, and symlinks consistently on supported platforms.

### FR9 — Git interop

- FR9.1 Import: an existing Git commit history is imported into changeset form — one changeset per Git commit, parents mapped, changed-file lists computed from Git diffs, blob contents stored under the VCS content hash; a mapping table links `git_oid ↔ changeset_id`.
- FR9.2 Export: a changeset chain can be materialized into Git commit objects inside the repo's bare repository and move a named Git branch — an explicit operation, never an automatic mirror. The branch move passes through the same branch-protection verdict path as an equivalent `git push` (protected-branch rules, required checks, hook verdicts); export must never bypass them.
- FR9.3 Import and export preserve authorship, timestamps, and messages; merges preserve their parent structure.
- FR9.4 Import runs lazily (on first read of a commit the VCS hasn't seen) or as an explicit backfill operation; both paths must converge to the same changeset ids.

### FR10 — RPC surface

- FR10.1 A `vcs.*` procedure family on `/api/rpc` covers the full operation set: `vcs.repo_enable`, `vcs.status`, `vcs.changeset_create`, `vcs.changeset_get`, `vcs.log`, `vcs.diff`, `vcs.tree`, `vcs.file_get`, `vcs.blame`, `vcs.path_history`, `vcs.merge_base`, `vcs.merge`, `vcs.rebase`, `vcs.ref_list`, `vcs.ref_update` (CAS), `vcs.git_import`, `vcs.git_export` (exact names finalized at contract-design; new names are snake_case).
- FR10.2 Every procedure registers in all five places: `PROCEDURES`, dispatch arm, `oxidean-core` wire types, `proc_classes`, and the `rpc-gen` client template; regenerated client committed in the same change. `ox vcs` commands also update the `required_procedures` map and `CLI_PROCS` list, and `vcs_*` tools the `MCP_PROCS` list (`oxidean-cli` `commands.rs`, all covered by the fail-closed coverage test). Every new `OXIDEAN_VCS_*` variable is documented in `.env.example` and `docs/CONFIGURATION.md` (affirmed rule).
- FR10.3 `system.manifest` advertises the family; the `Oxidean-RPC-Version` gate applies unchanged.

### FR11 — Object-transfer routes

- FR11.1 `GET /api/vcs/{owner}/{repo}/objects/{hash}` — fetch a blob by content hash (immutable, cacheable).
- FR11.2 `POST /api/vcs/{owner}/{repo}/objects` — upload blob content; returns its content hash; deduplicating.
- FR11.3 All `/api/vcs/…` routes sit under the existing `/api/` edge prefix — no Traefik, Caddy, or `oxidean-web` proxy table changes.

### FR12 — CLI

- FR12.1 `ox vcs …` command family on the existing hand-rolled parser and dispatch: `enable`, `status`, `commit`, `log`, `diff`, `show`/`cat`, `blame`, `history`, `merge-base`, `merge`, `rebase`, `checkout`, `ref` subcommands, `import`, `export`.
- FR12.2 Command implementations live in `oxidean-vcs`; `oxidean-cli` holds thin argv-parse → call glue.
- FR12.3 `ox api vcs.*` remains the raw-RPC escape hatch; new procedures appear in the generated `@oxidean/api-client` for TS consumers.

### FR13 — MCP tools

- FR13.1 Tool set mirroring the core RPC operations — `vcs_status`, `vcs_log`, `vcs_diff`, `vcs_tree`, `vcs_file_get`, `vcs_blame`, `vcs_path_history`, `vcs_merge_base`, `vcs_changeset_create`, `vcs_merge`, `vcs_rebase`, `vcs_ref_list` — each a thin map onto its same-named `vcs.*` procedure. Checkout is deliberately absent: it is a client-side disk operation (FR8), not an RPC, so MCP agents materialize content through the read tools instead.
- FR13.2 Tools are gated by the existing `Access` classes (`RepoRead`/`RepoWrite`) and the `OXIDEAN_MCP_ENABLED` + `admin.mcp.*` overrides; domain denials surface as `isError` tool content.
- FR13.3 Tool definitions are produced by `oxidean-vcs` and consumed by `mcp.rs` `tool_defs()`.

### FR14 — Authentication and authorization

- FR14.1 `/api/vcs/…` object routes accept PAT (Basic/Bearer) and OAuth access tokens — never session cookies (affirmed rule, D-12 precedent); PAT lookup reuses the sha256 path and `FailedAuthLimiter`.
- FR14.2 `vcs.*` procedures get explicit `proc_classes` entries with a read/write split mirroring repo permissions (the fail-closed default must not leave them SessionOnly).
- FR14.3 Authorization reuses the existing repo permission model (owner/collaborator/read-write); no new credential type and no per-directory ACLs this cut.
- FR14.4 Rate limiting applies per existing patterns (`FailedAuthLimiter`, request-size limits on object PUT).

### FR15 — The dedicated crate

- FR15.1 `crates/oxidean-vcs` owns: changeset/object domain logic, `vcs.*` RPC handler functions, `/api/vcs/…` route handlers, `ox vcs` command implementations, and MCP tool definitions — one crate, consumed internally.
- FR15.2 `oxidean-api` wires it: `Arc<dyn VcsEngine>`-style slot on `AppState` (the `GitBackend` precedent), dispatch arms delegating to crate handlers, route mounting in `app.rs`.
- FR15.3 `oxidean-cli` and `mcp.rs` consume the crate's command/tool surfaces as thin shells.
- FR15.4 The crate registers a component + scope in `versioning.toml` and its public surface cites decision IDs in `docs/ARCHITECTURE.md`.
- FR15.5 Persistence goes through the `oxidean-db` façade: migrations authored ×3 (`postgres`/`mysql`/`sqlite`, filename parity) plus a `vcs` façade module and row types; all VCS surfaces fail closed with `db.not_configured` when `Database::skipped()`.

### FR16 — Activity feed

- FR16.1 Repo-visible VCS writes record `repo_activity` rows via `Database::insert_repo_activity` (the façade-level insert reachable from `oxidean-vcs`; the crate never calls `oxidean-api` functions — dependency direction stays host→crate), following the git ref-update precedent (`record_ref_updates`, `record_branch_creation`, …): `changeset_create` (one row per changeset), `merge`, `rebase`, ref create/update/delete, `git_import`, `git_export`, and repo enable/disable — each attributed to the authenticated identity. VCS events without an existing feed-kind analogue extend the kind allow-list (`VALID_PUSH_TYPES` precedent) with `vcs.*` kinds; ref-less rows' field conventions are fixed at contract-design. Object PUT and other internal writes do NOT emit per-object rows (they would flood the feed).
- FR16.2 `audit_events` is an identity/admin-scoped surface (`audit.rs`): VCS repo operations do not write it this cut; no new audit channel is introduced. Read operations (`log`, `diff`, `blame`, `path_history`, object GET) are not activity-recorded.

## Non-Functional Requirements

- NFR1 — **Incremental cost**: every commit-path query is bounded by the change manifest and direct parent edges — no unindexed full-history or full-tree walk. Checkable: a nextest test asserts row/object visit counts during `changeset_create` + derived updates stay flat while committed-history depth grows (dedicated perf infrastructure does not exist — recorded debt; the bound test is the pass/fail evidence this cut).
- NFR2 — **Lock-free writes**: no global lock, ref coordination, or cross-request serialization in the commit path; concurrent changesets on one parent always succeed.
- NFR3 — **Immutability**: all stored objects and changesets are content-addressed and immutable once written — any cache tier may serve them safely.
- NFR4 — **Stateless serving**: object-serving and `vcs.*` read routes hold no per-request shared mutable state beyond the `Database` pool and config — verifiable by code review plus a two-request smoke asserting identical responses for identical object hashes across connections. (The tiered daemon/cache fleet itself is out of scope.)
- NFR5 — **Compatibility**: the RPC family rides the existing `Oxidean-RPC-Version` SemVer gate; procedure names, error codes, and response shapes are wire-stable once shipped.
- NFR6 — **Dialect parity**: every persisted structure ships migrations for Postgres, MySQL, and SQLite with filename parity (CI-enforced); behavioral equivalence is verified on the dialects CI actually executes — SQLite always, Postgres/MySQL only where `DATABASE_URL` lanes run — per the affirmed practice (the skip-when-unset behavior is recorded debt, not silently claimed coverage).
- NFR7 — **Resumability**: fetches are per-object and resumable; partial transfers never corrupt the local cache (write-to-temp-then-rename or hash-verify).
- NFR8 — **Observability**: operations emit `tracing` events consistent with existing modules; failures surface typed `AppError` codes (`vcs.*` domain) without leaking internals.
- NFR9 — **Security**: no new credential types; cookies never authorize `/api/vcs/*`; object PUT size limits + repo quotas enforced; no secrets in config or tests.
- NFR10 — **Verification**: unit + integration tests per the affirmed testing posture (Rust nextest, real SQLite DBs, real-client protocol smokes via Docker Compose; dialect tests where CI executes them).

## Constraints

- C1 — One codebase for cloud + self-host; the feature must work under Docker Compose and a single API process.
- C2 — The dedicated crate holds server+CLI+MCP logic together (user-specified), while dialect SQL stays in `oxidean-db` and generated sources are regenerated, never hand-edited.
- C3 — Git interop is side-by-side: Git remotes, PRs, releases, and the web UI continue working unchanged on repos that also enable VCS.
- C4 — `vcs.*` names are snake_case; existing camelCase procedure names remain frozen (wire-stable).
- C5 — SemVer commit grammar (`feat(vcs): …` with a registered `vcs` scope) and all CI gates apply.
- C6 — Docker-first verification: local testing/debugging uses the Compose stack and `act`-style CI lanes where practical.
- C7 — No new heavyweight dependencies without evidence; prefer crates already in the workspace.

## Assumptions

- A1 — Content hash is sha256 (not Git's sha1) — the VCS object model must not couple to Git's object format; the Git-interop mapping table handles the bridge.
- A2 — Crate name `oxidean-vcs`, RPC namespace `vcs.*`, CLI family `ox vcs`, MCP tools `vcs_*`, env prefix `OXIDEAN_VCS_*`.
- A3 — Blob compression (e.g. zstd) at rest is a domain-design detail; the wire contract only fixes content addressing.
- A4 — The first deployment target is single-node; the design leaves room for external caches later (immutable objects make that possible without protocol changes).
- A5 — `Database::skipped()` deployments get no VCS surface (fail closed) — persistence is inherent to the feature.

## Out of Scope

- O1 — Virtualized (materialize-on-read) checkouts / filesystem mounts.
- O2 — Per-directory and path-level ACLs beyond the existing repo permission model.
- O3 — Merge queues, auto-landing pipelines, and review-surface (PR) changes.
- O4 — Multi-tier daemon/cache infrastructure (node daemons, regional caches), multi-region replication.
- O5 — Web-UI surfaces for VCS history/blame/diff views (the API/CLI/MCP surface lands first; Octane UI is a later intent).
- O6 — Migration tooling to convert a repo's primary surface from Git to VCS (import/export provide the manual path).
- O7 — Changeset signatures/signing, LFS-in-VCS large-file tiering (all blobs are CAS — large-file policy is quota-limited this cut).

## Open Questions

- OQ1 — Eager vs lazy computation policy per derived-data type (decided at domain-design with cost evidence).
- OQ2 — Rename-detection heuristics for import (exact `from/to` on write; Git import needs similarity detection) — domain-design.
- OQ3 — Object size ceiling and streaming thresholds for `objects` PUT (quota defaults at contract-design).
- OQ4 — Whether sparse-checkout filters also need directory-glob support beyond path prefixes (contract-design).
- OQ5 — GC cadence/retention policy for unreferenced objects (defaults following LFS GC precedent).
