# Scalability Design — connector-client (U7)

Consumes: `nfr-requirements/scalability-requirements.md`
(NFR-S1–S5), `functional-design/rules.md` BR4.1/BR5/BR8/BR9,
`entities.md` RunState.

## Concurrency model (NFR-S1/S2)

Single-process serial executor by contract — one `tokio::select!`
loop, ≤1 entry in `RunState.running`, one outstanding claim, one
child process. There is no intra-client parallelism to bound; the
claim FIFO is the server's. State sizing:

```text
running:   HashMap<ActionId, ChildHandle>   // ≤1
settled:   HashSet<ActionId>                // dedupe set — evicts an
           // id once a subsequent response's cancel set no longer
           // re-announces it (server-side close confirmed)
backoff:   { fail_streak: u32, next_delay: Duration }
root:      PathBuf (canonicalized once)
secrets:   Vec<SecretString>                // enumerated scrub set,
           // assembled at launch + custody events
```

## Growth edges (NFR-S3–S5)

- Multi-target: `pilot-targets.json` is a map keyed by `target_id`;
  `run`/`disconnect` resolve `--target` or demand it when ambiguous;
  `status` always prints ALL rows (no `--target` gate — BR9.2).
  One process per target is the v1 model; a multi-target run loop is
  out of scope.
- Payload growth is pinned server-side (`OUTPUT_MAX`) and client-side
  (64 KiB exec tail, 128 KiB FS_IO_CAP) — no path a larger file or
  noisier child can open.
- Claim load is self-limiting: heartbeat is a fixed 4/min (15s
  cadence) plus one claim per wait-window (~1–2/min at the 30–60s
  bound); a flapping connector decays to the 60s backoff ceiling
  rather than amplifying load — scale-out is N connectors × N
  targets, each independent, no shared client-side resource.

Scalability is otherwise N/A for this unit (Q1-A).
