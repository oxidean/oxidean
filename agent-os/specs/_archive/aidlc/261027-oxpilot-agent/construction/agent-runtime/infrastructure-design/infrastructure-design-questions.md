# Infrastructure Design Questions — agent-runtime

Unit: `agent-runtime` (U3, kind `service`) — runtime behavior lives in
process inside `oxidean-api`/`oxidean-api-oxpilot`; infra concerns are
about what the pipeline must know to keep this unit's seams green.

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U3.
- [design] `construction/agent-runtime/{functional-design,
  nfr-requirements, nfr-design}/*` — the seams CI must cover.
- [team] `team.md` — Way of Working (trunk, squash-merge, ruleset).

## Q1. Does agent-runtime add infrastructure?

- A. None — all in-process (tokio tasks, DashMap, CancellationToken);
  no new deployable, no new service, no migration beyond U2's tables,
  no env vars beyond the existing `OXIDEAN_OXPILOT_*` set.
- B. A dedicated runtime sidecar — violates the single-process posture.
- C. Redis/NATS for the turn map — scope creep; `ActiveTurns` is
  deliberately process-local.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — all in-process; no new deployable/service/env vars. Conductor-decided (2026-10-27); ratify at PR review.
## Q2. New test surfaces CI must cover?

- A. The runtime unit tests ride the crate's existing nextest shard
  (`oxidean-api-oxpilot`); the SSE route smokes extend the shipped
  `crates/oxidean-api` test binary; `turn_cancel` joins the closed
  PROCEDURES pin. No new jobs, no new harness.
- B. A dedicated agent-runtime CI job — the seams don't need their
  own lane; over-sharding is cost without signal.
- C. Live-provider e2e in CI — non-deterministic, credential-bearing;
  provider fakes + wiremock cover the loop contract.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — runtime tests ride existing nextest shards + SSE smokes; turn_cancel joins PROCEDURES pin; no new jobs. Conductor-decided (2026-10-27); ratify at PR review.
## Q3. Deployment/restart behavior to record?

- A. Single-replica caveat already documented (NFR11.x); the boot
  sweep reaps `streaming` rows on restart; in-memory turn map is
  empty at boot by construction — infra spec records restart-time
  invariants, no readiness gate.
- B. A readiness probe tied to turn count — single-replica posture
  makes it theatre.
- C. Graceful-drain endpoint for active turns — scope creep; the
  C8 abort path is the graceful mechanism.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — single-replica caveat + boot-sweep restart invariants recorded; no readiness gate. Conductor-decided (2026-10-27); ratify at PR review.
## Q4. Monitoring additions?

- A. Log-based observability only (NFR6): the BR7.1 table + warn
  degraded arms are the operator surface; CI smoke asserts the log
  shape once. No new metrics, no dashboards.
- B. Prometheus counters per turn phase — no registry exists in repo.
- C. Alerting rules — out of scope for the unit; ops concern.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — BR7.1 log table is the operator surface; no metrics registry. Conductor-decided (2026-10-27); ratify at PR review.
## Consolidated Summary Confirmation

- Infrastructure: none added — in-process runtime only (Q1).
- CI: existing shards absorb the unit tests; PROCEDURES pin extends;
  no new jobs (Q2).
- Deployment: single-replica caveat + boot-sweep restart story (Q3).
- Monitoring: BR7.1 log table only; no metrics registry (Q4).

Does this all look correct before I write the infrastructure
artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
