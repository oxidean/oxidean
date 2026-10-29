# Performance Validation — Clarifying Questions

## Q1. Traffic pattern?

**Answer:** Steady-state chat turns (1 per session) + SSE long-lived connections; peak = concurrent sessions per user × users. First rollout is flag-off→opt-in, so real load data arrives post-enable.

## Q2. Target percentiles?

**Answer:** From session-core NFR3.x — first SSE frame <250ms p99, `session_list` <150ms p99 (p99 targets, per NFR).

## Q3. Throughput to sustain?

**Answer:** Single-replica caveat bounds concurrency: one live turn per session, queue drains serially; no published RPS SLO — resource bounds (128 KiB transcript, 2 KiB summaries, 60s tool timeout, 8 rounds, 10min turn) are the operative limits.

## Q4. Bottlenecks?

**Answer:** Provider streaming latency (dominates first-frame time), DB transcript assembly under the 128 KiB bound, single-replica in-memory turn state (no horizontal scale for live turns).

## Consolidated Summary Confirmation

- Flag-off→opt-in rollout; steady chat+SSE traffic model (Q1).
- p99 SLOs: <250ms first frame, <150ms session_list — measured post-deploy (Q2).
- Resource bounds are the operative limits; single-replica concurrency (Q3).
- Provider latency, transcript assembly, in-memory turn state = bottlenecks (Q4).

Does this all look correct before I write the performance-validation artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
