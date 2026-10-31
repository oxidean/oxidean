# Personas — Side-by-Side Agentic VCS Protocol

Priority ranking: **P1 Agent** (primary — the feature exists for this persona), **P2 Developer-Reviewer** (human trust loop — consequential changes stay human-gated per the article's authz guidance), **P3 Forge Operator** (operability and lifecycle), **P4 CI Integrator** (secondary machine consumer).

## P1 — Ava, the Coding Agent

- **Role**: autonomous/assisted coding agent (LLM-driven) performing repository work at machine pace via `vcs.*` RPC procedures and `vcs_*` MCP tools.
- **Goals**:
  - Commit work cheaply and immediately: create changesets server-side without reading refs, holding locks, or materializing a checkout.
  - Read precisely what's needed: files, diffs, blame, path history, merge base — via API, never via full clone.
  - Fetch objects want-based: request only missing content-addressed blobs; resume partial transfers safely.
  - Retry safely after interruptions: identical resubmits return the same changeset id.
  - Reconcile concurrent work: merge/rebase server-side with machine-readable conflicts.
- **Pain points today**: every commit forces a local clone + packfile machinery; ref contention serializes concurrent commits; history queries (blame, path history) need the full repository; a failed push wastes the whole object round-trip.
- **Context**: always online; authenticates with PAT/OAT (never cookies); runs at write volumes no human produces — idempotency, typed errors, and per-object resumability are core, not edge cases.

## P2 — Ravi, the Developer-Reviewer

- **Role**: human maintainer who reviews, qualifies, and lands agent-produced work; the human check on consequential machine changes.
- **Goals**:
  - Materialize an agent's changeset locally with `ox vcs checkout` — only the paths needed (sparse), sharing a bounded local object cache.
  - Inspect diffs, blame, and history of agent work.
  - Export a changeset chain onto a Git branch so the existing protected review flow (PRs, branch protection, required checks) applies unchanged.
  - Keep using Git remotes/PRs/releases on the same repo — the VCS surface must not break or alter them.
- **Pain points today**: agent commits on a parallel system are invisible to Git tooling; without an export path, human review of agent work means reading raw diffs outside the familiar PR surface.
- **Context**: uses the `ox` CLI; treats VCS as plumbing, not a second mental model; trusts the export path because it obeys the same protection verdicts as `git push`.

## P3 — Sana, the Forge Operator

- **Role**: self-host admin / repo owner who enables, configures, and operates the VCS surface.
- **Goals**:
  - Enable/disable VCS per repo with immediate effect; verify state via `vcs.status`.
  - Bound resource usage: object size limits, repo quotas, predictable storage layout, GC for unreferenced objects.
  - See agent write activity in the repo's activity feed at human-legible granularity — not a flood of per-object rows.
  - Trust lifecycle: repo delete/transfer/archive does the right thing with VCS rows and shared objects.
  - Run Git import/export explicitly, converging on stable changeset ids.
- **Pain points today**: a new subsystem that forks ops tooling, has unbounded storage growth, or silently orphans data on repo deletion is unshippable.
- **Context**: admin/owner scope; configures via `OXIDEAN_VCS_*` env vars documented like every other setting.

## P4 — Kit, the CI Integrator

- **Role**: pipeline and tooling automation consuming the protocol headlessly (generated `@oxidean/api-client`, or `ox api vcs.*` raw-RPC escape hatch).
- **Goals**:
  - Stable, versioned procedure surface (`Oxidean-RPC-Version` gate) with deterministic `vcs.*` error codes.
  - Headless auth via PAT with explicit procedure classes — a narrowly-scoped token gets exactly read or write, never ambient session power.
  - Resumable per-object transfers; machine-readable conflict/failure payloads.
  - Everything reachable that a human/agent does via tools — no UI-only capabilities.
- **Pain points today**: protocols that hide operations behind UI flows, unstable error strings, or cookie-based auth break automation.
- **Context**: PAT Basic/Bearer on `/api/vcs/*` object routes; Bearer on `/api/rpc`; fails closed (`db.not_configured`, `SessionOnly`) rather than half-working.
