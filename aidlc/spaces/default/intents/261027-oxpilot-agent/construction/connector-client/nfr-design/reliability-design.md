# Reliability Design — connector-client (U7)

Consumes: `nfr-requirements/reliability-requirements.md`
(NFR4.1–4.12), `functional-design/rules.md` BR2.3/BR4/BR5/BR8,
`entities.md` RunState, `nfr-design-questions.md` Q3-A/Q4-A.

## The degrade taxonomy as a dispatch table (NFR4.1–4.6)

One `classify(C3Error, route) -> Disposition` — the enum makes the
taxonomy total; every code lands on exactly one arm:

```text
Unauthorized               → KillInFlight; NoReport; DeleteCreds; Exit(1)
Forbidden (off-pair)       → KillInFlight; NoReport; DeleteCreds; Exit(1)
Forbidden (on pair)        → PairingReject(msg)                  (pair path)
Revoked                    → KillInFlight; ReportCancelledRevoked;
                             DeleteCreds; Exit(0)
NotFound (claim/heartbeat) → KillInFlight; ReportCancelledRevoked;
                             DeleteCreds; Exit(0)
NotFound (report)          → DropAction; Continue                (WF3.5)
BadInput                   → KillInFlight; ReportCancelled;
                             Exit(1)                             (creds KEPT —
                             the cred is live, so the close-out
                             report lands and the row isn't
                             stranded server-side)
Disabled                   → Backoff(transport-class); creds KEPT; loop lives
Internal / Unknown         → Backoff; creds KEPT; loop lives
RateLimited{retry_after}   → Sleep(retry_after); Continue — never
                             counted as transport failure
Transport                  → Backoff(1s→×2→60s, ±20% jitter,
                             reset-on-success); loop lives
PairingExpired/PairingUsed → PairingReject(msg); Exit(1)         (pair path;
                             on LOOP routes they take the
                             Internal/Unknown arm — unreachable there,
                             redeem_pairing is their only source, and
                             a server bug must not Exit(1) a healthy
                             run loop)
```

Pair-route rows (pair has no loop — every pair-path failure exits):

```text
Disabled  (pair)             → verbatim reason → Exit(1)   // oxpilot
                               // gate answers {error:"disabled"}
                               // BEFORE auth — the commonest pair fail
Internal/Unknown (pair)      → reason + "retry" guidance → Exit(1)
BadInput  (pair)             → verbatim reason → Exit(1)
Transport (pair)             → transport class + "retry" → Exit(1)
RateLimited (pair)           → honor Retry-After header, retry ONCE,
                               else report the limit and Exit(1)
loop-only codes (pair)       → Unauthorized/Revoked/NotFound on the
                               pair route take the Internal/Unknown
                               arm — Exit(1). redeem_pairing never
                               emits them, but a code that did arrive
                               must not fall through to the loop
                               table's Exit(0) (a failed pair is not
                               "cleanly retired").
```

`terminate(reason)` is the single exit funnel (Q4-A): every
KillInFlight arm runs the BR5.2 process-group kill FIRST; report
attempts happen only while the credential is still live (a dead cred
can't land a report — skipping it isn't a shortcut, it's the only
honest path); custody deletes follow the table; the exit code is the
pinned constant. No `process::exit` outside the funnel.

## Exactly-once cancel close-out (NFR4.8/4.9)

```text
apply_cancels(ids: &[ActionId]):
  for id in ids:
    if RunState.settled.contains(id): continue          // re-announce dedupe
    if RunState.running.remove(id) -> child:
        child.killpg() / taskkill /T
        report(id, failed, "cancelled:mid-flight")
    else:
        report(id, failed, "cancelled:pre-start")       // never-claimed ids
    RunState.settled.insert(id)                          // AFTER the report
```

`settled` inserts only after the report lands — a report that fails
transport-class retries on the next announce (the server re-announces
until close, U6-BR5.3), and a crash mid-close re-reports rather than
strands. Mid-flight fs_* ops can't be killed mid-syscall — the
non-regular refusal bounds every op to a finite regular file, making
block-forever impossible rather than unlikely (NFR4.9).

`settled` is also consulted on the two exec-path edges, so a
cancelled action produces exactly one report per BR7.4:

```text
exec-entry:   for each claimed action, if settled.contains(id) → skip
              (a batch member cancelled while a sibling ran was
              already reported cancelled:pre-start — it must not
              execute when its serial turn arrives)
completion:   on the child-completion arm, if settled.contains(id)
              → suppress the report (the mid-flight kill already
              reported cancelled:mid-flight; a second, different-body
              report for the same action_id is a no-op server-side
              but violates one-report-per-action)
```

## Idempotent report design (NFR4.7)

`report(action_id, body)` reposts the IDENTICAL JSON body on retry —
the server's action_id-keyed idempotency makes a duplicate a no-op.
The client never mutates a report between attempts (output/exit_code
frozen at construction), so at-least-once send degrades to
exactly-once effect.

## Signal close-out (NFR4.6)

`tokio::signal::ctrl_c` + unix `signal(SignalKind::terminate())` race
the select! loop as a third arm: on receipt → process-group kill the
in-flight child → report `cancelled:<sigint|sigterm>` → exit 0. The
signal arm shares terminate()'s kill machinery but bypasses the cred
deletes (a signal isn't a credential event).

## Serial-execution integrity (NFR4.11/4.12)

- `running: HashMap<ActionId, ChildHandle>` holds ≤1 entry by
  construction — the serial claim contract; insert before spawn,
  remove on settle/cancel.
- Exec phase is itself a `select!`, not a bare await: while one
  action runs, the per-action select races {child completion,
  EXEC_TIMEOUT deadline, heartbeat tick — whose response `cancel`
  set is the mid-flight cancel delivery, signal}. The
  heartbeat/cancel arms stay live DURING execution — the heartbeat
  tick keeps the target inside the 90s liveness window on >90s jobs,
  and a heartbeat-delivered cancel set reaches the running child for
  the BR5.2 mid-flight kill (the running id is in `running`, so
  apply_cancels hits its kill branch). fs_* ops run on
  `spawn_blocking` so a slow disk doesn't stall the same arms.
- `EXEC_TIMEOUT` 300s default: a `tokio::time::timeout` INSIDE that
  per-action select; expiry → group kill → report `failed` "timeout"
  reason — a hung child can't pin the loop.
- Backoff state (`fail_streak`, `next_delay`) lives in RunState,
  resets to 1s on ANY successful call — disabled-flap and
  transport-flap share the same decay path.

## Crash-recovery posture

- Crash mid-action → the client never resumes half-executed work; on
  restart it re-claims fresh (at-most-once client-side, reported
  once). Server-side the row STRANDS `claimed`/`running`: U6-BR5.1
  pins dequeue-at-once with NO requeue and there is no sweeper — the
  row stays `claimed`/`running` until an owner-side close lands
  (revoke, disconnect, session-delete, session-grant-revoke).
  Accepted residual — C3 pins no ack for claim receipts and a
  requeue would break at-most-once semantics.
- Crash mid-delete → zombie row (verifier gone, row present) →
  BR9.3's named-zombie path; crash between row delete and verifier
  delete is prevented by ordering (verifier first).
- Corrupt `pilot-targets.json` (unparseable) → startup refuses with a
  named parse error pointing at the file — never silently discards
  or overwrites user state.
