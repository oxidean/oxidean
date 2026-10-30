# Security Design — connector-gateway (U6)

Consumes: `nfr-requirements/security-requirements.md` (NFR1.1–1.17,
NFR5.1–5.4), `functional-design/{rules,entities}.md` (BR1–BR7),
`nfr-design-questions.md` (Q1–Q7), contract-summary.md (C1, C3, C7).

## Defense-in-depth layout

| boundary | control | covers |
|----------|---------|--------|
| proc | `oxpilot.target_*`/`grant_*`/`target_get` SessionOnly-by-registration + `require_enabled` top of handler; PAT `auth.pat_scope`, anon `auth.unauthenticated`, flag-off `oxpilot.disabled`; foreign ids ⇒ `not_found` (BR7.1) | NFR1.1 |
| mint gate | `pairing_create` validates `body.caps ⊆ {exec_shell, fs_read, fs_write}` — FR5.3's closed enum, a minted pairing can never carry a GUI cap (BR1.1) — and rides a per-USER limiter (BR1.5/NFR1.12 credential-mint pattern, same `FailedAuthLimiter` shape keyed on `user_id`, check+record per call); `/pair` itself carries NO limiter — the pairing code IS the credential (BR1.5's explicit carve-out) | NFR1.12 |
| connector auth | `/pair` unauthenticated (code IS the credential); `/claim`,`/report`,`/heartbeat` first `check_ip` against `FailedAuthLimiter` — tripped ⇒ 429 + `Retry-After` (TS8 vocabulary, bearer.rs gate order) — then `Authorization: Bearer <verifier>` → `sha256` → find-by-hash on `targets.verifier_hash`; miss ⇒ 401 `{error:"unauthorized"}` + limiter record + `connector_auth_failed` audit | NFR1.2, NFR1.3 |
| enablement (C3) | connector routes gate on `resolve_enablement_slot(&*state.oxpilot.slot.read().await)` — the shipped AppState-side seam (routes.rs precedent), NOT `require_enabled` (needs `RpcCtx`); disabled ⇒ `{error:"disabled"}` fail-closed before auth (BR2.5) | NFR1.3 |
| identity binding | resolved `ConnectorIdentity{target_id, user_id, caps, status}` (entities pin — `caps` feeds the claim intersection, `status` feeds `revoked:true`; `connector_id` on the wire is the same `target_id` value) is the ONLY scoping source on C3 paths; body `target_id` ≠ identity's ⇒ 403 `{error:"forbidden"}` (BR2.4) | NFR1.3 |
| dispatch authz | ordered checks inside per-target lock: session-owner (C5 `get_session` → `not_found`) → target `status=connected` + liveness-fresh (`Denied{not_connected}`/`{offline}`) → `kind ∈ caps` (`Denied{capability}`) → live `action_session` grant → else pending dedupe/mint → `Denied{consent_required, grant_id}` (BR4.1) | NFR1.4 |
| consent respond | `grant_respond` mutates `pending` grants (any flavor) + the BR6.4 `allow:false`-on-allowed-`action_session` arm; all else ⇒ `grant_not_pending`; resolved-on-dead-target ⇒ `grant_not_pending` (BR6.3) | NFR1.5 |
| dequeue recheck | per candidate `queued` row inside the lock, TWO distinct arms (BR5.1): (a) grant fails the `allowed`+unexpired live-test ⇒ flip the row `cancelled`+audit{reason:grant_expired}+skip — the grant itself is NOT stamped (`allowed→expired` is outside the closed transitions; lazy expiry stamps `pending` only); (b) `kind ⊄ claim.capabilities` or `kind ⊄ target.caps` ⇒ SKIP ONLY — the row stays `queued` (a connector claiming a narrower capability set must not cancel consented work it didn't advertise for) | NFR1.6 |
| caps ceiling | `/pair` `requested_caps ⊄ pairing.requested_caps` ⇒ `forbidden` — the mint-time caps are the hard ceiling (BR1.2) | NFR1.2 |
| report ingest | `result_summary = redact(output)` bounded to `RESULT_MAX` SERVER-SIDE before any store/audit — `oxidean_core::redact` (shipped helper, routes.rs precedent); the connector's own redaction is defense in depth, never the barrier (NFR5.4/BR5.2) | NFR5.4 |

## Secret custody design

```rust
// mint (pairing_create, SessionOnly proc):
ensure!(body.caps ⊆ {exec_shell, fs_read, fs_write}
        → bad_input);              // BR1.1/FR5.3 closed-enum bound —
                                   // `caps` is the proc's wire field
                                   // (pairing_create{target_name,caps});
                                   // `requested_caps` is only the
                                   // stored column + /pair body field
mint_limiter.check(user_id)?;      // per-USER limit (BR1.5/NFR1.12)
mint_limiter.record(user_id);      // every call records — a check
                                   // with nothing recorded never
                                   // trips (FailedAuthLimiter shape:
                                   // check-then-record)
let code = rand_token(PAIRING_CODE_ENTROPY_BITS /* >=160 */);
insert_pairing {
    pairing_id, user_id, code_hash: sha256(code),
    target_name: body.target_name, requested_caps: body.caps,
    expires_at: now + PAIRING_CODE_TTL /* 10min */,
    state: Pending, created_at: now,
};
audit(pairing_created);            // post-commit (BR1.1/NFR5.5)
return { pairing_id, pairing_code: code, expires_at };  // shown once

// redeem (POST /connector/pair, unauthenticated — no limiter; the
// code IS the credential, BR1.5's carve-out):
let hash = sha256(body.pairing_code);
tx {
    let n = UPDATE pairings SET state='claimed'
            WHERE code_hash=? AND state='pending' AND expires_at>now;
    // n==0 + row exists+expired → pairing_expired (lazy stamp+audit)
    // n==0 + row exists+claimed → pairing_used
    // n==0 + no row            → pairing_used — C3's refusal trio is
    //   expired|used|forbidden; an unknown-but-wellformed code folds
    //   into `used` (no never-existed/consumed split leaks); only a
    //   malformed body shape answers `bad_input` (R-13 pin, mirrored
    //   in reliability-design's idempotency map)
    if body.requested_caps ⊄ pairing.requested_caps → forbidden;
    let verifier = rand_token(VERIFIER_ENTROPY_BITS /* >=256 */);
    INSERT targets {
        target_id: id, user_id: pairing.user_id, pairing_id,
        name: body.device_label.or(pairing.target_name), // BR1.2 fallback
        caps: body.requested_caps,
        status: PendingConsent, verifier_hash: sha256(verifier),
        last_seen_at: null, created_at: now};
    INSERT grant {kind: 'connect', state: 'pending', target_id,
        session_id: NULL, user_id,
        expires_at: now + CONNECT_GRANT_TTL /* 10min */};
}
return { connector_id: id /* == target_id in v1, BR1.2 */,
         target_id: id, verifier };               // shown once
audit(pairing_claimed)  // post-commit; first connect-grant mint rides
                        // this event
```

Custody invariants (BR7.3): `code_hash`/`verifier_hash` are the only
stored forms; plaintext exists only in the two mint responses; no
projection serializes the hash columns; re-pair mints a NEW Target row
— `verifier_hash` is write-once-then-tombstone, never rotated. The
write-only invariant gets a store-level pin test (NFR1.11): a
`TargetView`/`GrantView`/`ActionView` projection is serialized and
asserted to carry no `*_hash`/secret field — pinned in
`unit-test-instructions.md` at codegen, not just asserted here.

## Consent state machine (transaction shapes)

```rust
// dispatch_action(user_id, session_id, target_id, kind, args):
lock(target_id).await;
let sess = session_svc.get_session(user_id, session_id)?;   // (1)
let t    = store.target(target_id)?;                          // (2)
ensure!(t.user_id == user_id → not_found);
ensure!(t.status == Connected → Denied{not_connected});
ensure!(t.last_seen_at fresh  → Denied{offline});
ensure!(kind ∈ t.caps         → Denied{capability});        // (3)
// normalize ONCE at the ingest boundary — the stored form is the
// dedupe key AND the Action payload (entities: args bounded+redacted
// at insert, not at claim). args_summary derives from the stored
// args — the consent-card text source (runtime.rs truncate_marker
// precedent):
let stored_args    = redact(args).bounded(ACTION_ARGS_MAX);
let stored_summary = truncate(stored_args, SUMMARY_MAX);
// (4) live session grant? — coverage is (target, session) scope,
//     NOT per action-kind (BR4.1(4)/NFR1.14): one "Allow session"
//     covers every subsequent dispatch on that pair until
//     expiry/revoke.
if let g = store.live_session_grant(target_id, session_id) {
    let a = tx { INSERT action{queued, grant_id: g.id, kind,
                    args: stored_args,
                    args_summary: stored_summary, …} };
    notify_waiters(target_id); audit(action_dispatched);  // post-commit
    return Ok(a);                  // Result<Action, Denied> — C7 pin
}
// (5) pending dedupe → Denied — dedupe key is the STORED bounded
//     `action_args` text (no args_hash column exists): same
//     (target_id, session_id, action_kind, action_args) joins the
//     existing pending grant.
match store.pending_grant(target_id, session_id, kind, stored_args) {
    Some(g) => {
        // BR3.1 lazy expiry on observation: a past-due pending grant
        // is stamped `expired` + audited `action_grant_expired`
        // BEFORE it can be joined — the consent card must never
        // receive an un-approvable grant_id; mint a fresh one.
        if g.expires_at <= now {
            tx { g.state='expired' };
            audit(action_grant_expired);         // post-commit
            let g2 = tx { INSERT grant{kind:'action', state:'pending',
                action_kind: kind,
                action_args: stored_args,
                action_summary: stored_summary,
                session_id, user_id,
                expires_at: now+ACTION_GRANT_TTL} };
            audit(action_grant_pending);
            return Denied{consent_required, grant_id: g2.id};
        }
        return Denied{consent_required, grant_id: g.id}
    }
    None    => { let g = tx { INSERT grant{kind:'action',
                    state:'pending', action_kind: kind,
                    action_args: stored_args,
                    action_summary: stored_summary,
                    session_id, user_id,
                    expires_at: now+ACTION_GRANT_TTL} };
                 audit(action_grant_pending);    // post-commit
                 return Denied{consent_required, grant_id: g.id} }
}

// grant_respond{grant_id, allow, scope}:
lock(grant.target_id).await;
let g = store.grant(grant_id)?;  // owner-scoped → not_found
let t = store.target(grant.target_id)?;  // bound under the same lock
// BR3.1 lazy expiry FIRST: a past-due `pending` grant is `expired`
// before the write evaluates — never `allow`able past its window.
if g.state == Pending && g.expires_at <= now {
    tx { g.state='expired' };
    audit(if g.kind == Connect { connect_grant_expired }
          else { action_grant_expired });        // post-commit
    return grant_not_pending;
}
if g.state != Pending {
    // BR6.4 carve-out FIRST: allowed action_session + allow:false
    if g.kind == ActionSession && g.state == Allowed && !allow {
        tx { g.state='revoked'; queued-under-g→cancelled;
             claimed/running→cancel_requested }
        audit(grant_revoked);                // post-commit
        for a in queued_rows {               // BR3.1's paired rule —
            audit(action_cancelled,          // every cancelled row
                  reason=grant_revoked);     // owes its own event
        }
        notify_waiters(target_id);
        return {state: revoked};
    }
    return grant_not_pending;  // incl. dead-target (BR6.3)
}
// capture the pre-stamp kind — the scope stamp applies to ACTION
// grants only; a connect allow flips the target, never materializes.
let was_connect = g.kind == Connect;
tx {
    if !allow { g.state='denied'; g.resolved_at=now }
    else {
        // resolved_at stamps on BOTH outcomes — a denied grant is
        // resolved (NFR-S4's "recent resolved by resolved_at desc"
        // ordering places denied cards too):
        g.state='allowed'; g.resolved_at=now;
        if was_connect {
            t.status=Connected;
        } else {
            // scope names the flavor; absent scope defaults `once`
            // (BR3.5/C1 amendment):
            g.kind = scope.unwrap_or(Once) → action_once|action_session;
            // BR4.2 atomic materialize — every required column
            // sourced from the grant row:
            INSERT action{queued, action_id, grant_id,
                target_id: g.target_id, user_id: g.user_id,
                session_id: g.session_id, kind: g.action_kind,
                args: g.action_args,
                args_summary: g.action_summary, created_at: now};
        }
    }
}
// post-commit emissions (audits never ride the tx):
if !allow {
    audit(if was_connect { connect_grant_denied }
          else { action_grant_denied });          // kind-branched
} else if was_connect {
    audit(connect_grant_allowed); audit(target_connected);
} else {
    audit(action_grant_allowed); audit(action_dispatched);
}
notify_waiters(target_id);
```

## Session-delete hook

```rust
// ConnectorDeleteHook (registered after U3's abort hook — main.rs):
fn on_session_delete(session_id, user_id) -> Result<()> {
    tx {
        UPDATE grants SET state='cancelled'
         WHERE session_id=? AND state IN (pending, allowed);
        UPDATE actions SET state='cancelled'
         WHERE session_id=? AND state IN
             (queued, claimed, running, cancel_requested);
    }
    for each touched row: audit(grant_cancelled|action_cancelled,
        reason=session_deleted);
    Ok(())  // idempotent; Err aborts the whole delete (C8)
}
```

No `cancel_requested` on this path — the rows cascade-delete, so a
cancel set built from them is undeliverable; `cancelled` is the honest
terminal (BR6.5). Late connector report ⇒ 404 `not_found` ⇒ client
kills local exec (C3-pinned).

## Revoked/disconnected ending designs

- `target_revoke` (BR6.2): verifier STAYS; `status→revoked`; ALL
  grants → `revoked`; queued → `cancelled`; claimed/running →
  `cancel_requested` + notify (heartbeat delivers: claim refuses
  `{error:"revoked"}`); audit `target_revoked` + `grant_revoked` +
  `action_cancelled`{reason:target_revoked}.
- `target_disconnect` (BR6.1): `verifier_hash→NULL` (tombstone);
  `status→disconnected`; LIVE grants `WHERE state IN (pending,
  allowed)` → `cancelled` (BR3.1's live-state rule — an `allowed`
  `action_session` grant must not dangle live on a dead target);
  queued → `cancelled`; claimed/running/cancel_requested →
  `cancelled` DIRECTLY — a `cancel_requested` row can never reach its
  report-close once the verifier is tombstoned (entities' explicit
  pin: "its terminal lands via report … or via the disconnect path";
  same state set the session-delete hook filters); audit
  `target_disconnected` + `grant_cancelled`/`action_cancelled`
  {reason:target_disconnected}. Every later connector route ⇒ 401
  (hash column gone).
- Revoked-target report exception (BR2.2/BR5.2): report on
  claimed/running/cancel_requested rows of a revoked target still
  closes them (terminal + audit) — the ONE accepted post-revoke path.
