# Phase 14: Git LFS - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-14
**Phase:** 14-git-lfs
**Areas discussed:** Storage layout, Transport, Auth & ACL, Operator & limits, Client UX

---

## Storage layout

| Option | Selected |
|--------|----------|
| Separate `OXIDEAN_LFS_DIR` volume | ✓ |
| Under each bare repo | |

| Option | Selected |
|--------|----------|
| Instance-wide OID dedup | ✓ |
| Per-repo OID namespaces | |

| Option | Selected |
|--------|----------|
| OID-sharded dirs + DB refcounts | ✓ |
| Flat OID files | |

| Option | Selected |
|--------|----------|
| Factory reset wipes LFS_DIR | ✓ |
| Keep LFS on reset | |

---

## Transport

| Option | Selected |
|--------|----------|
| HTTPS only (SSH deferred) | ✓ |
| HTTPS + LFS-over-SSH | |

| Option | Selected |
|--------|----------|
| Under `repo.git/info/lfs/…` | ✓ |
| Separate `/lfs/…` prefix | |

| Option | Selected |
|--------|----------|
| Basic transfer only | |
| Basic + multipart/resumable | ✓ |

| Option | Selected |
|--------|----------|
| Pointer clone without smudge OK | ✓ |
| Reject clone if LFS missing | |

**Note:** User confirmed pointer/smudge behavior matches GitHub/Gitea before continuing.

---

## Auth & ACL

| Option | Selected |
|--------|----------|
| PAT Basic; Read download / Write upload; no cookies | ✓ |
| Also session cookies | |

| Option | Selected |
|--------|----------|
| Always on when configured | |
| Per-repo enable/disable | ✓ |

| Option | Selected |
|--------|----------|
| Admin toggles LFS | ✓ |
| Write+ toggles | |

| Option | Selected |
|--------|----------|
| Reuse contents/repo PAT scopes | ✓ |
| New fine-grained LFS permission | |

---

## Operator & limits

| Option | Selected |
|--------|----------|
| Max size only | |
| Max size + per-repo/user quotas | ✓ |
| No limits beyond disk | |

| Option | Selected |
|--------|----------|
| Env defaults + Admin UI overrides | ✓ |
| Env only | |
| Primarily per-repo UI | |

| Option | Selected |
|--------|----------|
| Reject over-limit uploads | ✓ |
| Soft warn only | |

| Option | Selected |
|--------|----------|
| Refcount + periodic GC | ✓ |
| No auto-GC in Phase 14 | |

---

## Client UX

| Option | Selected |
|--------|----------|
| Docs + settings only | |
| + pointer badges | |
| + LFS browser / quota dashboard | |
| **All three (1+2+3)** | ✓ |

| Option | Selected |
|--------|----------|
| Document `.gitattributes` only | ✓ |
| Suggest in UI | |
| Auto-commit starter attributes | |

| Option | Selected |
|--------|----------|
| Blob Download via LFS | ✓ |
| Pointer text only in UI | |

| Option | Selected |
|--------|----------|
| Repo settings + Admin dashboards | ✓ |
| Admin only | |
| Repo only | |

**User addendum:** Include **usage breakdown** on both dashboards.

---

## Claude's Discretion

- Default numeric limits, multipart details, GC schedule, breakdown dimensions, Settings copy

## Deferred Ideas

- LFS-over-SSH; S3 backends; auto `.gitattributes`; non-parity clone rejection
