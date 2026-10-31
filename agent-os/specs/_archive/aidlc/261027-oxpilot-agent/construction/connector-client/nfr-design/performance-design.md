# Performance Design — connector-client (U7)

Consumes: `nfr-requirements/performance-requirements.md`
(NFR3.1–3.11), `functional-design/rules.md` BR2.2/BR5.4/BR6.2/BR7/
BR8, `functional-spec.md` Pins, `nfr-design-questions.md` Q3-A/Q5-A.

## Pacing mechanics (NFR3.1–3.5)

```text
claim request body: wait_seconds = clamp(flag_or_default(30), 1, 60)
claim client timeout: wait_seconds + 15s    (long-poll outlives wait)
pair/report/heartbeat timeout: 30s          (device.rs precedent)

loop arms (tokio::select!):
  1. heartbeat_interval.tick()   — 15s tokio interval
  2. claim_future                — long-poll
  3. signal                      — close-out (reliability-design)

exec-phase arms (nested select! per action — the outer loop is
suspended while one action runs, so the child wait itself races):
  1. child completion
  2. EXEC_TIMEOUT deadline       — 300s group-kill path
  3. heartbeat tick              — keeps the target inside the 90s
     liveness window on >90s jobs; response `cancel` ids feed the
     mid-flight kill against RunState.running
  4. signal                      — same close-out funnel

after each response:
  sleep = retry_after (200 body) or Retry-After (204 header)
  if cancel-only answer (actions empty && cancels applied):
      sleep = max(sleep, 1s)         // BR5.4 floor — server answers
                                     // instantly on pending cancels
  sleep.max(1s if unset)             // BR8.1 default re-poll
```

Backoff (separate from hints): `delay = min(60, 2^n × (1 ± 0.2))`
with `fail_streak` reset to 0 on any successful call — disabled-flap
and transport-flap share the decay path. The jitter is ±20% of the
computed delay (uniform), applied BEFORE the ceiling clamp so 60s is
the true ceiling.

## Payload mechanics (NFR3.6–3.9)

- Exec output collection: two pump threads (stdout/stderr) writing
  into a shared tail buffer that retains the LAST 64 KiB — a
  `yes`-class child can't grow memory unboundedly (the bound is on
  the buffer, not the readback). UTF-8 lossy-trim to char boundary
  at cap, `…<n> bytes truncated` marker prepended.
- fs_* payloads: `FS_IO_CAP` 128 KiB raw checked BEFORE read
  (metadata len) — oversized refuses `failed:too_large` without
  reading the file at all; within-cap content base64'd (~171 KiB
  wire < `OUTPUT_MAX` 256 KiB).
- Universal bound: report `output` asserted `< OUTPUT_MAX` at
  construction — a violation is a client bug reported `failed`
  with a bounded message, never sent oversized.
- Scrub is O(output) substring replace over ≤ ~dozens of ≥8-char
  values — bounded linear scan, no regex, no entropy sweeps.

## Resource bounds (NFR3.9–3.11)

- `RunState.running` ≤ 1 entry (serial contract); `settled` grows
  by action throughput — bounded by process lifetime, cleared on
  exit; for a long-lived connector the dedupe set rotates once the
  server stops re-announcing (close-confirmed ids can evict —
  documented bound: insert-on-report, evict when the server stops
  re-announcing that id, i.e. when a subsequent response's cancel
  set no longer contains it).
- Single in-flight HTTP request per route by construction — the
  serial loop issues claim, then report(s), then heartbeat ticks;
  no connection pooling surface to bound beyond reqwest defaults.
- fs ops are synchronous calls wrapped in `tokio::task::spawn_blocking`
  so a slow disk doesn't stall the heartbeat arm.
