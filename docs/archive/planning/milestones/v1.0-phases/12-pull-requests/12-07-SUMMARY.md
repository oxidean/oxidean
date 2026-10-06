# Phase 12 Plan 07: Forks, links, compare, validation Summary

**Minimal repo.fork + fork-head PRs, real `pr` issue links, compare→create CTA, factory_reset pulls, docs/VALIDATION.**

## What Landed
- `repo.fork` via `GitBackend::clone_bare` + `forked_from_repo_id`
- Fork-head `pull.create` path greened in lifecycle tests
- Issue linked-PRs prefer `pr` → `/pull/{n}`; compare CTA to `/pulls/new`
- factory_reset_pulls CASCADE wipe; API/ARCHITECTURE/VALIDATION updated

## Verification
- `cargo nextest run -p oxidean-api -E 'test(pull_lifecycle)'`
- `cargo nextest run -p oxidean-db -E 'test(factory_reset_pulls)'`
- `make web-lint` / `make web-format-check`
- Vitest pulls integration (11 pass)

## Remaining / deferred
- Full Explore fork UX (SOC-04 → Phase 21)
- List filter depth (author/label/assignee/review) beyond Open/Closed/All shell
- PR-08 branch protection (Phase 13)
