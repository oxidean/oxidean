# NFR Requirements Questions — session-core

Unit: `session-core` (U2, kind `service`) — `SessionService` trait port,
the four new tables (tool_call/queued/proposal/usage_counter) ×3
dialects, the C8 delete-hook seam, redaction boundary, union dedupe,
and UTC-day budget counters with `BudgetExceeded` refusal.

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U2.
- [req] requirements.md — NFR1 (session-only auth + redaction custody),
  NFR2 (per-user isolation, plaintext-after-redaction disclosure),
  NFR3 (transcript responsiveness at a 500-message session proxy),
  NFR4 (no silent stalls; single-replica limiter caveat),
  NFR6 (reuse observability surfaces), NFR9 (collect-only coverage +
  dialect test row).
- [design] construction/session-core/functional-design/{entities,rules,
  functional-spec}.md — BR1–BR7 the NFRs bind to.
- [contract] contract-summary.md — C5 service surface, C8 hooks.

## Q1. Which NFR domains does session-core carry real obligations in?

- A. Security (scoping, redaction custody, no-secret-leak), Privacy
  (per-user isolation), Performance (seq minting, transcript reads at
  500-message scale, queue_list poll, drain atomicity cost),
  Reliability (orphan sweep interplay, drain crash window, hook-fail
  abort, refusal-not-drop), Scalability (single-replica limiter
  caveat + counter/index shape), Observability (reuse existing
  surfaces, no new stack), Testability (dialect test + proc pin).
  All assessed at standard depth; empty domains recorded as N/A rather
  than invented.
- B. Security only — misses the 500-message proxy NFR3 names and the
  FR6.7 limiter caveat.
- C. Defer NFRs to U3/U8 — the persistence NFRs are this unit's.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — all nine domains assessed at standard depth; populated where the unit has real obligations. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q2. Scoping enforcement posture?

- A. Service-layer `user_id` on every call, `not_found` anti-enumeration
  (BR1.1); proc layer SessionOnly + `oxpilot.disabled` gate (BR1.3);
  no elevated read path exists — the admin surface deliberately has no
  transcript reader in v1 (privacy: admins can't read user chats
  through a backdoor proc).
- B. Admin transcript-reader proc for support — invents a privileged
  path NFR2's per-user isolation doesn't authorize.
- C. RLS/DB-row security — project pattern is service-layer scoping;
  tri-dialect RLS parity is out of band.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — service-layer caller scoping + not_found; no admin transcript reader in v1. Conductor-decided (2026-10-27); ratify at PR review.

## Q3. Redaction custody boundary?

- A. `oxidean_core::redact()` at the service boundary on every
  body-bearing write — content, queued content, proposal title/body,
  tool summaries, goal/title (BR3.1). Stored form is post-redaction
  plaintext (NFR2's disclosed model). No raw-input escape hatch; the
  detector is the single pattern table U1 ships.
- B. Redact at proc layer too — service boundary already covers all
  writers including non-proc callers (U3, U5); double-redacting is
  idempotent anyway, but the rule belongs at the service.
- C. Redact on read — stored plaintext violates at-rest intent.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — `redact()` at the service boundary on every body-bearing write; stored form is disclosed post-redaction plaintext. Conductor-decided (2026-10-27); ratify at PR review.

## Q4. Performance envelope?

- A. Transcript read returns the full non-streaming set as ONE
  index-covered scan ordered by seq (BR7.2 pins `Vec<Message>` —
  windowing is the U3 caller's concern, not service pagination); the
  NFR3 proxy binds the query shape so a 500-message session reads
  trivially inside the interactive budget;
  `queue_list` is index-covered by (session_id, queue_seq); `seq` and
  `queue_seq` mint under the session's insert (per-session
  serialization, no global counter); drain is one bounded transaction
  sized by queue depth — no N per-row round trips; usage upsert is
  O(1) per scope.
- B. Service-layer pagination of the transcript — would violate
  BR7.2/C5's `Vec<Message>` full-set signature.
- C. Add caching layer — premature; one indexed scan suffices.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — transcript returns the full non-streaming set as ONE index-covered scan (BR7.2 pins Vec<Message>; windowing is the U3 caller's), index-covered queue_list, per-session seq minting, single-tx drain, O(1) counter upsert. Conductor-decided (2026-10-27); ratify at PR review. Refined at iter-1 review (R-02): 'bounded page' would have contradicted the C5 signature — the perf claim binds the query shape, not row count.

## Q5. Reliability posture for the new seams?

- A. Boot-time orphan sweep already marks stranded `streaming` rows
  `error` (U1) — U2 extends nothing there; the drain is atomic
  claim+delete+append so crash windows are closed (BR4.2) plus the
  turn-start residual rule; hook failure aborts delete loudly
  (BR5.2); `BudgetExceeded` is a typed refusal, not a drop (BR6.2);
  single-replica limiter caveat documented (FR6.7/NFR4).
- B. Add retries/queue for hook failures — C8 pins abort, not retry.
- C. Two-phase delete — over-engineering; cascade is one statement.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — atomic drain + residual rule, abort-on-hook-fail, typed budget refusal, documented single-replica caveat. Conductor-decided (2026-10-27); ratify at PR review.

## Q6. Observability surface?

- A. Reuse existing tracing/logging — span attrs on service calls
  (session_id only, never content); audit_events for session_delete
  (user-initiated destructive op; field names only, no content);
  `record_usage`/`check_budget` counters are the spend telemetry;
  no new observability stack (NFR6).
- B. Per-message audit rows — audit spam; C8-level events (delete)
  suffice.
- C. New metrics pipeline — NFR6 forbids.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — span attrs carry ids only, audit row for session_delete, counters-as-telemetry; no new stack. Conductor-decided (2026-10-27); ratify at PR review.

## Q7. Scalability limits to document?

- A. Single-process limiter semantics per FR6.7 — `check_budget` reads
  counters under the app process; multi-replica deployments share the
  DB counters but the check/act window can over-admit across replicas
  (documented caveat, NFR4); per-session mints serialize on the
  session's unique-key insert — horizontal parallelism unaffected.
- B. Distributed locking — far beyond need for v1.
- C. No limits — dishonest; FR6.7 names the caveat.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — document the single-process limiter caveat + per-session mint serialization; counters shared across replicas via the DB. Conductor-decided (2026-10-27); ratify at PR review.

## Q8. Testability posture?

- A. `dialect_oxpilot` test extends to the four new tables (joins the
  `db-dialects` -E filter + TESTING.md row per NFR9); closed-set
  PROCEDURES pin test updates for +5 procs; integration coverage:
  message_queue 409→queue→drain flow, union dedupe, delete cascade +
  hook-abort probe, budget refusal, redaction assertions.
- B. Unit tests only — misses dialect parity + proc pin regressions.
- C. Live-SQL manual testing — NFR9 wants machine-checked coverage.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — dialect test extension + PROCEDURES pin update + integration coverage for queue/dedupe/delete/budget/redact. Conductor-decided (2026-10-27); ratify at PR review.

## Consolidated Summary Confirmation

- Domains: all nine assessed; security/privacy/perf/reliability/
  scalability/observability/testability carry real obligations (Q1).
- Scoping: service-layer caller scope + not_found; no admin transcript
  reader (Q2).
- Redaction: service-boundary `redact()` on all body-bearing writes (Q3).
- Performance: full-set transcript as one index-covered scan (BR7.2),
  index-covered queue_list, per-session seq mints, single-tx drain,
  O(1) counters (Q4, iter-1 refined).
- Reliability: atomic drain + turn-start residual, abort-on-hook-fail,
  typed refusal, single-replica caveat (Q5).
- Observability: id-only spans, audit row on delete, counters as
  telemetry (Q6).
- Scalability: limiter caveat + per-session serialization documented
  (Q7).
- Testing: dialect row + proc pin + integration suite (Q8).

Does this all look correct before I write the NFR artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
