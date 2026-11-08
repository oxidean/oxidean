# Unit Test Instructions — agent-runtime (U3)

## Framework & config

- Rust: `cargo-nextest` (workspace `.config/nextest.toml`, default
  profile).
- Turn-engine unit tests: colocated `#[cfg(test)]` module in
  `crates/oxidean-api-oxpilot/src/runtime.rs` (ScriptProvider fake +
  in-memory `SessionService` impl; no provider SDK, no network).
- Proc/route integration: `crates/oxidean-api/tests/oxpilot.rs`
  (in-memory router harness; PAT/session fixtures as in U1/U2).
- Provider wire mapping: `crates/oxidean-api-oxpilot/tests/oxpilot_providers.rs`
  (wiremock goldens; ChatMsg tool fields now serialized).
- Dialect: `crates/oxidean-db/tests/dialect_oxpilot.rs` (sqlite scratch
  DB under `crates/oxidean-db/var/`, `-j 1` per the U2 pin).

## Exact unit-scoped commands

```bash
cargo nextest run -p oxidean-api-oxpilot                       # crate units incl. runtime
cargo nextest run -p oxidean-api --test oxpilot               # proc/route integration incl. turn_cancel
cd crates/oxidean-db && DATABASE_URL="sqlite:./var/dialect_oxpilot.db" \
  cargo nextest run -p oxidean-db -E 'binary_id(oxidean-db::dialect_oxpilot)' -j 1
make rpc-gen && git diff --exit-code packages/api-client        # generated types in-tree
```

Shared `DATABASE_URL` ⇒ dialect binary runs `-j 1` (pinned from U2).
Full suite: `make test` at checkpoint.

## Coverage targets

- Strategy: standard — 5-8 tests per component; unit + integration for
  key boundaries. `oxidean-api-oxpilot` remains collect-only coverage
  per the affirmed Testing Posture; change-type minimums apply.
- Runtime unit coverage (`runtime::tests`, ~30 tests): emit_terminal
  first-wins + concurrent exactly-once, effective call_id mint,
  truncate/summary bounds, assemble_prompt windowing + tool summary
  lines, run_round deltas/calls/done + silent-end + deadline + cancel,
  invoke_call ok/timeout/abort, execute_calls persist+feedback +
  error-card, run_turn done/no_model/missing_provider/open_failure/
  rounds_exceeded/null_tools, drain_and_chain
  empty/nonempty/drain-Err/depth-cap/stale/claim-loss, spawn_turn_task
  panic/done, abort_hook idempotence.
- Integration coverage: turn_cancel live/absent/foreign, delete-hook
  abort, 429 budget refusal, 409 turn_active, idempotent resend,
  late-attach replay to terminal, queued-card emission, PROCEDURES pin.

## Mocking/stubbing

- `ScriptProvider` (test fake): queued `ChatStreamItem` scripts per
  round; records `ChatRequest`s for prompt-shape assertions.
- `PanicProvider` for the wrapper JoinError arm; `NullTools` and a
  scripted `ToolExecutor` for invoke ok/error/timeout paths.
- `tokio::time` bounds exercised via short real durations on the fake
  stream (ms-scale) — no wall-clock sleeps in assertions beyond
  `eventually_async` polling.

## Test data

- Sessions/rows seeded through the `SessionService` impl on the dialect
  scratch DB; turn maps built via `ActiveTurns::claim` with the test
  `turn()` helper (fanout + emitted_deltas + admission_lock).
- Queued rows inserted via `svc.enqueue` to exercise drain ordering.

## Runner verification (code-generation)

- All pinned commands green at unit completion:
  `oxidean-api-oxpilot` 108/108, `oxidean-api --test oxpilot` 24/24,
  `dialect_oxpilot` 16/16 (fresh scratch DB), `make rpc-gen` output
  byte-identical on rerun, `make test` workspace green (Rust nextest
  2158 passed / 3 skipped; web vitest 142 files / 708 tests).
- Strict lint gates clean:
  `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo clippy -p oxidean-vcs --all-targets --all-features -- -D warnings`.
