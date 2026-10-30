## Review

**Verdict:** NOT-READY
**Reviewer:** aidlc-architecture-reviewer-agent
**Date:** 2026-10-28
**Iteration:** 1

The design set is structurally sound on the big choices (hash-only custody, per-target keyed locks, state-conditional CAS, set-once seam) but contains one C7 contract violation and a string of mechanism-level defects: a claim long-poll that never checks the queue before parking, a `claimed→running` flip ordered so it fires on the just-returned batch, a dequeue re-check that cancels capability-mismatched rows, two shipped-seam citations that cannot compile on the C3 surface (`audit::record`, `require_enabled` both take `RpcCtx`), and a `/pair` unknown-code answer that contradicts itself across artifacts. Several pinned requirements (NFR1.12 mint limit, BR1.1 caps bound, BR3.1 lazy expiry/live-state cancel, NFR5.4 server-side redaction) have no mechanism at all.

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| R-01 | Critical | logical-components.md §Component catalogue `trait ConnectorGateway` (line 11) | `dispatch_action(...) -> Result<DispatchOutcome>` with variants `Dispatched{action}` \| `Denied{reason, grant_id?}` moves `Denied` into the Ok channel. Pinned shape is `Result<Action, Denied>` (entities.md:194; contract-summary.md C7 amendment; rules.md BR4.4 — "`Denied` serializes into the error's `data` payload (`data:{denied:{reason, grant_id?}}`)"). U5 distinguishes consent refusals from hard errors by the denied marker in error data; an Ok-variant breaks that seam — a silent contract-shape change the ownership rules forbid. | Restore `Result<Action, Denied>` (Denied as the AppError-domain error with `data.denied`), matching C7/entities verbatim. | New |
| R-02 | Major | security-design.md §Secret custody redeem block (line 44) | `INSERT targets {id, ..., connector_id: new_id(), ...}` mints a connector_id distinct from the target id. Ground truth pins `connector_id := target_id` in v1 — "the sketch's two response fields carry one value" (rules.md BR1.2:29-31; functional-spec.md C3 delta:151). A distinct value also makes the `ConnectorIdentity{connector_id, target_id}` binding ambiguous (which id does `body.target_id` compare against?). | `connector_id = id` (the new Target row's id); return the same value in both response fields. | New |
| R-03 | Major | security-design.md line 16; performance-design.md lines 41–45 | The dequeue re-check collapses all failures into one arm: "grant still `allowed`+unexpired AND `kind ⊆ claim.capabilities` AND `⊆ target.caps` — else `cancelled`+audit+skip". BR5.1/NFR1.6 pin `cancelled` ONLY for dead-grant rows ("a `queued` row whose grant died … is flipped `cancelled` + audit"). A connector claiming a narrower `capabilities` set would mass-cancel consented `queued` work it simply didn't advertise for — a connector-side kill vector and an integrity violation. | Split the re-check: grant dead/expired ⇒ `cancelled`+audit; capability or target-caps mismatch ⇒ skip only (row stays `queued`). | New |
| R-04 | Major | performance-design.md line 44; observability-design.md line 28 | `action_cancelled` emitted with `reason=grant_dead` — outside NFR5.15's closed enum `reason ∈ {session_deleted, target_disconnected, target_revoked, grant_revoked}` (observability-requirements.md:23; BR7.2's bounded-enum pin). | Carry the grant's own terminal reason, or flag `grant_dead` as an explicit enum amendment with basis. | New |
| R-05 | Major | performance-design.md §Long-poll design (lines 11–21) | The claim parks on `notify.notified()` via `timeout(...)` BEFORE any dequeue attempt — work `queued` before the claim arrived (the steady-state case: dispatch lands between polls) is never detected until a future notify or the full `wait_seconds` elapses. NFR3.2 pins "waits on the FIRST of: a `queued` action … becoming dequeuable, `wait_seconds` elapsing" — already-queued work is dequeuable at arrival, and `notify_waiters` stores no permit for pre-subscription wakes. | Subscribe, then dequeue-check; park on `notified()` only when the first dequeue is empty (check → subscribe → re-check or hold the lock across check+subscribe). | New |
| R-06 | Major | performance-design.md §Long-poll design (line 18) | `flip_claimed_to_running(target_id)` runs AFTER `dequeue`, which already CAS'd this response's batch to `claimed` — the flip then marks the just-claimed batch `running` in the same request. NFR3.4 pins "a claim response alone never marks `running`"; `claimed→running` is proof from the NEXT touch (BR4.3; entities.md Action.state:111). | Flip before dequeue (or exclude the current batch's ids from the flip). | New |
| R-07 | Major | security-design.md §Consent state machine (line 72) | `store.live_session_grant(target_id, session_id, kind)` narrows the live-grant test to the action kind. The pin is (target, session) scope only — BR4.1(4) "an `action_session` grant `allowed`+unexpired for (target,session)"; BR3.3 "covers subsequent dispatches on that target+session"; NFR1.14 "only for its stamped (target_id, session_id)". Literal implementation re-prompts for consent on every kind under a live session grant — a different semantic than "Allow session". | Drop the `kind` parameter from the live-grant lookup (coverage is target+session), or flag the narrowing as an explicit deviation. | New |
| R-08 | Major | security-design.md §Consent state machine (lines 78–110) | No lazy-expiry evaluation on the consent path: `grant_respond` reads `g.state != Pending` without stamping a past-due `pending` row `expired` first, violating BR3.1 ("a past-due `pending` grant is `expired` before the write evaluates, never `allow`able past its window"). Same gap in dispatch's `pending_grant` dedupe — it can join an expired pending grant, handing out a `grant_id` the consent card can never approve. | Evaluate `expires_at` on observation at both sites: stamp `expired`+audit `*_grant_expired`, then answer `grant_not_pending` / mint a fresh pending grant. | New |
| R-09 | Major | security-design.md §Secret custody mint block (lines 23–29) + Defense-in-depth table | `pairing_create` shows no `caps ⊆ {exec_shell, fs_read, fs_write}` validation (BR1.1/FR5.3 — a minted pairing can never carry a GUI cap) and no per-USER rate limit (BR1.5/NFR1.12 — "the credential-mint pattern"; only the per-IP `FailedAuthLimiter` for connector auth appears). The `/pair` NO-limit carve-out (BR1.5's explicit pin, NFR1.12) is also unstated. | Add the closed-enum caps check at mint, pin a per-user limiter on `pairing_create`, and state the `/pair` no-limit carve-out. | New |
| R-10 | Major | security-design.md §Secret custody (lines 24–28, 34, 44–45) | Pairing/Target row drift vs entities: `insert_pairing` omits `target_name` (entities.md:19 `required: true` — the source of BR1.2's "device_label fallback target_name", which the Target INSERT then can't implement); `claimed_at=now` stamps a column entities doesn't declare; `label: body.device_label` drops the fallback. | Add `target_name` to the Pairing row, apply `device_label.or(target_name)` at Target insert, drop or justify `claimed_at`. | New |
| R-11 | Major | observability-design.md §Audit emission design (line 9) | Emission pins `audit::record` — but `audit::record(ctx: &RpcCtx, …)` requires an `RpcCtx` (audit.rs:14-21). C3 routes hold `AppState`, and `ConnectorDeleteHook::on_session_delete(session_id, user_id)` receives no ctx either (oxpilot.rs:379). Roughly half the pinned events (connector_auth_failed, pairing_claimed, action_result, hook cancels) originate where `audit::record` cannot be called; the shipped sibling seam is `audit::record_with(db, client, …)` (audit.rs:24-32, written for ctx-less call sites). | Pin `audit::record_with` for C3-route and hook emission; `audit::record` only on proc paths. | New |
| R-12 | Major | logical-components.md §C3 routes row (line 15) | C3 routes pinned to "`require_enabled` fail-closed" — `require_enabled(ctx: &RpcCtx)` needs an `RpcCtx` the verifier routes never build (oxpilot.rs:935) and returns `oxpilot.disabled`, not C3's pinned `{error:"disabled"}`. The shipped AppState-side seam is `resolve_enablement_slot(&*state.oxpilot.slot.read().await)` (oxpilot.rs:912 — explicitly for routes that hold AppState; precedent routes.rs:88-99). | Cite `resolve_enablement_slot` + `{error:"disabled"}` mapping for the connector subtree; keep `require_enabled` for procs only. | New |
| R-13 | Major | security-design.md lines 38–41 vs reliability-design.md line 30 | Internal contradiction on an unpinned edge: an unknown-but-wellformed pairing code answers `pairing_used` in security-design ("unknown-but-wellformed → pairing_used") but `bad_input` in reliability's idempotency map ("unknown → `bad_input`"). C3's pinned trio is `pairing_expired` \| `pairing_used` \| `forbidden` + `bad_input` for malformed shape — the artifacts disagree and neither cites a basis. | Pick one answer (the trio's `pairing_used` is the more defensible: no never-existed/consumed split) and state it identically in both files. | New |
| R-14 | Major | security-design.md §Revoked/disconnected endings (line 144); reliability-design.md line 52 | `target_disconnect` transitions only "pending grants → `cancelled`". BR3.1 pins live states (pending AND `allowed`) → `cancelled` on disconnect/session-delete — an `allowed` `action_session` grant would dangle `allowed` on a dead target with no `grant_cancelled` audit (NFR5.12's `target_disconnected` reason exists precisely for this arm). | Cancel `WHERE state IN (pending, allowed)` on disconnect, matching the session-delete hook's filter. | New |
| R-15 | Major | traceability.json NFR5.4 → security-design.md; observability-design.md line 26 | NFR5.4 pins report output "secrets-redacted SERVER-side before storage/audit — the connector's own redaction is defense in depth, not the barrier" (security-requirements.md:54; BR5.2/FR5.5). No artifact pins the mechanism — observability shows only "(redacted)" with no site or helper; the claimed coverage file doesn't contain it. The shipped helper is `oxidean_core::redact` (crates/oxidean-core/src/redact.rs:77, used at routes.rs:287). | Pin `result_summary = redact(output).tail(RESULT_MAX)` at the report boundary server-side; repoint the traceability row. | New |
| R-16 | Minor | security-design.md throughout; logical-components.md | Column/kind drift vs entities: `owner_user_id` (entities: `user_id`); `caps`/`pairing.caps` (entities: `requested_caps`); `t.lifecycle` as the Target column (entities: `status`; `lifecycle` is the wire field); `g.flavor` (entities: `kind`); `INSERT grant{pending, action_grant, …}` uses `action_grant` as a kind value — the enum is `connect` \| `action` \| `action_once` \| `action_session` and pending action grants mint `action` unflavored (BR3.3); `args_hash` dedupe key is not an entity field (stored `action_args` compares directly). | Rename to the entities columns verbatim; pin kind=`action` at mint; dedupe on stored `action_args` or declare an `args_hash` column. | New |
| R-17 | Minor | security-design.md line 13; logical-components.md line 18 | `ConnectorIdentity{connector_id, target_id, owner_user_id}` drops entities.md's pinned `caps` and `status` fields (entities.md:137-138 — caps feeds the claim intersection, status feeds `revoked:true`). | Carry `{target_id, user_id, caps, status}` per the entity pin (add `connector_id` only as a documented duplicate of target_id). | New |
| R-18 | Minor | security-design.md §Consent state machine (lines 93–108) | `audit(action_grant_denied)`, `audit(action_dispatched)`, `audit(connect_grant_allowed, target_connected)` and `notify` are drawn INSIDE `tx { }` blocks — contradicting observability-design's "Emission point: post-commit" pin (and `audit::record*` is a separate write that can't join the tx). | Move audit/notify calls outside the tx braces (post-commit), matching the dispatch arm at line 74. | New |
| R-19 | Minor | security-design.md line 100 | The deny arm audits `action_grant_denied` unconditionally — a denied CONNECT grant must emit `connect_grant_denied` (BR7.2/NFR5.10 split). | Branch the event name on grant kind. | New |
| R-20 | Minor | observability-design.md §Audit emission design (lines 17–25) | Detail payloads omit `user_id` on several rows where the pinned detail contract requires it — `pairing_expired`/`pairing_used` (NFR5.7/5.8 pin pairing_id + user_id), `grant_respond` row (NFR5.10/5.11 pin user_id), `action_dispatched` (NFR5.13 pins user_id, target_id, action_id, kind). | Add `user_id` to the affected detail columns. | New |
| R-21 | Minor | observability-design.md §Tracing design (line 43) | No tracing line covers grant resolutions — NFR6.1 pins "grant resolutions" among the required `oxpilot_connector` subjects; the info row jumps from `pairing_claimed` to `action_dispatched`. | Add grant-resolution events (allowed/denied) at `info`. | New |
| R-22 | Minor | security-design.md line 12 | The connector-auth row records misses into `FailedAuthLimiter` but never states the entry-side `check_ip` gate or its 429 + `Retry-After` answer — TS8 lists "limiter 429" in the C3 vocabulary and bearer.rs:189 gates before token work. | State the check-IP-before-lookup gate and the 429 answer for a tripped limiter. | New |
| R-23 | Minor | logical-components.md lines 12, 33 | "registers as the `ConnectorGateway` trait impl on the shared slot" — `slot` is the loaded term for the REBUILDABLE `OxpilotRuntimeSlot` (oxpilot.rs:719); functional-spec pins the install "beside `sessions` on `OxpilotRuntime` … not the rebuildable slot, so settings rebuilds can't drop it" (functional-spec.md:28-30; entities.md:191 `RpcCtx.oxpilot.gateway`). | Say "set-once field on `OxpilotRuntime` beside `sessions` (OnceLock), not `OxpilotRuntimeSlot`". | New |
| R-24 | Minor | reliability-design.md line 34 | "`claim` \| — \| pure read+dequeue" mislabels the route: claim mutates (dequeue CAS, `last_seen_at` touch, `claimed→running` flip). The "pure" framing invites a wrong implementation. | Reword as "no idempotency key; at-most-once delivery via dequeue CAS". | New |
| R-25 | Minor | security-design.md line 102; nfr-design-questions.md line 22; observability/traceability | Small gaps: (a) `g.flavor=scope→action_once` \| `action_session` omits the pinned absent-scope default `once` (BR3.5/C1 amendment); (b) NFR1.11's "store-level pin test asserts the secret columns never join a projection" has no designed test (only the invariant sentence); (c) Q1 cites "`AgentRuntime`" as a SharedOxpilot trait precedent — it is a U3 module, not a trait object on the runtime; (d) NFR-S4's `grant_list` stable ordering (pending-first then recent resolved, functional-spec.md:183) appears in no artifact. | Pin each: default-once, the projection pin test, the trait precedent wording, the list ordering. | New |
| R-26 | Minor | logical-components.md line 14 vs entities.md:48 | `verifier_hash NULLABLE (disconnect tombstone)` deviates from entities' `required: true` on the attribute (the tombstone needs NULL or a sentinel). The design chooses NULL — reasonable, but it is an unflagged deviation from the reviewed entity, and scalability's "UNIQUE-where-not-null" must hold on all three dialects. | Flag the deviation explicitly (or amend entities) and confirm tri-dialect `UNIQUE … WHERE verifier_hash IS NOT NULL` semantics. | New |

> R-01 Reviewer note: Reviewer supplied prior ID R-01

> R-02 Reviewer note: Reviewer supplied prior ID R-02

> R-03 Reviewer note: Reviewer supplied prior ID R-03

> R-04 Reviewer note: Reviewer supplied prior ID R-04

> R-05 Reviewer note: Reviewer supplied prior ID R-05

> R-06 Reviewer note: Reviewer supplied prior ID R-06

> R-07 Reviewer note: Reviewer supplied prior ID R-07

> R-08 Reviewer note: Reviewer supplied prior ID R-08

> R-09 Reviewer note: Reviewer supplied prior ID R-09

> R-10 Reviewer note: Reviewer supplied prior ID R-10

> R-11 Reviewer note: Reviewer supplied prior ID R-11

> R-12 Reviewer note: Reviewer supplied prior ID R-12

> R-13 Reviewer note: Reviewer supplied prior ID R-13

> R-14 Reviewer note: Reviewer supplied prior ID R-14

> R-15 Reviewer note: Reviewer supplied prior ID R-15

> R-16 Reviewer note: Reviewer supplied prior ID R-16

> R-17 Reviewer note: Reviewer supplied prior ID R-17

> R-18 Reviewer note: Reviewer supplied prior ID R-18

> R-19 Reviewer note: Reviewer supplied prior ID R-19

> R-20 Reviewer note: Reviewer supplied prior ID R-20

> R-21 Reviewer note: Reviewer supplied prior ID R-21

> R-22 Reviewer note: Reviewer supplied prior ID R-22

> R-23 Reviewer note: Reviewer supplied prior ID R-23

> R-24 Reviewer note: Reviewer supplied prior ID R-24

> R-25 Reviewer note: Reviewer supplied prior ID R-25

> R-26 Reviewer note: Reviewer supplied prior ID R-26

### Validation Tool Results

| Tool | Result | Interpretation |
|---|---|---|
| (none specified) | n/a | No validation tool output was provided for this stage; all verification was manual cross-referencing of the artifacts against nfr-requirements, functional-design, contract-summary, and shipped code. |

### Summary

Security-design gets the custody posture and check ordering largely right, and the whole set stays honest about fire-and-forget audit and single-replica limits, but it is not implementable as written: the C7 dispatch signature flips `Denied` into a success variant (contract violation), `connector_id` is minted distinct from the pinned `:= target_id`, the claim loop parks without ever checking for already-queued work and then flips the fresh batch to `running`, and the dequeue re-check over-cancels capability mismatches while inventing the unpinned `grant_dead` audit reason. Two citations of `RpcCtx`-requiring helpers (`audit::record`, `require_enabled`) cannot compile on the verifier-authenticated C3 surface where half the pinned events and the enablement gate live — `record_with` and `resolve_enablement_slot` are the shipped seams. Pairing mint loses `caps ⊆ FR5.3`, `target_name`, and the BR1.5 per-user limit; consent paths skip BR3.1 lazy expiry and BR3.1's live-state cancel on disconnect; and the unknown-code answer contradicts itself between security- and reliability-design. Fix the Critical contract shape and the Major mechanism holes before codegen consumes these artifacts.
