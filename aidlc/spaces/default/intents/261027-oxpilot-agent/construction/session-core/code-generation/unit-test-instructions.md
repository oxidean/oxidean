# Unit Test Instructions — session-core (U2)

## Framework & config

- Rust: `cargo-nextest` (workspace `.config/nextest.toml`, `ci` profile —
  bounded retry=1, no fail-fast).
- Crate unit tests: colocated `#[cfg(test)]` in
  `crates/oxidean-api-oxpilot/src/services.rs`,
  `crates/oxidean-api-core/src/oxpilot.rs`.
- Store/dialect tests: extend
  `crates/oxidean-db/tests/dialect_oxpilot.rs` (already registered in the
  `db-dialects` `-E` filter — binary_id-keyed).
- Proc integration tests: `crates/oxidean-api/tests/oxpilot.rs` (existing
  harness — `test_app`, migrate-then-`unlock_signup`, admin user B).
- Closed-set proc registration: the per-crate
  `procedures_list_matches_dispatch` convention — `oxidean-api-oxpilot`
  ships its own copy; extend to the five new names.

## Exact unit-scoped commands

```bash
# dialect assertions for the four 0087 tables — run from
# `crates/oxidean-db` with a FILE url and -j 1 (the tests share one
# DATABASE_URL target; sqlite::memory: fails by design, and parallel
# runs race the shared file).
cd crates/oxidean-db && DATABASE_URL="sqlite:./var/dialect_oxpilot.db" \
  cargo nextest run -p oxidean-db -E 'binary_id(oxidean-db::dialect_oxpilot)' -j 1
# api-core trait/slot/frame tests
cargo nextest run -p oxidean-api-core -E 'test(oxpilot)'
# service impl unit tests + proc dispatch
cargo nextest run -p oxidean-api-oxpilot
# proc integration binary (existing harness)
cargo nextest run -p oxidean-api --test oxpilot
# generated-client drift
make rpc-sync-check
```

## Coverage targets

- Strategy: standard — 5-8 tests per component; unit + integration for
  key boundaries; scope floor (feature) = 80% line coverage + CI.
- `oxidean-api-oxpilot` is collect-only coverage (no numeric floor —
  affirmed Testing Posture); change-type minimums apply.
- Key boundaries: union dedupe arms (transcript/queue/collision),
  drain tx atomicity per dialect, cascade vs counter survival,
  hook abort ordering, budget breach order + never-refusing usage view,
  position ordinal ≠ queue_seq, five procs' auth/gate/error map.

## Mocking/stubbing

- Real `Database` over tempdir sqlite for service tests — no DB mocks;
  `#[cfg(test)] ProbeHook` for delete-hook order/abort scripts.
- No live provider or network in any test; `BudgetCaps` passed as
  literals.
- `find_owned_session` foreign-row cases use a second user's session id.

## Test data

- Fixture sessions/messages built via the service under test or the
  shipped `insert_session`/`insert_message` store fns directly.
- `client_msg_id` literals per dedupe arm; `queue_seq` gaps created by
  draining before re-enqueue (position ordinal assertion needs a real
  gap).
