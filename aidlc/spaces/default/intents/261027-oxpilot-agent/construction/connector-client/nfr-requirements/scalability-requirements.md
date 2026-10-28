# Scalability Requirements — connector-client (U7)

Consumes: functional-design `rules.md` BR4.1/BR5/BR8, `entities.md`
RunState, questions Q1-A (Scalability recorded N/A with pins).

## Concurrency model

| id | requirement | basis |
|----|-------------|-------|
| NFR-S1 | Single-process serial executor by contract: one in-flight action, one outstanding claim, one child process — there is no intra-client parallelism to bound; the claim FIFO is the server's, so multi-connector scale is the server's concern, not this unit's | BR4.1, Q7-A |
| NFR-S2 | `RunState` memory is O(running + settled): `running` holds the in-flight action map (≤1 by serial contract), `settled` dedupes reported cancel ids — bounded by actions-per-process-lifetime, cleared on exit | RunState, BR5.1 |

## Growth edges

| id | requirement | basis |
|----|-------------|-------|
| NFR-S3 | Multi-target: `pilot-targets.json` is a map keyed by `target_id` — multiple pairings per instance survive; `run`/`disconnect` resolve `--target` or demand it when ambiguous, while `status` always prints ALL rows (BR9.2 — no `--target` gate). One process per target is the v1 model; a multi-target loop is out of scope | BR9.1, BR9.2, BR9.3 |
| NFR-S4 | Output/payload growth is pinned server-side (`OUTPUT_MAX`) and client-side (64 KiB exec tail, 128 KiB FS_IO_CAP) — no growth path exists that a larger file or noisier child can open | BR7.1, BR6.2, NFR3.6–3.8 |
| NFR-S5 | Claim load is self-limiting: heartbeat runs at a fixed 4/min (15s cadence) plus one claim per wait-window (~1–2/min at the 30–60s bound) — and a flapping connector decays to the 60s backoff ceiling rather than amplifying load | BR8.1, BR8.2, BR8.5 |

Scalability is otherwise N/A for this unit — recorded per Q1-A.
