# Functional Design Questions — providers

Unit: `providers` (U4, kind `service`) — owns `LlmProviderRegistry`:
OpenAI/Anthropic/Ollama adapters over the U1 `LlmProvider` trait
(outbound `reqwest`, wiremock-fixture tests — never live providers), the
registry that rebuilds the U1-installed `RpcCtx` provider slot, the
`oxpilot_settings` override table + `admin.oxpilot.*` procs, and
provider-key AES-256-GCM custody under the `OXIDEAN_OXPILOT_*` key chain.
depends_on: U1.

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U4.
- [contract] `inception/contract-design/contract-summary.md` — C1
  (`admin.oxpilot.settings_get/settings_set`, `admin.oxpilot.providers_list`),
  C4 (`LlmProvider` full shape: `tools` field, `ToolCall`, `Done{usage}`).
- [req] FR6.4 (operator provider env), FR6.5 (env-default + DB-override
  flag pattern), FR6.7 (budget env vars); NFR1 (key custody),
  NFR4 (provider-failure semantics).
- [stories] AC5.1.1–AC5.1.5, AC5.4.1; budget config half of AC5.2.1
  (U2 owns the counters).
- [precedent] `admin.mcp.getSettings/updateSettings` +
  `mcp_settings` table (env default + tri-state override);
  `crate::crypto::encrypt(KEY_ENVS, …)` AES-256-GCM key chain.

## Q1. Entity set for providers?

- A. One persisted entity `OxpilotSettings` (singleton row: nullable
  `enabled_override`, `provider_id`, `model`, `base_url`,
  `key_ciphertext`, `key_last4`, `user_budget`, `instance_budget`,
  `updated_at`) + two computed views `ProviderDescriptor` (static registry
  metadata: id, models, configured) and `EffectiveProviderConfig`
  (env-default ← DB-override resolution result). Secret material is a
  ciphertext attribute + last4 only; plaintext never persists.
- B. One table per provider — over-models a singleton; the override is
  one logical row like `mcp_settings`.
- C. Store provider key plaintext — violates NFR1 custody.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — 1 persisted entity `OxpilotSettings` (sparse override row) + 2 computed views (`ProviderDescriptor`, `EffectiveProviderConfig`). Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q2. Effective-config resolution order?

- A. `OxpilotSettings` row is a sparse override: each field falls back to
  its `OXIDEAN_OXPILOT_*` env var when NULL (`enabled` ←
  `OXIDEAN_OXPILOT_ENABLED`, `provider_id` ← `OXIDEAN_OXPILOT_PROVIDER`,
  `model` ← `OXIDEAN_OXPILOT_MODEL`, `base_url`/key ←
  `OXIDEAN_OXPILOT_BASE_URL`/`OXIDEAN_OXPILOT_API_KEY`, budgets ←
  `OXIDEAN_OXPILOT_USER_BUDGET`/`OXIDEAN_OXPILOT_INSTANCE_BUDGET`).
  `enabled_override` is tri-state (NULL = env). Resolution output feeds
  the U1 EnablementGuard — same precedence as `admin.mcp.*`.
- B. DB row is authoritative when present, env ignored — breaks the
  env-default + DB-override contract (FR6.5).
- C. Env only — contradicts AC5.1.3 and C1's settings procs.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — sparse-override precedence: DB field when set else `OXIDEAN_OXPILOT_*` env; `enabled_override` tri-state — same as `admin.mcp.*`. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q3. Key custody and read model?

- A. `settings_set` accepts `api_key` plaintext once, encrypts via
  `crypto::encrypt(OXPILOT_KEY_ENVS, …)` with
  `KEY_ENVS = ["OXIDEAN_OXPILOT_SECRETS_KEY", "OXIDEAN_SESSION_SECRET"]`,
  stores ciphertext + `key_last4`; reads return `key_set` + `key_last4`
  only (write-only, `admin.mcp.*`/repo-secrets precedent). Decryption
  happens only inside provider construction, never on a read path.
- B. Store hashed key — a hash can't authenticate outbound calls.
- C. Return the key on `settings_get` for admin convenience — violates
  NFR1 "no stored secret ever leaves the system".
- D. Not decided.
- X. Other (please specify)

[Answer]: A — write-only plaintext → AES-256-GCM under `["OXIDEAN_OXPILOT_SECRETS_KEY","OXIDEAN_SESSION_SECRET"]`; reads return `key_set`+`key_last4` only. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q4. Provider adapter architecture?

- A. One adapter per provider behind the U1 `LlmProvider` trait:
  `OpenAiProvider` (chat-completions SSE), `AnthropicProvider`
  (messages API SSE), `OllamaProvider` (`/api/chat` NDJSON on
  `base_url`, no key required), plus the U1 `StubProvider` kept for
  tests. Shared reqwest SSE/NDJSON line-decoder maps provider frames to
  `ChatStreamItem`; provider HTTP/auth/quota errors map to stable
  `ProviderError.code` values (`unauthenticated`, `forbidden`,
  `rate_limited`, `context_length`, `unavailable`, `internal`).
- B. One generic OpenAI-compatible adapter — Anthropic's messages API
  differs enough to corrupt the seam; per-provider adapters are the
  honest split.
- C. SDK crates per provider — adds heavyweight deps for three HTTP
  calls; reqwest is already in the tree.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — per-provider adapters (`OpenAiProvider`, `AnthropicProvider`, `OllamaProvider`) + kept `StubProvider`; shared reqwest line-decoder; stable `ProviderError.code` mapping. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q5. Slot rebuild semantics on settings change?

- A. The provider slot is an `RwLock<Option<Arc<dyn LlmProvider>>>` on
  the shared `OxpilotState`; `admin.oxpilot.settings_set` rebuilds the
  resolved provider and swaps the Arc — in-flight turns keep their
  captured Arc (U1 turns hold `state.oxpilot.provider.clone()` at
  spawn), new turns get the new provider. A failed rebuild leaves the
  old provider installed and returns `invalid_provider`.
- B. Rebuild requires restart — contradicts the admin-settings UX; a
  hot swap is exactly what `admin.mcp.*` does.
- C. Cancel in-flight turns on rebuild — surprising; provider swap is
  an admin action, not a turn kill.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — refined by architecture review (R-01/R-03/R-04/R-13/R-14): the slot carries provider+model+disabled-reason (`OxpilotRuntimeSlot`), the holder is `RwLock`-swappable, and `settings_set` runs resolve→validate→build→persist→swap→audit as ONE mutex-serialized section — build before persist so failures leave row+slot untouched; the new slot reuses the shared `Arc<ActiveTurns>` handle so in-flight turns keep their captured Arc. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q6. `admin.oxpilot.*` proc set?

- A. Exactly the contract set: `admin.oxpilot.settings_get`,
  `admin.oxpilot.settings_set`, `admin.oxpilot.providers_list` —
  admin-session scoped via `require_admin`, closed-set pinned in the
  same registration test pattern U1 used. `providers_list` reports
  `{id, models, configured}` per adapter (`configured` = resolvable
  from env or DB override); `settings_set` validates `provider_id`
  against the registry → `invalid_provider` otherwise.
- B. Also `admin.oxpilot.test_connection` — useful but not in C1; add
  later under the additive-change rule.
- C. Piggyback on `admin.mcp.*` — wrong domain, wrong settings row.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — exactly `admin.oxpilot.settings_get`/`settings_set`/`providers_list`, admin-scoped, closed-set pinned. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q7. C4 trait upgrade mechanics?

- A. U4 upgrades the U1 `LlmProvider` seam to the full C4 shape:
  `ChatRequest` gains `tools: Vec<ToolSpec>` and `ChatStreamItem` gains
  `ToolCall{id,name,args_json}` + `Done{usage}` — U1's stub +
  `run_turn` adapt (`tools: vec![]`, usage ignored until U3's tool
  loop). Contract owner is U1 per the ownership table; the upgrade is
  additive to consumers and lands in the same PR — recorded here so
  U1's code-generation delta is deliberate, not drift.
- B. Freeze the trait at U1's reduced shape — leaves U3's tool loop
  with no seam; C4's `ToolCall`/`usage` were specified for a reason.
- C. New `LlmProviderV2` trait — contract forbids silent shape change,
  and a parallel trait duplicates the slot.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — U4 upgrades the C4 seam (`tools`, `ToolCall`, `Done{usage}`); U1 stub + `run_turn` adapt; lands in the same PR as deliberate owner-approved change. Second seam delta (review R-07): U1's sync env-only `resolve_enablement`/gate/`status` become DB-aware async reads of the `RwLock`-held slot — the immutable env-built `SharedOxpilot` is replaced by the swappable `OxpilotRuntimeSlot` in the same PR. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q8. Outbound-call guardrails?

- A. Per-request `reqwest` timeout (connect 10 s, total 300 s — a
  streamed reply is long-lived), no retries on stream open (a retried
  POST could double-bill), exactly one reconnect attempt on pre-first-
  byte transport failure only. Base URLs restricted to http(s) —
  `file:`/`gopher:` and non-443-private hosts rejected at validation
  (SSRF bound); Ollama exempts the private-host rule since self-host is
  its purpose (operator-set `base_url` is trusted input).
- B. Unlimited timeouts + retry-all — a hung provider pins the turn
  forever; blind retries double-bill.
- C. No guardrails — SSRF via `base_url` is a real NFR1 hole.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — connect 10 s / total 300 s; retry bound tightened by review (R-05/R-06): at most one retry and ONLY on connect-phase transport failure (request never reached the wire — a sent-but-unanswered POST could double-bill); `redirect(Policy::none())` so a validated URL can't 302 onto private egress; http(s) only, private-host bound keyed on the RESOLVED provider (literal IP forms + DNS-resolved answers classified; documented TOCTOU bound), Ollama exempt. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q9. Testing posture for adapters?

- A. `wiremock` HTTP fixtures in `crates/oxidean-api-oxpilot/tests/`:
  per-adapter golden SSE streams (delta ordering, done-frame, usage),
  error-mapping cases (401/403/429/context-length/5xx→codes), timeout
  path, settings procs round-trip (override → resolve → list → get),
  key round-trip (set → last4 → never plaintext), slot-rebuild
  semantics. Never a live provider — network egress in tests is
  forbidden by the unit definition.
- B. Real provider calls behind an env flag — flaky and needs keys in
  CI; contract forbids it.
- C. Unit tests only on the mapping table — misses framing bugs per
  adapter.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — wiremock fixtures only (golden streams, error-code mapping, settings round-trip, custody, slot rebuild); never a live provider. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Assumption Confirmation

> Recorded under standing instruction (2026-10-27); ratify at PR review.
- Providers are operator-configured only — no per-user BYO key in v1.
- `StubProvider` remains selectable via `OXIDEAN_OXPILOT_PROVIDER=stub`
  for tests/dev-auth stacks after U4 lands.
- Budget *counters* are U2's; U4 stores only the configured caps.
- `OXIDEAN_OXPILOT_API_KEY` env var is the bootstrap key source; the DB
  override replaces it at runtime without echoing it back.
