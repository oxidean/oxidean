# Phase 20: Packages Registry - Discussion Log

> **Audit trail only.** Do not feed to planning/research/execution agents.
> Decisions are in CONTEXT.md.

**Date:** 2026-09-14
**Phase:** 20-packages-registry
**Areas:** Scope & routing, Auth, Storage, Lifecycle UI, Format depth

---

## Scope & routing

| Topic | Choice |
|-------|--------|
| Exposure | Same host, path-based |
| Namespace | Owner + name; optional repo link |
| URL layout | OCI `/v2/`, npm `/npm/`, generic `/generic/` |

## Auth

| Topic | Choice |
|-------|--------|
| Token model | Hybrid: existing ACL/scopes **and** new `package:read`/`package:write` |
| Anonymous | Public pull allowed; private + all pushes auth |
| Publish/delete | Write+ publish; Admin delete |

## Storage

| Topic | Choice |
|-------|--------|
| Volume | Separate `OXIDEAN_PACKAGES_DIR` |
| Dedup | Content-addressed cross-package |
| Limits | Max blob + per-owner quotas; reject over limit |

## Lifecycle UI

| Topic | Choice |
|-------|--------|
| Immutability | Match GitHub (OCI tags mutable; npm/generic versions immutable; delete not yank) |
| UI | Owner packages page + repo-linked views |
| Delete UX | Type-to-confirm |

## Format depth

| Topic | Choice |
|-------|--------|
| Coverage | All three formats in Phase 20 |
| npm | Fully featured (dist-tags, deprecate, search) |
| OCI + generic | Fully featured to match |

## Claude's Discretion

- Spec subsets, visibility when unlinked, GC, cosign only if required

## Deferred

- Other ecosystems; S3; optional cosign; replication
