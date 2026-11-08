# NFR Requirements Questions — connector-gateway

Unit: `connector-gateway` (U6, kind `service` — greenfield) —
`ConnectorGateway`: pairing mint/redeem, target registry, verifier-class
connector auth, consent grants (connect + action_once/action_session),
action queue (dispatch → claim → report), disconnect/revoke, session-
delete hook (C8), audit transitions, the user-facing `oxpilot.target_*` /
`oxpilot.grant_*` procs, and the C3 connector HTTP surface.

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U6.
- [req] requirements.md — NFR1 (session-only auth; connector auth
  PAT-class w/ hashed verifier; no secret leaves), NFR2 (per-user
  isolation; egress disclosure), NFR3 (claim-side perf bounds),
  NFR4 (connector disconnects degrade
  gracefully), NFR5 (audit_events for actions/approvals/lifecycle),
  NFR6 (reuse observability), NFR9 (`dialect_oxpilot` + db-dialects
  `-E` row; connector outbound-HTTP fixture tests; SessionOnly pin).
- [design] construction/connector-gateway/functional-design/{entities,
  rules, functional-spec}.md — BR1–BR7 the NFRs bind to.
- [contract] contract-summary.md — C1 RPC surface (+amendments), C3
  connector protocol, C5 service, C7 dispatch seam, C8 hooks.

## Q1. Which NFR domains does connector-gateway carry real obligations in?

- A. Security (verifier custody — hash-on-write, never returned;
  pairing code shown once, single-use, expiry-before-state; fail-
  closed auth + per-IP failed-auth limiter (BR2.1) + per-user mint limit (BR1.5); consent enforced
  server-side; redacted bounded audit), Privacy (per-user scoping on
  every read/mutation; no cross-user enumeration — `not_found`), 
  Performance (claim long-poll ≤ 60 s clamp; bounded claim payload
  per connector; FIFO by `created_at`), Reliability (report
  idempotent by `action_id`; terminal-state CAS first-wins; at-most-
  once claim documented; revocation/cancel propagation paths),
  Observability (`audit_events` vocabulary + lifecycle logs on
  existing surfaces), Testability (dialect test + fixture + pin
  tests). Scalability recorded N/A beyond per-target lock not
  serializing globally; Accessibility/Usability N/A (U8's surface —
  U6 contributes only named error codes).
- B. Security only — misses NFR4's graceful-degrade and NFR9's dialect/
  fixture rows which are this unit's explicit contract.
- C. Include Accessibility — no user-facing render lives here; U8 owns
  WCAG.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — the six named domains carry real pins; Scalability N/A beyond per-target-lock-not-global, A11y/Usability N/A (U8's surface, U6 contributes named error codes only)

## Q2. Verifier custody and connector authentication posture?

- A. Verifier minted at `/pair` (pairing-code redeem), returned ONCE
  plaintext in `{connector_id, target_id, verifier}`; stored only as
  `SHA-256(verifier)` on the target row — never returned by any RPC/
  HTTP/audit surface thereafter. Claim/report/heartbeat Bearer check =
  hash → find-by-hash lookup (shipped `sha256_hex` idiom). Failed-auth
  feeds the per-IP `FailedAuthLimiter` (BR2.1); the `Target`
  row's `lifecycle`/`last_seen_at` never double as credentials.
  Rotation = disconnect + re-pair only.
- B. Store a second encryptable column like the target secret custody
  (FR5.4's AES) — verifier is authentication, not user secret;
  hashing suffices and nothing can recover it to the wire.
- C. Include verifier in `target_get` for the owner's re-display — C1
  pins "secret material never in payload"; re-pair is the recovery.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — verifier SHA-256 only, shown once at /pair, shipped find-by-hash lookup, re-pair is the sole rotation; never on any RPC/HTTP/audit surface

## Q3. Claim long-poll and liveness performance budget?

- A. `wait_seconds` clamped to `CLAIM_WAIT_MAX = 60 s` (over-60
  clamps, never rejects — BR5.1/C3); long-poll wakes on dequeable
  action or timeout — 200 returns C3's pinned `{actions[], cancel[],
  retry_after}`, timeout returns `204` + `Retry-After` header (no
  body); a per-claim `CLAIM_ACTIONS_MAX = 50` cap is an NFR-introduced
  bound, flagged honestly. Liveness = `last_seen_at` touched on ANY
  authenticated connector touch (claim/report/heartbeat — never
  `/pair`); `online` iff `last_seen_at` within
  `HEARTBEAT_LIVENESS_WINDOW = 90 s` (≈3× the client interval).
  Dequeue marks `claimed` only — `running` flips on the connector's
  NEXT authenticated touch (BR4.3).
- B. Poll-free push (websocket/SSE to connector) — the connector is
  outbound-only with no listener; long-poll IS the push channel.
- C. Return unbounded action lists per claim — per-claim bound keeps
  one connector from monopolizing; claim loop pulls the rest next
  window.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — wait_seconds clamped to CLAIM_WAIT_MAX=60s (clamps, never rejects); response shape {actions, cancel, retry_after} verbatim per C3 with 204+Retry-After on timeout; CLAIM_ACTIONS_MAX=50 is an NFR-introduced bound (flagged, not fd-pinned); liveness on any authenticated touch; online iff last_seen_at within HEARTBEAT_LIVENESS_WINDOW=90s

## Q4. Action-state integrity under concurrency and retries?

- A. Per-target admission lock serializes dispatch/grant_respond/
  revoke/close decisions on that target (fd Q4/Q5). Report is
  idempotent keyed by `action_id`: repeat terminal reports are no-ops
  returning success; terminal-state CAS (`queued→claimed→running→
  done|failed`; cancel paths) is first-wins — a `running→done` CAS
  loses to a concurrent `→failed`, second report is the idempotent
  ack. Lost claim = action stranded in `claimed`/`running` —
  accepted at-most-once residual, visible via `action_status`; no
  server auto-requeue. Report-404 (unknown `action_id`) = client
  kills local exec (post-delete / revoked arms).
- B. Server auto-requeues a `claimed` row on heartbeat-miss — breaks
  at-most-once (a live-but-partitioned execution would run twice).
- C. Keep a `cancel_requested` delivery queue for cascade-deleted
  sessions — reconciled design: direct `→cancelled` on rows whose
  session is being deleted; report-404 IS the kill signal.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — per-target admission lock serializes grant/dispatch/close; report idempotent on action_id; terminal CAS first-wins; stranded-claim is the accepted at-most-once residual (visible via action_status); report-404 kills local exec

## Q5. Persistence and multi-dialect obligations?

- A. `dialect_oxpilot.rs` (already shipped + registered in
  `db-dialects -E oxpilot` + `docs/TESTING.md` by U1/U2) is EXTENDED
  for the new connector tables — and the `docs/TESTING.md` row is
  updated in the same change —
  `docs/TESTING.md` job table IN THIS CHANGE (NFR9's dead-coverage
  clause). Dialect-conditional rows for the new connector tables: targets +
  pairings (FK to users; `ON DELETE CASCADE` on session-owned grant/
  action rows per AC1.5.2, `audit_events` separate/no-FK per US5.3);
  unique indexes enforce one live pairing per target-mint and FIFO
  claim order (`created_at, action_id` tiebreak). SQLite/Postgres/
  MySQL column + upsert idioms follow the shipped
  `dialect_oxpilot.rs`/`dialect_vcs.rs` precedents.
- B. Postgres-only with a TODO — NFR9 pins the same-change rule;
  unregistered dialect coverage is dead coverage.
- C. FK `audit_events` to actions for cascades — audit must survive
  session delete (AC1.5.2 keeps `audit_events`; they're the consent
  evidence).
- D. Not decided.
- X. Other (please specify)

[Answer]: A — extend the shipped dialect_oxpilot.rs (already in db-dialects -E + TESTING.md) for connector tables + update the TESTING.md row in this change; CASCADE for session-owned rows (AC1.5.2), audit_events no-FK (US5.3); FIFO index (target_id, state, created_at, action_id)

## Q6. Audit and observability vocabulary — what must be emitted?

- A. `audit_events` rows on BR7.2's pinned set verbatim:
  `pairing_created`/`pairing_claimed`/`pairing_expired`/`pairing_used`;
  `connect_grant_pending|allowed|denied|expired` (pending covers lazy
  re-mints); `action_grant_pending|allowed|denied|expired`;
  `grant_revoked`/`grant_cancelled`; `action_dispatched`/
  `action_result{done|failed}`/`action_cancelled`;
  `target_connected`/`target_disconnected`/`target_revoked`;
  `connector_auth_failed`.
  Detail = bounded JSON: actor user_id, target_id, grant/action ids,
  kind, `reason` (bounded enum), `args_summary`/`result_summary` ≤
  SUMMARY_MAX/RESULT_MAX — NEVER
  verifier hash, pairing code, raw args, or unredacted output.
  Emission rides shipped fire-and-forget `audit::record`. Logs on the
  existing `tracing` surface (`oxpilot_connector` target) — no new
  stack (NFR6).
- B. Log full args in audit detail — bounded+redacted only; the bounded redacted
  stored args live only inside the pending-grant row until
  resolution.
- C. Metrics counters for claim latency etc. — nice-to-have; the
  pinned surface is audit_events + existing tracing.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — BR7.2's pinned vocabulary verbatim (pairing_created/claimed/expired/used, connect_grant_* + action_grant_* lifecycle, grant_revoked/grant_cancelled, action_dispatched/action_result{done|failed}/action_cancelled, target_* lifecycle, connector_auth_failed); detail = actor+ids+kind+reason+bounded summaries, never codes/verifiers/raw-args; fire-and-forget audit::record; tracing on oxpilot_connector target

## Q7. Failure-mode matrix — graceful degradation pins (NFR4)?

- A. Disabled-`oxpilot` → procs return `oxpilot.disabled`, C3 routes
  refuse `{error:"disabled"}` fail-closed (BR2.5); no target/grant
  mint. Offline = stale `last_seen_at` only → `Denied{offline}`;
  `pending_consent`/`disconnected`/`revoked` → `Denied{not_connected}`.
  Disconnect TOMBSTONES the verifier — every connector route answers
  401 `{error:"unauthorized"}`; pending grants + queued + claimed/
  running → `cancelled` DIRECTLY (dead channel, no cancel
  deliverable). Revoke KEEPS the verifier: heartbeat answers
  `{revoked:true}` + cancel set; claim/report refuse
  `{error:"revoked"}` EXCEPT report's terminal-close arm; in-flight
  → `cancel_requested`. Session delete → U3 abort hook first, then
  U6 hook: the session's grants pending/allowed + actions
  queued/claimed/running → `cancelled` DIRECTLY (reason
  `session_deleted`; rows cascade-delete so `cancel_requested` could
  never be delivered) — all BEFORE the FK cascade; `audit_events`
  survive; connect grants (`session_id NULL`) untouched. Report on
  cascade-deleted action → `not_found` (the kill-local signal).
- B. Silently drop reports on missing actions — the report-404 kill-
  local signal IS the cancellation path for cascade-removed rows.
- C. Keep cancelled rows for a grace window — AC1.5.2 pins row
  removal; the audit row is the surviving evidence.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — disabled → proc oxpilot.disabled + C3 {error:disabled} fail-closed; offline(stale last_seen_at) → Denied{offline}, pending_consent/disconnected/revoked → Denied{not_connected}; disconnect tombstones verifier (401 unauthorized everywhere, claimed/running→cancelled directly); revoke keeps verifier (heartbeat {revoked:true}+cancel set, claim/report {error:revoked} EXCEPT report's terminal-close arm, in-flight→cancel_requested); session-delete = abort-hook → grants+actions→cancelled direct → FK cascade → audit survives; report-404 = kill-local

## Q8. Test-surface obligations?

- A. Pin tests: `SessionOnly` classification for all `oxpilot.target_*
  /grant_*` procs; `procedures_list_matches_dispatch`; C3 routes NOT
  session-cookie auth (verifier-Bearer pin test); `pairing_code`
  appears in no store/audit row after redeem (secret-scan test on
  the store's projection). Integration: pairing mint→redeem→claim→
  dispatch→report happy path; expiry-before-state; once-consume;
  session-scope reuse; revoke+heartbeat propagation; cross-user
  denial; report idempotency; dialect round-trip. Fixture: outbound
  HTTP simulator hitting the four C3 routes (NFR9).
- B. Rely on U8's UI tests for coverage — U8 tests the render; the
  auth/consent/state-machine pins belong at this layer.
- C. Skip dialect tests until U7 — NFR9 says same-change or dead.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — SessionOnly pin, verifier-not-cookie pin, no-secret-in-store pin, procedures-dispatch test; happy-path + expiry + once-consume + session-scope + revoke-propagation + cross-user + idempotent-report integration; outbound-HTTP fixture for C3

## Consolidated Summary Confirmation

- Security: verifier hash-only custody, single-show pairing code,
  fail-closed auth, consent server-side, bounded redacted audit (Q1, Q2).
- Privacy: per-user scoping, not_found anti-enumeration (Q1).
- Performance: claim clamp [0,60]s, bounded claims + retry hints,
  HEARTBEAT_LIVENESS_WINDOW = 90 s liveness window (≈3× client interval) (Q3).
- Reliability: per-target lock, idempotent report, CAS first-wins,
  at-most-once residual, report-404 kill-local (Q4).
- Persistence: tri-dialect same-change, CASCADE session rows, audit
  no-FK (Q5).
- Observability: pinned audit vocabulary, existing tracing (Q6).
- Failure matrix: named degradation, ordered session-delete (Q7).
- Tests: SessionOnly/verifier/no-secret pins, integration matrix,
  outbound-HTTP fixture (Q8).

Does this all look correct before I write the NFR artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
