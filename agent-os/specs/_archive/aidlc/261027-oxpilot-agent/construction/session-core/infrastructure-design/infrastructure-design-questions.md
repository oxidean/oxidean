# Infrastructure Design — Questions (session-core)

Decisions for this stage. Resolved under the standing self-drive instruction; ratify at PR review.

## Q1 — New infrastructure footprint
Does U2 provision any new infrastructure?
[Answer]: A
- A. Zero new services — `SessionService`/`SessionDeleteHook` traits + DTOs land in `oxidean-api-core::oxpilot`, `OxpilotSessionService` + the five new procs land in `oxidean-api-oxpilot`, and migration `0087_oxpilot_session_core` adds four tables (`oxpilot_tool_calls`, `oxpilot_queued_messages`, `oxpilot_proposals`, `oxpilot_usage_counters`) on the existing databases across all three dialects. No new Railway service, worker, queue, cache, listener, or egress path.
- B. A dedicated session/persistence sidecar service.
- C. An external queue/broker for `oxpilot_queued_messages`.

## Q2 — Networking posture
What network change does the session service require?
[Answer]: A
- A. None — every service call is in-process (`ctx.oxpilot.sessions()?` → `OxpilotSessionService` → `oxidean-db` pool). No new listener, port, DNS, ingress or egress rule; the only network hop is the existing DB connection the api service already holds.
- B. A private service-mesh hop for the session service.
- C. A redis/queue hop for drain ordering.

## Q3 — Secrets in deployment
Does U2 add secrets or change the secret chain?
[Answer]: A
- A. None — no new env vars, no keys, no credentials. Session rows carry no secrets (redaction boundary is data-hygiene, not key management). `railway.ts`, `.env.example`, and `docs/CONFIGURATION.md` gain nothing from U2.
- B. A per-session encryption key material env.
- C. A queue-broker credential.

## Q4 — CI/CD change set
What does the pipeline need for U2?
[Answer]: A
- A. Reuse existing jobs — migration `0087` joins the already-registered `dialect_oxpilot` test binary (the `-E` filter is binary_id-keyed and already covers it — ci.yml:886; only the `docs/TESTING.md` row's descriptive text updates in the same change per the dialect-pairing rule); service/proc integration tests ride the `api` test shards; no new job or matrix value ⇒ no `Main` ruleset change.
- B. A dedicated session-service CI job.
- C. New jobs + ruleset update.

## Q5 — Monitoring/alerting
What monitoring does U2 add at platform level?
[Answer]: A
- A. None — existing surfaces cover it: `audit::record` writes `oxpilot.session_delete` audit rows (with `hook_index` detail on failure), the turn span carries session_id/turn_id, warn-on-internal-error on stdout. No metrics exporter, alerts, or dashboards.
- B. Queue-depth metrics + alert rules.
- C. A session-health endpoint.

## Q6 — Deploy/rollback posture
How does U2's change reach environments and roll back?
[Answer]: A
- A. Existing path unchanged: PR env → squash-merge → `sha-<sha>` → staging → manual promote. Migration 0087 is additive-only (four NEW tables — no existing-table alteration, no backfill), so a rollback leaves orphans in no path the code reads. Flag-off (`enabled_override=false` / env) restores `oxpilot.disabled` without a deploy. Crash-window semantics are designed, not incidental: residual `oxpilot_queued_messages` rows are drained by the next turn-start `drain_queue_tx` (NFR4.3).
- B. Canary for the session procs.
- C. Feature-branch deploy track.

## Q7 — Boot ordering for service installation
Where does `install_sessions` run and how does it order against U4's rebuild?
[Answer]: A
- A. `main.rs` calls `oxpilot.install_sessions(Arc::new(OxpilotSessionService::new(db, vec![])))` immediately after `AppState::new`, in the same unconditional block as U4's `settings::rebuild` — the service has no dependency on the resolved provider slot (it is stateless over `Database` + hooks; caps arrive per-call as `BudgetCaps`), so installation order vs the rebuild is irrelevant; what matters is it runs before `axum::serve` listens. Uninstalled ⇒ the `sessions()` accessor answers `oxpilot.internal` (boot invariant).
- B. Install lazily on first proc call — races the first request against installation.
- C. Install inside the U4 rebuild — couples session availability to provider resolution.

## Consolidated Summary Confirmation

Decisions consolidated:
- Q1 A: zero new services — traits/DTOs in api-core, service+procs in api-oxpilot, four 0087 tables on existing DBs.
- Q2 A: no network change — in-process service + existing DB pool.
- Q3 A: no new secrets — railway.ts/.env.example/CONFIGURATION.md untouched by U2.
- Q4 A: reuse jobs — same dialect binary (filter+docs row updated in-change), same api shards, no ruleset change.
- Q5 A: existing audit + tracing surfaces only.
- Q6 A: unchanged deploy path; additive migration; flag-off rollback; residual-drain semantics at next turn start.
- Q7 A: `install_sessions` unconditionally after `AppState::new` in main.rs, before listen; unset ⇒ `oxpilot.internal`.

Does this all look correct before I write the infrastructure artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
