# CI/CD Pipeline Design — session-core (U2)

Consumes: `nfr-design/` (all), `team.md` (Way of Working, Testing
Posture), `inception/contract-design/contract-summary.md` (C1, C5).
Questions: Q4, Q6 (conductor-decided).

U2 changes **no CI jobs and no deployment topology** — the pipeline
contract is reused end-to-end. This document records where U2 touches
the existing pipeline and the same-change registration set it must
land to stay green.

## Build & test stages touched

| surface | what U2 adds | gate effect |
|---------|--------------|-------------|
| `job: rust-build` (nextest archive) | no new crate deps — `OxpilotSessionService` composes shipped `oxidean_db` + `oxidean_core::redact`; traits land in api-core | unchanged supply-chain |
| `job: api-rust` shards | service unit tests (dedupe arms, redact boundary, drain ordering, hook abort ordering via `cfg(test)` `ProbeHook`, budget breach order), proc integration tests (five procs' auth/gate/error map, `{queued_id,position}` shapes, transcript-hit null/null), the closed `OXPILOT_PROCEDURES`-set test extended to the five new names (NFR10.8) | existing sharding absorbs them |
| `job: db-dialects` | the **already-registered** `dialect_oxpilot` binary gains assertions for the four 0087 tables in each dialect (insert/fk-cascade/unique/drain-tx shapes) — the `-E` filter is binary_id-keyed and already contains `dialect_oxpilot` (ci.yml:886), so NO filter change exists; only the `docs/TESTING.md` row's descriptive text updates in the same change (NFR10.2, NFR10.9, dialect-pairing rule) | same job — no ruleset change |
| `make rpc-gen` / `job: rpc-sync` | generated client output for the five new procs must be committed — Rust is the source of truth | existing job; fails on uncommitted drift |
| coverage manifests | new proc names + the service module ride existing collect-only coverage | unchanged mechanism |

## Deployment strategy

Unchanged rolling path: PR → `pr-<n>` image set → ephemeral PR
environment → squash-merge → `sha-<sha>` tag → Railway staging →
manual `production-deploy.yml` promote. U2 ships inside the existing
`api` image — `images.yml` untouched.

**Migration ordering**: `0087_oxpilot_session_core` is additive-only
(four NEW tables; no alteration/backfill). A new binary against a
pre-migration database fails service calls `oxpilot.internal` (store
error), never boot — and `install_sessions` takes no DB dependency, so
the invariant is the accessor's `oxpilot.internal`, not a startup
crash. `AUTO_MIGRATE` ordering is the same contract every migration
already rides.

## Rollback procedures

Layered levers, cheapest first:
1. `admin.oxpilot.settings_set{enabled_override:false}` or env flag —
   `oxpilot.disabled` gates all five procs without a deploy.
2. Existing `deploymentRollback` image repin — additive-only migration
   means the rolled-back binary simply never reads the four new
   tables; `ON DELETE CASCADE` FKs point session→children (not
   children→session), so no write path the old binary uses can orphan.
   **Caveat**: `deploymentRollback` may also restore that deployment's
   custom variables (`docs/DEPLOYMENT.md`) — a rollback can revert
   dashboard-set `OXIDEAN_OXPILOT_ENABLED`/budget envs to an older
   snapshot while the DB `enabled_override` row persists; the operator
   re-verifies the env set after any image rollback.
3. Residual `oxpilot_queued_messages` rows are self-healing once a U2
   binary runs again — the next turn-start `drain_queue_tx` delivers
   them (NFR4.3); under a rolled-back pre-U2 binary the rows simply
   sit unread (the old binary doesn't know the table). No manual queue
   drain runbook is needed.
