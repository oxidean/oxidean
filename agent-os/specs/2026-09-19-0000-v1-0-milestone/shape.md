# v1.0 Milestone — Shaping Notes (imported)

## Scope

Self-hostable GitHub-shaped code forge MVP: git hosting, issues, PRs, orgs,
LFS, releases, packages, Actions-compatible CI, admin console. One codebase
serving both Oxidean Cloud and self-hosted installs.

## Decisions

- Dual-mode product, one brand, one Docker-based release train — no
  hosted-vs-self-host feature split
- Octane `.tsrx` islands on Astro static shells; Rust Axum API; multi-DB
  only inside `oxidean-db`
- Trunk-based development on `main`, squash-merge, SemVer-prefixed commit
  subjects driving version bumps
- Git protocol compatibility is a hard requirement — real git clients must
  keep working

## Context

- **Origin framework:** GSD (Git.Ship.Done.) milestone workflow — per-phase
  `PLAN`/`SUMMARY`/`CONTEXT`/`RESEARCH`/`UAT`/`VERIFICATION` records in the
  archive
- **Product alignment:** `agent-os/product/mission.md` and `roadmap.md`
  ("Where we are" derives from this record)

## Outcome

All 24 phases verified; milestone audit passed with caveat rows later
reconciled. The GSD framework was retired by the Agent OS migration —
follow-up work shapes new specs here instead of new phases.
