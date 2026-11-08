# Infrastructure Specification — session-core (U2)

Consumes: `nfr-design/` (all), `functional-design/{entities,functional-spec}.md`,
`inception/domain-design/components.md`,
`inception/contract-design/contract-summary.md` (C5, C8),
`infrastructure-design-questions.md` (conductor-decided).

U2 provisions **zero new services** — the session trait/impl, the five
procs, and migration `0087_oxpilot_session_core` all land inside the
existing `api` binary and the existing databases. The infra delta is
**four new tables ×3 dialects** and **one new `OnceLock` install at
boot**. This document records the placement.

## Deployment

| Facet | Choice | Rationale |
|-------|--------|-----------|
| Compute model | Existing `api` container only — `oxidean-api-core::oxpilot` gains the `SessionService`/`SessionDeleteHook` traits + DTO set + `OxpilotRuntime.sessions`/`install_sessions`/`sessions()`; `oxidean-api-oxpilot` gains `services::OxpilotSessionService` + five proc handlers; `oxidean-db` gains the 0087 store fns | Q1; one-way dep api-oxpilot → api-core keeps trait ports downstream of impl dependents |
| Networking topology | Unchanged — no new listener, port, DNS, ingress, or egress rule; every service call is in-process and the only network hop is the existing DB pool | Q2 |
| Storage strategy | `oxidean-db` only: `oxpilot_tool_calls`, `oxpilot_queued_messages`, `oxpilot_proposals`, `oxpilot_usage_counters` on postgres/mysql/sqlite via `0087_oxpilot_session_core` — all four FK-children of `oxpilot_sessions` (`ON DELETE CASCADE`) except `oxpilot_usage_counters` (scope-keyed, survives session deletes by design, NFR4.6) | NFR10.1; cascade delete is the pinned delete mechanism (NFR4.5) |
| Environments | dev / staging / ephemeral PR envs / production — unchanged topology; additive-only migration means a pre-migration deploy of the new binary can only fail `oxpilot.internal` on the service call, never boot | Q6; additive migration keeps rollback clean |
| IaC approach | None — no env vars, secrets, or service declarations added; `railway.ts`, `.env.example`, `docs/CONFIGURATION.md` untouched by U2 | Q3 |
| Resource sizing | No re-sizing: tables are row-per-artifact (`queued_messages` bounded by user behavior, `usage_counters` = users+1 rows/day, `tool_calls`/`proposals` per turn output); the service adds one indexed read (`find_owned_session`) per session-scoped op | NFR3.x budgets; existing pod envelope absorbs it |
| Boot ordering | `main.rs` calls `oxpilot.install_sessions(Arc::new(OxpilotSessionService::new(db, vec![])))` unconditionally after `AppState::new`, beside U4's `settings::rebuild` — before `axum::serve` listens; order vs the rebuild is irrelevant (the service never reads the slot; caps arrive per-call as `BudgetCaps`). Uninstalled ⇒ `sessions()` answers `oxpilot.internal` | Q7; boot invariant, never lazy |
| External dependencies | None — in-process service + existing DB pool | Q2 |

## Infrastructure Services

| Service | Role | Configuration | Notes |
|---------|------|---------------|-------|
| `api` (existing Railway service) | Hosts the trait port (api-core), `OxpilotSessionService`, five proc handlers, `install_sessions` at boot | unchanged image path; no new deps beyond what U4 added | single-replica posture unchanged; `check→act` budget window is the documented single-replica caveat (NFR4.10/NFR11.3) |
| `oxidean-db` (managed Postgres / sqlite / mysql) | four new tables via `0087_oxpilot_session_core` | additive-only DDL — new tables, uniques, indexes; no existing-table alteration or backfill | `dialect_oxpilot` binary extends to the 0087 tables in the same change (NFR10.9) |
| Queue / cache / secrets vault / search | none | — | `oxpilot_queued_messages` is a DB table, not a broker (NFR10.6) |

## Shared Infrastructure

| Shared Resource | Owner Unit | Consumer Units | Access Boundary |
|-----------------|------------|----------------|-----------------|
| `trait SessionService` (api-core) + `OxpilotSessionService` impl (api-oxpilot) | U2 | U1 surfaces (proc dedupe via `ctx.oxpilot.sessions()?`), U3 (turn lifecycle reads/writes through the port — `append_message`, `set_message_*`, `append_tool_call`, `drain_queue`), U5 (proposal store), U6 (`RevokeGrantsHook` registration) | `Arc<dyn SessionService>` behind the runtime's `OnceLock`; trait object shared, never constructed outside the shell |
| `trait SessionDeleteHook` (api-core) | U2 (port) | U3 `AbortTurnsHook` (index 0), U6 `RevokeGrantsHook` (index 1) — registration order = run order | hooks vec owned by `OxpilotSessionService` at construction |
| `oxpilot_*` tables (0087) | U2 | U3 (tool_calls read for snapshot merge + writes during turns; queued drain at turn start), U5 (proposals), none for usage_counters beyond the service | service-only access — no unit reads these tables directly (BR7.1) |
| `OxpilotRuntimeSlot.budget_caps` | U4 (slot shape) + U2 (field) | U2 procs (`message_queue` budget pre-check, `usage_get` view), U3 (turn-start `check_budget`) | read through the shared slot `RwLock`; populated per-constructor |

## Capacity & limits

| Resource | Bound | Enforcement |
|----------|-------|-------------|
| `oxpilot_usage_counters` rows | `users + 1` per UTC day (two scopes) | `record_usage` upserts `(user, user_id)` + `(instance, '__instance__')` in one tx (NFR4.9) |
| `oxpilot_queued_messages` | unbounded by design — `409 turn_active` on the SSE route + explicit `message_queue` keeps queue writes user-driven | `(session_id, queue_seq)` unique + bounded retry (8) |
| `oxpilot_tool_calls` | one row per tool invocation | `(session_id, call_id)` unique (BR7.3) |
| `oxpilot_proposals` | one row per saved diff | cascade child of session |
