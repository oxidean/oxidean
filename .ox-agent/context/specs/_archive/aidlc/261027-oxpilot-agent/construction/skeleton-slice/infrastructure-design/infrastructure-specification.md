# Infrastructure Specification — u1-skeleton-slice

Consumes: `nfr-design/` (all), `functional-design/functional-spec.md`,
`inception/domain-design/components.md`,
`inception/contract-design/contract-summary.md`,
`codekb/oxidean-app/` (deployment topology).
Questions: `infrastructure-design-questions.md` (conductor-decided).

U1 provisions **zero new infrastructure** — every component lands inside
existing services and databases. This document records the placement,
not a provisioning plan.

## Deployment

| Facet | Choice | Rationale |
|-------|--------|-----------|
| Compute model | Existing containers only: `api` service (new `oxidean-api-oxpilot` crate + SSE routes), `web` service (new `/oxpilot` route). No new service, worker, or sidecar | Q1; turn tasks run in-process on the api's tokio runtime (per-process ActiveTurn, single-replica posture NFR3.13) |
| Networking topology | Unchanged: Traefik/edge → api service; the two SSE routes join `/api/` under the same ingress. Streaming is proven end-to-end by the shipped `stream_job_log` precedent: the `text/event-stream` compression exclusion lives on the web tier's dev/e2e proxy path (`oxidean-web`), the api tier has no compression layer at all, and the cloud gateway's global `encode gzip` already carries SSE traffic | existing SSE precedent; no new ports, listeners, or DNS |
| Storage strategy | `oxidean-db` only: `oxpilot_sessions` + `oxpilot_messages` on sqlite/postgres/mysql via existing sqlx stack; composite indexes `(user_id,last_activity_at)` + `(session_id,seq)` | NFR3.14; no new store, no S3/bucket usage in U1 |
| Environments | dev / staging (Railway, `sha-<sha>` tag set) / ephemeral PR envs (`pr-<n>`) / production (manual promote) — all unchanged | existing topology; flag defaults off so the feature is inert everywhere until enabled |
| IaC approach | `.railway/railway.ts` gains **only `preserve()` declarations** for `OXIDEAN_OXPILOT_ENABLED/_PROVIDER/_MODEL` on the api service — `railway config apply` is declarative, so undeclared dashboard-set vars risk being pruned by a later apply, silently reverting enablement to `flag_off`. `preserve()` keeps operator-set values across applies while absence stays valid (flag-off default). No service, image, or resource change | Q1/Q5 + R-08 durability: declare-with-preserve so enablement survives routine applies |
| Resource sizing | No re-sizing: per-turn memory is O(in-flight message) bounded by the 64 KiB body cap; readers bounded by fd limits; per-process turns ≤ live sessions | NFR3.6/3.7/3.18 — existing api pod envelope absorbs it |

## Infrastructure Services

| Service | Role | Configuration | Notes |
|---------|------|---------------|-------|
| `api` (existing Railway service) | Hosts oxpilot RPCs + SSE + ActiveTurn map + turn tasks | unchanged image path; new crate linked into the same binary | single-replica posture is the v1 bound (NFR3.13) |
| `web` (existing Railway service) | Serves `/oxpilot` shell + `.tsrx` bundle | unchanged | route joins SHELLS/RESERVED_TOP_SEGMENTS |
| `oxidean-db` (managed Postgres in cloud; sqlite/mysql self-host) | `oxpilot_sessions`, `oxpilot_messages` | two tables + two indexes + cascade FK; migrations ×3 dialects | `dialect_oxpilot.rs` proves parity |
| LLM provider | none in U1 | `StubProvider` in-process | real adapters (U4) may add egress rules + secret slots — not U1 |
| Queue / cache / search / CDN / LB | none | — | explicitly rejected (Q1) |

## Shared Infrastructure

| Shared Resource | Owner Unit | Consumer Units | Access Boundary |
|-----------------|------------|----------------|-----------------|
| `ActiveTurn` map slot on `RpcCtx` | U1 (owns claim/release) | U2 (abort hook reads/cancels), U4 (turn internals), U6 (connector actions bound to turns) | in-process; single-replica scope — recorded, not shared via network |
| `oxpilot_sessions` / `oxpilot_messages` tables | U1 (migrations + FKs) | U2 (delete/abort procs), U3 (builder metadata column additions own their own tables), U6 (target FK joins) | dialect-pinned schema; later units add columns/tables via new migrations, never edit U1's |
| `EnablementGuard` helper | U1 | all later units' oxpilot surfaces | in-crate fn; `resolve_enablement`/`require_enabled` pair |
| `redact()` util in `oxidean-core` | U1 (authors) | U2+ (connector output, tool output redaction) | crate-public fn |
| `oxpilot_settings` read seam | U1 (probe-read in resolver) | U4 (provider-slot override), settings UI (later) | keyed read per call; absent table ⇒ env fallback |
