# Performance Requirements — connector-client (U7)

Consumes: functional-design `rules.md` BR2/BR5/BR6/BR7/BR8,
`functional-spec.md` Pins, questions Q5-A, shipped U6 constants
(`CLAIM_WAIT_MAX_SECS=60`, `CLAIM_RETRY_AFTER_SECS=0`,
`OUTPUT_MAX=256*1024`, 90s liveness window).

## Pacing bounds

| id | requirement | basis |
|----|-------------|-------|
| NFR3.1 | `wait_seconds` sent client-clamped [1, 60] — the server clamps the top only; `0` parks an instant `204 + Retry-After: 0` that would spin re-polls at RTT speed (the same spin class BR5.4 floors) | BR2.2, N-06 |
| NFR3.2 | Request timeouts: `pair`/`report`/`heartbeat` = 30s (device.rs precedent); `claim` = `wait_seconds + 15s` — the long-poll must outlive its own wait or every cycle fake-times-out | Pins, R-13 |
| NFR3.3 | Cancel-only claim answers (`actions: []`, re-announced cancel ids, `retry_after: 0`) re-poll with a ≥1s floor — the server answers instantly when a cancel set is pending; without the floor an unclosed strand spins at RTT speed | BR5.4, `CLAIM_RETRY_AFTER_SECS=0` |
| NFR3.4 | Heartbeat every 15s on its own timer — inside the server's 90s liveness bound, far enough out to not be chatty; heartbeat results' `cancel`/`revoked` sets apply exactly like a claim's | BR8.5, `oxpilot.rs:537` |
| NFR3.5 | `retry_after` (200 body) + `Retry-After` (204 header) honored exactly — the server is the pacing authority; absent a hint the client re-polls after 1s | BR8.1 |

## Payload bounds

| id | requirement | basis |
|----|-------------|-------|
| NFR3.6 | exec `output` = last ≤ 64 KiB combined stdout+stderr (UTF-8, char-boundary trim) + `…<n> bytes truncated` marker | BR7.1, OUTPUT_CAP |
| NFR3.7 | `FS_IO_CAP` = 128 KiB raw both directions — wire-derived: 128 KiB → base64 ≈171 KiB < server `OUTPUT_MAX` 256 KiB; oversize → `failed:too_large` with byte count, never a truncated `done`; cap applied to RAW bytes BEFORE base64 encode | BR6.2, BR4.3, BR4.4, N-02 |
| NFR3.8 | Universal bound: the final report `output` string stays under `OUTPUT_MAX` post-scrub/post-encode — a report the server can't accept (`bad_input`) strands the action row | BR7.1, `routes.rs:47` |
| NFR3.9 | Serial executor = bounded resource use by construction: at most one child process, one in-flight action, one claim request outstanding | BR4.1 |

## Client-side cost discipline

| id | requirement | basis |
|----|-------------|-------|
| NFR3.10 | The scrub pass is O(output) substring replacement over an enumerated set (≤ ~dozens of values ≥8 chars) — bounded, no regex sweeps, no entropy scans | BR7.2, Q3-A |
| NFR3.11 | `RunState` is O(running + settled) — the settled set is bounded by action throughput; no unbounded history structures | RunState, BR5.1 |
