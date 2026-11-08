# Phase 19: Actions & Runners - Context

**Gathered:** 2026-09-16  
**Status:** Ready for planning  
**Discuss mode:** Auto-decided per orchestrator defaults (GitHub Actions–compatible YAML; push + pull_request; run UI + logs; official runner image + open registration protocol; no managed cloud minutes in v1). Branch-protection required checks (Phase 13) noted as integration surface.

<domain>
## Phase Boundary

Repos can run GitHub Actions–compatible workflows on operator-registered runners (official Oxidean runner image + open third-party registration/job-dispatch protocol). Delivers ACT-01…ACT-07.

**Requirements:** ACT-01, ACT-02, ACT-03, ACT-04, ACT-05, ACT-06, ACT-07

**Success criteria (from ROADMAP):**
1. Repo can define workflows in a GitHub Actions–compatible YAML layout; push and pull_request events trigger runs
2. User can view workflow run status and logs in the UI
3. Operator can register and run the official Oxidean runner image; docs cover Compose sidecar or standalone bring-up
4. Forge exposes an Actions-compatible registration/job-dispatch protocol (custom `runs-on` labels); jobs only run on registered runners — no managed Oxidean Cloud minutes in v1

**Depends on:** Phase 12 (Pull Requests), Phase 13 (Branch Protection) — event wiring for `pull_request` and commit-status / required-check consumption.

**Out of scope:**
- Managed / sold Oxidean Cloud runner minutes (explicitly forbidden in v1 — ACT-07)
- GitHub-hosted runner fleets operated by Oxidean
- Triggers beyond push + pull_request (`schedule`, `workflow_dispatch`, `release`, `workflow_call`, etc.)
- Full Actions marketplace / OIDC federation / reusable workflow deep parity
- Artifact download UI, cache backend, and matrix fan-out beyond what the act-compatible runner already executes when present in YAML
- Phase 13 branch-protection **enforcement UI** (owned by Phase 13) — Phase 19 only **publishes** check contexts Phase 13 will require

**UI hint:** yes — repo **Actions** tab; run list + run/job detail with logs; Admin/instance runner registration; repo Settings Actions enable + secrets.

</domain>

<decisions>
## Implementation Decisions

### A — Workflow definition (ACT-01)
- **D-ACT-01:** Workflows live at **`.github/workflows/*.{yml,yaml}`** on the triggering ref (GitHub Actions–compatible path layout) — **Reversibility:** costly — path is a client/docs contract
- **D-ACT-02:** Support a **GitHub Actions–compatible YAML subset** sufficient for `name`, `on` (push / pull_request), `jobs.<id>.runs-on`, `jobs.<id>.steps`, `env`, `defaults`, and common step fields (`uses` / `run` / `name` / `if` / `with` / `env`). Unsupported constructs fail the job with a clear error rather than inventing a parallel DSL — **Reversibility:** costly — parser surface
- **D-ACT-03:** Job execution is delegated to an **act-compatible runner** (nektos/act lineage / Gitea act_runner family). The forge does **not** interpret step scripts in-process — **Reversibility:** costly — runner dependency

### B — Triggers (ACT-02)
- **D-ACT-04:** v1 event triggers are **`push`** and **`pull_request` only** (open, synchronize/reopened, and close-as-needed for cancel semantics) — **Reversibility:** reversible (additive later)
- **D-ACT-05:** Fire workflow evaluation **after successful git receive-pack** (HTTPS + SSH) for push events, and from **Phase 12 PR lifecycle hooks** for pull_request events — **Reversibility:** costly — hook placement
- **D-ACT-06:** Instance gate via **`OXIDEAN_ACTIONS_ENABLED`** (default **true** self-host and Cloud). Per-repo **Admin** toggle to enable/disable Actions for that repo (default **enabled** when instance gate is on) — **Reversibility:** reversible

### C — Runner protocol & labels (ACT-04, ACT-06, ACT-07)
- **D-ACT-07:** Expose a **Gitea Actions–compatible runner registration and job-dispatch protocol** (actions-proto / ConnectRPC-over-HTTP family) on the existing API HTTP port so **act_runner-class** and **Blacksmith-class** third-party providers can integrate against a documented open surface — **Reversibility:** costly — wire protocol contract
- **D-ACT-08:** Registration uses **registration tokens** (instance Admin; optional org/repo scope) plus optional bootstrap env **`OXIDEAN_RUNNER_REGISTRATION_TOKEN`** for ephemeral/Compose runners — **Reversibility:** costly — token model
- **D-ACT-09:** Custom **`runs-on` labels** are first-class; label mapping format **`label[:schema[:args]]`** (Gitea/act_runner parity, e.g. `ubuntu-latest:docker://node:20`) — **Reversibility:** costly — label semantics
- **D-ACT-10:** Workflow jobs are **only assigned to registered runners** whose declared labels match `runs-on`. Oxidean **does not** sell or host managed runner minutes in v1 (Cloud and self-host identical: operator/third-party compute only) — **Reversibility:** one-way for product positioning (ACT-07)
- **D-ACT-11:** Ship an **official Oxidean runner image** (act_runner-based) for Compose sidecar **or** standalone `docker run` / binary register+daemon — **Reversibility:** costly — image name becomes ops contract

### D — Runs UI, logs, storage (ACT-03, ACT-05)
- **D-ACT-12:** Repo chrome gains an **Actions** tab (`active: "actions"`); routes **`/{owner}/{repo}/actions`** (list) and **`/{owner}/{repo}/actions/{run}`** (detail + job logs). Do not remount chrome in leaves (Phase 11.1 D-QH-01) — **Reversibility:** reversible
- **D-ACT-13:** Persist run/job metadata in DB; store log blobs under **`OXIDEAN_ACTIONS_LOG_DIR`** (Compose volume, distinct from repos/LFS/packages/release-assets). UI polls (or streams if cheap) via session RPC — **Reversibility:** costly — volume split
- **D-ACT-14:** Docs cover **Compose sidecar** (`profile: actions` or documented service) **and** standalone registration against `OXIDEAN_PUBLIC_ORIGIN` — **Reversibility:** reversible

### E — Phase 13 integration surface (required checks)
- **D-ACT-15:** Each workflow job publishes a **commit status / check context** named for branch-protection consumption (stable context string: **`{workflow_name} / {job_id}`** or GitHub-equivalent). Phase 13 **requires** these contexts; Phase 19 **owns publishing** them on queued/in_progress/success/failure/cancelled — **Reversibility:** costly — context naming contract with Phase 13
- **D-ACT-16:** Status rows are queryable via forge API/RPC so Phase 13 merge/push gates can evaluate “required checks green” without scraping the Actions UI — **Reversibility:** costly — shared status schema

### F — Secrets & ACL
- **D-ACT-17:** Repo **Actions secrets** (Admin write; values never returned after create; available to jobs as `secrets.*`). Org-level secrets may follow if cheap; instance secrets out of scope — **Reversibility:** reversible
- **D-ACT-18:** ACL: **Read** → view runs/logs for visible repos; **Write** → no special run control beyond push/PR; **Admin** → enable Actions, manage secrets, view registration token for repo-scoped runners. Runner protocol auth is **registration/runner tokens**, never session cookies — **Reversibility:** reversible

### G — Lifecycle / ops
- **D-ACT-19:** Factory reset **wipes** Actions runs, log dir contents, and runner registrations (registration tokens regenerated / env bootstrap remains operator-owned) — **Reversibility:** reversible (policy)
- **D-ACT-20:** Queued jobs with **no matching registered runner** remain **queued** (visible in UI) until a matching runner appears or the run is cancelled — never silently execute on the API host — **Reversibility:** reversible

### Claude's Discretion
- Exact ConnectRPC/prost wiring vs hand-rolled Connect-compatible Axum handlers (must stay act_runner-wire-compatible)
- Exact subset of pull_request activity types beyond open/synchronize/reopened
- Whether cancel-in-progress on new push to same workflow+ref ships in Phase 19
- Log polling interval vs SSE/WebSocket
- Default runner labels shipped in the official image
- Whether org-scoped registration tokens ship alongside instance + repo
- Artifact/cache: only if the chosen act_runner already needs forge endpoints to not fail common workflows; otherwise defer dedicated artifact UI

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Planning / requirements
- `.planning/ROADMAP.md` — Phase 19 goal, ACT-01…07; depends on Phase 12 + 13
- `.planning/REQUIREMENTS.md` — ACT-01…07; ORG-05/06 + PR-08 (Phase 13 consumers of status checks)
- `.planning/PROJECT.md` — Actions: official runner + 3rd-party protocol; no managed minutes
- `.planning/phases/12-pull-requests/12-CONTEXT.md` — PR lifecycle hooks for `pull_request` events
- `.planning/phases/10-orgs-permissions/10-CONTEXT.md` — Capability ACL
- `.planning/phases/11.1-quality-hardening/11.1-CONTEXT.md` — RepoChrome layout ownership
- `.planning/phases/08-git-https-pats/08-CONTEXT.md` — receive-pack auth patterns (push hook placement)
- `.planning/phases/14-git-lfs/14-CONTEXT.md` — separate volume + factory-reset wipe pattern
- `.planning/phases/20-packages-registry/20-CONTEXT.md` — Compose volume / Traefik / Vite routing patterns

### External (pin while researching)
- Gitea Actions design — https://docs.gitea.com/usage/actions/design/
- Gitea act_runner registration — https://docs.gitea.com/runner/registration/
- actions-proto-def (Gitea runner gRPC/Connect definitions)
- GitHub Actions workflow syntax (subset for D-ACT-02)

### Code / ops mirrors
- `crates/oxidean-api/src/routes/git_smart_http.rs` — receive-pack completion hook point
- `crates/oxidean-api/src/ssh/pack.rs` — SSH receive-pack completion
- `apps/web/src/components/repo/repo-chrome.tsrx` + `apps/web/src/lib/repo-chrome-active.ts` — Actions tab
- `docker-compose.yml` — runner sidecar + `OXIDEAN_ACTIONS_LOG_DIR`
- `docs/CONFIGURATION.md` / `docs/API.md` / `docs/DEPLOYMENT.md`

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- Smart HTTP + SSH receive-pack paths for push event emission
- Phase 10 `Capability::{Read,Write,Admin}` for UI/RPC ACL
- Volume + factory-reset wipe patterns from LFS/packages
- RepoChrome tab extension pattern (Issues/Packages)
- Compose Traefik path routing + Vite proxy patterns for new API prefixes (`/api/actions/…`)

### Integration Points
- Phase 12: invoke Actions dispatcher on PR open/sync/reopen/(close)
- Phase 13: read commit status/check contexts published by Actions for required checks
- No in-tree Actions code yet — greenfield domain modules under `crates/oxidean-api/src/actions/` + DB migrations + runner image tree

</code_context>

<specifics>
## Specific Ideas

- User/orchestrator defaults locked: GHA-compatible YAML; push + pull_request; UI + logs; official runner + open protocol; **no** managed cloud minutes
- Prefer **Gitea Actions wire protocol** so existing act_runner / third-party “Blacksmith-class” providers can target Oxidean without a proprietary-only runner API
- Phase 13 required-check **names** must be stable and documented as part of Phase 19

</specifics>

<deferred>
## Deferred Ideas

- Managed Oxidean Cloud runner minutes / hosted fleets
- Additional triggers (`schedule`, `workflow_dispatch`, `release`, …)
- Full marketplace, OIDC cloud role assumption, reusable workflow authoring UX
- Dedicated artifacts/cache product UI (beyond runner-native behavior)
- Windows/macOS official runner images
- CODEOWNERS / required review interaction beyond Phase 13’s own scope

</deferred>

---

*Phase: 19-actions-runners*  
*Context gathered: 2026-09-16 (auto-decide)*
