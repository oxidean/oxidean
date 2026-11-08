# Phase 11: Issues - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-14
**Phase:** 11-issues
**Areas discussed:** Issue model & numbering, Labels & assignees, Comments & markdown, Issue↔PR linking, List/filter IA

---

## Issue model & numbering

| Option | Description | Selected |
|--------|-------------|----------|
| Per-repo sequential `#N` | GitHub-style URLs | ✓ |
| Global sequential | Instance-wide numbers | |
| You decide | | |

**User's choice:** Per-repo sequential `#N`

| Option | Description | Selected |
|--------|-------------|----------|
| Open/closed only | No delete in v1 | |
| Soft-delete + admin purge | | |
| Open/closed + admin hard-delete | GitHub/Gitea parity | ✓ |
| You decide | | |

**User's choice:** Open/closed + admin hard-delete (after asking for GitHub/Gitea parity recommendation)

| Option | Description | Selected |
|--------|-------------|----------|
| Author + Write+ edit anytime | GitHub-like | ✓ |
| Author only for body | | |
| Admin only after create | | |
| You decide | | |

**User's choice:** Author + Write+ anytime

| Option | Description | Selected |
|--------|-------------|----------|
| No history in v1 | | |
| Full edit history | | ✓ |
| Edited timestamp only | | |
| You decide | | |

**User's choice:** Full edit history

---

## Labels & assignees

| Option | Description | Selected |
|--------|-------------|----------|
| Repo-scoped only | | |
| Org-scoped only | | |
| Org defaults + per-repo overrides | | ✓ |
| You decide | | |

**User's choice:** Org defaults + per-repo overrides

| Option | Description | Selected |
|--------|-------------|----------|
| Multiple assignees | | ✓ |
| Single assignee | | |
| You decide | | |

**User's choice:** Multiple assignees

| Option | Description | Selected |
|--------|-------------|----------|
| Write+ for assign + label CRUD | | |
| Write+ assign; Admin label defs | | ✓ |
| Admin only for both | | |
| You decide | | |

**User's choice:** Write+ assign; Admin manages label definitions

| Option | Description | Selected |
|--------|-------------|----------|
| Read+ eligible assignees | GitHub-like | ✓ |
| Any org member | | |
| Write+ only | | |
| You decide | | |

**User's choice:** Anyone with Read+ on the repo

---

## Comments & markdown

| Option | Description | Selected |
|--------|-------------|----------|
| Author edit/delete own; Write+ delete others | | ✓ |
| Author only | | |
| Write+ edit any | | |
| You decide | | |

**User's choice:** Author edit/delete own; Write+ can delete others’

| Option | Description | Selected |
|--------|-------------|----------|
| Write \| Preview tabs | | ✓ |
| Side-by-side live | | |
| Raw only | | |
| You decide | | |

**User's choice:** Write | Preview tabs

| Option | Description | Selected |
|--------|-------------|----------|
| No reactions in Phase 11 | | |
| GitHub-style emoji reactions | | ✓ |
| You decide | | |

**User's choice:** GitHub-style emoji reactions

| Option | Description | Selected |
|--------|-------------|----------|
| Full comment edit history | | ✓ |
| Edited timestamp only | | |
| No history | | |
| You decide | | |

**User's choice:** Full comment edit history

---

## Issue↔PR linking

| Option | Description | Selected |
|--------|-------------|----------|
| `#N` autolink only | | |
| Autolink + Linked PRs stub panel | | ✓ |
| Minimal PR records in Phase 11 | | |
| You decide | | |

**User's choice:** Autolink + Linked PRs stub panel

| Option | Description | Selected |
|--------|-------------|----------|
| Closing keywords wait for Phase 12 | | ✓ |
| Parse in 11, enforce on merge in 12 | | |
| Closing from commits in Phase 11 | | |
| You decide | | |

**User's choice:** Wait for Phase 12

| Option | Description | Selected |
|--------|-------------|----------|
| Same-repo `#N` only | | |
| Also `owner/repo#N` | | ✓ |
| You decide | | |

**User's choice:** Same-repo + cross-repo `owner/repo#N`

| Option | Description | Selected |
|--------|-------------|----------|
| Autolink only | | |
| Autolink + manual Link control | | ✓ |
| You decide | | |

**User's choice:** Also manual Link issue/PR control

---

## List/filter IA

| Option | Description | Selected |
|--------|-------------|----------|
| Default Open; Closed + All | | ✓ |
| Default All | | |
| You decide | | |

**User's choice:** Default Open

| Option | Description | Selected |
|--------|-------------|----------|
| Author, label, assignee + text search | | ✓ |
| Open/closed + text only | | |
| Full GitHub search grammar | | |
| You decide | | |

**User's choice:** Author, label, assignee + text search

| Option | Description | Selected |
|--------|-------------|----------|
| Newest-updated + offset pagination | | ✓ |
| Infinite scroll | | |
| Multi-sort + pagination | | |
| You decide | | |

**User's choice:** Newest-updated + offset pagination

| Option | Description | Selected |
|--------|-------------|----------|
| Issues tab + New issue from list/empty | | ✓ |
| New issue from list only | | |
| You decide | | |

**User's choice:** Issues tab + New issue CTAs

---

## Claude's Discretion

- Reaction emoji set size
- Org/repo label override merge rules
- Linked PR stub shape before Phase 12
- Hard-delete confirm UX
- Exact RPC naming and migration numbering

## Deferred Ideas

- Closing keywords on merge → Phase 12
- Full PRs → Phase 12
- Notifications → Phase 17
- Full search grammar / milestones / projects → later
