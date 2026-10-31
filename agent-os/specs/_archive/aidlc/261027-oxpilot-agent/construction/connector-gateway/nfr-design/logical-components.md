# Logical Components — connector-gateway (U6)

Consumes: `functional-design/{entities,rules,functional-spec}.md`,
`nfr-requirements/*`, `nfr-design-questions.md` (Q1–Q7),
`codekb/oxidean-app/technology-stack.md`.

## Component catalogue

| component | crate | kind | owns |
|-----------|-------|------|------|
| `trait ConnectorGateway` | `oxidean-api-core::oxpilot` | trait port | C7's surface + backing verbs: `dispatch_action(user_id, session_id, target_id, kind, args) -> Result<Action, Denied>` — `Denied` is the AppError-domain error carrying `data:{denied:{reason, grant_id?}}` (C7 amendment pin; U5 distinguishes consent refusals from hard errors by that marker); `action_status(user_id, action_id) -> Result<ActionView>`; owner-side verbs `pairing_create`, `target_list/get/disconnect/revoke`, `grant_list/get/respond`; connector-side verbs `redeem`, `claim`, `report`, `heartbeat`; consumed by U5's `ToolExecutor` (dispatch path) and by the C3/proc routes |
| `GatewayImpl` | `oxidean-api-oxpilot::connector_gateway` | service | owns `GatewayState{locks: Mutex<HashMap<TargetId, Arc<Mutex<()>>>>, notify: Mutex<HashMap<TargetId, Arc<Notify>>>}`; every mutating decision takes `lock(target_id)`; mints/hashes secrets via workspace `sha2`+`rand`; calls `audit::record`/`audit::record_with` post-commit by call-site ctx; installs as a set-once `ConnectorGateway` field on `OxpilotRuntime` beside `sessions` (`OnceLock` — NOT the rebuildable `OxpilotRuntimeSlot`, so settings rebuilds can't drop it) |
| `trait ConnectorStore` | `oxidean-api-core::oxpilot` (or gateway module) | store port | row-level ops the service composes: `insert_pairing`, `claim_pairing_tx` (atomic pending→claimed + target + connect-grant insert), `pairing_by_hash`, `target`, `list_targets(owner)`, `grant(grant_id)`, `insert_grant`, `update_grant_state`, `live_session_grant`, `pending_grant`, `insert_action`, `action`/`resolve_action(action_id)` (report's idempotent resolve), `dequeue` (candidate scan + per-row CAS), `update_action_state` (CAS), `cancel_set`, `tombstone_verifier`, `touch_last_seen`, `flip_claimed_to_running`, `expire_pairings/grants` (lazy stamp), `session_cascade` (hook's bulk cancels); impl'd over `crates/oxidean-db` connector tables |
| connector tables | `oxidean-db` migrations `{postgres,sqlite,mysql}/00NN_oxpilot_connector.sql` + `oxidean-db/src/oxpilot_connector.rs` | schema | `pairings`, `targets`, `grants`, `actions` per entities.md; CASCADE FK on session-owned grant/action rows (AC1.5.2); indexes per scalability-design; `verifier_hash` NULLABLE — flagged deviation from entities' `required: true` (the disconnect tombstone needs NULL or a sentinel; NULL chosen) — requires tri-dialect `UNIQUE … WHERE verifier_hash IS NOT NULL` semantics verified in the dialect test |
| C3 routes | `oxidean-api-oxpilot::connector_routes` | http | `POST /api/oxpilot/connector/{pair,claim,report,heartbeat}`; `/pair` unauthenticated (code IS credential); others resolve `ConnectorIdentity` via Bearer→sha256→find-by-hash + `FailedAuthLimiter` check-IP-then-record (IP) + enablement via `resolve_enablement_slot(&*state.oxpilot.slot.read().await)` (the AppState-side seam — `require_enabled` needs an `RpcCtx` these routes never build) fail-closed ⇒ `{error:"disabled"}`; body `target_id` ≠ identity ⇒ 403 `forbidden` |
| `oxpilot.target_*`/`grant_*`/`pairing_create`/`target_get` procs | `oxidean-api-oxpilot::{procs,dispatch}` | rpc | SessionOnly + `require_enabled` + owner-scoped; `Denied` mapping; `TargetView`/`GrantView`/`ActionView` projections never carry hash/secret columns |
| `ConnectorDeleteHook` | `oxidean-api-oxpilot::connector_gateway` | hook | `SessionDeleteHook` impl; registered in `crates/oxidean-api/src/main.rs` AFTER U3's abort hook; single-tx bulk `cancelled` transitions + per-row audit; `Err` aborts delete (C8); idempotent |
| `ConnectorIdentity` | `oxidean-api-oxpilot::connector_routes` | type | `{target_id, user_id, caps, status}` per the entities pin — `caps` feeds the claim intersection, `status` feeds `revoked:true`; resolved at auth — the sole C3 scoping source (BR2.4) |
| `dialect_oxpilot` extension | `crates/oxidean-db/tests/dialect_oxpilot.rs` (existing file) | test | extends shipped file with connector-table parity legs; `docs/TESTING.md` row updated in the same change (NFR9) |

## Dependency shape

```
connector (U7, outbound-only)
        │  POST /pair,/claim,/report,/heartbeat  (verifier Bearer, /pair unauth)
        ▼
api-oxpilot::connector_routes ──▶ ConnectorGateway (trait on shared slot)
                                        │  GatewayImpl
                                        ├─▶ ConnectorStore ──▶ oxidean-db tables
                                        ├─▶ SessionService (C5) — ownership check
                                        ├─▶ audit::record (fire-and-forget)
                                        └─▶ FailedAuthLimiter (IP)
U5 ToolExecutor ──dispatch_action()──▶ same trait  (in-process)
owner procs (oxpilot.target_*/grant_*) ──▶ same trait
main.rs: register_delete_hook[U3 abort, U6 connector] ──▶ session cascade
```

The one-gateway rule: procs, C3 routes, and U5's dispatch all reach the
SAME `ConnectorGateway` impl — two auth entry classes (SessionOnly
cookie/PAT-refusal upstream vs verifier Bearer), one code path, one
state machine. The impl lives on `OxpilotRuntime` as a set-once field
beside `sessions` (per functional-spec's placement pin), reached from
procs via `RpcCtx.oxpilot.gateway` and from C3 routes via
`AppState.oxpilot.gateway` — `AppState.oxpilot` IS the
`SharedOxpilot`/`OxpilotRuntime` (app.rs), the same set-once accessor
pattern as `state.oxpilot.sessions()`.
