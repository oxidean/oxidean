**Collaborator:** aidlc-developer-agent

## Contribution

1. **blocking — US3.4 (AC3.4.1/AC3.4.2)**: "A fetch constrained to path filters returns only objects under those paths; unrequested subtrees are never sent" describes a server-side filtered-fetch operation that does not exist in the defined surface. FR11 routes are per-hash object GET/POST only; there is no tree/manifest procedure accepting a path filter (US4.4's `vcs.tree` has no filter param, `vcs.file_get` is per-path). Sparse fetch is only implementable *client-side* (resolve manifest → request wanted blob hashes). Either add a filter parameter to `vcs.tree`/`changeset_get` in the contract or rewrite the AC as client-side resolution; as literally stated it is unimplementable/vacuous.

2. **blocking — missing story (`vcs.changeset_get`, `vcs.file_get`)**: The manifest read that the entire want-based fetch protocol stands on (US3.3 AC3.3.2: "resolves changed-file entries and blob hashes") has no behavioral contract anywhere — it appears only in US8.1's registration list. `file_get` similarly has zero ACs (mentioned only in AC1.2.1 as "still succeeds"). The linchpin read procedures need response-shape ACs (manifest entries, parents, message; file bytes + mode at changeset).

3. **blocking — US2.4 (AC2.4.1)**: "performs a 3-way merge" hides outsized risk. Changesets store no tree pointer (AC2.1.2), so merge requires materializing three trees (base + both sides) *and* — if conflicts are content-level, not just file-level — a line-merge engine over blob contents. The AC doesn't state conflict granularity ("paths + kinds" suggests file-level, which is far cheaper). Pin the merge granularity explicitly; a full content-level merge engine is epic-scale and should not ride a Should-Have story unstated.

4. **blocking — US3.5 (AC3.5.1)**: Refcount-from-changesets GC has a correctness race: an object is uploaded via PUT, then referenced by `changeset_create` later — between those calls its refcount is zero and the periodic GC can reclaim in-flight content. Needs an AC for a grace window (mtime-age threshold, LFS-GC precedent) or a pending-reference/lease mechanism, else AC3.1.1's "PUT then commit" flow is unsafe.

5. **suggestion — US4.2 vs US4.4 (priority inversion)**: `vcs.diff` (Must) requires materializing trees on both sides — the `tree` derived type — yet US4.4 is Should. Same dependency for US6.1 checkout and US7.2 export. Promote `vcs.tree` to Must or record the dependency explicitly.

6. **suggestion — US7.2/US7.4 (export fidelity)**: `GitBackend` (oxidean-git/src/backend.rs:410) is CLI-backed; `commit_files` (:460) sets committer = forge identity/now and lands via push (hooks run — good for AC7.2.2). But AC7.4.1 requires preserving *committer timestamps* per commit and exporting a *chain* — that needs new plumbing (hash-object/mktree/commit-tree or fast-import + one ref move), and N separate `commit_files` pushes would fire N hook verdicts and N activity rows. AC should note single-branch-move semantics for a chain export.

7. **suggestion — US7.1 (import)**: "changed-file lists computed from diffs" needs `--name-status` output; `GitBackend::diff` (backend.rs:521) returns unified patches. Also AC7.1.2 convergence (lazy == backfill ids) is only implementable if rename detection is deterministic and identical on both paths — git rename detection is heuristic-flag-controlled; pin the flags. And "lazy import on first read" never names which read triggers it (changeset_get by git oid? log walk?).

8. **suggestion — missing story (typed error taxonomy)**: P4's core ask is "deterministic `vcs.*` error codes" (personas.md:44); no story/AC enumerates the code set (`vcs.disabled`, `vcs.conflict`, `vcs.ref_conflict`, `vcs.quota_exceeded`, `vcs.not_found`…) or pins wire stability. NFR5 covers stability generally; a code-list AC anchors contract-design.

9. **suggestion — US1.2/US8.4**: disable path is unnamed — procedure list has `vcs.repo_enable` (implying `{enabled: bool}`) and AC8.4.1 lists `enable` but no `disable` verb. Name the mechanism in an AC.

10. **suggestion — US4.x**: no pagination/cursor contract on `vcs.log`, `vcs.tree`, `vcs.path_history`, `vcs.blame` — unbounded responses on deep history/large trees violate the incremental-cost posture; add limit/cursor ACs.

11. **suggestion — US5.1**: CAS is non-idempotent across reconnects (a retried `ref_update` fails on `expected_old` even when the first attempt landed). Acceptable by design, but the agent persona's "retry safely" goal wants an explicit AC (e.g., documented read-then-compare pattern or server idempotency key).

12. **suggestion — US8.3 (AC8.3.3)**: `ToolDef`/`Access` are private to `oxidean-api::mcp` (mcp.rs:567,581) and the dependency direction is host→crate — oxidean-vcs cannot reference them. Producing defs in the crate means either moving the types to oxidean-core or having the crate emit name/description/schema tuples while mcp.rs keeps `access` gating. Name the seam so contract-design doesn't guess.

13. **suggestion — sizing**: 34 stories vs the ~20–26 granularity answered in Q3 — mild drift concentrated in G8 (6 stories). Acceptable, but flag since the answer bounds granularity.

14. **nit — US7.3 (AC7.3.1)**: "byte-for-byte as before" across push/fetch/PR/release/web is untestable as phrased; scope to named behaviors (clone/push round-trip, PR merge, release create).

15. **nit — US2.1 (AC2.1.1)**: bundles two transports ("blob uploads" are the HTTP objects route; "inline contents" ride the RPC body) into one AC, and inline contents need a size cap vs RPC body limits. Split the AC.

16. **nit — US6.1 (AC6.1.1)**: "unrequested trees are never fetched" — the object store has no tree objects, only blobs; rephrase as "unrequested blobs are never fetched."

## Positions

- **Wire-level, FR-tagged behavioral ACs (Q5-A)** — exactly what the fail-closed coverage tests and contract-design consume; keep the style verbatim.
- **AC8.6.1 fail-closed `SessionOnly`** — real mechanism exists (`proc_classes.rs:127` `_ => SessionOnly`, plus the `CLI_PROCS.iter().chain(MCP_PROCS)` coverage test at commands.rs:3048 and `procedures_list_matches_dispatch`); the story maps to enforced reality, must survive.
- **US2.2 idempotent-id excluding server-assigned fields** — content-hash id over canonical {parents, manifest, author, message} is the right, cheap convergence; keep AC2.2.2.
- **US5.1/US5.2 CAS-only ref surface** — small, race-safe, correctly keeps refs off the commit path; the expected-absent/null-target create+delete trick avoids extra procedures. Keep.
- **US2.7 activity-feed scoping** — one row per op, none per object PUT, kind allow-list extension; `insert_repo_activity` facade exists (lib.rs:4525) and host→crate direction is correctly preserved. Keep.
- **US8.5 PAT/OAT-only object auth** — `oxidean_pat_/fg_/oat_` prefix dispatch + shared `FailedAuthLimiter` precedent is real (mcp.rs:373, 446); keep "never cookies."
- **AC7.2.2 export-through-protection-verdict** — the hook path exists (`commit_files` pushes through `hooks/update` with `OXIDEAN_ACTOR_CAPABILITY`, backend.rs:447-471); explicit-never-mirror export is the right trust boundary. Keep.
- **AC1.2.2 `db.not_configured` fail-closed** — `Database::skipped()` is a real mode (lib.rs:117); keep as Must-level coverage for every VCS surface.
