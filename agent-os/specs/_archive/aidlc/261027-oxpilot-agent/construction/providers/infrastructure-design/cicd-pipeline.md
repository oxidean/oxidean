# CI/CD Pipeline Design — providers (U4)

Consumes: `nfr-design/` (all), `team.md` (Way of Working, Testing
Posture), `inception/contract-design/contract-summary.md` (C1).
Questions: Q4, Q6 (conductor-decided).

U4 changes **no CI jobs and no deployment topology** — the pipeline
contract is reused end-to-end. This document records where U4 touches
the existing pipeline and the same-change registration set it must
land to stay green.

## Build & test stages touched

| surface | what U4 adds | gate effect |
|---------|--------------|-------------|
| `job: rust-build` (nextest archive) | `reqwest` + `async-trait` deps on `oxidean-api-oxpilot` (per-crate pins — root `[workspace.dependencies]` doesn't carry them) compile into the shared archive | supply-chain unchanged: committed `Cargo.lock` + digest-pinned `oxidean-ci` images (`cargo --locked` only gates `cargo install` in the CI base image) |
| `job: api-rust` shards | adapter unit tests (decoders, classify tables, custody helpers, SSRF validator) + `settings_set`/`providers_list`/`settings_get` integration tests + the closed-PROCEDURES pinning test | existing sharding absorbs them |
| `job: db-dialects` (run-step `-E` filter) | `oxpilot_settings` migrations join the **already-shipped** `dialect_oxpilot.rs` binary — U1 registered the `binary_id(...)` filter entry (`ci.yml`) and the `docs/TESTING.md` row; U4 only adds the table's dialect assertions to that file | same job — no ruleset change; dialect-pairing rule preserved |
| `make rpc-gen` / `job: rpc-sync` (runs `rpc-sync-check`) | generated client output for `admin.oxpilot.settings_get/settings_set/providers_list` must be committed — same Rust-source-of-truth flow as U1 | existing job; fails on uncommitted drift |
| `wiremock` dev-dep | adapter tests against recorded SSE/NDJSON fixtures — **no live provider calls, no provider keys in CI** (NFR8.1) | hermetic: `fixtures/{openai,anthropic,ollama}/*.sse`, `*.ndjson` + 401/403/429/context-length/5xx response set |
| coverage manifests | new proc names + the `pub(in crate::providers)` decrypt boundary ride the existing collect-only coverage (NFR9) | unchanged mechanism |

## Deployment strategy

Unchanged rolling path: PR → `pr-<n>` image set → ephemeral PR
environment → squash-merge → `sha-<sha>` all-or-nothing tag → Railway
staging → manual `production-deploy.yml` promote. U4 ships inside the
existing `api` image — `images.yml`'s build is untouched.

**Rollout risk design**: `oxpilot.enabled` defaults off AND a
provider-less resolved config is a valid disabled state, so a green
deploy carries the feature dark and a half-configured deployment
degrades to `no_provider`/`misconfigured` — never a boot failure or a
hung proc. The unconditional post-`AppState::new` `rebuild` in
`main.rs` means deployment ordering is robust: no-migrate boot ⇒
env-only degrade + `warn!`, not a crash loop.

## Rollback procedures

Layered levers, cheapest first:
1. `admin.oxpilot.settings_set{enabled_override:false}` — flag-off,
   keeps the provider Arc but gates every user surface to
   `oxpilot.disabled`. Gate-exempt admin procs stay reachable
   (recovery path by design).
2. `admin.oxpilot.settings_set{clear_overrides:true}` — restores pure
   env config without touching secrets mechanics.
3. `admin.oxpilot.settings_set{clear_key:true}` — NULLs
   ciphertext+last4 atomically; env key still resolves.
4. Existing `deploymentRollback` image repin — migrations are
   additive-only (new singleton table); no existing table altered, so a
   rollback never orphans writable state the old binary needs.
   **Caveat**: `deploymentRollback` may also restore that deployment's
   custom variables (`docs/DEPLOYMENT.md`) — after an image rollback
   the operator must re-verify the `OXIDEAN_OXPILOT_*` (and session-
   secret) env set on the api service, since a rollback can revert
   dashboard-set values to an older snapshot.
5. `OXIDEAN_OXPILOT_SECRETS_KEY` rotation concern: rotating the secrets
   key without re-keying strands stored ciphertext — documented as an
   operator caveat (decrypt fails ⇒ `misconfigured`, `clear_key` is
   the recovery lever).

## Environment promotion

dev → staging automatic on merge; staging → production stays the manual
promote. U4 does not alter promotion gates.

## Secrets & config in CI/CD

Two real secrets ship in this unit: `OXIDEAN_OXPILOT_SECRETS_KEY`
(encryption root; `OXIDEAN_SESSION_SECRET` fallback) and
`OXIDEAN_OXPILOT_API_KEY` (provider key). Handling:

- `.railway/railway.ts` declares all five `OXIDEAN_OXPILOT_*` vars with
  `preserve()` — a declarative `railway config apply` must never prune
  a dashboard-set secret (a pruned `SECRETS_KEY` strands every stored
  ciphertext). The fallback link gets the same treatment:
  `OXIDEAN_SESSION_SECRET` is also `preserve()`-marked — an undeclared
  fallback root is the same stranding hole under the documented
  `OXIDEAN_OXPILOT_SECRETS_KEY → OXIDEAN_SESSION_SECRET` chain.
- `.env.example` shows placeholder-only shapes + `docs/CONFIGURATION.md`
  documents custody + rotation caveats (BR5.6 tri-pairing in the same
  change).
- **CI never sees them**: `wiremock` fixtures cover every adapter path;
  tests needing a secrets key use a test-scoped ephemeral key generated
  in-process, never an env var — the flag-defaults-off posture means
  e2e harnesses never mint real provider traffic (NFR8.1).
- `bun --frozen-lockfile` / committed `Cargo.lock` / digest-pinned
  images cover supply-chain; the only new runtime deps are `reqwest` +
  `async-trait` on api-oxpilot, pinned per-crate (not workspace deps).
