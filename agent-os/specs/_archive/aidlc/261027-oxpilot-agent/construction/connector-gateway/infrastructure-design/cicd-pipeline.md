# CI/CD Pipeline Design — connector-gateway (U6)

Consumes: `nfr-design/*`, `team.md` (Way of Working, Testing
Posture), `contract-summary.md` (C3, C7), `.github/workflows/ci.yml`,
`docs/TESTING.md`.

U6 adds **no CI jobs, no matrix values, no workflow files** — the
pipeline contract is reused end-to-end. This records where U6
touches the existing lanes and what must land in the same change to
stay green.

## Build & test stages touched

| surface | what U6 adds | gate effect |
|---------|--------------|-------------|
| `job: rust-build` / `api-rust` shards | connector unit + integration tests in `crates/oxidean-api-oxpilot/tests/` (C3 route smokes: pair/claim/report/heartbeat auth matrix; dequeue ordering; consent state transitions) | existing sharding absorbs |
| `job: db-dialects` (cross-dialect lane) | **NEW test rows** in the already-listed binary `crates/oxidean-db/tests/dialect_oxpilot.rs` (connector-table CRUD + migration parity) | existing lane, new rows |
| `db-dialects` nextest filterset | the lane's `-E` arg is a binary-id **inclusion** union — `binary_id(oxidean-db::dialect_oxpilot)` is already a term (ci.yml), so connector tests placed inside that shipped file run automatically. **Same-change constraint: connector dialect tests MUST land in `dialect_oxpilot.rs`** — a NEW test binary would never match the filterset and would vacuously skip on `api-rust` (no `DATABASE_URL`) = silent zero real-engine coverage | wiring documented in `docs/TESTING.md` job table |
| `make rpc-gen` / `job: rpc-sync` | U6 registers ~8 new `oxpilot.*` verbs in `PROCEDURES`+dispatch (`pairing_create`, `target_list`, `target_get`, `target_disconnect`, `target_revoke`, `grant_list`, `grant_get`, `grant_respond`) — C1 pins them on paper; they land new here. `rpc_gen.rs` emits per-proc client methods, so `check-rpc-sync.sh` (`git diff --exit-code` on `packages/api-client`) fails unless `make rpc-gen` output is committed in the same change — **unconditional wiring**, not "if codegen touched". Pin tests `procedures_list_matches_dispatch`/`all_procs_are_session_only` auto-cover the new procs; `classify_pat_proc` stays fail-closed `SessionOnly` | existing job enforces |
| `job: commit-lint`, `rust-fmt`, `dead-code` (clippy/cargo-machete/doc-warnings steps) | unchanged | unchanged |
| `job: web-octane` (`make web-lint`/`web-format-check` steps) | none — U6 is API+DB only | unchanged |

## Migration discipline

- One `x3` migration set (next number after the current max) adding
  the 4 tables — additive-only, never mutating existing tables.
- Cross-dialect parity is a CI gate via `db-dialects`: identical DDL
  semantics on SQLite/Postgres/MySQL — including the
  **partial-unique `verifier_hash` index** (NULL tombstones must not
  collide on any dialect) and `ON DELETE CASCADE` on session-owned
  FKs (AC1.5.2: grant/action rows removed on session delete).
- Indexes ride the same migration (NFR-S5/NFR3.13):
  `(target_id, state, created_at, action_id)` for the FIFO dequeue,
  `(owner_user_id)` for owner-scoped lists, equality lookups on
  `code_hash`/`verifier_hash` — no sequential scan on any
  documented path.
- The migration lands in `crates/oxidean-db` only — dialect-specific
  SQL never leaks into domain crates (repo rule).

## Testing documentation (same change)

- `docs/TESTING.md` gains the connector-dialect row: how to invoke
  the new dialect test surface locally, which CI lane runs it, and
  the `--test-threads=1` shared-`DATABASE_URL` caveat already
  established for the dialect binary.
- New env vars: none — so `.env.example` and `docs/CONFIGURATION.md`
  need no variable entries; CONFIGURATION.md gains only the
  connector-egress paragraph noted in the questions.

## Deployment strategy

Unchanged rolling path (PR → `pr-<n>` → ephemeral env → squash →
`sha-<sha>` → staging → manual promote). U6 rides the same `api`
image — `images.yml` untouched.

**Ordering**: additive migration only — a new binary against a
pre-U6 database fails only when connector paths are exercised;
owner procs already refuse `oxpilot.disabled` per U4's flag.
Connectors can only exist after pairing, so there is no pre-existing
connector fleet to break.

## Rollback procedures

1. `admin.oxpilot.settings_set{enabled:false}` (wire field; lands
   in the `enabled_override` column) / env flag —
   C3 routes and owner procs all resolve enablement and return
   `{error:"disabled"}` / proc error; nothing new is admitted.
2. Flag-off is a work-stop lever on **live** connectors, not just
   admission (BR2.5): all four C3 routes fail closed with
   `{error:"disabled"}` so queued work cannot execute while the
   feature is nominally off. It tombstones no state — verifier
   hashes, targets, grants, and queued actions persist and
   re-enablement resumes the fleet. State teardown belongs to the
   explicit levers `target_disconnect` and `target_revoke`
   (the pinned C1 procs), not the flag. Both levers are idempotent:
   on an already-terminal target they answer `{ok}` — no
   "already dead" vocabulary exists (BR6.7).
3. Migration is additive — rollback to the pre-U6 binary leaves the
   tables orphaned but harmless; standard posture, no down-migration
   required by team policy.
4. Single-replica boundary (NFR-S2 caveat stated not hidden; NFR-S6
   no cross-node coordination) is a deployment-shape constraint, not
   a rollback lever — documented in
   `infrastructure-specification.md`.
