# Infrastructure Specification — connector-gateway (U6)

Consumes: `nfr-design/*`, `nfr-requirements/*`,
`functional-design/{rules,entities,functional-spec}.md`,
`inception/domain-design/components.md` (component inventory),
`team.md` (Way of Working), `contract-summary.md` (C3, C7).

## Deployable surface

U6 adds **no deployable and no sidecar**. The gateway is in-process
work inside the existing single API binary. What ships:

| surface | change | infra impact |
|---------|--------|--------------|
| `crates/oxidean-db` | 4 connector tables + migration set (`oxpilot_pairings`, `oxpilot_targets`, `oxpilot_grants`, `oxpilot_actions`); store ops per nfr-design logical-components | none — same crate, additive migration |
| `oxidean-api-core` | `ConnectorGateway` trait port (`dispatch_action`, consent, close paths) beside the existing service traits | none — library type |
| `oxidean-api-oxpilot` | gateway impl; `OxpilotRuntime` set-once `gateway: OnceLock<Arc<dyn ConnectorGateway>>` field beside `sessions` (U3's `OnceLock<Arc<dyn SessionService>>` precedent); C3 HTTP routes under `/api/oxpilot/connector/{pair,claim,report,heartbeat}`; owner RPC procs (`oxpilot.target_*`, `oxpilot.grant_*`) already pinned in C1 | none — same binary, same image, same router |
| connector state | per-target `Mutex`/`Notify` map, process-local (NFR-S2/S6 single-replica) | none — no Redis/NATS |
| deps | `oxidean-api-oxpilot`'s manifest gains `sha2`+`rand` (id mint/hashing; workspace-pinned, no new external crate enters the lock) — OR the mint/hash helpers land in api-core if codegen prefers; hashing itself can ride api-core's public `sha256_hex`. Either way the lockfile gains nothing | supply chain unchanged |

No container, compose service, or ingress object is added. The C3
routes are ordinary `axum` routes on the shipped `/api/` surface —
they ride the same listener and middleware chain as every U1–U4
route.

## Environment

No new variables. The gateway consumes:

- U4's enablement flag surface — C3 routes resolve it via
  `resolve_enablement_slot(&*state.oxpilot.slot.read().await)`
  (NOT `require_enabled`: C3 routes hold `AppState`, no `RpcCtx`),
  returning `{error:"disabled"}` on the off path.
- Existing DB pool (`DATABASE_URL`), existing audit client, the
  shipped `FailedAuthLimiter` in both of its keyings — **IP-keyed**
  for connector auth (`check_ip` before credential lookup) and
  **per-user** for the pairing-mint limiter
  (`check_user`/`record_user`, BR1.5) — plus the existing
  `oxidean_core::redact` helper.
- No pairing/verifier secret material crosses the env surface —
  pairing codes and verifiers are minted at request time and only
  their SHA-256 hashes persist (NFR1.x).

## Restart invariants

- The per-target lock/Notify map is empty at boot by construction —
  rebuild is lazy on first dispatch/claim.
- `claimed`/`running` actions whose response was in flight across a
  restart are stranded in non-terminal state by design (at-most-once
  claim, NFR4.x residual): the connector's next authenticated touch
  (claim or heartbeat) flips its own prior `claimed` rows to
  `running`; rows stranded with no attached
  connector are closed only by the explicit close paths (target
  disconnect/revoke, session cascade, grant revoke). No boot sweep is
  introduced — the close paths, not a sweeper, own termination.
- Lazy expiry replaces any sweeper: pending pairings/grants expire on
  touch (`expires_at` check), so no cron/poller exists to miss state
  across restarts.
- Single-replica boundary (NFR-S2 caveat stated, NFR-S6 no
  cross-node coordination): the in-memory Notify map and
  at-most-once claim loop are only coherent under one API
  process. Deployed replicas >1 violates the pinned contract; this is
  recorded in `monitoring-design.md` as an operator note, not a code
  gate.

## Resource shape

- Per-target: one `Mutex` + one `Notify` + one dequeue cursor —
  count bounded by registered targets, not sessions or threads.
- `claim` parks on `Notify` up to `wait_seconds` clamped at 60 s —
  one parked task per attached connector.
- Dequeue is FIFO `(created_at, action_id)` bounded at 50 rows;
  `stored_args`/result summaries bounded before persist by
  `oxidean_core::redact` + `SUMMARY_MAX`.
- `oxpilot_actions` + session-scoped `oxpilot_grants` row growth is
  bounded by session lifetime — the session cascade deletes rows on
  session destroy. Connect grants (`session_id NULL`) are
  target-lifetime-bounded instead: they survive session delete
  (BR6.6) and disconnect/revoke cancels them in place — terminal
  rows persist as audit history. Rows carrying verifier material
  are hash-only so the tables hold no retrievable secret.

## Network posture

| path | auth | bound |
|------|------|-------|
| `/pair` | pairing code IS the credential (unauth HTTP) | explicit limiter carve-out: no rate limit on redemption; mint-side per-user limiter covers code-generation pressure |
| `/claim`, `/report`, `/heartbeat` | `Authorization: Bearer <verifier>` → SHA-256 → `oxpilot_targets.verifier_hash` | IP check via `FailedAuthLimiter` BEFORE credential lookup; 429+Retry-After when tripped; 401 for absent/unknown/`disconnected`-tombstoned credentials; a `revoked` verifier still authenticates — heartbeat answers `{revoked:true}` + the cancel set, claim/report answer `{error:"revoked"}` except report's terminal-close arm (BR2.2/NFR4.7) |
| owner procs (`target_*`, `grant_*`) | session auth (existing gate) | scoping = authenticated identity only; foreign `target_id`/`grant_id` → `not_found` (anti-enumeration, BR7.1); `forbidden` on identity mismatch is C3-only (BR2.4) |

Outbound surface: none added. The gateway never egresses — the
connector dials in; the protocol is pull-only (`claim` long-poll +
`report` push).
