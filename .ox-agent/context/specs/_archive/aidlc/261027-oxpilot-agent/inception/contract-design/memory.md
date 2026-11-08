# Contract Design — stage diary

## What ran

- 5 questions resolved under standing instruction; summary confirmed.
- 8 contracts pinned: C1 RPC families, C2 SSE frames, C3 connector
  OpenAPI sketch, C4 `LlmProvider`, C5 `SessionService`, C6
  `ToolExecutor`, C7 action-queue, C8 `on_session_delete` hook.
- Advisory review: READY, 2 major + 7 minor, all applied.

## Findings applied

- R-01 (major): `oxpilot.status` marked gate-exempt — it is the flag-off
  discovery proc.
- R-02 (major): `POST /chat` error vocabulary enumerated incl. 409
  `turn_active` → client must call `message_queue`.
- R-03: `admin.oxpilot.*` members pinned (settings get/set with
  write-only key material + providers_list).
- R-04: `turn_cancel`/`usage_get` error lists added.
- R-05: `retry_after` moved to `Retry-After` header on 204 + body field
  on 200; `wait_seconds>60` clamped.
- R-06: abort-on-delete single enforcement = C8 hook inside
  `SessionService.delete_session`; idempotent, ordered, API proc does no
  turn logic.
- R-07: C3 block declared a contract-shape sketch; real openapi.yaml is
  a U7 code artifact.
- R-08: `oxpilot.grant_get` + `oxpilot.queue_list` added for card refs.
- R-09: `ActionView` public trim pinned for C7.
