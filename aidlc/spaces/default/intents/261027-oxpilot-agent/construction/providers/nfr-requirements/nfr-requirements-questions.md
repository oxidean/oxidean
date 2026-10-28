# NFR Requirements Questions — providers

Unit: `providers` (U4) — `LlmProviderRegistry` (OpenAI/Anthropic/Ollama
adapters + stub), `oxpilot_settings` override table + `admin.oxpilot.*`
procs, AES-256-GCM key custody, hot slot rebuild. Depends on U1.

## Sources

- `construction/providers/functional-design/functional-spec.md` + `rules.md`
  (BR1–BR5 — custody, resolution, resolved-config validation, swap mechanics,
  vocabulary).
- `inception/requirements-analysis/requirements.md` — NFR1 (security/custody),
  NFR4 (provider-failure semantics), NFR9 (collect-only coverage).
- `inception/contract-design/contract-summary.md` — C1 admin procs, C4
  `LlmProvider` seam.
- `codekb/oxidean-app/technology-stack.md`.

## Q1. NFR domain coverage at this depth?

- A. All nine domains assessed; populated where the unit has real
  obligations: security (custody, SSRF, admin auth, audit), reliability
  (build-before-persist, mutex-serialized swap, retry/timeout bounds,
  in-flight survival), performance (provider latency external; rebuild +
  gate latency), scalability (singleton row, closed registry, egress
  concurrency), observability (field-names-only audit, error-code
  telemetry, no-secret logging), tech-stack decisions.
- B. Security only — the unit is mostly a config surface.
- C. Copy U1's requirements wholesale.
- X. Other (please specify)

[Answer]: A — all nine domains assessed at standard depth; populated where the unit has real obligations. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q2. Key-custody crypto?

- A. Reuse `crate::crypto::encrypt`/`decrypt` AES-256-GCM with
  `KEY_ENVS = ["OXIDEAN_OXPILOT_SECRETS_KEY", "OXIDEAN_SESSION_SECRET"]`
  (actions-secrets precedent); ciphertext+last4 stored; decrypt only
  inside provider construction; write-only reads.
- B. New crypto path — duplicates a vetted primitive.
- C. Plaintext under flag — violates NFR1.
- X. Other (please specify)

[Answer]: A — reuse `crate::crypto::encrypt`/`decrypt` AES-256-GCM, `KEY_ENVS=["OXIDEAN_OXPILOT_SECRETS_KEY","OXIDEAN_SESSION_SECRET"]`; ciphertext+last4; decrypt in construction only. Conductor-decided (2026-10-27); ratify at PR review.

## Q3. SSRF enforcement depth on `base_url`?

- A. Per BR3.2/BR4.5: http(s) only; non-ollama resolved providers forbid
  private/loopback/link-local/metadata targets over literal IP forms +
  DNS-resolve-all-public at validation; `redirect(Policy::none())` on
  clients; documented TOCTOU bound; Ollama exempt (self-host purpose).
- B. Hostname allowlist — can't cover Ollama's arbitrary self-host URLs.
- C. No bound — SSRF hole on admin-set egress (NFR1).
- X. Other (please specify)

[Answer]: A — BR3.2/BR4.5 bound: http(s), literal-IP + DNS-resolve-all-public at validation, no redirects, documented TOCTOU, Ollama exempt. Conductor-decided (2026-10-27); ratify at PR review.

## Q4. Slot swap + serialization mechanism?

- A. `RwLock<OxpilotRuntimeSlot>` holder on app state + one async rebuild
  mutex serializing settings_set's resolve→validate→build→persist→swap→
  audit; slot carries `Arc<ActiveTurns>` so in-flight turns survive;
  boot-time initial build from env+DB row.
- B. Restart-required config — contradicts admin UX, admin.mcp precedent.
- C. Per-request provider construction — pays build cost per turn and
  loses the captured-Arc guarantee.
- X. Other (please specify)

[Answer]: A — `RwLock<OxpilotRuntimeSlot>` + rebuild mutex over resolve→validate→build→persist→swap→audit; `Arc<ActiveTurns>` survives swaps; boot-time initial build env+DB. Conductor-decided (2026-10-27); ratify at PR review.

## Q5. Boot-time resolution when the DB row is unreadable?

- A. Degrade to env-only resolution + warn log (flag semantics preserved);
  a later settings_set/boot rebuild picks the row up. Never crash boot
  on a missing table pre-migration-ordering.
- B. Fail boot — a settings table read failure must not down the API.
- C. Cache last-known slot — no prior slot exists at boot.
- X. Other (please specify)

[Answer]: A — DB-unreadable at boot ⇒ env-only degraded resolution + warn; never crash boot; later rebuild picks the row up. Conductor-decided (2026-10-27); ratify at PR review.

## Q6. Observability surface?

- A. Audit rows on every settings mutation (actor, action, field NAMES,
  ts — never values); provider calls get tracing spans with
  provider_id/model attrs (never payload/key); `ProviderError.code` lands
  in the C2 terminal frame + a warn log; no secret material in any log.
- B. Log request/response bodies — leaks prompts and keys.
- C. Nothing new — provider failures are invisible to operators.
- X. Other (please specify)

[Answer]: A — audit rows field-names-only; tracing spans carry provider_id/model attrs; ProviderError.code → C2 terminal + warn log; no secret material in any log. Conductor-decided (2026-10-27); ratify at PR review.

## Q7. Provider-call reliability bounds?

- A. Connect 10 s / total 300 s timeouts; at most ONE retry and only on
  connect-phase transport failure (request never transmitted — no
  double-bill); redirects never followed; streamed stalls never retry;
  failures classify to stable `ProviderError.code`.
- B. Unlimited retry on stream open — double-billing + pinned turns.
- C. No timeouts — a hung provider wedges the turn.
- X. Other (please specify)

[Answer]: A — connect 10s / total 300s; ≤1 retry connect-phase-only; no redirects; no stream-stall retry; stable ProviderError.code. Conductor-decided (2026-10-27); ratify at PR review.

## Q8. Adapter test isolation?

- A. Wiremock HTTP fixtures only (BR5.3): golden request bodies, canned
  SSE/NDJSON replies covering the full ChatStreamItem vocabulary,
  error-code mapping, timeout path; never a live provider, no network
  egress in tests (NFR9 collect-only coverage via the closed PROCEDURES
  set).
- B. Live calls behind env flag — needs keys in CI, flaky; forbidden.
- X. Other (please specify)

[Answer]: A — wiremock fixtures only; no live provider, no egress; NFR9 collect-only via closed PROCEDURES. Conductor-decided (2026-10-27); ratify at PR review.

## Assumption Confirmation

> Recorded under standing instruction (2026-10-27); ratify at PR review.
- Provider latency/throughput is external — U4's performance section
  bounds OUR overhead (gate read, rebuild, swap) not provider SLAs.
- `stub` stays registry-internal; its test use needs no custody rules.
- The `admin.oxpilot.*` gate-exempt recovery path is intentional (BR1.1).

## Consolidated Summary Confirmation

- Scope: all nine NFRs at standard depth (Q1).
- Custody: crypto.rs AES-256-GCM under the OXPILOT key chain; write-only
  keys, decrypt-in-construction-only (Q2).
- SSRF: http(s), literal+DNS public check, no redirects, TOCTOU bound,
  Ollama exempt (Q3).
- Swap: RwLock slot + rebuild mutex + shared Arc<ActiveTurns>; boot-time
  initial build (Q4); env-degraded on unreadable row (Q5).
- Observability: field-names-only audit, attr-only spans, coded terminal
  frames, no secret logging (Q6).
- Reliability: connect 10s/total 300s, connect-phase-only retry (Q7).
- Testing: wiremock only, closed PROCEDURES collect-only coverage (Q8).

Does this all look correct before I write the NFR artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
