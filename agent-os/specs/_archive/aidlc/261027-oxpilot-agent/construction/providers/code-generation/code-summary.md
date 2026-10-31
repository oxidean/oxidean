# Code Summary — providers (U4)

Provider/configuration subsystem landed: sparse `oxpilot_settings` DB
overrides over env, a swappable `OxpilotRuntime` slot replacing U1's
immutable holder, three real streaming provider adapters, and the
`admin.oxpilot.*` admin RPC surface — all while preserving U1 stub
behavior and captured-provider in-flight turns.

## Data layer

- **`crates/oxidean-db`** — migration `0086_oxpilot_settings.sql` x3
  dialects: singleton `oxpilot_settings` (`id=1`, PK-enforced) with
  nullable sparse columns `enabled_override`, `provider_id`, `model`,
  `base_url`, `key_ciphertext`, `key_last4`, `user_budget`,
  `instance_budget`, server-set `updated_at`. `oxpilot_settings.rs`
  store: `get_row` / `upsert_row` / `clear_row`, with ciphertext+last4
  written and cleared atomically in one statement. `dialect_oxpilot.rs`
  extended for the new table (schema parity + singleton enforcement);
  CI `db-dialects` filter and `docs/TESTING.md` row updated to include
  the binary.

## Runtime seam (api-core)

- `oxpilot.rs`: `OxpilotRuntime { slot: RwLock<OxpilotRuntimeSlot>,
  rebuild_lock: Mutex<()> }`; slot carries `provider`, `model`,
  `disabled_reason`, shared `turns: Arc<ActiveTurns>`, and
  `built_from: Option<ProviderIdentity>` (`provider_id`, `model`,
  `base_url`, `key_source`). `AppState.oxpilot` / `RpcCtx.oxpilot` are
  `Arc<OxpilotRuntime>`; enablement reads are async slot reads — hot
  paths never touch the DB. Trait delta: `ToolSpec`, `Usage`,
  `ChatRequest.tools`, `Done { usage: Option<Usage> }`. U1's
  env-only boot slot still constructs synchronously; the real resolved
  config installs via boot `rebuild`.

## Providers tree (`src/providers/`)

- `registry.rs` — public `REGISTRY` = exactly `{openai, anthropic,
  ollama}` descriptors (id, advertised models, `requires_key`); internal
  resolution adds `stub` (env-buildable U1 parity, never listed, refused
  by `settings_set` outside debug-shaped builds).
- `key.rs` — custody boundary: `encrypt_key`/`key_last4` are
  `pub(crate)`; `decrypt_key` is `pub(in crate::providers)` so only
  adapter construction can decrypt. Key-source chain
  `OXIDEAN_OXPILOT_SECRETS_KEY → OXIDEAN_SESSION_SECRET`; missing source
  → `crypto.unavailable`; undecryptable stored ciphertext →
  `invalid_provider` (never silent env fallback).
- `egress.rs` — SSRF guard: http/https only, literal-IP checks in all
  forms (dotted/hex/octal/integer/IPv6/v4-mapped), `lookup_host`
  all-public for names, Ollama + provider-less exemptions.
- `stream.rs` — `SseDecoder` (`\n\n` + `\r\n\r\n`, EOF flush) and
  `NdjsonDecoder` shared by the adapters.
- `openai.rs` / `anthropic.rs` / `ollama.rs` — chat-completions SSE,
  messages SSE (`max_tokens: 8192`), `/api/chat` NDJSON; all emit
  `Delta`/`ToolCall`/`Done{usage}`/`Error` with per-adapter status→
  `ProviderError.code` tables. Shared client policy: connect 10 s,
  total 300 s, redirects disabled, ≤1 connect-phase retry.

## Settings (`src/settings.rs`)

- `EffectiveProviderConfig::resolve_over_env` field-by-field
  (DB non-NULL wins); `apply()` tri-state semantics — absent = keep,
  explicit `null` = clear, `clear_overrides` wipes the row and wins
  over same-payload field writes; `api_key`+`clear_key` and
  empty/whitespace keys → `rpc.bad_input`.
- Single `rebuild_lock` serializes `resolve → validate → build →
  persist → swap → audit`. Provider-affecting writes build the
  candidate BEFORE persist (failed build leaves old row + old slot);
  flag/budget-only identity-preserving writes skip the build.
  `built_from` identity carry preserves the provider `Arc` across
  flag-off→on. Disabled slots stamp `flag_off`/`no_provider`/
  `misconfigured`. Audits log field names only.
- Iteration-1 review corrections: (1) `build_required` now gates on
  `cfg.enabled` — a combined write under an effective flag-off can
  never construct/install an enabled slot; (2) `validate` refuses only
  values the payload itself names (`provider_id`/`model`/`base_url`/
  budgets) — env-resolved or stale-row provider problems degrade to a
  reason-bearing slot instead of wedging unrelated writes (env `stub`
  stays legal everywhere); (3) the v6 public check additionally
  refuses NAT64 `64:ff9b::/96`, 6to4 `2002::/16`, and Teredo
  `2001::/32` (embedded-v4 transition forms); (4) model is never
  fabricated — `EnablementState.provider` carries `Option<model>` and
  a model-less turn fails with an honest terminal instead of sending
  `stub-1`.

## API surface

- `admin.oxpilot.settings_get` (effective values + per-field
  `*_overridden` flags + `key_set`/`key_last4`/`key_source` — never
  plaintext/ciphertext), `admin.oxpilot.settings_set`,
  `admin.oxpilot.providers_list` (public registry + computed
  `configured`, `stub` omitted). Registered in the crate's closed
  `PROCEDURES` set with a pinning test; `SessionOnly` + `require_admin`,
  PATs deny `auth.pat_scope` pre-dispatch; gate-exempt from
  `oxpilot.disabled` so a broken unit stays administrable. `rpc_gen.rs`
  gained the TS types + `admin.oxpilot` client namespace + TanStack
  option helpers; `make rpc-gen` output committed.
- `crates/oxidean-api/src/main.rs`: unconditional
  `settings::rebuild(&state.db, &state.oxpilot)` right after
  `AppState::new`, outside `AUTO_MIGRATE`; bounded lookups so boot
  cannot stall listen.

## Tests

`cargo nextest run -p oxidean-api-oxpilot` — 58 green: colocated unit
tests (resolution matrix, tri-state apply, slot semantics, validation,
egress literal/DNS, custody, decoders, status classification, retry
boundary) + `tests/oxpilot_providers.rs` wiremock golden tests for all
three adapters (no live calls, no real keys) +
`tests/oxpilot_settings.rs` integration (rejection matrix,
round-trip, build-failure atomicity, provider-less writes, Arc carry,
concurrent-write serialization, release-shaped stub refusal).
Repository tests in `oxpilot_settings.rs` store + dialect parity in
`dialect_oxpilot.rs`.

## Config

`.env.example` + `docs/CONFIGURATION.md`: five new
`OXIDEAN_OXPILOT_*` vars (API_KEY, BASE_URL, USER_BUDGET,
INSTANCE_BUDGET, SECRETS_KEY) with custody/rotation caveats, the
Ollama private-target disclosure, and the DNS TOCTOU caveat.
`.railway/railway.ts`: `preserve()` for all five +
`OXIDEAN_SESSION_SECRET`.

## Deviations from plan

- `settings.rs` exceeds the plan's test-count target (~30 colocated
  tests) — resolution/apply/slot matrices justified the density.
- Generated-client additions required editing `rpc_gen.rs` (the
  generator template), not hand-editing `packages/api-client` — the
  same-change regen keeps client and procedures in lockstep.
