# NFR Requirements Questions — agent-runtime

Unit: `agent-runtime` (U3, kind `service`) — `AgentRuntime` turn loop,
prompt assembly, provider streaming + SSE frame emission,
tool-call orchestration seam (C6), queue drain/chaining, admission
lock, per-turn provider pinning, `oxpilot.turn_cancel`, and the
abort-on-delete (C8) coordination.

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U3.
- [req] requirements.md — NFR1 (session-only auth, no secret leak),
  NFR3 (chat responsiveness — stream liveness at transcript scale),
  NFR4 (no silent stalls; single-replica caveat), NFR6 (reuse
  observability surfaces), NFR9 (collect-only coverage + test row).
- [design] construction/agent-runtime/functional-design/{entities,
  rules, functional-spec}.md — BR1–BR6 the NFRs bind to.
- [contract] contract-summary.md — C2 SSE vocabulary, C4 provider
  trait, C5 service surface, C6 tool seam, C8 hooks.

## Q1. Which NFR domains does agent-runtime carry real obligations in?

- A. Security (provider credentials never enter frames/prompt beyond
  redacted form; tool args/results stay inside the authz seam),
  Privacy (prompt assembly stays user-scoped; transcript window is
  the owner's own rows), Performance (turn wall cap, per-round tool
  timeout, output/arg buffer caps, prompt-window bound),
  Reliability (admission-lock atomicity, panic/JoinError claim
  release, terminal first-wins CAS, drain/chain liveness),
  Scalability (one active turn per session; per-session lock, no
  global serialization), Observability (turn lifecycle logs/metrics
  on the existing surfaces), Testability (mock provider + fake
  executor seams for the loop). All at standard depth; empty domains
  recorded N/A.
- B. Security only — misses NFR3's stream liveness and NFR4's
  no-silent-stall obligations which are this unit's core.
- C. Defer to U8 — the client renders; the runtime's latency and
  liveness obligations live here.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — all nine domains assessed at standard depth; populated where the unit has real obligations. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.
## Q2. Prompt-window bound policy?

- A. Token-budget assembly: newest-first transcript rows until a
  pinned byte window (`PROMPT_TRANSCRIPT_MAX_BYTES = 128 KiB` newest-first
  byte window), oldest evicted beyond it; system + repo context
  always included; bound is constant regardless of session age —
  NFR3's latency guarantee needs the request size fixed.
- B. Full transcript every turn — unbounded request growth;
  violates NFR3 at mature-session scale.
- C. Model-context-window-aware packing — provider tokenizers vary;
  the byte bound is the honest portable proxy in v1.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — PROMPT_TRANSCRIPT_MAX_BYTES = 128 KiB newest-first byte window (fd BR2.3). Conductor-decided (2026-10-27); ratify at PR review.
## Q3. Frame-buffer and stream-liveness posture?

- A. `emitted_deltas` unbounded by design (accepted risk — implicitly
  bounded by `TURN_MAX_DURATION` × provider throughput; recorded in
  functional-design entities). Liveness via C2 `ping` frames every
  the shipped `PING_INTERVAL` (15 s) so a silent provider can't wedge the client
  view; terminal CAS guarantees exactly one ending. `ping` cadence
  reused from U1's shipped constant.
- B. Bounded ring buffer with replay gaps — breaks C2's
  snapshot+replay model (late attachers would miss deltas).
- C. No liveness frames — C2 already pins `ping`; removing them is a
  contract regression.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — unbounded emitted_deltas recorded as accepted risk; ping cadence reuses the shipped constant; terminal CAS unchanged. Conductor-decided (2026-10-27); ratify at PR review.
## Q4. Admission-lock performance/contention posture?

- A. Per-session `tokio::sync::Mutex<()>` stored beside the map entry,
  dropped with it; held across `drain_queue`'s DB I/O — acceptable
  because the lock serializes only this session's
  enqueue-vs-release race (~ms DB write, no cross-session wait);
  guard-scoped so panic can't wedge a session. No lock on the
  read/stream path — `ActiveTurns` map reads stay lock-free-ish via
  DashMap/RwLock semantics.
- B. Global admission lock — serializes unrelated sessions' enqueues;
  violates scalability.
- C. Lock-free CAS on a state field — DB enqueue isn't CAS-able; the
  atomic check+commit genuinely needs a mutex.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — per-session tokio Mutex beside the map entry; no global lock; guard-scoped against panic. Conductor-decided (2026-10-27); ratify at PR review.
## Q5. Secret-isolation posture inside the runtime?

- A. Provider key material lives behind the pinned `Arc<dyn
  LlmProvider>` (U4 owns decrypt); the runtime never sees plaintext
  keys. Prompt assembly pulls transcript rows already redacted at
  the U2 service boundary — the runtime adds a second `redact()` pass
  on tool-result content before it becomes `ChatMsg` (BR3.2), and
  zero secrets flow into SSE frames (frames carry only ids, deltas,
  statuses, bounded summaries).
- B. Runtime-side prompt scanning with a secret detector — defense-
  in-depth theater; U2's boundary redaction is the pinned custody
  point, and a second detector creates false-positive stalls.
- C. Nothing needed — wrong: tool results are a NEW content path the
  U2 write boundary never saw; they need the runtime-side pass.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — key material stays behind U4's Arc<dyn LlmProvider>; runtime-side redact() on tool-result ChatMsg content; zero secrets in frames. Conductor-decided (2026-10-27); ratify at PR review.
## Q6. Failure-liveness posture (no silent stalls)?

- A. Every turn outcome reaches a terminal frame or transport close;
  panic/JoinError → claim released + `internal` terminal; provider
  stream stall → client sees `ping` cadence and can cancel via
  `turn_cancel` (no read-side stall watchdog in v1 — the 10-min wall
  cap is the bound); claim-loss exits quietly. Single-replica limit:
  `ActiveTurns` is in-memory — a crash mid-turn leaves `streaming`
  rows the sweeper reaps via the orphan sweep (U2 seam).
- B. Read-side provider stall watchdog + auto-terminal — extra timer
  machinery; the wall cap + ping + cancel cover the same guarantee.
- C. Rely on client disconnect detection — doesn't free the server
  claim; the sweeper is the honest crash answer.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — every outcome reaches terminal/close; panic releases claim; sweeper owns crash-time streaming rows. Conductor-decided (2026-10-27); ratify at PR review.
## Q7. Observability surface for the turn loop?

- A. Reuse the existing tracing/log stack (NFR6): structured
  `tracing` spans per turn (`session_id`, turn generation, provider
  id — never content), `debug!` on terminal emission + `warn!` on
  abort/drain-Err/claim-loss/panic-recheck arms; NO new metrics registry —
  log-based observability matches the project's current posture.
- B. Prometheus metrics per turn phase — no metrics stack exists in
  this repo; inventing one exceeds scope.
- C. Silent loop — NFR4's operability intent fails; the warn arms are
  how an operator sees claim-loss churn.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — tracing spans per turn (ids only, never content); BR7.1's pinned levels; no new metrics registry. Conductor-decided (2026-10-27); ratify at PR review.
## Q8. Testability seams the NFRs must name?

- A. `LlmProvider` mock (U4 already ships fakes), `ToolExecutor`
  fake asserting `ToolCall`/`ToolCtx` plumbing, deterministic
  `Clock`/`tokio::time` control for timeout tests, seeded
  `drain_queue` fixtures for chain tests — all inside existing
  `cargo-nextest` + wiremock posture; dialect tests reuse U2's
  `-j 1` invocation.
- B. New e2e harness — Playwright coverage is U8's; the runtime's
  seams are unit/integration-level.
- C. Mock-free testing against real providers — non-deterministic,
  leaks credentials into CI.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — mock provider + fake executor + deterministic time; inside nextest/wiremock posture. Conductor-decided (2026-10-27); ratify at PR review.

## Consolidated Summary Confirmation

- Domains: all nine assessed at standard depth — Security, Privacy,
  Performance, Reliability, Scalability, Observability, Testability,
  plus N/A-recorded gaps (Q1).
- Prompt bound: newest-first window, `PROMPT_TRANSCRIPT_MAX_BYTES =
  128 KiB` newest-first byte window (Q2).
- Stream liveness: `ping` cadence + terminal CAS; `emitted_deltas`
  unbounded-by-design accepted risk (Q3).
- Admission lock: per-session `tokio::sync::Mutex<()>`, no global
  serialization (Q4).
- Secrets: key material behind U4's provider Arc; runtime-side
  `redact()` on tool results; zero secrets in frames (Q5).
- Failure liveness: terminal-or-close on every outcome; panic
  releases claim; sweeper owns crash rows (Q6).
- Observability: id-only tracing spans at BR7.1's pinned
  levels, no new registry (Q7).
- Testability: mock provider + fake executor + deterministic time
  inside existing nextest/wiremock posture (Q8).

Does this all look correct before I write the NFR artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
