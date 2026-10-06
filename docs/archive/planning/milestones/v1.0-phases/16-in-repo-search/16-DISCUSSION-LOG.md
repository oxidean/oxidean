# Phase 16 — Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.

**Date:** 2026-09-16  
**Mode:** Auto-decide (campaign defaults) — no interactive gray-area menu

## Inputs

- ROADMAP Phase 16 / REQUIREMENTS GIT-18
- User: lock GitHub-like in-repo search (code, commits, issues, PRs; permission-aware); auto-decide gray areas; assume Phase 12 PR model from `12-CONTEXT.md`

## Locked without discussion

| Area | Lock |
|------|------|
| Scope | Repo-only; global chrome search deferred |
| Types | Code / Commits / Issues / Pull requests tabs |
| Code/commits engine | `git grep` / `git log` via `GitBackend` (no external indexer) |
| Issues/PRs | DB substring + Phase 12 PR tables |
| Qualifiers | Modest subset (`is:`, `author:`, `path:`) |
| ACL | Read + soft not_found |

## Notes

- Phase 11 deferred “full GitHub search grammar” toward Phase 16 adjacency — this phase takes a **subset**, not full grammar (D-SRCH-13).
