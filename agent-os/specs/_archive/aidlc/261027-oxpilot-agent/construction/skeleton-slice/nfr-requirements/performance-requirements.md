# Performance Requirements — u1-skeleton-slice

Consumes: `construction/skeleton-slice/functional-design/functional-spec.md`,
`functional-design/rules.md` (BR4.x),
`inception/requirements-analysis/requirements.md` (NFR3),
`inception/contract-design/contract-summary.md` (C2),
`codekb/oxidean-app/technology-stack.md`.

The skeleton's provider is in-process (StubProvider, BR4.5), so every
target below is measurable without a network provider. Wall-clock LLM
budgets belong to U3/U4.

## Latency targets

| id | requirement | target | basis |
|----|-------------|--------|-------|
| NFR3.1 | First SSE frame (snapshot or first `append`) after an accepted `POST .../chat` | < 250 ms p99 on a warm local process | NFR3 proxy; stub is in-process, so this measures our own pipeline not the provider |
| NFR3.2 | `snapshot` frame on `GET .../chat/stream` | < 250 ms p99 at ≤500 persisted messages | one indexed `oxpilot_messages` read — seq-indexed, NFR3's 500-message proxy |
| NFR3.3 | Keep-alive `ping` cadence | ≥ every 30 s of stream silence | C2 pin (BR4.3) |
| NFR3.4 | `oxpilot.session_list` round-trip | < 150 ms p99 at ≤200 caller sessions | indexed `(user_id, last_activity_at)` read (BR3.2) |
| NFR3.5 | `oxpilot.status` / `session_create` RPC | < 150 ms p99 | single insert/read on the skeleton tables |

## Throughput and resource bounds

| id | requirement | target | basis |
|----|-------------|--------|-------|
| NFR3.6 | Concurrent streamed turns per process | bounded by ActiveTurn map — one per session, no global cap beyond connection limits | BR4.2/BR4.6 |
| NFR3.7 | `emitted_deltas` ring buffer | bounded to the in-flight message's delta count; deltas are small strings — O(msg) memory per live turn, never O(session) | entities.md ActiveTurn; no unbounded per-session growth |
| NFR3.8 | Snapshot payload | single `oxpilot_messages` scan per attach — no per-delta re-read; message rows are the only transcript source | BR4.4, WF4 |
| NFR3.9 | Composer echo responsiveness | typing echo under ~100 ms on the minimal page (NFR3 proxy, aspirational flag kept) | NFR3; the skeleton page has no heavy per-key work |

## Measurement notes

Measurement protocol for the latency pins: a stack test
(`*.stack.test.ts` against a live API) asserts each bound as a
coarse single-shot ceiling — deterministic stub, N=1 per CI run,
pass/fail gate, warmup = one session_create round-trip before the
timed call. These are smoke-level SLOs, not load guarantees: no
performance-test infrastructure exists (recorded debt in team.md),
and p99 here means "one observed call must beat the bound," not a
percentile over a sample.
