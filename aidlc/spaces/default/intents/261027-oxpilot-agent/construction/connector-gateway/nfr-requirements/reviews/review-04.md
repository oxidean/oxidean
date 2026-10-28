## Review

**Verdict:** READY
**Reviewer:** aidlc-architecture-reviewer-agent
**Date:** 2026-10-28
**Iteration:** 1

All iteration-3 fixes verified against ground truth: every basis cell resolves to a real BR1–BR7 pin, every named constant matches the functional-spec Pins verbatim, every state-machine claim matches rules.md/entities.md, and the questions file is free of the prior fabricated vocabulary in options, answers, and notes. Traceability is internally consistent — 9 upstream rows with correct residual notes, 77 reverse rows matching the 77 declared local ids exactly (unique, valid statuses, non-empty targets). Six Minor findings remain; none blocks implementation.

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| R-01 | Minor | nfr-requirements-questions.md > Q5 option A | Selected option asserts "SQLite/Postgres/MySQL column + upsert idioms follow the existing `dialect_sessions.rs` precedent" — no such file exists in `crates/oxidean-db/tests/`; the real precedent is `dialect_oxpilot.rs` itself or `dialect_vcs.rs`'s information_schema leg. Same fabricated-filename class as prior residues. | Repoint the precedent to `dialect_oxpilot.rs`/`dialect_vcs.rs`. | Resolved |
| R-02 | Minor | nfr-requirements-questions.md > Q2 option B | "(FR5.3's AES)" mis-cites the capability-bound clause — AES-256-GCM target-credential custody is FR5.4 (requirements.md). | Change to FR5.4. | Resolved |
| R-03 | Minor | scalability-requirements.md > NFR-S2 | "(shared with U3's limiter)" misattributes the single-replica caveat — U3 owns no limiter; the caveat is the shared deployment model, and this unit's limiters are BR1.5's per-user mint limit and BR2.1's per-IP `FailedAuthLimiter`. | Rephrase to the shared single-replica caveat without the wrong-unit attribution. | Resolved |
| R-04 | Minor | performance-requirements.md > NFR3.8 | "`TargetView.status = online` iff `last_seen_at` within `HEARTBEAT_LIVENESS_WINDOW`" is literally false for `revoked` (verifier still authenticates; entities pins revoked→`revoked`) and `pending_consent`/`disconnected` (→`offline` regardless). | Scope the iff to `lifecycle=connected`: "revoked → `revoked`; else online iff within window, else offline." | Resolved |
| R-05 | Minor | security-requirements.md > NFR1.14 | "lazy expiry at dispatch/read/claim stamps `expired` first" asserts an `allowed→expired` lazy stamp that BR3.1's closed transition set does not pin (`pending→expired` only; live states exit only to `revoked`/`cancelled`). | Scope the lazy stamp to BR3.1's pending arm; state the bound as BR4.1(4)'s allowed+unexpired live test. | Resolved |
| R-06 | Minor | observability-requirements.md Consumes; tech-stack-decisions.md Consumes; questions Sources | Header-source hygiene: observability Consumes cites "(NFR5, NFR6)" while NFR2.1's basis is NFR2+US4.6; tech-stack Consumes cites "(NFR1, NFR9)" while TS7's rationale cites NFR6; the questions [req] line omits NFR3 though the unit carries a full performance-requirements.md. | Add NFR2, NFR6, NFR3 to the respective Consumes/Sources lines. | Resolved |

### Validation Tool Results

| Tool | Result | Interpretation |
|---|---|---|
| aidlc-traceability sensor (manual application) | PASS | Shape valid; statuses in closed set; all upstream ids NFR1–NFR9 covered with residual notes on shared clauses; reverse has 77 rows = 77 declared local ids, no duplicates, no orphans; every BR pin cited resolves to rules.md BR1–BR7. |
| Constant cross-check vs functional-spec Pins | PASS | `CLAIM_WAIT_MAX = 60 s`, `HEARTBEAT_LIVENESS_WINDOW = 90 s`, `ACTION_ARGS_MAX = 2 KiB`, `SUMMARY_MAX = 512 B`, `RESULT_MAX = 2 KiB`, entropy ≥160/≥256 bits all verbatim; `CLAIM_ACTIONS_MAX = 50` honestly flagged NFR-introduced. |
| Fabricated-vocabulary grep sweep | PASS with 1 residue | Zero hits for prior fabrications except one residual `dialect_sessions.rs` filename (R-01). |
| State-machine cross-check vs rules.md/entities.md | PASS | Disconnect tombstones + direct `cancelled`; revoke retains verifier + `cancel_requested` via heartbeat + report terminal-close exception; session-delete order verbatim; dequeue recheck; `claimed→running` on next touch. |

### Summary

The artifact set is implementable as written against the functional design, contract, and shipped code. The verifier/hash custody, `/pair`-as-sole-unauthenticated, forbidden-vs-not_found split, `{error:"revoked"}` vs `{revoked:true}` shapes, tombstone-vs-retained-verifier endings, direct-cancel-on-dead-channel, dequeue recheck, and honest fire-and-forget audit all match their ground truths, and the NFR5 keyspace collision is resolved. The six tabled Minors plus fold-in nits (NFR1.4's wire-vs-row naming/`∈`, NFR4.11's lock-set scope, NFR1.1's `pairing_id` not_found, `dequeuable` spelling, TS7's `rand` note) were applied in the post-verdict fix pass under relaxed guard policy. Note: BR7.2's snake_case audit names diverge from the shipped dotted `oxpilot.*` convention — that divergence is fd-pinned and correctly mirrored here, not an NFR defect.
