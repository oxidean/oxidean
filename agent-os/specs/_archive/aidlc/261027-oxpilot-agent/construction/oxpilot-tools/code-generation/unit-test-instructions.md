# Unit Test Instructions — oxpilot-tools (U5)

## Framework & config

- Rust: `cargo-nextest` (workspace `.config/nextest.toml`, `ci` profile —
  bounded retry=1, no fail-fast).
- Crate unit tests: colocated `#[cfg(test)]` modules inside the new
  `crates/oxidean-api-oxpilot/src/tools/{mod,registry,validate,read,
  propose,dispatch,comment,procs}.rs` files.
- Store/dialect tests: extend
  `crates/oxidean-db/tests/dialect_oxpilot.rs` (already registered in the
  `db-dialects` `-E` filter — binary_id-keyed) for the `ops_json`
  column and `find_oxpilot_proposals_by_session`.
- Proc integration tests: extend `crates/oxidean-api/tests/oxpilot.rs`
  (existing harness — `test_app`, migrate-then-`unlock_signup`, admin
  user B) for `oxpilot.diff_list` / `oxpilot.diff_get` /
  `oxpilot.diff_apply`.
- Closed-set proc registration: extend the crate-local
  `procedures_list_matches_dispatch` test in `oxidean-api-oxpilot` to
  the three new proc names.
- Generated client: `make rpc-gen` then `make rpc-sync-check` — the
  `oxpilot.diff_*` trio must appear in `packages/api-client`.

## Exact unit-scoped commands

```bash
# dialect assertions for the ops_json column + find-by-session verb —
# run from `crates/oxidean-db` with a FILE url and -j 1 (tests share
# one DATABASE_URL target; sqlite::memory: fails by design)
cd crates/oxidean-db && DATABASE_URL="sqlite:./var/dialect_oxpilot.db" \
  cargo nextest run -p oxidean-db -E 'binary_id(oxidean-db::dialect_oxpilot)' -j 1
# registry/validate/invoke-ladder/handler unit tests (whole crate —
# it is this unit's home plus prior oxpilot units' suites)
cargo nextest run -p oxidean-api-oxpilot
# proc integration binary (existing harness)
cargo nextest run -p oxidean-api --test oxpilot
# compile gate for the crate + its tests
cargo check -p oxidean-api-oxpilot --tests
# generated-client drift
make rpc-sync-check
```

## Coverage targets

- Strategy: standard — 5-8 tests per component; unit + integration for
  key boundaries; scope floor (feature) = 80% line coverage + CI green.
- `oxidean-api-oxpilot` is collect-only coverage (no numeric floor —
  affirmed Testing Posture); change-type minimums apply.
- Key boundaries: schema-gate rejection matrix (closed
  `additionalProperties`, per-bound violations naming the field);
  invoke-ladder ordering (unknown_tool → invalid_args → not_found);
  injected `{owner,name}` from the RESOLVED session (never args);
  verbatim AppError→ToolError code mapping; diff↔ops path-set
  coherence (missing/extra path named); proposal bounds at each
  ceiling; denied-marker matrix (`consent_required` success-shape +
  card, three hard reasons, malformed ⇒ `internal`); target
  resolution (arg/one/zero/many); ArtifactScope reject + redacted
  body; diff_apply ordering (branch→ops→PR), fail-stop
  `apply_failed:<step>`, second apply ⇒ `diff_not_pending`;
  `PROPOSAL_SELECT_*`/insert column parity on all three dialects.

## Mocking/stubbing

- Real `Database` over tempdir sqlite for service/proposal tests — no
  DB mocks (dialect file asserts all three SQL shapes textually where
  the convention does).
- `rpc_dispatch` seams: handler unit tests stub the dispatch fn with a
  recording closure asserting `{procedure, input}` literals and
  returning canned `RpcResponse::Err{code}` for the verbatim-map
  matrix; proc integration tests exercise the real dispatch path.
- `ConnectorGateway` seams: a `#[cfg(test)]` stub gateway implementing
  the trait returning scripted `OxpilotActionRow` / `AppError` with
  `data.denied` markers — never a live connector.
- `emit_frame` / `turns()`: assert card emission via the live-turn map
  with a real `ActiveTurn` (the `message_queue` queued-card test
  precedent) and silent drop on a terminalled turn.
- No live provider, network, or git backend in any test.

## Test data

- Fixture sessions/proposals built via `svc.create_session` /
  `svc.save_proposal` or the `insert_*` store fns directly;
  foreign-user and other-session rows for the ownership pins.
- `diff_body` fixtures: canonical unified diffs with `diff --git a/X
  b/Y` headers covering create/update/delete/rename shapes; mismatch
  fixtures adding/dropping header paths for the coherence arm.
- `DeniedOutcome` fixtures built as `AppError{code:
  "oxpilot.action_denied", data: {denied: {reason, grant_id?}}}` for
  all four reasons plus the missing-marker arm.
