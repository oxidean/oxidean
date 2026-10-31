# Reliability Design — connector-gateway (U6)

Consumes: `nfr-requirements/reliability-requirements.md`
(NFR4.1–4.12), `functional-design/rules.md` (BR4.x–BR6.x),
`nfr-design-questions.md` (Q4, Q5, Q6), contract-summary.md (C3, C8).

## State-transition integrity

Every mutating transition is a **state-conditional UPDATE** — the WHERE
clause carries the expected from-state; `rows_affected` decides the
outcome:

| transition | statement shape | loser sees |
|------------|-----------------|-----------|
| `queued→claimed` | `UPDATE … WHERE action_id=? AND state='queued'` inside per-target lock | 0 rows → skip (already claimed/cancelled) |
| `claimed→running` | `UPDATE … WHERE state='claimed' AND target_id=?` on any authenticated touch | 0 rows → already running/terminal |
| terminal report | `UPDATE … SET state=? WHERE action_id=? AND state IN (claimed,running,cancel_requested)` | 0 rows → `{ack:true}` idempotent |
| cancel-close | report on `cancel_requested` → `cancelled`; the close emits `action_cancelled` — the reason is inferred AT CLOSE (the row carries no reason column): `target.status==revoked ⇒ target_revoked`, else covering `grant revoked ⇒ grant_revoked` | — |
| dead-path direct | disconnect/delete: `UPDATE … SET state='cancelled' WHERE state IN (queued, claimed, running, cancel_requested)` — dead channel cancels EVERYTHING non-terminal directly; revoke differs: `queued` → `cancelled` direct but `claimed/running` → `cancel_requested` (heartbeat delivers — verifier survives revoke) | already terminal → 0 rows, idempotent |
| `pending→claimed` pairing | `UPDATE pairings SET state='claimed' WHERE code_hash=? AND state='pending' AND expires_at>now` | 0 rows → expired/used lookup |

A racing pair of terminal decisions resolves to exactly one winner —
first-wins CAS, never two terminal states (NFR4.2).

## Idempotency map

| call | key | second-call answer |
|------|-----|--------------------|
| `/report` | `action_id` | terminal row → `{ack:true}`; unknown/cross-target → 404 `not_found` (kill-local on client) |
| `/pair` | `code_hash` | `claimed` → `pairing_used`; expired → `pairing_expired` (evaluated first); unknown-but-wellformed → `pairing_used` (C3's trio admits no never-existed/consumed split — mirrors security-design); only a malformed body shape → `bad_input` |
| `grant_respond` | `grant_id` | resolved → `grant_not_pending` (incl. dead-target grants, BR6.3) |
| `target_disconnect`/`revoke` | `target_id` | already-terminal → `{ok}` (BR6.7) |
| `ConnectorDeleteHook` | `session_id` | no live rows → `Ok(())`; `Err` aborts delete (C8) |
| `claim` | — | no idempotency key; at-most-once delivery via dequeue CAS (mutates: dequeue CAS, `last_seen_at` touch, `claimed→running` flip) |

## Accepted residual (documented, not hidden)

A claim response lost in transit strands its actions in
`claimed`/`running`: the connector never received the `action_id` so
it can never report, and the server never requeues (at-most-once would
break). The row stays owner-visible via `action_status` until a
close path lands — revoke/disconnect/session-delete (BR6.x) or
BR6.4's session-grant revoke. Tracing notes the strand at `warn` on
observation; there is no sweeper (BR5.1's accepted risk).

## Failure-mode matrix (mechanism per named degradation)

| condition | mechanism |
|-----------|-----------|
| `oxpilot` disabled | procs → `oxpilot.disabled`; C3 routes → `{error:"disabled"}` fail-closed (BR2.5) — checked before auth |
| offline (stale `last_seen_at`, lifecycle connected) | dispatch → `Denied{offline}`; claim still authenticates + touches `last_seen_at` (liveness recovers on next touch) |
| disconnected | verifier NULL ⇒ 401 `{error:"unauthorized"}` everywhere; live grants `IN (pending, allowed)` + actions `IN (queued, claimed, running, cancel_requested)` → `cancelled` at disconnect time (dead channel — a `cancel_requested` row can never reach report-close; BR3.1's live-state filter, same set the session-delete hook applies) |
| revoked | verifier retained ⇒ heartbeat `{revoked:true}` + cancel set; claim/report `{error:"revoked"}` EXCEPT report's terminal-close arm on claimed/running/cancel_requested |
| session deleted | hook: grants pending/allowed + actions → `cancelled` direct (reason `session_deleted`) → FK cascade → audit survives → late report 404 |
| grant dies mid-consent (target revoked/disconnected/deleted) | `grant_respond` → `grant_not_pending` (BR6.3) |
| pending grant dedupe | identical (target,session,action_kind,action_args) joins existing pending grant; distinct args mints a new one |

## Transaction boundaries

| operation | tx scope | post-commit |
|-----------|----------|-------------|
| redeem pairing | claim-UPDATE + target-INSERT + connect-grant-INSERT in ONE tx | `pairing_claimed` audit (first mint rides it) |
| `grant_respond(allow)` | state-conditional UPDATE + (action grant arm only) Action INSERT in ONE tx (BR4.2) | kind-branched: action arm → `action_grant_allowed` + `action_dispatched`; connect arm → `connect_grant_allowed` + `target_connected`; + notify |
| session-delete hook | bulk grant+action `cancelled` UPDATEs in ONE tx | per-row `grant_cancelled`/`action_cancelled` audits |
| disconnect/revoke | lifecycle UPDATE + grant/action transitions in ONE tx | `target_*` + cancel/revoke audits + notify |
| dequeue | per-row CAS inside per-target lock (not one giant tx) | `action_cancelled` on dead-grant skips |
