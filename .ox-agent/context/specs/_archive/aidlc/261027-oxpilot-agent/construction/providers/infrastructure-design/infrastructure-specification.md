# Infrastructure Specification — providers (U4)

Consumes: `nfr-design/` (all), `functional-design/functional-spec.md`,
`inception/domain-design/components.md`,
`inception/contract-design/contract-summary.md` (C4), `codekb/oxidean-app/`.
Questions: `infrastructure-design-questions.md` (conductor-decided).

U4 provisions **zero new services** — provider adapters, the settings
procs, and the runtime slot all land inside the existing `api` binary;
one new singleton table lands on the existing databases. The real infra
delta is **outbound egress to operator-configured provider endpoints**
and **two secret env vars**. This document records the placement.

## Deployment

| Facet | Choice | Rationale |
|-------|--------|-----------|
| Compute model | Existing `api` container only — `oxidean-api-oxpilot` gains `providers/` + `settings.rs`; `oxidean-api-core` gains `OxpilotRuntime`/`OxpilotRuntimeSlot` types; `oxidean-db` gains `oxpilot_settings` store | Q1; one-way dep api-oxpilot → api-core keeps the slot types with `RpcCtx`/`AppState` |
| Networking topology | Unchanged ingress (Traefik/edge → api); NEW **egress path**: `api` → operator-configured `base_url` over http/https. No new listener, port, DNS, or ingress rule | Q2; SSRF validator bounds destinations (public-IP-only for non-Ollama, redirect-free clients); Ollama private targets are the documented exception (NFR1.10) |
| Storage strategy | `oxidean-db` only: `oxpilot_settings` singleton table (id=1, all-nullable sparse overrides + `key_ciphertext`+`key_last4` pair) on sqlite/postgres/mysql via existing sqlx stack | NFR3.9; no new store — the row rides the same DB the app already migrates |
| Environments | dev / staging / ephemeral PR envs / production — unchanged topology. `oxpilot.enabled` defaults off; provider-less configs are valid disabled states so a deploy never bricks | Q6; flag-off rollout risk design |
| IaC approach | `.railway/railway.ts` gains **`preserve()`-marked declarations** for all five `OXIDEAN_OXPILOT_*` vars plus `OXIDEAN_SESSION_SECRET` (the fallback root of the `SECRETS_KEY` chain — an undeclared fallback is the same stranding hole) on the api service — secrets MUST survive declarative `railway config apply` runs; a pruned key source would strand every stored `key_ciphertext` undecryptable. `preserve()` + absence-valid defaults keep both properties | Q3; BR5.6 tri-pairing (env + `.env.example` + `docs/CONFIGURATION.md` in the same change) |
| Resource sizing | No re-sizing: adapters add one pooled `reqwest::Client` per built provider; rebuilds are admin-rare; the settings singleton is a fixed-size row | NFR3.4/3.6 — existing pod envelope absorbs it |
| Boot ordering | `main.rs` calls `settings::rebuild(&state)` ONCE, unconditionally after `AppState::new` (NOT inside the `AUTO_MIGRATE` branch) — env-only slot exists from construction; the row upgrades it. Absent table ⇒ row-read failure ⇒ env-only degrade + `warn!`. Bound: the rebuild's `lookup_host` runs before `axum::serve` listens and is not covered by the 10s connect timeout — code-generation must bound or defer it so a hung resolver can't stall listen | reliability-design NFR4.11–4.12; a no-migrate deploy can never strand an env-configured provider at `misconfigured` |
| Egress dependencies | Optional reachability only — no adapter call happens at build; a dead provider produces `misconfigured`/`no_provider` slots, never a failed boot | NFR4.12; provider-less configs are valid disabled states |

## Infrastructure Services

| Service | Role | Configuration | Notes |
|---------|------|---------------|-------|
| `api` (existing Railway service) | Hosts adapters, settings procs, runtime slot, egress clients | unchanged image path; `reqwest`+`async-trait` deps added to api-oxpilot; `wiremock` dev-dep only | single-replica posture unchanged (NFR3.15 multi-replica convergence bound documented) |
| `oxidean-db` (managed Postgres / sqlite / mysql) | `oxpilot_settings` singleton row | one table, one row, all-nullable sparse columns; key pair written atomically in one statement | `dialect_oxpilot.rs` extended in the same change — dialect pairing rule |
| LLM provider endpoints (operator's) | `base_url` destinations for openai/anthropic/ollama adapters | HTTPS out; connect 10s / total 300s / ≤1 connect-phase retry / no redirects | operator-configured; SSRF-validated pre-commit |
| Queue / cache / secrets vault / search | none | — | `OXIDEAN_OXPILOT_SECRETS_KEY` env chain replaces any vault dependency (Q3) |

## Shared Infrastructure

| Shared Resource | Owner Unit | Consumer Units | Access Boundary |
|-----------------|------------|----------------|-----------------|
| `OxpilotRuntime` (`RpcCtx.oxpilot`, `AppState.oxpilot`) | U4 (owns slot shape + rebuild) | U1 surfaces (slot reads via `resolve_oxpilot`), U2 (budget checks read resolved ceilings), U3 (agent runtime captures provider Arc per turn), U6 (connector actions may inherit the slot) | in-process `RwLock` read; swaps only inside `rebuild_lock` critical section |
| `oxpilot_settings` table | U4 (migrations + store + procs) | U2 (budget fields consumed by counters), settings UI (later unit reads via `settings_get` only) | singleton row; all writes inside the rebuild mutex — no direct table writes from consumers |
| `LlmProvider` trait + `ChatRequest.tools`/`ToolSpec`/`Done{usage}` (C4 delta) | U4 (declares in api_core.oxpilot) | U3 (agent loop consumes ToolCall/Done), U5 (tool specs become ChatRequest.tools) | trait objects shared by `Arc` |
| `audit_oxpilot_settings` helper | U4 | none (proc-internal) | in-crate fn; fire-and-log after swap |
| `egress::validate_base_url` | U4 (authors) | U6 (connector remote targets reuse the same SSRF bound) | crate-public fn inside api-oxpilot; U6 wraps or relocates then |
