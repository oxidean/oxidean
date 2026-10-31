<!-- INVARIANT: examples are single-line HTML comments so a fresh template parses to total=0 (MEMORY_EMPTY). Do NOT un-comment or split across lines. t100 guards this. -->
> This file is kept up to date automatically while the stage runs. Add observations at the review step, not by editing here directly.

## Interpretations
<!-- example: ts — chose REST over GraphQL; the consuming team only needs CRUD, revisit if subscriptions land -->
2026-10-18T07:00:00Z — modeled the flag gate as TWO cases after design review: absent row (never-enabled) denies everything incl. vcs.status; enabled=false denies writes only (AC1.2.1). One flat "deny all" would have broken "history stays readable"
2026-10-18T07:00:00Z — ObjectEntry is a GLOBAL CAS row + ObjectLink per-repo rows; referenced-ness computed at GC by join (LFS self-healing model) rather than a stored refcount that drifts under cascade deletes
2026-10-18T07:00:00Z — crate ships cargo feature split: client (default: WorkingCopy+CliSurface, no DB/git deps) vs server (stores+interop) so ox doesn't transitively compile sqlx/oxidean-git

## Deviations
<!-- example: ts — skipped the optional caching layer the stage prose suggested; the dataset is small enough that it adds risk -->
2026-10-18T07:00:00Z — no new AppState limiter for the VCS surface: FailedAuthLimiter + DefaultBodyLimit + quota bound it; a new per-process limiter would diverge from single-node precedent

## Tradeoffs
<!-- example: ts — picked TDD over BDD this run; the team is unit-first and the domain is well-understood -->
2026-10-18T07:00:00Z — in-app /api/ mount over aux listener: zero edge-parity work, auth/CORS/wire-version reused; gives up process isolation the surface doesn't need (flag gate is the kill-switch)
2026-10-18T07:00:00Z — incremental index inside the create tx (no consistency window, reads O(index)) costs index-write latency per commit + upsert semantics needed for concurrent same-parent children (AC2.3.1)
2026-10-18T07:00:00Z — export pays O(repo) temp clone per call via the hook-driven fast_forward_ref/force_update_ref path; bare update_ref rejected (bypasses hooks, clobbers concurrent pushes)

## Open questions
<!-- example: ts — confirm the retention window with compliance before the next stage hardens the schema -->
2026-10-18T07:00:00Z — insert_repo_activity is push-shaped (required ref_name/before_oid/after_oid/commits_count): refless vcs ops need nullable push columns or a generalized signature — contract/functional design owns the op→row mapping + nullability call
2026-10-18T07:00:00Z — VALID_PUSH_TYPES is private to oxidean-api and gates the activity filter: vcs.* kind constants land in oxidean-core; host extends the allow-list from them
2026-10-18T07:00:00Z — still parked for contract-design: merge-conflict payload fields beyond {path,kind}, vcs.diff pagination/boundedness, vcs.status frozen field set, whether vcs_changeset_get ships as an MCP tool, import commits-per-call cap, vcs.file_get inline-size cap (bulk bytes stay on the objects GET route)
2026-10-18T07:00:00Z — ReadIndex↔ChangesetStore broke its would-be cycle by having ReadIndex read changeset rows at the table level through VcsPersistence; the only component edge is the one-way create-tx hook ChangesetStore→ReadIndex. Rule of thumb: same-crate components may read each other's tables via the façade but only one direction may hold a component-level dependency
