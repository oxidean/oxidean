# CI/CD Pipeline Design — oxpilot-tools (U5)

Consumes: `nfr-design/*`, `team.md` (Way of Working, Testing
Posture), `contract-summary.md` (C1/C6/C7), `.github/workflows/
ci.yml`, `docs/TESTING.md`, `Makefile` (`rpc-gen`).

U5 changes **no CI jobs and no deployment topology** — the
pipeline contract is reused end-to-end. This records where U5
touches the existing lanes and what must land in the same change
to stay green.

## Build & test stages touched

| surface | what U5 adds | gate effect |
|---------|--------------|-------------|
| `job: rust-build` (nextest archive) | no new deps — hand-authored pattern matchers; serde_json/tokio/tracing only (TS-9) | unchanged supply chain |
| `job: api-rust` shards | catalog pin test (23 rows verbatim + `EntryKind` literal bindings), validate-walker unit tests, handler tests, proc-chain tests for `diff_{list,get,apply}` | workspace-wide archive replays — no `-E` filterset term needed |
| `job: db-dialects` | `0089` rides the already-registered `dialect_oxpilot` binary in the `-E` filterset (ci.yml) + the migration filename-parity test enforcing ×3 presence | existing leg, unchanged |
| `make rpc-gen` / `job: rpc-sync` | `oxpilot.diff_{list,get,apply}` client types must be generated + committed — Rust is the source of truth | existing job fails on drift |
| `job: rust-fmt`, `job: dead-code` (clippy/machete steps), `commit-lint` | new modules inherit workspace-wide | unchanged |

Zero new jobs ⇒ no `Main` ruleset required-status-checks edit.

## Deployment strategy

Unchanged rolling path (PR → `pr-<n>` → ephemeral env → squash →
`sha-<sha>` → staging → manual promote). U5 rides the same `api`
image — `images.yml` untouched.

**Ordering**: `0089` applies on api boot through the existing
migration runner, same append-only contract as `0085`–`0088`.
The column is additive — a new binary against a pre-0089
database waits on boot migration like every prior table; an old
binary against the migrated schema never reads `ops_json` and is
unaffected.

## Rollback procedures

1. `admin.oxpilot.settings_set{enabled_override:false}` / env
   flag — the catalog only surfaces inside an enabled oxpilot
   session (`invoke_call` never reaches it otherwise) and the
   user-side `diff_*` procs gate on `require_enabled`, so
   disabling oxpilot severs the entire unit with zero deploy.
2. Standard image repin — the executor is stateless; code
   rollback leaves the inert `ops_json` column (additive,
   append-only policy forbids down-migration).
3. Mid-apply leftovers need no machinery: a failed chain leaves
   a `pending` proposal row + a deletable
   `oxpilot/proposal-<id8>` branch — both operator-removable,
   neither touches the target branch.
