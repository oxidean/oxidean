# Reliability Requirements — agent-runtime (U3)

Consumes: `construction/agent-runtime/functional-design/rules.md`
(BR3.x, BR4.x, BR5.x), `functional-design/functional-spec.md` (WF1–WF5),
`inception/requirements-analysis/requirements.md` (NFR4),
`inception/contract-design/contract-summary.md` (C2, C5, C8).

NFR4's "no silent stalls" obligation lands here — the turn is the
longest-running server-side work in the feature.

## Claim & release integrity

| id | requirement | basis |
|----|-------------|-------|
| NFR4.1 | One active turn per session is enforced by `ActiveTurns.claim` BEFORE the user row persists; post-claim early exits (dedupe-recheck hit, residual-drain `Err`) release the claim so a rejected POST never wedges the session (fd WF1.4–6) | BR1.1, C2 |
| NFR4.2 | Panic/JoinError inside the detached turn task still runs the release path — `spawn_turn_task` awaits the inner `JoinHandle` and the wrapper owns `release` on every exit shape (fd WF3) | BR5.3 |
| NFR4.3 | A process crash mid-turn leaves `streaming` transcript rows + a stale claim; the U2 orphan sweep reaps `streaming` rows on restart and the in-memory claim dies with the process — restart clears the state rather than corrupting it | BR5.x, C5 sweep seam |

## Queue liveness

| id | requirement | basis |
|----|-------------|-------|
| NFR4.4 | No queued row can commit into a drainless window: `message_queue`'s live-check + `enqueue` commit run under `admission_lock`, and the dying turn's release→recheck-drain holds the same lock — either the locked recheck chains the row or the enqueue refuses `session_active_turn_absent` and the client's POST path drains it (fd BR3.4/BR4.1/WF5) | BR3.4, BR4.1 |
| NFR4.5 | End-of-turn drain is atomic per U2's `drain_queue` (one tx claim+delete+append); a drain `Err` at turn end ⇒ warn-log `{session_id}` + treat-as-empty + still emit terminal + release (BR4.2's pinned arm), never a silent drop | BR4.2 |
| NFR4.6 | Chain depth is bounded (`MAX_CHAIN_DEPTH = 16`) — a queue-flooding client can't keep a single turn lineage alive forever; depth-cap exit leaves rows for the next turn start's residual drain | BR4.3 |

## Terminal correctness

| id | requirement | basis |
|----|-------------|-------|
| NFR4.7 | Terminal emission is first-wins via `terminal_sent` CAS across its three writers (abort hook, `run_turn` exits, wrapper Err arm) — exactly ONE terminal frame ever reaches `emitted_deltas`/`fanout`; no frame is emitted after terminal (fd BR5.4) | BR5.4, C2 |
| NFR4.8 | Provider `ChatStreamItem::Error` and stream-open failures always terminal `{result:"error", code}` with the provider's stable code — a failed turn is a visible transcript state, never an ambiguous hang | BR5.2, C4 |
| NFR4.9 | Missing model at turn start (`model: None`) terminals `no_model` — a configured-but-model-less provider produces a typed refusal, not a fabricated request | BR5.2 |
| NFR4.10 | `turn_cancel`/delete-abort emits `{result:"aborted"}` (no code) then releases normally; on the delete path the doomed-session recheck exits quietly via the pinned Err/claim-loss arms — no successor turn is chained onto a deleted session (fd WF4) | BR4.1, WF4 |

## Tool-call durability

| id | requirement | basis |
|----|-------------|-------|
| NFR4.11 | Tool cards persist BEFORE their live `card` frame emits — BR2.4's durable-first ordering (card `running` → frame; terminal status write → terminal frame); a crash mid-ordering leaves rows, never phantom frames | BR2.4 |
| NFR4.12 | Card-persistence failure feeds back to the loop as a ToolError result (`card_persist`), never silently skips the call — the provider sees the tool answered even when the card write failed | BR5.2 |
