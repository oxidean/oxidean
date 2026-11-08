# Monitoring Design — agent-runtime (U3)

Consumes: `nfr-design/observability-design.md`, `nfr-requirements/
observability-requirements.md`, `functional-design/rules.md` (BR7.1).

NFR6 forbids new stack — monitoring is the BR7.1 log table riding the
existing `tracing` pipeline.

## Operator signals

| signal | emission | reads |
|--------|----------|-------|
| turn started | `info!` `{session_id, turn_id, provider_id, model}` | a live turn exists; provider+model pinned |
| tool activity | `debug!` `{session_id, tool, call_index, status}` | tool loop health |
| queue drain | `debug!` `{session_id, drained_count, generation}` | chain liveness; drained>0 with generation increments shows chains |
| abort | `warn!` `{session_id}` | user/delegate kills a live turn |
| budget refusal | `debug!` `{scope}` (BR7.1) | refusal pressure per axis |
| terminal | `debug!` `{session_id, result, code?}` | outcome vocabulary — done/error/aborted + typed codes |
| degraded | `warn!` `{session_id, generation}` + reason | recheck-drain Err, panic-release, card_persist |
| claim-loss | `warn!` `{session_id, generation}` | BR4.1's "exit quietly" is the CHAIN arm (no successor spawns); the warn line is the pinned visibility per NFR6.3 |

## What's absent by design

- No metrics registry — the project ships log observability; a
  counter layer is an ops-stage concern.
- No per-turn health endpoint — the `streaming` row + span ids give
  the same answer through existing tools.
- No content in any line — ids and typed codes only (NFR6.1).

## Failure-shape reading

- `info` turn-start with no `debug` terminal + continued `ping`
  frames ⇒ wedged provider stream (alive, stalled).
- `warn` join-error ⇒ panicked task — claim was released; next POST
  reclaims cleanly.
- `warn` claim-loss bursts ⇒ admission-lock starvation — the signal
  for a lock-hold regression.
