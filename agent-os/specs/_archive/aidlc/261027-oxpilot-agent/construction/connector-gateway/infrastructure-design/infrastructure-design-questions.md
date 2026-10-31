# Infrastructure Design Questions — connector-gateway

Unit: `connector-gateway` (U6, kind `service` — greenfield) —
`ConnectorGateway` service, 4 connector tables, C3 HTTP routes under
`/api/oxpilot/connector/*`, `oxpilot.target_*`/`grant_*` procs,
`ConnectorDeleteHook`. Infra concerns: what the pipeline must know to
keep this unit's seams green.

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U6.
- [design] `construction/connector-gateway/{functional-design,
  nfr-requirements, nfr-design}/*` — the seams CI must cover.
- [contract] `contract-summary.md` (C3) — connector HTTP surface.
- [team] `team.md` — Testing Posture (`dialect_*` + `db-dialects` -E
  filter + TESTING.md same-change rule), Deployment, Code Style.

## Q1. Does connector-gateway add deployable infrastructure?

- A. None — same single API process: new HTTP routes under the
  existing `/api/` prefix (no edge/proxy edits), new procs on the
  existing `oxidean-api-oxpilot` crate, a set-once field on
  `OxpilotRuntime`, 4 new DB tables via the ×3-migration discipline.
  No new service, sidecar, queue, cache, or cron — long-poll is
  in-process `tokio::sync::Notify`; single-replica boundary is
  documented, not worked around.
- B. A connector-broker deployable between API and connectors —
  violates the outbound-only posture AND the single-process model;
  the gateway IS the API.
- C. A sweeper/worker for lazy expiry — BR3.1/BR1.x pin lazy-at-
  observation; a sweeper is dead code on this deployment.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — no new deployable; tables+routes+procs inside the existing single API process and /api/ prefix; lazy expiry, no sweeper. Conductor-decided (2026-10-28); ratify at PR review.

## Q2. New test surfaces CI must cover?

- A. Connector tables extend the shipped `dialect_oxpilot.rs` test
  with parity legs — the `db-dialects` lane's `-E` filterset is a
  binary-id inclusion union and `binary_id(oxidean-db::dialect_oxpilot)`
  is already a term, so tests placed inside that file run
  automatically (a NEW binary would never match the filterset and
  would vacuously skip — tests must NOT go in a new file). The
  `docs/TESTING.md` job-table row updates in the same change (same
  job — no ruleset edit). Connector HTTP routes get Rust
  integration tests in the
  crate's `tests/` (verifier auth, forbidden/not_found split,
  claim/report/heartbeat flows — wiremock not needed; the surface
  is inbound). `oxpilot.target_*`/`grant_*` procs extend the
  closed-set `procedures_list_matches_dispatch`-family pin tests.
- B. A dedicated `dialect_connector.rs` file — the shipped
  `dialect_oxpilot.rs` is the declared home (nfr-design); a second
  file duplicates setup.
- C. E2E-stack coverage for the C3 routes — the unit's own stack
  story is U8's; U6's contract-level integration tests suffice.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — connector dialect tests land inside the shipped dialect_oxpilot.rs (filterset is binary_id-inclusion, already covers it); TESTING.md row in the same change; crate tests/ for C3 routes; proc pin tests. Conductor-decided (2026-10-28); ratify at PR review.

## Q3. Migration discipline for the 4 connector tables?

- A. One `00NN_oxpilot_connector.sql` written ×3 in
  `migrations/{postgres,mysql,sqlite}` — `oxpilot_pairings`,
  `oxpilot_targets`, `oxpilot_grants`, `oxpilot_actions` per
  entities.md; `UNIQUE(code_hash)`;
  `UNIQUE … WHERE verifier_hash IS NOT NULL` tri-dialect (partial
  unique — SQLite/Postgres support filtered indexes; MySQL's
  nullable-unique semantics differ and gets the documented dialect
  note — the dialect test asserts the real behavior per engine);
  CASCADE FK on session-owned grant/action rows (AC1.5.2); indexes
  per scalability-design's plan; filename-parity test picks it up
  automatically.
- B. Per-dialect hand tuning beyond the documented difference —
  the parity tests exist to catch drift; keep them identical.
- C. Split migrations per table — one feature-one-file matches the
  existing `00NN_oxpilot.sql` convention.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — one ×3 migration set; partial-unique verifier_hash with tri-dialect-verified semantics; CASCADE on session-owned rows. Conductor-decided (2026-10-28); ratify at PR review.

## Q4. CI pipeline changes required?

- A. One change: the matching `docs/TESTING.md` job-table row only —
  the `db-dialects` filterset already includes
  `binary_id(oxidean-db::dialect_oxpilot)` so no CI-file edit is
  needed at all. Same job, no ruleset edit (matrix/job names
  unchanged). No new jobs, no new matrix values, no new images. The
  unit's Rust tests ride existing `api-rust` shards via the shared
  nextest archive.
- B. A dedicated connector job — the archive already feeds the
  shards; a new job is ruleset work for zero coverage gain.
- C. Secrets/env additions to CI — connector tests are self-
  contained; no live connector, no new credentials.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — TESTING.md job-table row only; the binary_id filterset already covers dialect_oxpilot; no new jobs/matrix/ruleset edits, no ci.yml edit. Conductor-decided (2026-10-28); ratify at PR review.

## Q5. New environment variables / configuration surface?

- A. None new — the enablement gate rides the existing
  `OXIDEAN_OXPILOT_*` set the earlier units shipped; connector
  secrets are per-pairing rows, never env. `docs/CONFIGURATION.md`
  gains the connector-egress paragraph (outbound-only, what C3
  carries) — the mandated same-PR doc update for the surface.
- B. `OXIDEAN_OXPILOT_CONNECTOR_*` vars for tuning (claim clamp,
  liveness window) — the constants are fd-pinned; env-tuning a
  security bound invites drift.
- C. A secrets key var — verifiers/pairing codes are hash-at-rest
  verifier-class, not retrievable secrets; `crypto.rs` is unused
  here.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — no new env vars; CONFIGURATION.md gains the connector-egress paragraph. Conductor-decided (2026-10-28); ratify at PR review.

## Q6. Monitoring/metrics additions?

- A. Reuse shipped surfaces only: `oxpilot_connector` tracing
  (ids/counts/reasons), existing route latency/4xx/5xx metrics
  where the repo carries them, `audit_events` for the BR7.2 set,
  `FailedAuthLimiter` trip counts via existing observability. No
  new metrics layer, no connector-specific counters, no alerting
  rules — NFR6's reuse-what-ships rule.
- B. Prometheus counters per grant/action transition — the audit
  table already records transitions durably; a parallel counter
  layer violates reuse.
- C. Dashboards/alerts for connector health — the deployment
  model is single-replica self-host; alerting infra is outside
  this unit.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — reuse tracing/metrics/audit/limiter surfaces; no new observability stack. Conductor-decided (2026-10-28); ratify at PR review.

## Q7. Edge/proxy/router changes for the C3 routes?

- A. None — `/api/oxpilot/connector/*` sits under the existing
  `/api/` prefix the four hand-synced path sets (API_PREFIXES,
  Traefik labels, Caddyfile matchers, astro dev proxy) already
  cover; mounting happens in the crate's route registration,
  consistent with U1's `/api/oxpilot/*` mounting.
- B. A separate `/connector/` top-level mount — costs the
  three-layer edge-parity edit for zero benefit (the recorded
  project learning).
- C. Long-poll needs proxy read-timeout overrides — the 60 s
  claim clamp sits under every default; document only.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — no edge changes; /api/ prefix covers C3; 60s clamp under proxy defaults. Conductor-decided (2026-10-28); ratify at PR review.

## Consolidated Summary Confirmation

- No new deployable infra — tables+routes+procs in-process (Q1);
  connector dialect tests land in the shipped dialect_oxpilot.rs
  binary (binary_id filterset already covers it) + TESTING.md row,
  crate tests cover C3/procs (Q2); ×3 migration w/ tri-dialect
  partial-unique verifier_hash + CASCADE (Q3); CI = TESTING.md row
  only — no ci.yml/ruleset edits (Q4); no new env vars,
  CONFIGURATION.md egress paragraph (Q5); reuse-only monitoring
  (Q6); no edge changes (Q7).

Does this all look correct before I write the infrastructure-design
artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
