## Review

**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 2

### Prior findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|
| R-01 | Minor | `crates/oxidean-api-oxpilot/src/connector/gateway.rs:346-412`; `crates/oxidean-db/src/oxpilot.rs:4138-4254` | BR3.2 re-mint TOCTOU + `allow_connect_grant_tx` committing on a 0-row target CAS | Re-verify `pending_consent` under the per-target lock; check target `rows_affected` in all 3 dialects and roll back on miss | Resolved |
| R-02 | Nit | `gateway.rs:156-161, 295-306, 312-340, 346-412, 446-503, 637-693, 1069-1073, 1495-1531` | Audit writes awaited inside the per-target lock | Deferred-audit queue emitted post-`drop(_g)` on every path | Resolved |
| R-03 | Nit | `gateway.rs:758-764` | `mint_pairing` limiter check-then-record used two mutex acquisitions | Hold one `MutexGuard` across `check_user` + `record_user` | Resolved |

### New findings

No new findings.

### Residual notes

- `oxpilot.rs:4135-4137` and `lib.rs:7494-7495` doc comments still read "`false` ⇒ the grant CAS lost" — `false` now also covers the target-CAS-miss rollback. Stale wording only; caller semantics unchanged. (Applied post-review.)
- `Ok(false)` on target-CAS miss leaves the grant row `pending` while `respond_grant` answers `grant_not_pending` — intended rollback semantics (grant remains deniable; lazy expiry cleans up); the pinned BR6.3 fold.
- Ordering inconsistency, cosmetic only: `dispatch_action` arm-4 notifies waiters before its post-lock audit, while `respond_grant` arms audit before notify. Both are post-commit/post-lock; the contract "audit is post-commit, fire-and-forget" holds either way.
- `Ok::<(), AppError>(())` at `gateway.rs:406` was a redundant annotation — removed.

### Verification detail (prior findings)

**R-01(a):** `ensure_live_connect_grant` re-reads `find_oxpilot_target(&t.id)` under the held `_g` and returns `Ok(())` early when the target is absent or `live.status != "pending_consent"`. On both early returns `_g` drops via RAII and no audit is lost — `let mut audits` is only created after the checks, and only reads precede those returns so no event is owed.

**R-01(b):** All three dialect arms of `allow_connect_grant_tx` now do `let t = …rows_affected(); if t == 0 { return Ok(false); }` before `tx.commit()`. `Ok(false)` mid-tx drops `tx` → rollback → the grant stays `pending`. Caller `respond_grant` maps `!won` → `grant_not_pending`. Under the per-target lock all target-status writers serialize, so `t == 0` can only mean the target already left `pending_consent` — the pinned BR6.3 dead-target fold.

**R-02:** `DeferredAudit` + `emit_deferred` — field mapping verified: `a.user_id.as_deref()` → `Option<&str>`; `a.target.as_ref().map(|(k, id)| (*k, id.as_str()))` → `Option<(&'static str, &str)>`. Every queue site reaches an emit post-lock: `expire_if_past_due` pushes only on CAS `== 1`; `dispatch_action` emits after `drop(_g)` on both `Ok`/`Err` (a committed expire's audit survives a later mint failure); `respond_grant` emits before the `grant_not_pending` return; both `claim` dequeue sites emit before `first?`/`again?`, with `drain` preventing double-emit.

**R-03:** `mint_pairing` holds one `MutexGuard` across `check_user`/`record_user`; blocked attempts return without recording — consistent with the IP-bucket convention.

### Compile-level inspection (repo not compiled locally — verified by reading)

- `mint` async block's error type infers `E = AppError` uniquely; `audits.drain(..).collect()` infers `Vec<DeferredAudit>`; `let-else`, match-ergonomics `&` binding, `Some(&g.id)` → `Option<&str>` coercion all type-check by inspection.
- `emit_deferred`/`record_with` is infallible — emits can't error out a response path.
- `dequeue`/`expire_if_past_due` are private to `impl OxpilotConnectorGateway`; all call sites verified — no external callers broken by the signature changes.
- Claim park-loop semantics intact: waiter armed before first dequeue, cancel-set immediate answer, spurious-wake → `Timeout`, no notify ordering regression.

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| R-01 | Minor | crates/oxidean-api-oxpilot/src/connector/gateway.rs:330 | BR3.2 re-mint TOCTOU. `ensure_live_connect_grant` acquires the per-target lock (line 330) but never re-verifies `t.status == "pending_consent"` inside it — callers check status on the pre-lock read (lines 779, 801, 923). A `disconnect_oxpilot_target_tx`/`revoke_oxpilot_target_tx` that commits between the caller's read and the lock acquisition lets `insert_oxpilot_grant` (line 353) mint a `pending` connect grant on a `disconnected`/`revoked` target — BR6.3's "dead targets refuse everything" leaks. Downstream, `grant_respond(allow)` on that dangling grant wins the grant CAS while the covering `UPDATE oxpilot_targets … WHERE status='pending_consent'` (crates/oxidean-db/src/oxpilot.rs:4164) hits 0 rows yet the tx commits unconditionally — emitting `connect_grant_allowed` + `target_connected` audits for a target that never connected (audit-trail falsehood, BR7.2). No auth path opens (verifier is tombstoned or ride-revoked), and lazy expiry reaps the grant if it is never resolved — low blast radius, confirmed contract gap. | Re-read/re-check `t.status == "pending_consent"` inside `ensure_live_connect_grant` under the held lock before inserting (mirror the `respond_grant` re-read at line 1032); optionally also gate `allow_connect_grant_tx` on the target CAS landing (`rows_affected == 1` else roll back) so a dead target can never record `target_connected`. | New |
| R-02 | Nit | crates/oxidean-api-oxpilot/src/connector/gateway.rs:456 | Audit writes are awaited inside the per-target decision lock in three places: `dequeue`'s dead-grant `action_cancelled` fan-out (lines 456–469, invoked while `_g` is held at 1464/1491), `ensure_live_connect_grant`'s `connect_grant_pending` audit (366–373), and `dispatch_action`'s `action_grant_pending` audits (626–639, 653–667, plus `expire_if_past_due` on the locked call sites 339/1040). All rows land post-commit so correctness holds; they only lengthen the critical section — and the file's own `drop(_g)`-then-audit pattern (582, 832, 1055, 1130, 1182) shows the intended convention. | Emit the post-commit audits after dropping the per-target guard, matching the established pattern. | New |
| R-03 | Nit | crates/oxidean-api-oxpilot/src/connector/gateway.rs:732 | Mint limiter check-then-record is not atomic: `self.limiter().check_user(user_id)` (732) and `self.limiter().record_user(user_id)` (735) take the `FailedAuthLimiter` mutex in two separate statements, so two concurrent `pairing_create`s can interleave check→record and land one call over the 10-per-15-min user bucket. | Hold one `MutexGuard` across both calls (`let mut lim = self.limiter(); lim.check_user(..)?; lim.record_user(..)`). | New |

> R-01 Not re-checked this round

> R-02 Not re-checked this round

> R-03 Not re-checked this round
