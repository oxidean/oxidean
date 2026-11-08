# Decision Log — Ideation Phase

All decisions made during Ideation for intent `261014-semver-rollout`.

| # | Decision | Source |
|---|----------|--------|
| 1 | "Uniform" means one SemVer **practice** with per-component version numbers, not one shared number | intent-capture Q3 |
| 2 | Every component starts at `0.1.0`; each bumps only when changes hit it | intent-capture Q3 |
| 3 | A change to a shared crate/package counts as a change to its dependents | intent-capture Q8 |
| 4 | Changelog layout is two-tier: per-application/crate changelogs + one global consolidated changelog referencing them | intent-capture Q7 |
| 5 | Release flow stays tag-driven with generated changelogs; commit prefixes drive bump detection | intent-capture Q4 |
| 6 | Maintainer holds the primary call on version policy; others are affected as contributors | intent-capture Q6 |
| 7 | Every workspace member (each Cargo crate, each Bun app/package) versions independently — fine-grained, for changelog accuracy and debugging | scope-definition Q1 |
| 8 | The API/RPC wire contract gets its own version alongside component versions | scope-definition Q2 |
| 9 | Registry publishing is deferred to a later change; no pre-`0.1.0` changelog backfill; no new versioned container/distro artifact types | scope-definition Q3 |
| 10 | Every confirmed capability is Must-have — nothing in the boundary is deferrable | scope-definition Q4 |
| 11 | Deadline posture: ASAP but quality-gated — tests required; CI/Docker changes verified locally with docker + act | scope-definition Q5 |
| 12 | Work order is dependency-first: convention → detection → bump → changelog → release wiring → docs | scope-definition Q6 |
| 13 | Nice-to-haves are in scope: PR-title linting, automated release PRs/bump branches, prerelease channels, signed/annotated tags | scope-definition Q7 |
| 14 | Four intent assumptions accepted (component enumeration deferred to Inception, unnamed future issue, no extra reporting cadence, stakeholder list as recorded) | intent-capture assumption confirmation |
| 15 | No skipped Ideation stage needs pulling back; initiative approved to proceed to Inception | approval-handoff Q1–Q4 |
