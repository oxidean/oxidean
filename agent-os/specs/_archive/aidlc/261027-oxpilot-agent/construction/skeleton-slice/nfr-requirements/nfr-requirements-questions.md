# NFR Requirements — Questions (u1-skeleton-slice)

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U1.
- [fd] `construction/skeleton-slice/functional-design/` entities, rules,
  spec — BR1.x–BR5.x, ActiveTurn, enablement gate.
- [req] NFR1–NFR9 (inception requirements).
- [contract] C1, C2, C4.
- [codekb] technology-stack (Rust/axum, sqlx×3, Octane, axum-sse,
  tracing, audit.rs, crypto.rs, FailedAuthLimiter pattern).

## Q1. Which NFR categories does the skeleton carry?

- A. All nine at skeleton depth — NFR1–4 in full, NFR5 reduced to
  session-lifecycle audit only, NFR7 reduced to semantic-minimal page
  posture, NFR6/NFR8/NFR9 in full (they are cheap invariants).
- B. Security + testability only — defer the rest to owning units.
- C. None — the skeleton proves the round-trip; NFRs are U2+ scope.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — all nine at skeleton depth; NFR5 = session-lifecycle
audit only (no connector/approval surfaces exist yet), NFR7 = landmark/
heading/labels baseline only (aria-live choreography is U8).
Conductor-decided under standing instruction (2026-10-27); ratify at PR
review.

## Q2. Performance targets for the stub round-trip?

- A. Contract-level pins only: first frame after POST-accept < 250 ms on
  a warm process (the stub is in-process, no network), snapshot on GET
  < 250 ms at ≤500 persisted messages (a single indexed read), deltas
  bounded so 60 s streams never buffer O(n²). Wall-clock provider
  budgets stay U3/U4 scope.
- B. Full provider-latency budgets now — premature; no real provider.
- C. No targets — unverifiable.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — stub-measurable pins (250 ms first frame/snapshot,
bounded buffers); provider latency stays with the adapter units.
Conductor-decided under standing instruction (2026-10-27); ratify at
PR review.

## Q3. Security specifics beyond the functional design?

- A. Confirm and sharpen: `SessionOnly`-by-omission + closed-set test
  (BR1.2); anti-enumeration `not_found` on every session-scoped path
  (BR3.3); redaction ships via a NEW `redact()` detector util in
  `oxidean-core` authored by this unit (review correction: no existing
  secrets-pattern filter exists — the runner's `mask_secrets` is
  known-value masking; BR3.5); the stub provider slot carries NO key
  material —
  `OXIDEAN_OXPILOT_MODEL`/`_PROVIDER` are config, not secrets; SSE
  routes get the same auth gate as RPC (BR4.1); audit `detail` never
  carries message content.
- B. Add rate limiting on chat POST now — belongs with U2's budgets
  (FR6.7), not the skeleton.
- C. Defer redaction to U2 — NFR1 says "applied to stored transcript
  content," which U1 already stores.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — sharpened set as listed; per-message budgets are FR6.7
and stay U2 scope. Conductor-decided under standing instruction
(2026-10-27); ratify at PR review.

## Q4. Audit surface in the skeleton?

- A. Session create emits `audit_events` fire-and-forget
  (existing audit.rs); delete is not a U1 proc (review correction:
  skeleton ships exactly 3 procs — delete audit lands in U2); chat
  sends are not per-message audited — the
  persisted transcript is the record; detail carries ids/owner/repo,
  never message content.
- B. Audit every message + stream open — volume without value; the
  transcript already records it.
- C. No audit in U1 — auditability NFR5 has a skeleton share.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — create-only audit (delete is U2's proc), content-free
detail. Conductor-decided under standing instruction (2026-10-27);
ratify at PR review.

## Q5. Observability additions in the skeleton?

- A. Reuse `tracing` only — `oxpilot` target on gate denials, turn
  lifecycle (claim/terminal/409/orphan-sweep), and SSE open/close; no
  new metrics/exporters, no dashboards.
- B. Add Prometheus counters for turns — no metrics infra exists to
  reuse; inventing one violates NFR6.
- C. None.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — tracing only, named signals listed. Conductor-decided
under standing instruction (2026-10-27); ratify at PR review.

## Q6. Reliability pins for the skeleton?

- A. Orphan sweep at startup (stale `streaming` rows → `error`);
  detach-to-complete turn lifecycle (BR4.6); single-replica ActiveTurn
  caveat documented in `docs/CONFIGURATION.md` beside the env block;
  stub failure path exercised by an injected-failure test hook.
- B. Multi-replica turn locking — out of scope; the map is per-process
  by design.
- C. None.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — sweep + detach-to-complete + documented caveat + a
test-only failure injection for the stub. Conductor-decided under
standing instruction (2026-10-27); ratify at PR review.

## Consolidated Summary Confirmation

- Scope: all nine NFRs at skeleton depth; NFR5 reduced to
  session-lifecycle audit, NFR7 to baseline semantics (Q1).
- Perf: 250 ms first-frame/snapshot pins + bounded buffers (Q2).
- Security: SessionOnly pin, anti-enumeration, redaction, gate
  parity on SSE, no key material in U1 (Q3).
- Audit: create only, content-free (Q4).
- Observability: tracing only (Q5).
- Reliability: orphan sweep, detach-to-complete, replica caveat,
  failure-injection hook (Q6).

Does this all look correct before I write the NFR artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
