# Bolt Plan — Agentic VCS Protocol

A **Bolt** is one build pass over a piece of the work, ending in something that runs. Five Bolts, ordered by the Unit DAG — each Bolt's dependencies land in an earlier Bolt, so the sequence is a path through `unit-of-work-dependency.md`, not a reordering of it. Construction runs unit-major and serial (recorded defaults); the per-Bolt groupings record which units are parallel-capable without exercising it.

Walking-skeleton marker: none — skeleton is affirmed OFF for this intent (`team.md`), but the ordering still front-loads a thin end-to-end slice as an ordinary sequencing decision: B2 ends at an in-crate changeset round-trip and B4 is the first wire-visible proof.

## B1 — wire-spine

- **Units:** U1 `vcs-wire-foundation`, U2 `vcs-persistence`
- **Definition of Done:** `oxidean-vcs` crate skeleton (client/server features) in the workspace; all VCS wire DTOs + the 17-procedure name set + `proc_classes` tier entries in `oxidean-core`; `vcs.*` activity-kind constants; migrations ×3 (postgres/mysql/sqlite, filename parity) for flag/object_entry/object_link/changeset/file_change/ref/import_link/index tables; `VcsPersistence` façade compiling with `skipped()` → `db.not_configured` everywhere; `insert_repo_activity` push columns nullable per C4 with existing call sites migrated.
- **Confidence hypothesis:** the `insert_repo_activity` signature generalization (nullable push columns) lands without breaking existing push-activity consumers — the one change here that touches shared code paths.
- **Expected demo:** `make test` green; new façade methods exercised by unit tests against sqlite; `classify_pat_proc` returns the three-tier classes for all 17 names.

## B2 — changeset-core

- **Units:** U3 `vcs-object-store`, U4 `vcs-changeset-core`
- **Definition of Done:** sha-sharded CAS object store with `put`/`get`/`verify` + `ObjectLink` accounting; GC job honoring `OXIDEAN_VCS_GC_GRACE_SECS` (PUT→commit window) and sweeping stale `*.tmp`/lock files; `ChangesetStore` create (incl. `blob`/`content_b64` transports, `old_path` renames, idempotent resubmit) inside one tx that also upserts `ReadIndex`; `RefRegistry` CAS with `ref_conflict{actual}` and non-idempotent retry; derived queries log/tree/diff/blame/path_history/merge_base over the index.
- **Confidence hypothesis:** the create-tx read-index update is race-free under concurrent same-parent commits, and the GC grace window actually protects the PUT→commit gap — validated by a concurrent-create test and a grace-window GC test.
- **Expected demo:** crate-level round-trip — commit a changeset with mixed blob/inline changes, read back manifest/log/tree/diff/blame; CAS retry after a landed update fails `vcs.ref_conflict`.

## B3 — history-algebra

- **Units:** U5 `vcs-merge-engine`, U6 `vcs-git-interop`
- **Definition of Done:** merge producing clean `changeset_input` / typed `Conflicts[{path,kind,detail?}]` / `NoAncestor` / descendant fast-path (a-or-b); single-changeset rebase per `{changeset_id,new_parent_id}`; `import_chain` (cursor, ≤1000/call, `vcs.import_state` on bad cursor/concurrent run) writing id-convergent changesets; `lazy_import` on git-oid read miss; `export` via `ProtectionChecker` verdict → hook-driven `fast_forward_ref`/`force_update_ref` only.
- **Confidence hypothesis:** diff3-text/file-level conflict granularity is sufficient for agent flows — falsified if real merges produce conflicts the pinned `{path,kind}` shape can't express.
- **Expected demo:** fixture repo — import a git chain, merge divergent changesets clean and conflicting, rebase one changeset onto a new parent, export head to a branch and observe the protection verdict path (deny on protected, fast-forward on clean).

## B4 — api-surface

- **Units:** U7 `vcs-api-surface`
- **Definition of Done:** all 17 `vcs.*` procedures dispatched on `/api/rpc` (+`/api/rpc/ws`) and advertised in `system.manifest`; `VcsApi` flag gate enforcing the two-case semantics (absent row denies all incl. `vcs.status`; `enabled:false` denies writes only); object `GET`/`POST /api/vcs/…` routes with PAT/OAT-only auth (cookies denied), `413 vcs.quota_exceeded` reachable (body limit above the object cap), `Cache-Control: immutable` on GET; `repo_activity` rows written per C4 for every mutating op.
- **Confidence hypothesis:** the whole protocol is exercisable over wire endpoints with a PAT — this is the first externally visible proof; if the seam wiring (auth context, flag gate, route registration) is wrong it shows up here before client surfaces exist.
- **Expected demo:** live server — `vcs.repo_enable`, `vcs.status`, object POST/GET round-trip, `vcs.changeset_create` + `vcs.log` over `POST /api/rpc` with a PAT; cookie-only object GET denied `401 vcs.unauthenticated`.

## B5 — agent-surfaces

- **Units:** U8 `vcs-cli`, U9 `vcs-mcp`, U10 `vcs-web-surfaces`
- **Definition of Done:** `ox vcs` 16 commands (`enable status commit log diff show cat blame history merge-base merge rebase checkout ref import export`) rendering `Out::Rpc`/`Out::Report` correctly with `vcs.unauthenticated`/`vcs.forbidden` → exit 3; `WorkingCopy` checkout + sparse-filter fetch over the C2 routes; 13 MCP tools thin-mapping with Access gating; settings toggle (`vcs.status` read + `vcs.repo_enable` mutation, never-enabled → unchecked default) and `vcs.*` activity rows rendering with correct copy/links.
- **Confidence hypothesis:** the agentic throughput claim — an agent commits, reads back, and diffs entirely over RPC without a local checkout — is demonstrated by the CLI/MCP round-trip.
- **Expected demo:** `ox vcs` end-to-end against a live stack — enable, commit, log, diff, blame, checkout --paths subset; MCP tool calls return identical data; the settings toggle flips the flag and `vcs.*` activity rows appear in the repo feed.

## Sequencing notes

- B1→B2→B3→B4→B5 is a valid topological path: every unit's `depends_on` lands in an earlier bolt (U3→U2, U4→U3, U5/U6→U4, U7→U2..U6, U8/U9/U10→U7).
- Within-Bolt parallel pairs (U5∥U6; U8∥U9∥U10) build serially under the recorded `serial` execution but share one DoD/gate.
- Per-Bolt ownership: every Bolt → `aidlc-developer-agent` (team-formation was SKIP under classic scope; see `team-allocation.md`).
