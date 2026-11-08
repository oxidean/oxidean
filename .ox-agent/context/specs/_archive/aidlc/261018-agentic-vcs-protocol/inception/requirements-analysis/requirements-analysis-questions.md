# Requirements Analysis — Questions

Mode note: this stage ran under the user's standing instruction — "skip human verification until we have a sound implementation on a PR with green CI and reply to questions that might arise yourself through recommendation based on the document I shared." The document is https://www.anthropic.com/engineering/redesigning-source-control (Scion). Each answer records the conductor's recommendation, its rationale, and the article/repo evidence it rests on; the user ratifies or overturns any of them at PR review.

## Q1. Where does the protocol surface live?

Context: the intent says "server, cli and mcp side logic should live together inside its own crate." The repo's established seams are: typed JSON-RPC on `/api/rpc` (306 procedures, five-place registration), the in-process russh SSH sidecar pattern for parallel listeners, and `oxidean-web`'s `API_PREFIXES` where `/api/` already proxies everything below it — a `/api/vcs/…` prefix needs zero edge-parity changes, while a top-level `/vcs/` or `/{o}/{r}.vcs` prefix would need Axum + Traefik + Caddy + proxy updates and must dodge the greedy `/{o}/{r}.git` matcher.

- A. `vcs.*` typed-RPC family on `/api/rpc` for operations/metadata + `/api/vcs/…` Axum routes for content-addressed object transfer — all under the existing `/api/` prefix so Traefik, Caddy, and the web proxy need no new entries
- B. Dedicated sidecar listener on its own port (the SSH precedent)
- C. Top-level `/vcs/…` prefix outside `/api/`
- D. REST `RouteDef` facade only
- X. Other (please specify)

[Answer]: A — `vcs.*` RPC procedures + `/api/vcs/…` object routes. RPC dispatch gives auth, typing, `proc_classes` gating, generated client, and MCP bridging for free; object bytes ride small dedicated routes because immutable content-addressed GETs are the cache-friendly shape the article's want-based protocol wants. Staying under `/api/` removes the entire edge-parity surface from this change. Conductor-decided under standing instruction (2026-10-18); ratify at PR review.

## Q2. What is the Git interop model?

Context: the intent says "runs alongside it on the server for interop" — both surfaces must coexist on one repo identity. The article's lesson: changesets and git commits are different shapes (changed-file list + blob pointers vs full tree pointer), so mapping is mechanical in both directions but the sync policy is a product choice.

- A. Side-by-side per repo — a per-repo enable flag turns on the VCS surface for an existing repo; git commits are imported into changesets (parents mapped, changed-file lists computed from diffs); VCS changesets are exportable to git commits on demand (server materializes the commit tree into the bare repo and moves a git branch). Both directions exist; neither is automatic mirroring
- B. New VCS-only repo type with no git path
- C. Read-through index over git objects — the VCS surface can query but all writes stay git
- D. Continuous bidirectional mirror
- X. Other (please specify)

[Answer]: A — side-by-side per repo with explicit import/export. The user's words demand both directions available; continuous mirroring creates divergent-history hazards git can't reconcile cleanly, and a read-only index fails the "improved agentic throughput" goal (writes are the bottleneck the changeset model removes). Conductor-decided under standing instruction (2026-10-18); ratify at PR review.

## Q3. Which operations does the first cut support?

Context: Scion's surface includes changeset creation, want-based object fetch, diffs, merge bases, blame, path history, ancestry traversal, sparse/virtualized checkouts, and CAS'd refs — plus per-directory ACLs and derived-data machinery. A first cut must pick the subset that delivers "agentic efficiency and throughput" without boiling the ocean.

- A. Core set — enable/init per repo, server-side changeset create, want-based object fetch, tree/file/diff/log/blame/path-history reads, merge-base, server-side merge + rebase, sparse checkout materialization, ref create/list/CAS, git import + export. Deferred: virtualized (on-demand-materialize) checkouts, per-directory ACLs, merge queues, multi-tier cache daemons, multi-region
- B. Metadata-only — queries over imported git data, no VCS-native writes
- C. The full Scion surface including virtualized checkouts and per-directory ACLs
- D. Not yet defined
- X. Other (please specify)

[Answer]: A — the core set. Writes are the whole point (cheap lock-free server-side commits are the headline benefit); reads without writes would strand the crate. Virtualized checkouts, per-directory ACLs, and tiered daemons are scale-out machinery Oxidean's single-node self-host model doesn't need yet — the data model stays compatible so they can land later. Conductor-decided under standing instruction (2026-10-18); ratify at PR review.

## Q4. What is the working-copy model?

Context: the article's clients keep a bounded local object cache and lay out sparse checkouts; virtualized checkouts materialize files on demand. Oxidean's `ox` CLI is a thin RPC client with no repo state today.

- A. Sparse-by-default — `ox vcs checkout` materializes only requested path filters; the client keeps a bounded content-addressed object cache and fetches trees/blobs it wants (never negotiates what it has); full checkout is a degenerate sparse filter. Virtualized checkout is deferred
- B. Full checkouts only in v1
- C. Virtualized checkouts in v1
- D. Not yet defined
- X. Other (please specify)

[Answer]: A — sparse-by-default with a bounded object cache. It matches the want-based fetch (the client simply doesn't request trees it doesn't want), needs no FUSE/filesystem machinery, and is what an agentic CI workload actually uses. Conductor-decided under standing instruction (2026-10-18); ratify at PR review.

## Q5. What is the refs and concurrency model?

Context: the article is explicit — refs are discouraged, changeset creation never touches them, and moving a ref is a CAS on a single row separate from the write path. Oxidean's analogue is a per-repo refs table with optimistic CAS.

- A. Minimal refs — changesets are created ref-free and instantly fetchable; a small refs namespace (e.g. `main`, named heads) moves via compare-and-swap on `(repo, name, expected)`; nothing in the commit path requires ref coordination; concurrent changesets on the same parent are always accepted (forking is normal, resolution is a later merge)
- B. Ref-locked commits — creating a changeset on a ref requires holding it
- C. Git-style full ref namespace ported over
- D. Not yet defined
- X. Other (please specify)

[Answer]: A — lock-free commits with CAS'd minimal refs. This is the article's core throughput property: the commit path must scale with change size and never serialize on global state. Conductor-decided under standing instruction (2026-10-18); ratify at PR review.

## Q6. How do machine callers authenticate and get authorized?

Context: the affirmed team rule (practices-discovery, this intent) mandates PAT/pubkey-class credentials on transport surfaces and no session cookies — the D-12 precedent. `vcs.*` RPC procedures additionally need explicit `proc_classes` entries (the fail-closed default is SessionOnly). Repo permissions already exist (read/write/collaborator).

- A. `/api/vcs/…` object routes accept PAT Bearer/Basic and OAuth access tokens — never session cookies; `vcs.*` procedures get explicit `proc_classes` classes (read/write split mirroring repo permissions); PAT lookup reuses the existing sha256 path + `FailedAuthLimiter`; repo ACLs are the existing collaborator/permission model — no new credential type, no per-directory ACLs this cut
- B. New dedicated protocol token type
- C. Session-cookie auth on all surfaces for web parity
- D. Not yet defined
- X. Other (please specify)

[Answer]: A — reuse the PAT machinery and repo permissions verbatim; register `vcs.*` classes in `proc_classes` (fail-closed means forgetting this silently breaks PAT clients). Per-directory ACLs are deferred with virtualized checkouts. Conductor-decided under standing instruction (2026-10-18); ratify at PR review.

## Q7. Where does the data live?

Context: the affirmed boundary rule puts dialect SQL exclusively in `oxidean-db` (migrations ×3: postgres/mysql/sqlite, filename-parity tested), and the LFS/packages pattern gives content-addressed byte storage with quotas and GC. `Database::skipped()` is a supported no-DB boot mode.

- A. Metadata in `oxidean-db` — changesets, changed-file entries, blob index, refs, and derived-data rows via a `vcs` façade module + ×3 migrations; blob bytes in a content-addressed directory (`OXIDEAN_VCS_OBJECTS_DIR`, `{aa}/{bb}/{sha256}` sharding, refcount + GC job like LFS/packages); all VCS surfaces fail closed `db.not_configured` under `Database::skipped()`
- B. Crate-local storage engine (own DB pool / own files) independent of `oxidean-db`
- C. Store everything — including blob bytes — in the database
- D. Not yet defined
- X. Other (please specify)

[Answer]: A — metadata through the façade, bytes in a CAS directory. The mandated dialect boundary makes B a violation; C punts the scale problem the article is solving into the DB. `Database::skipped()` fails closed — VCS is a persistence feature by definition. Conductor-decided under standing instruction (2026-10-18); ratify at PR review.

## Q8. What does the one crate own, given dispatch/CLI/MCP live in existing crates?

Context: the intent requires server+cli+mcp logic co-located in one crate that becomes an internal dependency. The repo's dispatch lives in `oxidean-api/src/rpc.rs`, the CLI in `oxidean-cli`, MCP in `oxidean-api/src/mcp.rs`. The established seam is trait+adapter (`Arc<dyn GitBackend>`, `Arc<dyn EmailSender>`).

- A. `crates/oxidean-vcs` owns the engine + protocol logic: changeset/blob/derived-data domain code, the `vcs.*` handler functions (called from `dispatch` arms), the `/api/vcs/…` route handlers, the `ox vcs …` command implementations (called from the CLI's dispatch), and the MCP tool definitions + builders (called from `tool_defs()`). The host crates stay thin integration shells — `oxidean-api` mounts routes and adds dispatch arms, `oxidean-cli` parses argv and calls in, `oxidean-db` owns the dialect SQL the crate consumes. The crate exposes a trait object (`Arc<dyn VcsEngine>`) on `AppState` mirroring `GitBackend`
- B. The crate is storage/engine only; all handlers live in `oxidean-api` and all CLI code in `oxidean-cli`
- C. The crate owns entire binaries — its own server binary and its own CLI
- D. Not yet defined
- X. Other (please specify)

[Answer]: A — the crate owns the domain and protocol logic end-to-end; hosts wire it in. This matches both the user's "logic lives together in one crate" and the repo's trait+adapter idiom: `rpc.rs` gets thin `vcs.*` arms that delegate to crate handlers, `oxidean-cli` gets a thin `vcs` command branch, `mcp.rs` consumes crate-provided tool defs. B scatters the logic the user asked to co-locate; C breaks the single-binary deploy model. Conductor-decided under standing instruction (2026-10-18); ratify at PR review.

## Q9. What is the MCP surface?

Context: the existing endpoint is streamable-HTTP JSON-RPC in JSON mode with 20 thin tools mapping to RPC procedures behind `Access` gates (`RepoRead`/`RepoWrite`/`Identity`), gated by `OXIDEAN_MCP_ENABLED` + `admin.mcp.*`. Agentic callers are the primary audience for this whole feature.

- A. Mirror the core set — `vcs_status` (repo/head state), `vcs_log`, `vcs_diff`, `vcs_file_get`, `vcs_blame`, `vcs_path_history`, `vcs_changeset_create`, `vcs_refs_list`, `vcs_checkout` — each a thin map to its `vcs.*` procedure behind `RepoRead`/`RepoWrite` gates; domain denials surface as `isError` tool content
- B. Read-only MCP tools this cut
- C. No MCP tools this cut
- D. Not yet defined
- X. Other (please specify)

[Answer]: A — full core tool set including writes. MCP is the agentic interface — an agent that can read but not commit through it doesn't get the throughput benefit. `RepoWrite`-gated tools already exist (`issue_create`, `workflow_create`), so the pattern is proven. Conductor-decided under standing instruction (2026-10-18); ratify at PR review.

## Consolidated Summary Confirmation

Summary of confirmed answers:

- Surface: `vcs.*` typed-RPC procedures + `/api/vcs/…` object-transfer routes — entirely under `/api/`, zero new edge-parity work.
- Interop: side-by-side per repo (enable flag); git→changeset import and changeset→git export, both explicit — no mirroring.
- v1 scope: changeset create, want-based fetch, reads (tree/file/diff/log/blame/path-history/merge-base), server-side merge+rebase, sparse checkout, CAS'd refs, git import/export. Deferred: virtualized checkouts, per-dir ACLs, merge queues, tiered daemons.
- Working copy: sparse-by-default + bounded object cache; full checkout = degenerate filter.
- Refs: minimal namespace, CAS on `(repo, name, expected)`, never touched by commit creation.
- Auth: PAT/OAT on `/api/vcs/*` (no cookies); explicit `proc_classes` for `vcs.*`; existing repo ACLs; existing rate limiter.
- Storage: metadata via `oxidean-db` façade (migrations ×3) + CAS blob dir (`OXIDEAN_VCS_OBJECTS_DIR`, sha256, refcount+GC); `skipped()` fails closed.
- Crate: `oxidean-vcs` owns engine + handler + command + tool logic; hosts (`oxidean-api`, `oxidean-cli`) are thin wiring; trait object on `AppState`.
- MCP: full core tool set incl. writes, `RepoRead`/`RepoWrite` gated.

All nine answers are conductor decisions under the standing continuation instruction — each carries rationale and is open to reversal at PR review.
