# CI/CD Pipeline Design — u1-skeleton-slice

Consumes: `nfr-design/` (all), `team.md` (Way of Working, Testing
Posture, Deployment), `inception/contract-design/contract-summary.md`.
Questions: Q2, Q4, Q5 (conductor-decided).

U1 changes **no CI jobs and no deployment topology** — the pipeline
contract is reused end-to-end. This document records where U1's change
touches the existing pipeline and the same-change registration set it
must land to stay green.

## Build & test stages touched

The "surface" column names the enforcement point exactly — ci.yml job
names are marked `job:`; everything else is the make target or script an
implementer would grep (and the job that runs it).

| surface | what U1 adds | gate effect |
|---------|-------------|-------------|
| `job: rust-build` (nextest archive) | `oxidean-api-oxpilot` joins `Cargo.toml` workspace members → compiled into the shared archive | workspace-member drift fails the build if the crate isn't registered |
| `job: api-rust` shards | crate's colocated unit tests + `tests/` integration (incl. `procedures_list_matches_dispatch` per-crate convention) | existing 5-way sharding absorbs it; archive retry covers flakes |
| `job: db-dialects` (run-step `-E` filter) | `dialect_oxpilot.rs` added to the `binary_id(...)` filter **and** `docs/TESTING.md` job table in the same change | same job — no ruleset change; omitting either makes it dead-on-arrival |
| `job: route-coverage` (`make route-coverage-check`) | manifest rows for **both** `.tsrx` routes: `/oxpilot` (index) and `/oxpilot/$session` (workspace) — `route-coverage.manifest.ts` requires every user-facing route | change-aware mode rejects a skip-only row for added routes |
| `job: browser-coverage` (`make browser-coverage-check` + `ui-coverage-change.contract.sh`) | expected row: the new-session form's repo picker lands a high-risk control (`Select*`/portal/`form.Subscribe` per `browser-coverage.manifest.ts`) | Chromium `*.browser.test.tsx` proof required for high-risk interactive UI |
| `job: coverage-weighted` | nothing new — composite consumes web unit/integration v8 numbers + the frozen 7-path e2e checklist | 0.70 floor unchanged; the checklist is deliberately not grown (team.md) |
| `make web-lint` + `make web-format-check` (run inside `job: web-octane`; `check-route-sync` is `bun scripts/check-route-sync.ts` invoked from `web-lint`, not a standalone job) | new `.tsrx`/`oxpilot-queries.ts` files linted + formatted; `SHELLS` + Astro shell + `RESERVED_TOP_SEGMENTS += "oxpilot"` + regenerated routeTree | type-aware oxlint/oxfmt gate; omitting the segment misbinds `oxpilot` as `/{owner}` |
| `job: e2e-stack` (`scripts/dev-auth/run-stack-e2e.sh` harness) | **harness env exports** `OXIDEAN_OXPILOT_ENABLED=1` + `OXIDEAN_OXPILOT_PROVIDER=stub` so the spawned `oxidean-api` serves the enabled path; the `*.stack.test.ts` probe lands under `apps/web/e2e/stack/` (globbed into the `e2e-stack` vitest project) | the flag defaults OFF server-side (BR2.2) — without the exports every enabled-path probe denies `oxpilot.disabled`; no other mechanism can reach server env |
| `job: smoke-protocol` / `actions-e2e` | untouched (no protocol surface changes) | none |
| `job: commit-lint` (`versioning commit-check` + `versioning check`) | `oxidean-api-oxpilot` joins `versioning.toml` `[[component]]` + crate CHANGELOG **and** the `oxidean-api` component's `depends_on` list (which enumerates all domain crates for bump propagation); commit subjects SemVer-prefixed | without the `depends_on` reverse edge, checks pass silently but oxpilot-only changes stop rolling the api binary's component version |

## Deployment strategy

Unchanged rolling path: PR → `pr-<n>` image set → ephemeral Railway PR
environment → squash-merge → `sha-<sha>` all-or-nothing tag → Railway
staging → manual `production-deploy.yml` promote. The skeleton ships
inside the existing `api` and `web` images — `images.yml`'s four-service
build is untouched.

**Rollout risk design**: `OXIDEAN_OXPILOT_ENABLED` defaults off, so a
green deploy carries the feature dark. Enablement is an operator env
flip (or `oxpilot_settings` row later), not a deploy — no canary, no
blue-green needed for a flag-off surface (Q4).

## Rollback procedures

Existing `deploymentRollback` (repins previous image digests) covers a
bad U1 deploy; the feature-flag-off default is itself the first
rollback lever — disabling returns every surface to `oxpilot.disabled`
without an image change. DB: migrations are additive-only (two new
tables + indexes); the cascade FK contains blast radius to the new
schema objects; no existing table is altered.

## Environment promotion

dev → staging automatic on merge (existing `images.yml` → Railway);
staging → production remains the manual promote with latest-CI-green
ancestor requirement. U1 does not alter promotion gates.

## Secrets & config in CI/CD

No new secrets anywhere: `OXIDEAN_OXPILOT_ENABLED/_PROVIDER/_MODEL` are
config strings documented in `.env.example` + `docs/CONFIGURATION.md`.
CI exercises the enabled path via the `run-stack-e2e.sh` harness env
exports (R-01: the spawned `oxidean-api` needs them — the flag defaults
off and no test file can mutate server env); in-process Rust tests set
their own env. Never repo secrets. The `StubProvider` failure-injection
flag is a constructor option, deliberately not an env var (NFR4.10
decision). `bun --frozen-lockfile` / `cargo --locked` / digest-pinned
`oxidean-ci` images cover supply-chain unchanged — U1 adds zero
dependencies.
