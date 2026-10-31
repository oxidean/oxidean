### Review — nfr-design / connector-client

**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 3
**Verdict:** READY

### Review

### Prior findings

| ID | Prior finding | Resolution |
|----|---------------|------------|
| R-01 | Fabricated "stale-claim TTL re-queues" | Resolved |
| R-02 | Bare-await exec phase starves heartbeat/cancel | Resolved — Q3-A option and its `[Answer]` now pin the nested per-action `select! {child completion \| EXEC_TIMEOUT deadline \| heartbeat tick \| signal}`; no bare-await wording remains. Residual nit: consolidated summary still reads "serial exec" without naming the nested select — faithful but terse; non-blocking. |
| R-03 | Pair-route rows missing from dispatch table | Resolved |
| R-04 | Phantom "clap arg structs" | Resolved |
| R-05 | False keystore-fallback atomicity parity | Resolved — Q2-A now reads "strictly stronger than config.rs::save's non-atomic write; the tmp+rename precedent is self_update.rs's binary swap". Verified: `config.rs` `fs::write`+chmod (non-atomic); `self_update.rs` tmp+rename swap. |
| R-06 | Invented `Retry-After` precedent in device.rs | Resolved |
| R-07 | Jail launch check covered one `is_within` direction | Resolved |
| R-08 | `backend` recorded on identity row | Resolved |
| R-09 | New-dep gaps unnamed | Resolved |
| R-10 | `RateLimited{retry_after}` source unpinned | Resolved |
| R-11 | Corrupt pilot-secrets.json unclassified | Resolved |
| R-12 | Missing create-path compensation | Resolved |
| R-13 | Undocumented {0,1} exit-code divergence | Resolved |
| R-14 | BadInput `NoReport` vs cred-liveness funnel | Resolved — row now pins `KillInFlight; ReportCancelled; Exit(1)` with the cred-live rationale. Verified consistent with upstream BR8.3: its no-report clause is dead-cred-scoped, so reporting the killed child on a live cred does not conflict — it removes the prior self-contradiction and matches the revoked/signal close-out pattern. |
| R-15 | `settled` checks unpinned on exec edges | Resolved — both pins present and coherent: exec-entry `settled.contains → skip` (satisfies BR5.1's "reported cancelled:pre-start **without executing**"), child-completion `settled.contains → suppress` (BR7.4 one-report). Aligns with `apply_cancels`' insert-after-report ordering. |
| R-16 | `read_verifier` carries no backend for `status` | Resolved — signature is now `Result<Option<(Backend, SecretString)>>` and the status paragraph reports the backend from the same probe; consistent with the Backend-not-recorded pin. |
| R-17 | "cancel set" listed as a 5th exec arm | Resolved — now 4 arms with the cancel set annotated as the heartbeat arm's response payload; consistent with performance-design.md and logical-components.md. |
| R-18 | Backoff formula read clamp-then-jitter | Resolved — `delay = min(60, 2^n × (1 ± 0.2))` matches the jitter-before-clamp prose; 60s is the true ceiling. |
| R-19 | Exit-1 list missing corrupt secrets file | Resolved — "corrupt pilot-secrets.json" added. |
| R-20 | Crash-recovery said `running` only | Resolved — now `claimed`/`running` verbatim, matching U6-BR5.1's accepted residual; "no sweeper" verified (only lazy `action_grant_expired` audit markers at dequeue observation — no reaper closes claimed/running rows). |
| R-21 | Loop-only codes on pair route → Exit(0) | Resolved — "loop-only codes (pair)" row routes Unauthorized/Revoked/NotFound to the Internal/Unknown arm → Exit(1). Verified: `redeem_pairing` emits only bad_input/pairing_used/pairing_expired/forbidden/internal; the route layer adds disabled (gate) and bad_input (body/field bounds). `rate_limited` lives inside `resolve_verifier` — unreachable on pair, so the defensive RateLimited(pair) row is harmless coverage. |

### New findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|
| R-22 | Nit | reliability-design.md dispatch tables (totality, reverse direction) | The R-21 fix added a catch-all for loop-only codes arriving on the pair route, but the symmetric direction remains unpinned: `PairingExpired`/`PairingUsed` carry only a "(pair path)" arm — on a loop route (claim/report/heartbeat) the code would take `PairingReject; Exit(1)`, killing a healthy run loop on a code only `redeem_pairing` can emit. `classify(C3Error, route)` claims totality across both directions. Same impossibility class as R-21 (server bug only); the safe side would be Internal/Unknown (backoff, loop lives) rather than Exit(1). | Add a one-line symmetric pin, e.g. "pair-only codes on loop routes → Internal/Unknown arm (unreachable — redeem_pairing is their only source)". Non-blocking. | Open |

### Verification notes (checked clean)

- **Loop-only row vs totality claim** — coherent. `classify(C3Error, route)` takes the route as input; Forbidden and NotFound already route-split in the loop table, so per-route arm selection was the established pattern. Each (code, route) combination lands on exactly one arm; the catch-all's "take the Internal/Unknown arm" reads as sharing that arm's disposition (Exit(1)), which its parenthetical confirms.
- **BadInput `ReportCancelled` vs BR8.3/BR8.4** — consistent. BR8.3 pins kill-first + creds-preserved + exit non-zero; the no-report clause is conditioned on dead creds and does not extend to bad_input. The fix also aligns the row with the funnel's own "report attempts happen only while the credential is still live" rule and with Q4-A/logical-components' "report-or-not per cred liveness" wording. If the bad_input itself arrived on the report route, the close-out report is a single best-effort attempt — a second rejection strands the row, which is the design's already-accepted residual.
- **`NoReport` remaining uses** — only Unauthorized/Forbidden-off-pair (dead creds), correct.
- **`apply_cancels` comment** — the else-branch comment "// never-claimed ids" is terse (the branch also reports claimed-but-unstarted batch members), but the exec-entry paragraph immediately below spells out that case correctly; not a defect.
- **Cross-file consistency** — security-design deletion table (bad_input KEEP/"exits creds-kept"), observability exit-code list (bad_input in exit-1), entities.md RunState (`running` + `settled` fields) all align with the fixed rows.
- **Traceability** — unchanged targets remain valid; no new coverage rows required since the fixes edited existing sections within already-targeted files.

**Gate:** all prior findings resolved; R-22 is a non-blocking nit worth a one-line pin in the next touch.

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| - | - | - | No findings | No action required | Resolved |
