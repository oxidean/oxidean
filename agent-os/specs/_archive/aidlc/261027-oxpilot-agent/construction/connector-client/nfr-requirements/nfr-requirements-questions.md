# NFR Requirements Questions — connector-client

Unit: `connector-client` (U7, kind `service` — greenfield) —
`OxpilotConnector`: `ox pilot pair` / `run` / `disconnect` / `status`,
verifier custody (keyring + `pilot-secrets.json` fallback), the claim →
execute → report long-poll loop, heartbeat, error-code-keyed terminal
taxonomy, serial exec/fs executor with `--root` jail and config-dir
hard exclusion, output cap + secret-value scrub, cancel honoring,
signal close-out, and the client-side C3 contract surface
(`openapi.yaml`, wiremock fixtures).

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U7.
- [req] requirements.md — NFR1 (connector auth PAT-class w/ hashed
  verifier server-side; no secret leaves the system unhashed — the
  client side is its custody mirror), NFR2 (egress disclosure — the
  connector's report output is the last hop before server→provider
  egress), NFR4 (connector disconnects degrade gracefully — this unit
  IS the connector: its backoff/terminal behavior is the degrade
  contract), NFR6 (reuse existing observability — `--verbose` +
  stderr lifecycle lines on CLI conventions), NFR9's client half
  (connector outbound-HTTP fixture tests — the wiremock suite).
- [design] construction/connector-client/functional-design/{entities,
  rules, functional-spec}.md — BR1–BR9 the NFRs bind to; READY on
  adversarial review iteration 3.
- [contract] contract-summary.md — C3 wire shapes, C8 hook ordering,
  the connector-client procedure deltas.

## Q1. Which NFR domains does connector-client carry real obligations in?

- A. Security (client-side verifier custody — two-backend seam,
  never serialized/logged; jail + config-dir hard exclusion;
  OXIDEAN-scoped env strip; secret-value output scrub), Privacy
  (output is the last pre-egress surface — BR7.2's enumerated scrub
  is a privacy control, not just hygiene), Reliability (claim loop
  survives transport/feature flaps; idempotent reports; exactly-once
  cancel close-out; signal close-out kills in-flight first),
  Performance (long-poll pacing, floors against instant-answer
  spins, output/fs caps honoring server bounds), Observability
  (`--verbose` redacted request logs, stderr lifecycle, exit codes
  as the supervisor contract), Testability (wiremock fixtures +
  executor unit tests), Usability (CLI error text, `--help`, docs
  honesty — disconnect local-forget).
- B. Security only — misses NFR4's degrade contract (the client's
  retry/terminal table IS the graceful-degradation mechanism) and
  NFR9's fixture-test leg which names this unit explicitly.
- C. Include Accessibility — a CLI has no WCAG surface; U8 owns it.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — the seven named domains carry real pins; Accessibility N/A (CLI surface, U8 owns WCAG), Scalability N/A (single-process serial executor by contract)

## Q2. Client-side verifier custody — the security posture?

- A. Verifier held via the two-backend seam (BR2.4): keyring under
  composite instance key `pilot/<instance>/<connector_id>` first,
  `pilot-secrets.json` 0600 as defined headless fallback — pairing
  aborts only when BOTH fail; never in `pilot-targets.json`, never
  in `config.json`, never printed/logged/serialized (status/`--json`
  omit it structurally); deleted on disconnect, BR8.4 terminals,
  BR8.3 auth death; a row whose verifier is gone is a named zombie
  (BR9.3), never a silent skip.
- B. Verifier in `pilot-targets.json` beside metadata (0600): one
  artifact, but the file the design calls "safe to leak" then
  carries the one thing that authenticates — Q3-B rejected this at
  functional design.
- C. New dedicated secrets crate: reinvents the keystore seam the
  codebase already ships; adds a dependency for a solved problem.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — two-backend custody per BR2.4: keyring-first + defined file fallback, never beside metadata, deleted on every terminal path, zombie rows named not skipped

## Q3. Egress/privacy posture for report output — what must the client guarantee before send?

- A. Output is the last client-controlled surface before the forge's
  transcript store and (via U5 dispatch) the LLM provider's context:
  universal `output < OUTPUT_MAX` (256 KiB wire bound), exec tail ≤
  64 KiB + truncation marker, fs payloads ≤ FS_IO_CAP (128 KiB raw,
  oversize → `failed:too_large` never truncated `done`), and BR7.2's
  enumerated value scrub (verifier + `*TOKEN*`/`*KEY*`/`*SECRET*`/
  `*PASS*` env values ≥8 chars + `config.json` `instances.*.token` +
  `pilot-secrets.json` values) — the server redactor is the second
  pass (FR5.5), not the only one.
- B. Trust the server-side redactor alone: leaves the `cat
  config.json` residual and any child-emitted Oxidean secret
  plaintext into the forge's stored transcript before redaction —
  violates FR5.4's "never return stored secret material" client
  half.
- C. Scan output for generic secret patterns (entropy/regex sweeps):
  false-positive shreds legitimate output, unpredictable latency —
  the enumerated-value approach is deterministic and complete for
  secrets the client itself holds.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — bounded output honoring server OUTPUT_MAX + BR7.2 enumerated-value scrub as the client-side egress control; server redactor is second pass

## Q4. Reliability posture for the run loop — what degrades and how?

- A. The code-keyed taxonomy IS the degrade contract (NFR4 client
  half): `disabled`/`internal`/unknown → exponential 1s→60s ±20%
  backoff with creds preserved, loop stays alive for supervisors;
  `rate_limited` → Retry-After honored; `unauthorized`/off-pair
  `forbidden` → kill in-flight, delete creds, exit non-zero;
  `revoked`/claim-heartbeat `not_found` → kill + report + delete +
  exit 0; `bad_input` → kill in-flight, exit non-zero, creds kept;
  transport errors never kill the loop. Reports idempotent by
  action_id (safe repost); every announced cancel id closed exactly
  once incl. never-claimed `cancelled:pre-start`; signal close-out
  kills child first, reports `cancelled:<sig>`, exits 0.
- B. Exit on any error and let a supervisor restart: strands
  in-flight actions server-side, crash-loops on `disabled` flaps,
  and a `revoked` target exits non-zero (failure-signaled success).
- C. Retry forever silently: a dead server keeps the loop alive
  either way, but silent retry gives the operator no signal —
  backoff must be observable (--verbose/lifecycle lines).
- D. Not decided.
- X. Other (please specify)

[Answer]: A — code-keyed terminal/backoff taxonomy + idempotent reports + exactly-once cancel close-out + signal close-out; loop stays alive on transient failures for supervisor visibility

## Q5. Pacing/performance posture — the client's own bounds?

- A. Claim `wait_seconds` client-clamped [1,60] (server clamps top
  only; 0 = instant-answer spin), REQ_TIMEOUT claim =
  wait_seconds+15s / others 30s, heartbeat 15s inside the 90s
  liveness window, cancel-only re-poll floor 1s (server answers
  instantly with cancel sets), output/fs caps sized under the
  server's 256 KiB OUTPUT_MAX, serial executor = zero parallelism
  surface to bound.
- B. Client sends raw flag values unclamped: `--wait-seconds 0`
  spins at RTT speed and >60 sends an illegal value the server
  silently rewrites — the client must own a legal send.
- C. Faster heartbeat (5s) for quicker cancel discovery: heartbeat
  cancels ride BR5.1 exactly like claim cancels but 3× chattier for
  no real gain — the claim long-poll already carries the cancel set
  in-band.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — client-clamped [1,60] claim wait, +15s claim timeout, 15s heartbeat, 1s cancel-only floor, caps under OUTPUT_MAX; serial executor bounds itself

## Q6. Observability/usability posture for a headless CLI?

- A. stderr lifecycle lines on state transitions (paired, claimed,
  reported, cancelled, terminal) + `--verbose` request/response
  logging on pair/run with `pairing_code`/`verifier`/`Authorization`
  redacted (BR1.4's class); exit codes are the supervisor contract —
  0 = clean terminal (revoked/signal), non-zero = auth death, bad
  input, missing creds, pairing failure; `status` prints metadata
  only; `--help`/docs pin disconnect local-forget, the jail
  semantics, the env strip, and every bound. No metrics surface —
  server owns audit.
- B. Structured JSON logging always: `ox pilot` is a human-operated
  CLI first; `--json` exists for `status` but the loop's stderr is
  for operators — plain lines with redaction beats a log pipeline
  nobody runs.
- C. Silent loop (verbose-only output): an unattended connector that
  says nothing gives operators no flap/degrade signal — lifecycle
  lines are the NFR6 reuse-of-conventions answer.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — redacted --verbose + stderr lifecycle + exit-code supervisor contract + docs honesty; no metrics surface (server owns audit)

## Q7. Testability posture — what must be provable without a live forge?

- A. wiremock fixture suites mirroring `tests/rpc_http.rs`
  conventions: each C3 route's request shape asserted (headers,
  Bearer, body), each response family mapped (200/204+Retry-After/
  flat error codes incl. disabled/rate_limited/unauthorized/
  revoked/not_found/bad_input), retry_hint honored, cancel sets
  applied; executor unit tests with no network — jail escape,
  config-dir exclusion, non-regular refusal, FS_IO_CAP too-large,
  env strip, output cap+scrub, cancel-kill, settled dedupe, zombie
  row, custody fallback. `openapi.yaml` checked in + validated.
- B. Integration tests against a live dev forge: a connector CI lane
  can't assume a running forge — fixtures are the NFR9 answer and
  they run everywhere.
- C. Manual verification only: the claim loop's error taxonomy is
  exactly the class that rots without fixtures — NFR9's client half
  is explicit.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — wiremock C3 suites (all four routes, full code vocabulary) + no-network executor unit tests + checked-in openapi.yaml; no live-forge dependency

## Consolidated Summary Confirmation

- Domains: Security, Privacy, Reliability, Performance,
  Observability, Testability, Usability — seven with real pins;
  Accessibility/Scalability N/A (Q1).
- Custody: two-backend verifier seam, never serialized, deleted on
  every terminal, zombie rows named (Q2).
- Egress: output < OUTPUT_MAX + BR7.2 enumerated scrub — client is
  the last pre-egress surface (Q3).
- Reliability: code-keyed taxonomy is the degrade contract;
  idempotent reports; exactly-once cancels; signal close-out (Q4).
- Pacing: [1,60] claim clamp, +15s claim timeout, 15s heartbeat,
  1s cancel-only floor, caps under OUTPUT_MAX (Q5).
- Observability: redacted --verbose + stderr lifecycle + exit codes
  + docs honesty; no metrics (Q6).
- Tests: wiremock C3 fixtures + no-network executor units +
  openapi.yaml (Q7).

[Answer]: Looks correct
