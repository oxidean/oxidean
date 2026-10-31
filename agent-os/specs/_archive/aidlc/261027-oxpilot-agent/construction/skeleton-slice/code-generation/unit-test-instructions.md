# Unit Test Instructions — u1-skeleton-slice

Test strategy: **standard** (5–8 tests per component + key-boundary
integration). Methodology: **test-after**. Scope floor: existing suite
stays green; `oxidean-api-oxpilot` lands collect-only for Rust coverage
(affirmed — no numeric floor on the new crate).

## Framework setup

No new configuration: cargo-nextest for Rust (`.config/nextest.toml`),
Vitest for web (existing `unit`/`integration`/`browser`/`e2e-stack`
projects), `dialect_*` nextest binaries for DB parity. All runners
already exist — brownfield verify-before-first-test applies.

## Unit-scoped commands (exact paths; run before first test step)

```bash
# crate tests (unit + integration)
cargo nextest run -p oxidean-api-oxpilot
# or without nextest:
cargo test -p oxidean-api-oxpilot

# dialect parity test (sqlite locally; gated on OXIDEAN_TEST_DATABASE_URL for pg/mysql)
cargo test --manifest-path crates/oxidean-db/Cargo.toml --test dialect_oxpilot

# redact() detector units (lives in oxidean-core)
cargo test -p oxidean-core redact

# web tests for this unit (paths only — never a bare project run)
cd apps/web && bunx vitest run src/lib/oxpilot-queries.test.ts \
  src/routes/oxpilot.index.test.tsx --project unit
cd apps/web && bunx vitest run src/routes/oxpilot.session.test.tsx --project integration
cd apps/web && bunx vitest run src/components/oxpilot/session-form.browser.test.tsx --project browser

# stack probe (requires the dev-auth stack; env provided by harness)
OXIDEAN_OXPILOT_ENABLED=1 OXIDEAN_OXPILOT_PROVIDER=stub \
  bunx vitest run e2e/stack/oxpilot.stack.test.ts --project e2e-stack
```

## Coverage expectations

- `oxidean-api-oxpilot`: collect-only (affirmed); change-type minimums
  carry testing — aim for the standard 5–8/component on logic units.
- `redact()`: colocated unit tests for every pattern family +
  pass-through.
- DB layer: `dialect_oxpilot.rs` asserts schema parity; store tests
  cover ownership/ordering/dedupe/orphan semantics.
- Web: route-coverage rows ×2 require a mount test each (happy-dom or
  stack evidence); browser-coverage row requires a real Chromium
  interaction test.

## Mocking/stubbing guidance

- `StubProvider` is the provider seam — deterministic delta stream;
  constructor-only `fail` flag exercises the `terminal{error}` path.
  Never a live LLM.
- RpcCtx for tests: sqlite tempdir DB (existing test util pattern);
  PAT-class auth fixtures exercise SessionOnly refusal.
- SSE frames: parse `text/event-stream` lines, assert
  append/terminal/ping ordering; no EventSource for POST.

## Test data management

Per-test fresh sqlite tempdir DB via the existing test helper; sessions
carry caller `user_id` for isolation cases; `client_msg_id` fixtures
exercise resend/dedupe. No shared mutable fixtures across tests.
