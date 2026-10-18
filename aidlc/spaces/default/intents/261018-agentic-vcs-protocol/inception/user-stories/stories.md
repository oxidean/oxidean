# User Stories — Side-by-Side Agentic VCS Protocol

Format: `US{group}.{seq}` story ids; acceptance criteria `AC{group}.{story}.{seq}` tagged to requirement ids `[FRx.y]` / `[NFRn]` / `[Cn]`. Personas are defined in `personas.md` (P1 Agent, P2 Developer-Reviewer, P3 Forge Operator, P4 CI Integrator). Priorities use MoSCoW; the MVP boundary is formally decided at delivery-planning.

**Conventions:** refs are named heads — minimal-namespace pointers, not Git branches (canonical term "ref (named head)" on first use). `main` is the conventional default head name, not auto-provisioned — a fresh repo's ref list is empty until the first `vcs.ref_update` creates it. Where an AC says "typed error" the concrete `vcs.*` code is the one named in that AC or in the error taxonomy AC8.1.4; contract-design owns the full code manifest. "Materialize" means writing files to a local filesystem — `ox vcs checkout` carries no git-style working-copy state.

## G1 — Enablement and lifecycle

*INVEST: stories here are independently shippable toggles/lifecycle behaviors — each testable through one RPC call or repo state transition.*

### US1.1 — Enable the VCS surface on a repository (Must Have)

**As** Sana (Forge Operator), **I want** to enable the side-by-side VCS surface on an existing repo, **so that** agents can commit through it while Git keeps working.

- AC1.1.1 — `vcs.repo_enable` flips the stored flag and returns success; only repo admin/owner-authorized callers succeed (the procedure carries an admin-class `proc_classes` entry, not merely read/write). [FR1.1]
- AC1.1.2 — Immediately after enable, `vcs.status` reports the surface live with empty history (zero changesets); no Git import is required to start writing. [FR1.3]
- AC1.1.3 — VCS data is keyed to the repo's existing identity (`owner/name` → repo id); a repo serves Git, VCS, or both simultaneously. [FR1.2]

### US1.2 — Disable writes without losing history (Must Have)

**As** Sana (Forge Operator), **I want** to disable the VCS surface, **so that** new writes stop but stored history stays readable.

- AC1.2.1 — `vcs.repo_enable {enabled: false}` stops writes: every write-class procedure (`vcs.changeset_create`, `vcs.ref_update`, `vcs.merge`, `vcs.rebase`, `vcs.git_import`, `vcs.git_export` — the AC8.6.1 write tier) and object PUT fail with `vcs.disabled`; read procedures (`vcs.log`, `vcs.file_get`, object GET) still succeed. [FR1.4]
- AC1.2.2 — On a deployment with `Database::skipped()`, every VCS surface fails closed with `db.not_configured` — never partially works. [FR15.5]
- AC1.2.3 — On a repo where VCS was never enabled, `vcs.*` calls and `/api/vcs/…` object routes fail with `vcs.disabled` — identical to the disabled state (never-enabled and disabled are the same denial path). [FR1.4]

### US1.3 — Repository lifecycle carries VCS data correctly (Should Have)

**As** Sana (Forge Operator), **I want** repo delete/transfer/archive to handle VCS data properly, **so that** nothing orphans and shared objects survive while referenced.

- AC1.3.1 — Deleting a repo removes its VCS rows (changesets, refs, oid mappings) through the repo-deletion cascade; object bytes are NOT eagerly deleted — they drop out via refcount → FR3.3 GC. [FR1.5]
- AC1.3.2 — Transferring a repo preserves all VCS data (keys derive from repo id). [FR1.5]
- AC1.3.3 — An archived repo keeps serving VCS reads and denies VCS writes, matching the repo record's archival semantics. [FR1.5]

## G2 — Commit path (changesets and server-side writes)

*INVEST: each story is one commit-path behavior independently verifiable over RPC; concurrency stories are small and testable with two simultaneous calls.*

### US2.1 — Commit without touching a ref (Must Have)

**As** Ava (Agent), **I want** to create a changeset by supplying parent ids plus a changed-file list, **so that** committing never requires reading or moving a ref.

- AC2.1.1 — `vcs.changeset_create` accepts `{parent_ids, message, changed-file entries}` and returns a changeset id; no ref is read or moved. [FR2.1, FR7.1]
- AC2.1.2 — Blob contents arrive either as prior object PUTs (referenced by hash) or as inline contents in the request within the RPC body-size cap; the two transports are distinct request fields. [FR2.1, FR7.1]
- AC2.1.3 — Each file entry carries path, change kind (add/modify/delete/rename), mode, and blob hash; the changeset stores no full-tree pointer. [FR2.1]
- AC2.1.4 — Bootstrap is defined: with no parent known, `vcs.status` + `vcs.ref_list` yield a starting point (or an empty-parent root commit); the first `vcs.changeset_create` succeeds without a parent id. [FR2.3]
- AC2.1.5 — `vcs.changeset_create` naming a nonexistent parent id fails with `vcs.not_found`; the same typed not-found contract applies to `vcs.merge`/`vcs.rebase` inputs addressing nonexistent changesets — unknown ids on write ops are never silently accepted. [FR2.3]

### US2.2 — Safe retry after interruption (Must Have)

**As** Ava (Agent), **I want** identical re-submission to return the same changeset id, **so that** a dropped connection never produces a duplicate or ambiguous commit.

- AC2.2.1 — Resubmitting identical canonical content (same parents, same manifest, same author string, same message) returns the existing changeset id with no second row written. [FR2.2]
- AC2.2.2 — Two *concurrent* identical `vcs.changeset_create` calls both return the same id and exactly one row is written (unique-key race, distinct from AC2.3.1's divergent-content race). [FR2.2]
- AC2.2.3 — Server-assigned fields (sequence, wall-clock timestamp) do not participate in the id; two submissions at different times still converge. [FR2.2]

### US2.3 — Concurrent commits on one parent always succeed (Must Have)

**As** Ava (Agent), **I want** two agents to create children of the same parent simultaneously, **so that** forking is normal instead of a ref race.

- AC2.3.1 — Two concurrent `vcs.changeset_create` calls with the same parent id and different content both succeed with distinct ids. [FR2.3]
- AC2.3.2 — Each write is atomic at the stated boundary — changeset row + derived rows + any inline-content objects commit together or not at all; a fault-injected failure mid-write leaves no readable changeset. Objects already committed via earlier PUTs are unaffected (refcounted, GC-owned). [FR7.4]

### US2.4 — Server-side merge with machine-readable conflicts (Should Have)

**As** Ava (Agent), **I want** the server to merge two changesets, **so that** diverged agent work reconciles without a local checkout.

- AC2.4.1 — `vcs.merge` of two changeset ids computes merge-base, performs a 3-way merge, and creates a merge changeset recording ≥2 parents with its file list relative to the first parent. Merge granularity is pinned: line-level (diff3) content merge for text blobs modified on both sides; file-level conflicts for binary/mode/type/rename clashes. [FR2.4, FR7.2]
- AC2.4.2 — On conflict the response is a machine-readable conflict list `{path, kind}`; no partial or broken changeset is written. [FR7.2, FR7.4]
- AC2.4.3 — Edge cases are typed, not silent: no common ancestor → `vcs.no_common_ancestor`; merging an ancestor/descendant pair resolves to the descendant fast-path without inventing a merge node; rebase onto a descendant/cycle target fails typed. [FR7.2, FR7.3]

### US2.5 — Server-side rebase (Should Have)

**As** Ava (Agent), **I want** the server to replay a changeset's changed files onto a new parent, **so that** agent branches rebase without checkouts.

- AC2.5.1 — `vcs.rebase` given `{changeset_id, new_parent_id}` creates the replayed changeset; result preserves the changed-file set relative to its new first parent. [FR7.3]
- AC2.5.2 — Conflicts surface as the same machine-readable list shape as `vcs.merge`. [FR7.2, FR7.3]

### US2.6 — Rich file entries (Must Have)

**As** Ava (Agent), **I want** binary files, executables, symlinks, and renames representable in a changeset, **so that** real source trees round-trip.

- AC2.6.1 — Entries round-trip binary contents, executable bit, and symlink targets. [FR2.5]
- AC2.6.2 — A rename records `(from_path, to_path)` explicitly and reads back identically. [FR2.5]

### US2.7 — Agent writes appear in the activity feed (Should Have)

**As** Ravi (Developer-Reviewer), **I want** agent commits and ref moves to appear in the repo activity feed like pushes do, **so that** machine work is human-visible at the right granularity — and distinguishable from human work.

- AC2.7.1 — `vcs.changeset_create`, `vcs.merge`, `vcs.rebase`, ref create/update/delete, `vcs.git_import`, `vcs.git_export`, and repo enable/disable each record exactly one `repo_activity` row via `Database::insert_repo_activity`, attributed to the authenticated identity. [FR16.1]
- AC2.7.2 — Activity rows carry the actor identity and kind, so machine/agent bursts are filterable rather than indistinguishable from human pushes; object PUT emits no per-object row. [FR16.1]
- AC2.7.3 — VCS event kinds without an existing analogue extend the kind allow-list (`VALID_PUSH_TYPES` precedent) with `vcs.*` names; reads and `audit_events` stay untouched. [FR16.1, FR16.2]

## G3 — Object store and want-based fetch

*INVEST: each story is one transfer behavior on the `/api/vcs/…` object routes; fetch stories are independently testable with hash lists.*

### US3.1 — Upload objects once (Must Have)

**As** Ava (Agent), **I want** to PUT blob content and get its content hash, **so that** identical content is stored once and retried uploads are no-ops.

- AC3.1.1 — `POST /api/vcs/{owner}/{repo}/objects` returns the sha256 content hash; bytes are stored under `OXIDEAN_VCS_OBJECTS_DIR` (shard layout is a structural check, not a behavior). [FR3.1, FR11.2]
- AC3.1.2 — Re-uploading identical content is a deduplicating no-op returning the same hash. [FR3.2]
- AC3.1.3 — Uploads exceeding the configured limit fail with `vcs.quota_exceeded` (distinguished from repo-quota failures) — a typed error, never a truncated write. [FR3.2]

### US3.2 — Fetch objects by hash (Must Have)

**As** Kit (CI Integrator), **I want** to GET any blob by content hash, **so that** immutable content can be cached and served by any tier.

- AC3.2.1 — `GET /api/vcs/{owner}/{repo}/objects/{hash}` returns the exact bytes with `Cache-Control: immutable, max-age=31536000` semantics. [FR3.4, FR11.1]
- AC3.2.2 — An unknown hash returns `vcs.not_found`; stored bytes are hash-verified on write so mismatched content can never be served under a hash. [FR3.1, FR3.4]

### US3.3 — Want-based fetch, never have-negotiation (Must Have)

**As** Ava (Agent), **I want** to request only the objects I lack, **so that** fetches are minimal and any cache tier can answer them.

- AC3.3.1 — The protocol never asks the client to describe what it has: every fetch request names wanted object hashes, nothing else. [FR4.1]
- AC3.3.2 — From a changeset id the client resolves the changed-file manifest via `vcs.changeset_get`, checks its bounded local cache, and requests only missing hashes via the objects route. [FR4.2]
- AC3.3.3 — Transfers are per-object and resumable; a partial transfer or corrupt cache entry is detected by hash verify and never committed to the cache. [FR4.3]

### US3.4 — Sparse fetch by default (Must Have)

**As** Ava (Agent), **I want** to fetch only the blobs under my declared path filters, **so that** I never pull content I don't need — and the server never has to learn my filter.

- AC3.4.1 — Sparse selection is client-side: the client resolves the manifest via `vcs.changeset_get`, drops entries outside its path filters, and requests only the remaining blob hashes; unrequested blobs are never transferred. [FR4.4]
- AC3.4.2 — A full checkout is a degenerate filter (all paths), not a separate protocol mode. [FR4.4]

### US3.5 — Operator-bounded storage with GC (Should Have)

**As** Sana (Forge Operator), **I want** object quotas and garbage collection of unreferenced content, **so that** CAS storage stays bounded.

- AC3.5.1 — Objects are reference-counted from changesets; GC reclaims only objects that are unreferenced AND older than a configurable grace window — a blob PUT immediately before `vcs.changeset_create` can never be collected mid-flight. [FR3.3]
- AC3.5.2 — A changeset referencing an object created concurrently with a GC tick is never reclaimed (the refcount check and the changeset insert are ordered by the grace window + transaction). [FR3.3, FR7.4]
- AC3.5.3 — Repo and object size limits are configurable and enforced at write time with `vcs.quota_exceeded`. [FR3.2]

## G4 — History and derived data

*INVEST: each story is one query contract over the derived-data mechanism; lazily-computed types share the same contracts. Unknown/malformed identifiers are handled uniformly — see AC4.1.3.*

### US4.1 — Walk history without a clone (Must Have)

**As** Ava (Agent), **I want** `vcs.log` to return changeset ancestry, **so that** I can reason about history through the API.

- AC4.1.1 — `vcs.log` from a changeset or ref returns the ancestor walk (first-parent and merge structure) with per-entry `{changeset_id, parents, author, message, timestamp}` metadata. [FR5.2]
- AC4.1.2 — `vcs.log`, `vcs.path_history`, `vcs.tree`, and `vcs.blame` accept `limit`/`cursor` pagination — no unbounded responses on deep history or large trees. [FR5.1, NFR1]
- AC4.1.3 — Any query addressing a nonexistent changeset/path/ref returns `vcs.not_found`; a malformed identifier (unparseable, wrong shape) returns `vcs.invalid_input` — never an ambiguous empty success. The same contract holds on write ops addressing nonexistent changesets (AC2.1.5). [FR10.1]

### US4.2 — Diff and merge-base over the API (Must Have)

**As** Ava (Agent), **I want** `vcs.diff` and `vcs.merge_base`, **so that** change review and merge planning need no local objects beyond the diff itself.

- AC4.2.1 — `vcs.diff` between two changesets returns the file-level diff (per-path changes + content deltas). [FR5.1]
- AC4.2.2 — `vcs.merge_base` returns the merge-base changeset id or `vcs.no_common_ancestor`, without materializing full trees. [FR5.2]

### US4.3 — Path history and blame (Should Have)

**As** Ava (Agent), **I want** `vcs.path_history` and `vcs.blame`, **so that** I can attribute changes and follow a file's evolution server-side.

- AC4.3.1 — `vcs.path_history` returns which changesets touched a given path (paginated per AC4.1.2). [FR5.2]
- AC4.3.2 — `vcs.blame` returns per-line attribution for a file at a changeset; lazy computation from path history + diffs satisfies the contract. [FR5.2, FR5.3]

### US4.4 — On-demand tree materialization (Must Have)

**As** Ravi (Developer-Reviewer), **I want** `vcs.tree` to return a changeset's full file listing, **so that** I can enumerate contents before checkout or export — and so `vcs.diff` has its underlying derived type. (Must-priority because `vcs.diff`, sparse checkout, and `vcs.git_export` all consume this derived type — see Dependencies below.)

- AC4.4.1 — `vcs.tree` returns the complete path→entry listing for a changeset, computed on demand, cacheable by changeset id, paginated per AC4.1.2. [FR5.4]

### US4.5 — Point reads by id (Must Have)

**As** Ava (Agent), **I want** `vcs.changeset_get` and `vcs.file_get`, **so that** the single most common read — manifest or file bytes at a changeset — never needs a walk.

- AC4.5.1 — `vcs.changeset_get` returns the changeset record: ordered parents, author, message, server timestamp, and the changed-file manifest (path, kind, mode, blob hash per entry). [FR2.1, FR10.1]
- AC4.5.2 — `vcs.file_get` returns a file's bytes + mode at a given changeset+path; absent path → `vcs.not_found`. [FR10.1]

## G5 — Refs

*INVEST: two stories cover the entire deliberately-minimal ref surface; CAS semantics make each independently testable with two-call races.*

### US5.1 — Advance a ref atomically (Must Have)

**As** Ava (Agent), **I want** to move a ref (named head) via compare-and-swap, **so that** ref moves are explicit, race-safe, and never implicit in commits.

- AC5.1.1 — `vcs.ref_update` with `{name, expected_old, new}` succeeds iff the current value equals `expected_old`; a stale expectation returns `vcs.ref_conflict` and moves nothing. [FR6.2]
- AC5.1.2 — Two concurrent `vcs.ref_update` calls carrying the same `expected_old` yield exactly one winner; the loser gets `vcs.ref_conflict` and the ref equals the winner's target. [FR6.2]
- AC5.1.3 — CAS is intentionally non-idempotent across reconnects: a retried call after a landed first attempt fails `vcs.ref_conflict`; the documented client pattern is `vcs.ref_list` → re-CAS. [FR6.2]
- AC5.1.4 — Changeset creation never moves a ref; only the explicit op does. CAS checks `expected_old` only — no ancestry/fast-forward enforcement (non-FF moves are the caller's discipline this cut). [FR6.2]

### US5.2 — Manage the minimal ref namespace (Must Have)

**As** Ava (Agent) or Kit (CI Integrator), **I want** to list, create, and delete refs through the CAS contract, **so that** agents and automation manage their own per-task heads.

- AC5.2.1 — `vcs.ref_list` returns the repo's refs pointing at changeset ids; on a fresh repo it returns an empty list (`main` is created by the first `vcs.ref_update`, not implicitly). [FR6.1]
- AC5.2.2 — `vcs.ref_update` CAS semantics cover create (expected-absent) and delete (null target); a delete/move naming a nonexistent ref returns `vcs.not_found`. [FR6.3]
- AC5.2.3 — Diverged heads reconcile only via an explicit merge changeset (US2.4); no ref op silently merges. [FR6.3]

## G6 — Working copy

*INVEST: checkout stories are client-side behaviors verified on real filesystems in tests; Kit's path is the same — CI materializes via `vcs.tree` + object GETs or `ox vcs checkout`.*

### US6.1 — Sparse checkout to disk (Should Have)

**As** Ravi (Developer-Reviewer), **I want** `ox vcs checkout` to materialize only the paths I declare, **so that** I inspect agent work without fetching the whole repo.

- AC6.1.1 — `ox vcs checkout <changeset> --paths <filters>` writes only matching paths to disk; unrequested blobs are never fetched (the object store has no tree objects — selection is over blob entries). [FR8.1]
- AC6.1.2 — The materialized files' bytes hash-match the changeset's blob pointers. [FR8.1]
- AC6.1.3 — Failure modes are defined: an interrupted checkout leaves the target tree consistent per-file (temp-write-then-rename at file granularity); checkout into a non-empty/conflicting target dir fails typed rather than silently merging. [FR8.1]

### US6.2 — Bounded shared object cache (Should Have)

**As** Ravi (Developer-Reviewer), **I want** a bounded content-addressed cache shared across checkouts and repos, **so that** repeat materializations don't re-fetch.

- AC6.2.1 — A second checkout of an overlapping changeset fetches zero already-cached objects. [FR8.2]
- AC6.2.2 — The cache is keyed by content hash and bounded by a configurable size; a corrupted cache entry is detected by hash-verify on read and refetched. [FR8.2]

### US6.3 — Faithful file attributes (Should Have)

**As** Ravi (Developer-Reviewer), **I want** modes, executables, and symlinks preserved on checkout, **so that** a materialized tree is buildable.

- AC6.3.1 — Checked-out files carry standard modes, executable bit, and symlinks correctly on Linux and macOS; Windows behavior (symlink privilege, exec-bit mapping) is documented and degrades explicitly. [FR8.3]

## G7 — Git interop

*INVEST: import/export stories are explicit operations with observable mapping and protection behavior.*

**Compatibility invariant (constraint C3, Must):** enabling and actively using VCS on a repo must not change Git behavior — verified as an enumerated checklist, not a slogan: `git clone`/`fetch`/`push` round-trip, PR merge, release create, and web repo pages all succeed identically on a VCS-enabled repo. This is a regression gate on every other story, carried forward as release-blocking acceptance.

### US7.1 — Import Git history into changesets (Should Have)

**As** Sana (Forge Operator), **I want** an existing repo's Git history importable as changesets, **so that** the VCS surface can serve it without a flag day.

- AC7.1.1 — `vcs.git_import` creates one changeset per Git commit: parents mapped, changed-file lists computed from name-status diffs, contents stored under VCS content hashes; the `git_oid ↔ changeset_id` mapping is queryable. [FR9.1]
- AC7.1.2 — Lazy import (triggered on first `vcs.changeset_get`/`vcs.log` resolution of a Git oid the VCS hasn't seen) and explicit backfill converge on identical changeset ids; rename detection runs with pinned deterministic flags on both paths. [FR9.4]
- AC7.1.3 — Import preserves authorship, timestamps, messages, and ≥2-parent merge structure. [FR9.3]

### US7.2 — Export a changeset chain to a Git branch (Must Have)

**As** Ravi (Developer-Reviewer), **I want** to materialize a changeset chain into Git commits and move a named branch, **so that** agent work enters the normal protected review flow — this is the trust loop's only landing path and cannot wait.

- AC7.2.1 — `vcs.git_export` writes Git objects (blobs/trees/commits via the object-plumbing path, not N per-commit pushes) into the repo's bare repository and performs ONE branch move for the chain — explicit, never automatic mirroring. [FR9.2]
- AC7.2.2 — The branch move passes through the same protection verdict path as `git push`; protected-branch denials block the export exactly as they block a push. Export requires fast-forward on the target branch unless an explicit `force` flag is passed — same precondition the push path enforces. [FR9.2]
- AC7.2.3 — Export preserves authorship, timestamps, messages, and merge structure per commit (committer fields honored from the changeset, not rewritten to forge-identity/now). [FR9.3]
- AC7.2.4 — After export, `git fetch`/`log`/`show` on the moved branch read the exported commits normally; the compatibility invariant above still holds repo-wide. [FR9.2, C3]

### US7.4 — Interop fidelity and convergence (Should Have)

**As** Sana (Forge Operator), **I want** the bridge deterministic in both directions, **so that** history survives losslessly and ids never depend on who ran the import.

- AC7.4.1 — Round-tripped commits keep author (`Name <email>`), timestamps, and messages byte-identical. [FR9.3]
- AC7.4.2 — Non-platform git authors hash via the canonical `Name <email>` author string, so lazy and backfill imports converge on the same changeset id regardless of who triggered them. [FR2.2, FR9.4]

## G8 — Surfaces and authorization

*INVEST: surface stories are wire-level; registration ACs are enforced by the repo's own fail-closed coverage tests (marked "structural check").*

### US8.1 — The `vcs.*` RPC family (Must Have)

**As** Kit (CI Integrator), **I want** the full operation set available as typed `vcs.*` procedures on `/api/rpc`, **so that** one stable protocol serves every consumer.

- AC8.1.1 — The FR10.1 procedure set (`vcs.repo_enable`, `vcs.status`, `vcs.changeset_create`, `vcs.changeset_get`, `vcs.log`, `vcs.diff`, `vcs.tree`, `vcs.file_get`, `vcs.blame`, `vcs.path_history`, `vcs.merge_base`, `vcs.merge`, `vcs.rebase`, `vcs.ref_list`, `vcs.ref_update`, `vcs.git_import`, `vcs.git_export`) is callable over POST `/api/rpc` and the WS variant, with snake_case names. [FR10.1]
- AC8.1.2 — `system.manifest` advertises the family; the `Oxidean-RPC-Version` gate applies unchanged. [FR10.3]
- AC8.1.3 — All `/api/vcs/…` routes live under the existing `/api/` edge prefix — no Traefik/Caddy/proxy-table changes required. [FR11.3, C1]
- AC8.1.4 — The error taxonomy is enumerated at contract-design and wire-stable: `vcs.disabled`, `vcs.not_found`, `vcs.invalid_input`, `vcs.ref_conflict`, `vcs.conflict`, `vcs.no_common_ancestor`, `vcs.quota_exceeded`, `vcs.unauthenticated`/`vcs.forbidden` — all surfacing through the `AppError` envelope without internals. [NFR5, NFR8]

### US8.2 — Registration completeness (Must Have)

**As** Kit (CI Integrator), **I want** every advertised procedure present and typed in the generated client, **so that** the surface can't half-exist behind `system.manifest` claims.

- AC8.2.1 — Each `vcs.*` procedure appears in `PROCEDURES`, has a dispatch arm, has `oxidean-core` wire types, has a `proc_classes` entry, and the regenerated `@oxidean/api-client` contains it (structural check: `rpc-sync` parity + the coverage tests). [FR10.2]
- AC8.2.2 — `ox vcs` commands' procedures are in `required_procedures` and `CLI_PROCS`; `vcs_*` tools in `MCP_PROCS`; `OXIDEAN_VCS_*` vars are documented in `.env.example` + `docs/CONFIGURATION.md` (structural check: fail-closed coverage test + docs review). [FR10.2]

### US8.3 — MCP tools for agents (Must Have)

**As** Ava (Agent), **I want** `vcs_*` MCP tools mirroring the core procedures, **so that** MCP-connected agents get the full read/write protocol. (Elevated to Must beyond Q4's enumeration — MCP is the primary agent persona's surface; see Priorities rationale.)

- AC8.3.1 — Tools `vcs_status`, `vcs_log`, `vcs_diff`, `vcs_tree`, `vcs_file_get`, `vcs_blame`, `vcs_path_history`, `vcs_merge_base`, `vcs_changeset_create`, `vcs_merge`, `vcs_rebase`, `vcs_ref_list` each thin-map to the same-named `vcs.*` procedure. [FR13.1]
- AC8.3.2 — Tools are gated by existing `Access` classes (`RepoRead`/`RepoWrite`) plus `OXIDEAN_MCP_ENABLED`/`admin.mcp.*`; domain denials return `isError` tool content. [FR13.2]
- AC8.3.3 — `oxidean-vcs` emits tool descriptors (name/description/input-schema) which `mcp.rs` wraps with `Access` gating — the crate never references `oxidean-api`-private `ToolDef`/`Access` types (dependency direction stays host→crate; structural check). [FR13.3]

### US8.4 — The `ox vcs` CLI family (Must Have)

**As** Ravi (Developer-Reviewer), **I want** `ox vcs <subcommand>` for the core operations, **so that** the protocol is drivable from a shell.

- AC8.4.1 — Subcommands exist for: `enable`, `status`, `commit`, `log`, `diff`, `show`/`cat`, `blame`, `history`, `merge-base`, `merge`, `rebase`, `checkout`, `ref`, `import`, `export` — `enable` covers the disable path (`--disable` or the `{enabled}` flag matching `vcs.repo_enable`). [FR12.1]
- AC8.4.2 — Implementations live in `oxidean-vcs`; `oxidean-cli` holds thin argv-parse → call glue (structural check: dependency direction). [FR12.2]
- AC8.4.3 — `ox api vcs.*` remains the raw-RPC escape hatch; procedures appear in the generated client for TS consumers. [FR12.3]

### US8.5 — Headless authentication, no cookies (Must Have)

**As** Kit (CI Integrator), **I want** PAT/OAT auth on every VCS surface and never cookies, **so that** automation authenticates deterministically and safely.

- AC8.5.1 — `/api/vcs/…` object routes accept PAT (Basic/Bearer) and OAuth access tokens; a session-cookie-only request is denied. [FR14.1]
- AC8.5.2 — A valid PAT lacking repo permission on `{owner}/{repo}` is denied on object routes (repo ACL enforced, not just token validity); object PUT and requests carry the existing size/rate limits. (Structural check: PAT lookup reuses the sha256 path and `FailedAuthLimiter`.) [FR14.1, FR14.3, FR14.4]

### US8.6 — Fail-closed authorization (Must Have)

**As** Sana (Forge Operator) and Kit (CI Integrator), **I want** `vcs.*` procedures to carry explicit permission classes matching repo ACLs, **so that** nothing is accidentally callable and scoped tokens stay narrow.

- AC8.6.1 — Every `vcs.*` procedure has a `proc_classes` entry spanning three tiers — read (repo read: `vcs.status`, `vcs.changeset_get`, `vcs.log`, `vcs.diff`, `vcs.tree`, `vcs.file_get`, `vcs.blame`, `vcs.path_history`, `vcs.merge_base`, `vcs.ref_list`), write (repo write: `vcs.changeset_create`, `vcs.merge`, `vcs.rebase`, `vcs.ref_update`, `vcs.git_import`, `vcs.git_export` — export is write-class because it moves a Git branch through the same protection path a `git push` does), admin (`vcs.repo_enable`); a hypothetically-unlisted procedure resolves `SessionOnly` (fail closed). [FR14.2, FR1.1]
- AC8.6.2 — A read-class PAT calling `vcs.changeset_create` is denied; a read-class PAT on reads succeeds. [FR14.2]
- AC8.6.3 — Authorization reuses the repo permission model (owner/collaborator/read-write); no new credential type. [FR14.3]

## Cross-cutting notes

- **Won't Have (this intent)**: virtualized/materialize-on-read checkouts, per-directory ACLs, merge queues/auto-landing, multi-tier daemon/caches and multi-region, web-UI history views, repo-type conversion tooling, changeset signing and LFS-tiering — all deferred per requirements O1–O7.
- **Dependencies**: G2/G3/G4 stories depend on US1.1 (enablement); US4.4 underpins US4.2/US6.1/US7.2; G6 on G3 (fetch machinery); US7.2 on G2 merge semantics and the protection-hook path; G8 stories constrain the shape of every other group.
- **Priorities rationale**: the Must set is the smallest end-to-end slice that lets an agent write and read through the protocol AND lets a human land agent work through the protected Git flow. Elevations beyond the Q4 enumeration, all recorded per mob review: US7.2 (export is the trust loop's only landing path), US8.3 (MCP is the primary agent persona's surface — an MVP an agent can't reach isn't an MVP), and US8.4/US8.5/US8.6 (the protocol's shell and auth surfaces — without them the Must slice isn't callable or safe). Should items round out operability; Won't is O1–O7 verbatim.
- **Deferred structure-level requirements**: FR15.x (crate topology, AppState wiring, thin shells, registration) is intentionally not story-shaped — it is a design constraint verified structurally at domain-design/code-generation, not a user behavior. FR15.5 is split: its fail-closed clause is covered by US1.2 (AC1.2.2), its structural half (per-dialect migrations with filename parity, the `vcs` façade module + row types in `oxidean-db`) is deferred to domain-design with the rest of FR15.
