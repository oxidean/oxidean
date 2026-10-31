# Initiative Brief — Per-Component Semver & Release Automation

## Intent

Oxidean's version numbers carry no reliable meaning; releases/changelogs are manual; CLI/MCP/API surfaces lack a trustworthy version to compare. This initiative establishes one SemVer practice with per-component version numbers — every Cargo crate and Bun app/package starts at `0.1.0` and bumps only when changes hit it (shared-dependency changes propagate to dependents) — wired to automated changelogs, tag-driven releases, commit-prefix enforcement, and consistent version reporting.

## Customer

All consumer classes: self-hosted admins (upgrade decisions), contributors/maintainers (coordination), downstream tooling consumers (CLI, MCP, API). Release comms are two-tier: per-component changelogs plus one global consolidated changelog.

## Feasibility & Risk Highlights

- Existing tag-triggered release pipeline (`.github/workflows/ox-cli.yml`) is preserved and extended — no rebuild.
- Highest-complexity item: dependency-aware change detection spanning the Cargo and Bun graphs — first-class requirement, sequenced dependency-first.
- Client hook is bypassable (`--no-verify`) — paired CI commit-message lint is in scope as the authoritative gate.
- Convention adoption risk is covered by agent skill/helper + contributor doc updates (Must, not deferrable).

## Scope Boundary

**In:** per-component versioning (`0.1.0` baseline), commit-prefix convention + hook + CI lint, bump automation, changelogs (per-component + global), release wiring, API/RPC wire-contract version, version-surface consistency, agent/contributor docs — plus PR-title linting, automated release PRs, prerelease channels, signed/annotated tags.

**Out:** registry publishing (later change), pre-`0.1.0` changelog backfill, new versioned artifact types beyond existing packaging.

## Constraints

ASAP but quality-gated; CI/Docker changes verified locally with docker + act; trunk-based development on `main`; maintainer holds version-policy authority.

## Team

Solo maintainer decides; agents and contributors follow the documented practice.

## Recommendation

**GO.** Intent, scope, risks, and effort are all confirmed by the deciding stakeholder; no skipped stage needs pulling back. Proceed to Inception (reverse-engineering of version surfaces).
