# Domain Design — Architecture Decision Records

## ADR-001: Decompose `oxidean-vcs` into bounded components, not one engine

- **Context** — the crate carries server procedures, CLI logic, and MCP descriptors spanning storage, lineage, merge, interop, and client materialization — concerns with different change rates and correctness domains. A monolithic `VcsEngine` would blur entity ownership and make the highest-risk invariants (CAS refs, canonical hashing, protection path) hard to test in isolation.
- **Decision** — eight bounded engine components (VcsApi, ObjectStore, ChangesetStore, RefRegistry, ReadIndex, MergeEngine, GitInterop, WorkingCopy) plus thin adapters (CliSurface, McpSurface, ActivityRecorder), a persistence module (VcsPersistence), and one web component (VcsWebSurfaces — the settings toggle plus the activity-feed consumer side). Entities have exactly one owner each.
- **Consequences** — +each seam is independently testable (fake ObjectStore, stub ProtectionChecker); clear ownership; catalogue maps 1:1 to story coverage. −more internal boundaries to keep honest; cross-component transactions (create + index update) must ride one DB transaction through VcsPersistence.
- **Alternatives Rejected** — single-engine option (Option A in the boundary block): simplest to write, but merges concerns whose failure modes differ; rejected on change-rate and ownership-clarity grounds.

## ADR-002: Wire types and `proc_classes` entries live in `oxidean-core`

- **Context** — `vcs.*` procedures need request/response DTOs, typed error codes, and `proc_classes` PAT-scope entries (fail-closed `SessionOnly` default). `oxidean-core` is the highest-fan-out leaf crate (api/cli/db/web all depend on it) and already owns `RpcRequest`/`RpcResponse`/`AppError`.
- **Decision** — all VCS wire DTOs + `vcs.*` entries in `proc_classes` live in `oxidean-core`; `oxidean-vcs` consumes them.
- **Consequences** — +generated client, CLI, API, and MCP all share one type source; PAT classification stays fail-closed beside existing entries. −`oxidean-core` grows; the crate's domain types (engine-internal) stay inside `oxidean-vcs` and must not leak onto the wire.
- **Alternatives Rejected** — types exported from `oxidean-vcs` and imported by core (inverts the dependency rule); per-surface duplicate DTOs (drift).

## ADR-003: In-app mount under `/api/` — no auxiliary listener

- **Context** — the ratified transport decision puts the whole surface under `/api/` (RPC procedures + `/api/vcs/{o}/{r}/objects` routes). `oxidean-api` already centralizes per-request PAT auth, `proc_classes` gating, CORS, and the wire-version handshake; `API_PREFIXES` already includes `/api/` so edge parity is free.
- **Decision** — `vcs.*` joins `PROCEDURES`/dispatch like any family; object routes mount in `router_with_state`. `AppState` gets an `Arc<dyn …>`-style VCS slot following the `GitBackend` precedent. No sidecar listener.
- **Consequences** — +zero new edge-parity surface; auth/wire-version/CORS reused wholesale; one deployable. −none significant; a dedicated listener was only attractive for isolation, which the fail-closed flag gate (`vcs.disabled`) already provides.
- **Alternatives Rejected** — SSH-style aux listener: duplicates auth/CORS/version plumbing and adds a port/deploy concern for no throughput benefit (the WAL model's speed comes from the data model, not the socket).

## ADR-004: Blob bytes on filesystem CAS; metadata in `oxidean-db`

- **Context** — changesets are metadata + manifest; blob payloads are large, immutable, content-addressed. Two precedents exist: `oxidean-db` façade tables ×3 dialects, and on-disk content dirs (`OXIDEAN_LFS_DIR`, `OXIDEAN_PACKAGES_DIR`) with sha-sharding, refcounts, and GC jobs.
- **Decision** — blob bytes under `OXIDEAN_VCS_OBJECTS_DIR/{ab}/{cd}/{hash}` (streamed, hash-verified, refcounted, grace-windowed GC via `jobs/mod.rs`); all metadata (changesets, file changes, refs, import links, derived index rows, surface flag, object entries) in `oxidean-db` tables ×3 dialects.
- **Consequences** — +DB rows stay small; want-based GETs stream from disk; backup/GC story matches LFS. −two storage substrates to keep consistent (the refcount + grace window is the bridge); operators get one more env dir to manage.
- **Alternatives Rejected** — all-in-DB blobs: BLOB churn inside pages, no streaming reads, heavier backups; object-store service (S3-class): v1 targets self-host single-node first — the FS dir keeps deployment parity with LFS.

## ADR-005: Derived data maintained incrementally on write, inside the create transaction

- **Context** — the article's model computes blame/path-history/graph as derived data, incrementally, independently scalable. Choices: on-read (walk chains per request), on-write (index rows updated in the changeset tx), or async job (eventual consistency).
- **Decision** — on-write incremental inside the same transaction: ChangesetStore's create path updates DerivedIndex rows from the parent delta (the changeset itself IS the delta). Reads hit index tables only.
- **Consequences** — +reads are O(index) and never "not yet indexed"; the index is internal so the strategy can migrate to a job later without wire churn. −commit latency carries index-write cost; bulk import pays index cost per changeset (acceptable — import is admin/explicit).
- **Alternatives Rejected** — on-read: per-request full-chain walks get worse with depth; async job: introduces a consistency window the API would have to expose.

## ADR-006: `ProtectionChecker` trait port for `git_export` — host supplies the verdict

- **Context** — `git_export` must return identical protection verdicts to `git push` (the human-trust invariant), but the protection hook lives in `oxidean-api` and `oxidean-vcs` must not depend on its host.
- **Decision** — `oxidean-vcs` defines `trait ProtectionChecker` (the port) and consumes `Arc<dyn GitBackend>` (sibling `oxidean-git`); `oxidean-api`'s dispatch arm injects the real checker + backend. Tests stub the port.
- **Consequences** — +dependency direction host→crate preserved; verdict path is literally the same code git push uses; CLI/tests can drive export with a stub. −one more trait to version; host must remember to wire it (dispatch table wiring is single-site).
- **Alternatives Rejected** — export logic hosted in `oxidean-api` calling the crate: splits the domain operation across crates and strands the CLI/MCP surfaces; callback-from-crate: the impossible-dependency shape already rejected in requirements.

## ADR-007: Client working-copy and CLI command logic inside `oxidean-vcs`

- **Context** — the mandate puts server + CLI + MCP logic in one crate. Sparse checkout is client-side (manifest filter → want-fetch → materialize); `ox vcs` needs flag/argv parsing, `Out::Rpc` handling, and the `--change` grammar.
- **Decision** — `WorkingCopy` (fetch planning, shared object cache, materialization, local typed errors) and `CliSurface` (command impls) live in the crate; `oxidean-cli` adds a dependency on `oxidean-vcs` and keeps only argv glue (`Command` variants, USAGE rows, `required_procedures` entries).
- **Consequences** — +logic shareable by future harnesses/MCP-host tooling; CLI stays thin per AC8.4.2; the escape hatch (`ox api vcs.*`) is unaffected. −cli compile time +1 dep; WorkingCopy must stay transport-agnostic (host injects the RPC client).
- **Alternatives Rejected** — CLI logic in `oxidean-cli`: duplicates materialization/cache logic and violates the one-crate mandate.

## ADR-008: All VCS persistence inside `oxidean-db`'s façade (`VcsPersistence` module)

- **Context** — hard boundary: only `oxidean-db` writes dialect SQL; migrations ×3 with filename parity; `Database` is `Clone`, taken by value, and `skipped()` fails closed.
- **Decision** — a `vcs` module in `oxidean-db` owns tables (surface flag, object entries + per-repo object links, changesets, file changes, refs, derived index, import links), row types, and façade methods; `oxidean-vcs` components call façade methods only. Repo-row cascades carry VCS data deletion; CAS objects are refcounted, never eagerly deleted. Two known constraints handed to contract/functional design: `insert_repo_activity`'s signature is push-shaped (required `ref_name`/`before_oid`/`after_oid`/`commits_count`) — refless VCS ops need nullable push columns or a generalized signature; and `VALID_PUSH_TYPES` in `oxidean-api` must extend from the shared `vcs.*` kind constants in `oxidean-core`.
- **Consequences** — +parity-tested migrations; `db.not_configured` propagates uniformly; no SQL escapes the boundary. −façade surface grows (the 4,941-LOC `lib.rs` hotspot is noted debt — new code goes in a module file, not the façade root).
- **Alternatives Rejected** — crate-owned store trait with its own SQL: violates the dialect boundary; per-component row access through oxidean-api: adds a hop the data path doesn't need.

## ADR-009: `ref` is the wire field for ref names; `vcs.status` shape is provisional

- **Context** — `vcs.ref_update` needs the repo `name` AND the ref name in one input (collision risk); `vcs.status`'s field set is under-specified by requirements (only "live + empty history" pinned).
- **Decision** — the ref field is `ref`, matching the `#[serde(rename="ref")]` repo-arg precedent. `vcs.status` ships a minimal provisional shape (`enabled`, counts, heads); contract-design owns the frozen schema.
- **Consequences** — +no JSON duplicate-key ambiguity; precedent-consistent naming. −contract-design must revisit status fields before the client template is generated.
- **Alternatives Rejected** — `ref_name`/`target_ref` custom names: diverges from the existing `ref` convention; nested `ref: {name}` objects: heavier than needed.

## ADR-010: Global CAS rows + per-repo link rows; referenced-ness computed at GC

- **Context** — FR1.5 pins shared content-addressed storage (the same blob serves many repos), while quota/accounting is per-repo and repo-delete cascades must not leak objects. A stored refcount drifts silently under cascade deletes (platform review finding; LFS solved the same shape with a link table + computed referenced-ness at GC time).
- **Decision** — `ObjectEntry` is a global row keyed by hash; `ObjectLink (repo_id, hash)` rows carry per-repo references, inserted in the committing changeset's transaction. GC computes referenced-ness by joining links (self-healing under cascade) — no stored counter. Grace window covers the PUT→commit gap; GC also sweeps row-less files/stale `*.tmp` PUTs and clears its lock file at boot (gaps cloned from the LFS implementation are closed here).
- **Consequences** — +shared CAS honored; cascade deletes self-heal; the PUT race is covered by grace window, not refcount timing. −GC runs a join over links (fine at single-node scale); import of many repos sharing blobs needs link rows per repo.
- **Alternatives Rejected** — per-repo object rows: breaks shared CAS, duplicates bytes; stored refcount: drifts on cascade, needs compensating decrements forever.

## ADR-011: `oxidean-vcs` ships a cargo feature split — `client` (default) vs `server`

- **Context** — `oxidean-cli` needs `WorkingCopy`/`CliSurface` but must NOT transitively compile sqlx×3 + `oxidean-git` (today it depends only on `oxidean-core`-level crates). Server-side components (stores, VcsPersistence consumers, GitInterop) need those deps.
- **Decision** — the crate defines two features: `client` (default; WorkingCopy + CliSurface + wire-type consumers — no DB/git deps) and `server` (stores + persistence + interop). `oxidean-cli` depends on `oxidean-vcs` with default features; `oxidean-api` depends with `server`.
- **Consequences** — +`ox` stays lean (no sqlx/`oxidean-git` compile weight); the cli→vcs edge mirrors cli→core. −feature plumbing to maintain; CI must build both feature sets (`cargo check -p oxidean-vcs --all-features` covers it).
- **Alternatives Rejected** — unconditional deps (drags server weight into `ox`); split crates `oxidean-vcs-client`/`oxidean-vcs` (premature — one crate, two features keeps the mandate's "one dedicated crate").
