# Project Retrospective

*A living document updated after each milestone. Lessons feed forward into future planning.*

## Milestone: v1.0 — MVP

**Shipped:** 2026-09-19  
**Phases:** 24 | **Plans:** 220

### What Was Built

- Dual-mode GitHub-shaped forge (cloud + self-host) on one Compose release train
- Rust RPC API + Octane `.tsrx` web, multi-DB, git over HTTPS/SSH with PATs and keys
- Orgs, issues, PRs, branch protection (including packaged direct-push denial)
- Actions runners, packages registry, LFS/releases/search, notifications/webhooks, social explore
- CI Compose matrix + Railway-class IaC path; Phase 22.1 hygiene and verification backfill

### What Worked

- Fine-grained phases (thin slices) kept parallel tracks and gap-closure insertions manageable
- Wave 0 RED stubs → tracer → UI pattern made capability landing predictable
- Octane `.tsrx` + TanStack Query session helpers as the UI/server-state convention
- Milestone audit + Phase 22.1 insertion closed ORG-06 packaging before ship

### What Was Inefficient

- VERIFICATION fingerprints with incomplete digests / renamed `.tsx` paths forced a batch refresh before close
- Several phases needed lightweight VERIFICATION backfill late (01–03, 11.1, 12–13, 16–18, 21–22)
- Nyquist / WINDOWS hygiene lagged product completeness on mid phases

### Patterns Established

- `GitBackend` / `CliGitBackend` with documented future `GixGitBackend`
- Capability ACL coalesce for org + collaborator access
- Protection hooks packaged into the API image (not host-git only)
- Tests-as-proof fingerprint refresh acceptable for milestone close when suites + VERIFICATION already green

### Key Lessons

1. Declare complete `covered_files` + `covered_digest` when writing VERIFICATION, or stick to legacy mtime (Phase 19 style)
2. Insert a real `## Milestone vN.N` ROADMAP heading early — phase titles containing `v1.0` confuse the archive parser
3. Ship protection enforcement in the image operators run, not only in unit tests
4. Prefer one closure phase for audit gaps over reopening many finished phases

### Cost Observations

- Model mix: not tracked for this milestone
- Calendar: ~10 days scaffold → ship
- Notable: 220 plans across 24 phases; fingerprint batch cleared 22 stale phases without conversational UAT

---

## Cross-Milestone Trends

### Process Evolution

| Milestone | Sessions | Phases | Key Change |
|-----------|----------|--------|------------|
| v1.0 | — | 24 | Fine roadmap + Wave 0 RED; audit-driven 22.1 closure |

### Cumulative Quality

| Milestone | Plans | Requirements | Notes |
|-----------|-------|--------------|-------|
| v1.0 | 220 | 87/87 | Audit passed; residual CI/WINDOWS debt |

### Top Lessons (Verified Across Milestones)

1. Package operator-facing security controls into shipped images
2. Keep ROADMAP milestone headings explicit for archive tooling
