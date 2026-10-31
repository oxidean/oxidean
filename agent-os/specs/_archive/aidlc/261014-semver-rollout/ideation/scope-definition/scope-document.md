# Scope Document — Per-Component Semver & Release Automation

Upstream: `aidlc/spaces/default/intents/261014-semver-rollout/ideation/intent-capture/intent-statement.md`
Answers: `aidlc/spaces/default/intents/261014-semver-rollout/ideation/scope-definition/scope-definition-questions.md` (all confirmed)

## In Scope

- **Per-component versioning.** Every workspace member — each Cargo crate under `crates/*` and each Bun app/package under `apps/*` and `packages/*` — carries its own SemVer number, all starting at `0.1.0`. A component bumps only when changes hit it; a change to a shared crate/package counts as a change to every component that depends on it.
- **SemVer commit-prefix convention** and a git commit-msg hook that validates it locally.
- **CI-side enforcement** of the same convention, so `--no-verify` bypass is caught.
- **Automated version-bump determination** driven by commit-prefix detection and the dependency-aware change detection above.
- **Changelogs:** one per application/crate/package plus one global consolidated changelog that references the inner ones.
- **Release wiring** into the existing tag-triggered release pipeline (`.github/workflows/ox-cli.yml`), preserving the current flow.
- **API/RPC wire-contract version** as its own versioned surface alongside component versions.
- **Version-surface consistency:** CLI self-update, MCP, API, install manifests, and other consumer-facing places report the correct component version.
- **Agent skills/helpers and contributor documentation** so future agents and contributors follow the commit-prefix and versioning practice.
- **Nice-to-haves (confirmed in):** PR-title linting for the prefix convention, automated release PRs / bot-opened bump branches, prerelease channels (alpha/beta/nightly), and signed or annotated release tags.

## Out of Scope

- Publishing versioned artifacts to public registries (crates.io, npm) — explicitly deferred to a later change.
- Backfilling changelogs for history before `0.1.0` — changelog history starts going forward.
- New versioned container images or distro package types beyond the existing release packaging.
- Changes to the release pipeline beyond wiring versioning/changelog into the existing tag flow.

## Constraints

- **Deadline posture:** as soon as possible, quality-gated — no fixed date, but solid quality and tests are required, not optional.
- **Verification:** CI/Docker-related changes are exercised locally with docker + act before shipping; baseline suite runs before code changes (brownfield safeguard) and the relevant suite re-runs after.
- **Policy authority:** the primary maintainer holds the final call on version policy; contributors are affected stakeholders.
- **Trunk-based development** on `main` per org practice; short-lived branches, squash merges.

## Value Stream (dependency-first order)

1. Commit-prefix convention spec → 2. change detection (incl. dependency propagation) → 3. bump determination → 4. changelog generation (per-component + global) → 5. release/tag wiring → 6. hooks/CI enforcement + agent docs. Version baselines and surface consistency run alongside.

## Assumptions & Open Questions

- The concrete component list is enumerated in reverse-engineering (2.1); the intent-level assumption about the inventory is now resolved at the *policy* level: every workspace member versions independently.
- The unnamed future issue remains unnamed; the ASAP posture stands regardless.
