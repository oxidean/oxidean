# Unit Test Instructions — providers (U4)

## Framework & config

- Rust: `cargo-nextest` (workspace `.config/nextest.toml`, `ci` profile).
- Crate unit tests: colocated `#[cfg(test)]` in `crates/oxidean-api-oxpilot/src/**`
  and `crates/oxidean-api-core/src/oxpilot.rs`, `crates/oxidean-db/src/oxpilot_settings.rs`.
- Integration tests: `crates/oxidean-api-oxpilot/tests/oxpilot_settings.rs`;
  dialect: `crates/oxidean-db/tests/dialect_oxpilot.rs` (extend, already registered).
- Provider HTTP mocking: `wiremock` (dev-dep on `oxidean-api-oxpilot`) —
  golden fixtures `fixtures/{openai,anthropic,ollama}/`. **Never** live
  provider calls or real keys.

## Exact unit-scoped commands

```bash
# crate unit + integration tests for this unit
cargo nextest run -p oxidean-api-oxpilot
# settings proc integration binary only
cargo nextest run -p oxidean-api-oxpilot --test oxpilot_settings
# dialect assertions for the new table
cargo nextest run -p oxidean-db -E 'binary_id(oxidean-db::dialect_oxpilot)'
# api-core slot/trait tests touched by the seam migration
cargo nextest run -p oxidean-api-core -E 'test(oxpilot)'
# U1 integration binary (must stay green through the seam change)
cargo nextest run -p oxidean-api --test oxpilot
```

## Coverage targets

- Strategy: standard — 5-8 tests per component; unit + integration for
  key boundaries.
- `oxidean-api-oxpilot` is collect-only coverage (no numeric floor, per
  affirmed Testing Posture); the change-type minimums apply.
- Key boundaries covered: settings write path (integration), provider
  adapters (wiremock + fixtures), custody helpers (unit), SSRF
  validator (unit), rebuild/slot semantics (unit + integration).

## Mocking/stubbing

- `wiremock::MockServer` per adapter test; `base_url` pointed at the
  mock. SSRF validation: validator tests use literal-IP inputs and
  `lookup_host` on `localhost` names where resolvable — never external
  DNS in tests.
- `StubProvider` for U1-path tests; a test-only failing provider via a
  registry test hook (not an env var) for build-failure paths.
- Secrets-key tests generate an ephemeral `OXIDEAN_OXPILOT_SECRETS_KEY`
  in-process per test binary — never a committed value.

## Test data

- Settings rows built via the store helpers; singleton id=1.
- Fixture files committed under `tests/fixtures/` (or `src/providers/fixtures/`)
  capturing real provider frame shapes: OpenAI SSE chat-completions,
  Anthropic messages SSE, Ollama NDJSON; plus the error-status set
  (401/403/429/context-length/5xx bodies).

## Notes

- `tests/oxpilot.rs` (U1) must remain green — the seam migration updates
  its slot construction, not its assertions.
- `procedures_list_matches_dispatch` is the crate's closed-set pin.
- Concurrency test for rebuild serialization: two concurrent
  `settings_set` calls land sequentially with coherent row/slot/audit.

## Runner verification (code-generation)

- `cargo-nextest` 0.9.143 installed; `cargo nextest run -p
  oxidean-api-oxpilot` executed green against the crate skeleton
  (2/2 pinned-set tests) before first unit test was authored.
- All commands verified green at unit completion: `oxidean-api-oxpilot`
  58/58, `dialect_oxpilot` 12/12 (binary id is `oxidean-db::dialect_oxpilot`),
  `oxpilot_settings` 10/10, api-core `test(oxpilot)` 4/4, U1 `oxpilot`
  10/10.
