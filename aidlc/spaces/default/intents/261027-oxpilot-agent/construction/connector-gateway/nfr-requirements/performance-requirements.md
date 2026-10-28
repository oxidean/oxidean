# Performance Requirements — connector-gateway (U6)

Consumes: `construction/connector-gateway/functional-design/rules.md`,
`functional-design/entities.md`, `functional-design/functional-spec.md`,
`inception/requirements-analysis/requirements.md` (NFR3),
`inception/contract-design/contract-summary.md` (C3, C7).

Connector-gateway is a control plane — its performance obligations are
about holding long-polls cheaply, bounding payloads, and never letting
one target's queue block another's.

## Long-poll & claim latency

| id | requirement | basis |
|----|-------------|-------|
| NFR3.1 | `POST /connector/claim` clamps `wait_seconds` to `min(wait_seconds, CLAIM_WAIT_MAX = 60 s)` — an over-60 value clamps, never rejects (BR5.1/C3) | BR5.1, spec Pins |
| NFR3.2 | A claim waits on the FIRST of: a `queued` action for the identity's target becoming dequeuable, `wait_seconds` elapsing — timeout answers `204` + `Retry-After` header with NO body; a non-timeout answer is `{actions:[{action_id, kind, args, grant_id}], cancel:[action_id], retry_after}` (C3's pinned shape — the field names are `cancel`/`retry_after`, carried verbatim) | BR5.1, C3 |
| NFR3.3 | Dequeue is FIFO by `created_at` on the identity's target — each candidate's state-conditional update runs inside the per-target lock with the BR5.1 grant/caps re-check; scan/read paths are index-served, only the dequeue write serializes (BR5.1, BR4.1) | BR5.1, BR4.1 |
| NFR3.4 | Claim dequeue marks `queued→claimed` ONLY; `claimed→running` flips on the connector's NEXT authenticated touch — claim or heartbeat (BR4.3: any touch is execution proof). A claim response alone never marks `running` | BR4.3, BR5.1, BR5.3 |
| NFR3.5 | Delivery is at-most-once with no requeue: a claim response lost in transit strands the action `claimed`/`running` until a server-side close lands (BR6.x paths or BR6.4's session-grant revoke) — visible via `action_status`, never connector-closeable (no action_id was received). Accepted residual (BR5.1) | BR5.1 |
| NFR3.6 | `heartbeat` touches `last_seen_at`, flips claimed rows running, and returns `{revoked: bool, cancel: [action_id]}` — the cancel set re-announces ALL `cancel_requested` rows on every heartbeat/claim until closed; kill semantics are idempotent (BR5.3) | BR5.3 |

## Liveness bookkeeping

| id | requirement | basis |
|----|-------------|-------|
| NFR3.7 | `last_seen_at` updates on ANY authenticated connector touch — claim/report/heartbeat, never `/pair` (BR5.1/BR5.2/BR5.3); it is a cheap single-column write on the request path, not a background job | BR5.x |
| NFR3.8 | `TargetView.status`: `revoked` lifecycle → `revoked`; `disconnected`/`pending_consent` → `offline` regardless; `connected` lifecycle → `online` iff `last_seen_at` within `HEARTBEAT_LIVENESS_WINDOW = 90 s` (≈3× the documented client interval), else `offline` — derived at read, never a stored flag; `lifecycle` is the orthogonal persisted state (BR5.4) | BR5.4, spec Pins |
| NFR3.9 | Re-mint of the connect grant fires on ANY owner read (`target_list`/`target_get`/`grant_list`) of a `pending_consent` target that has NO live (pending+unexpired) connect grant — denied and never-minted covered alike; each mint audits `connect_grant_pending`; O(1) per read under the per-target lock (BR3.2) | BR3.2 |

## Bounds & resource caps

| id | requirement | basis |
|----|-------------|-------|
| NFR3.10 | Pinned field bounds (all redacted before write): `action_args`/`args` ≤ `ACTION_ARGS_MAX = 2 KiB`; `action_summary`/`args_summary` ≤ `SUMMARY_MAX = 512 B`; report output/`result_summary` ≤ `RESULT_MAX = 2 KiB` tail. No unbounded blob column exists in the connector schema (BR7.4) | BR7.4, spec Pins |
| NFR3.11 | One target's long-poll never serializes another target's work: the admission lock is keyed `target_id` and covers only dispatch/grant/close decisions ON that target plus dequeue writes; claim candidate scans and owner reads use indexes, not the lock — EXCEPT the BR3.2 re-mint write path, which serializes under the per-target lock (BR5.1, BR4.1, BR3.2) | BR5.1, BR4.1 |
| NFR3.12 | New NFR-introduced bound (not fd-pinned, flagged): `CLAIM_ACTIONS_MAX = 50` caps actions per claim response so one backlog cannot monopolize a poll window; overflow stays `queued` for the next claim | NFR-level bound |
| NFR3.13 | The queue index `(target_id, state, created_at, action_id)` ships in the same migration as the table; owner-scoped lists index `(owner_user_id)`; hash lookups are equality on `code_hash`/`verifier_hash` columns — no sequential scan on any documented path (BR5.1, NFR9) | BR5.1, NFR9 |
