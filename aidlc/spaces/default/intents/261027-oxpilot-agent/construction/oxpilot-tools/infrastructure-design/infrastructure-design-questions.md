# Infrastructure Design Questions — oxpilot-tools

Unit: `oxpilot-tools` (U5, kind `service` — embedded `api`) —
`OxpilotTools` tool catalog: `tools/{mod,registry,validate,read,
propose,dispatch,comment,procs}.rs` inside `oxidean-api-oxpilot`,
three USER-ONLY `oxpilot.diff_*` procs, `0089` additive
`oxpilot_proposals.ops_json` column ×3 dialects, one slot install
at shell assembly. Infra concerns: what CI must cover, whether the
additive column changes rollout/rollback, and which operator
surfaces exist.

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U5 (kind
  `service`, `embedded (api)` — no new service by contract).
- [design] `construction/oxpilot-tools/{functional-design,
  nfr-requirements, nfr-design}/*` — dispatch ladder, registry
  pins, `ops_json` additive chain, audit/card surfaces.
- [contract] `contract-summary.md` (C1/C6/C7) — proc additions and
  consumed gateway seams.
- [team] `team.md` — Testing Posture (nextest archive → api-rust
  shards; dialect probes are the only cross-dispatch lane), Code
  Style, Deployment (single api service, Railway).
- [ci] `.github/workflows/ci.yml` (api-rust shards, db-dialects
  `-E` filterset, rpc-sync, rust-fmt/clippy/dead-code,
  commit-lint), `docs/TESTING.md` (`dialect_oxpilot` row),
  `Makefile` (`rpc-gen`).

## Q1. Does oxpilot-tools add deployable infrastructure?

- A. None — U5 lands inside the existing `api` service: eight
  `tools/*.rs` modules in `oxidean-api-oxpilot`, three proc
  registrations + dispatch arms, one `Arc::new(OxpilotTools::
  new(state.clone()))` install onto `state.oxpilot.slot` at shell
  assembly (main.rs, post-`AppState::new`, beside
  `install_sessions`/`install_gateway`). The only persisted
  change is an additive `ops_json` column on the existing
  `oxpilot_proposals` table — `0089_oxpilot_proposals_ops.sql`
  ×3 dialects (applied migrations are append-only; 0087 is never
  edited). No new service, listener, port, queue, cache, or
  worker.
- B. A tool-executor sidecar for sandboxing tool dispatch —
  inverts the contract: tools are in-process library code whose
  security boundary is the closed registry + schema gate, not
  process isolation.
- C. A dedicated read-replica for tool repo-reads — repo reads
  ride the same `rpc_dispatch`/db paths interactive procs use;
  nothing about a tool call distinguishes its load.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — zero new deployables; tools are in-process modules in the shipped api crate; ops_json is an additive column on an existing table

## Q2. Which existing CI lanes must cover the new code — and do they?

- A. `api-rust` shards via the workspace nextest archive — the
  catalog pin test (23 rows verbatim + `EntryKind` literal
  bindings), validate-walker unit tests, handler tests, and the
  proc-chain tests replay workspace-wide with NO `-E` filterset
  term (the archive is workspace-wide; only `db-dialects` runs a
  binary_id inclusion filter). `db-dialects` covers 0089 through
  the already-registered `dialect_oxpilot` binary — its `-E` term
  and TESTING.md row are U1's, unchanged — plus the existing
  migration filename-parity test enforcing ×3 presence.
  `rpc-sync` regenerates `packages/api-client` for the three new
  `oxpilot.diff_*` procs and gates `git diff --exit-code`.
  `rust-fmt`/`clippy`/`dead-code`/`commit-lint` inherit
  workspace-wide. Zero new jobs ⇒ no `Main` ruleset edit.
- B. A dedicated `oxpilot-tools` job with its own filterset —
  duplicates the archive's coverage; the unit has no
  cross-dialect or browser dimension needing a bespoke leg.
- C. A stack-e2e lane driving propose→apply end-to-end — the
  apply chain is synthesized `rpc_dispatch` calls into shipped
  procs; per-proc behavior is already e2e-covered and the
  chain's seams are unit-tested. Recorded e2e debt, not a gate.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — workspace archive + existing dialect_oxpilot/rpc-sync lanes cover everything; no -E term, no new job, no ruleset edit

## Q3. New environment variables or secrets?

- A. None — enablement and keys are upstream units' surface:
  `require_enabled` reads the existing `OXIDEAN_OXPILOT_*`
  flag/provider chain (U1/U4); `require_verified` reads the
  caller's session; tools hold no provider credentials (provider
  auth lives in U4's adapters; connector custody in U6/U7).
  `.env.example` and `docs/CONFIGURATION.md` are untouched by
  this unit.
- B. A `OXIDEAN_OXPILOT_TOOLS_*` block for catalog/bounds tuning —
  every bound (ACTION_ARGS_MAX 2 KiB, depth 16, 23-row catalog)
  is a pinned constant, not an operator knob.
- C. A per-repo tool-allowlist env — repo binding comes from the
  session row, never env; a mutable allowlist would weaken the
  closed-registry guarantee.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — zero new env vars; enablement rides the existing OXIDEAN_OXPILOT_* surface; bounds are pinned constants

## Q4. What monitoring, alerts, or dashboards does the unit add?

- A. None — the observability surface is deliberately thin and
  entirely on existing infrastructure: three audit events
  (`oxpilot.proposal_created`/`diff_applied`/`artifact_comment`)
  land on the existing `audit` table via `record_with`;
  `card{diff|consent}` rides the existing turn frame sink; the
  durable U3 tool-call row is the read trail; `internal`
  ToolErrors warn-log via `invoke_call`'s existing path.
  Operator join keys: `session_id`/`proposal_id`/`pr_ref`/
  `grant_id`. No metrics exporter, no alert rules, no dashboard.
- B. Tool-call latency/error-rate metrics + alert rules — call
  counts and error codes are derivable from the durable
  tool-call rows the runtime already persists; a counter adds a
  surface with no new signal (NFR6.5 posture).
- C. A per-tool audit row on every invocation — double-writes
  every model fetch; the durable card already records
  who/what/how for reads.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — zero new observability surface; three audit events + cards + tool-call rows on existing tables/sinks

## Q5. Rollout and rollback story?

- A. Flag-gated by existing enablement: the catalog only surfaces
  inside an enabled oxpilot session (`invoke_call` never reaches
  it otherwise), and the user-side `diff_*` procs gate on
  `require_enabled` — disabling oxpilot severs the entire unit
  with zero deploy. The `ops_json` column is additive
  (append-only migration, absent ops on pre-existing rows read
  as none): a code rollback leaves a harmless unused column;
  no down-migration needed. Apply-path leftovers are
  operator-visible but benign — a failed mid-chain apply leaves
  a deletable `oxpilot/proposal-<id8>` branch and a `pending`
  proposal row, never a mutation on the target branch.
- B. A staged per-repo rollout flag — adds an env knob the
  security design deliberately excludes; session-scoped
  enablement is the shipped blast-radius control.
- C. Point-in-time migration rollback — append-only migration
  policy forbids editing/removing applied migrations; a reverted
  deploy keeps the inert column by design.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — enablement flag is the rollout control; additive column makes rollback a revert; apply failures leave only self-named deletable branches

## Q6. Resource-shape or scaling concerns inside the api process?

- A. None new — per-invocation work is bounded: catalog lookup
  O(23) fixed-order, schema walk O(keywords × nodes) at depth
  ≤16 vs authored ≤3, serialized action args ≤2 KiB
  (ACTION_ARGS_MAX, bound-and-send the same string), audit
  detail ids/counts/refs only. No new connection pools, threads,
  channels, caches, or background tasks — `emit_card` borrows
  the existing turn map, `dispatch_action` borrows the gateway's
  action queue, tool-call persistence is U3's row.
- B. A per-session tool-execution semaphore — the turn/admission
  locks U3 already holds serialize invoke per turn; a second
  limiter adds contention with no stated overflow to absorb.
- C. A dedicated db pool for tool reads — tool procs ride the
  same pool every other rpc_dispatch shares; splitting pools
  starves interactive traffic under the same ceiling.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — bounded in-process work only; no new pools/threads/queues; existing turn locks and gateway caps carry the load

## Q7. What must an operator provision for a user to reach the tools?

- A. Nothing — 0089 applies on api boot with the existing
  migration runner (×3 dialects, same as every table);
  enablement is the settings flag U4 already surfaces; the
  catalog self-registers at shell assembly. Per-user and
  per-repo provisioning: zero — repo binding is the session row,
  identity is the caller's session.
- B. Tool-grant records per user — tools carry no grant concept;
  consent grants exist only for exec/fs actions and are minted
  on demand by the gateway (U6's surface).
- C. Catalog seeding/config files — the registry is `static` Rust
  data pinned by test; there is nothing to seed.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — zero operator provisioning; migration auto-applies; enablement flag and session binding are the only gates

## Consolidated Summary Confirmation

- No deployable infra: eight in-process modules + three procs +
  one slot install; `0089` additive `ops_json` column ×3 (Q1).
- CI: api-rust archive + existing `dialect_oxpilot` filterset +
  rpc-sync regen; no new jobs, no ruleset edit (Q2).
- Env/secrets: none — `OXIDEAN_OXPILOT_*` enablement is U1/U4's
  surface; bounds are pinned constants (Q3).
- Monitoring: none new — three audit events + cards + durable
  tool-call rows on existing sinks (Q4).
- Rollout: enablement flag severs the unit; additive column is
  rollback-inert; apply leftovers are self-named deletable
  branches (Q5).
- Resources: bounded in-process work; no pools/threads/queues;
  turn locks + gateway caps carry load (Q6).
- Provisioning: zero — migration auto-applies, catalog
  self-registers, enablement flag is the only gate (Q7).

[Answer]: Looks correct
