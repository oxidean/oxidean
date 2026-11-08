# Phase 15: Releases & Transfer - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions live in CONTEXT.md.

**Date:** 2026-09-14
**Phase:** 15-releases-transfer
**Mode:** User selected all gray areas, then accepted the full recommended package (option 1) without per-question overrides.

---

## Package accepted

| Decision | Choice |
|----------|--------|
| Tag relationship | Tag must already exist |
| Draft / prerelease | Both supported |
| Edit / delete | Author or Write+ edit; Admin delete |
| Asset storage | Separate release-assets volume (not LFS) |
| Asset limits | Configurable max size; replace on edit OK |
| Rename | Admin only; HTTP redirects for retention window |
| Transfer | Admin only; to user or org; type-repo-name confirm |
| Transfer payload | Git + issues + LFS associations |
| Who publishes | Write+ create/publish |
| UI | Releases tab; list + detail + assets |

## Alternatives not chosen (summarized)

- Create tag during release publish
- Store release assets in LFS OID store
- Write+ rename/transfer
- No redirects after rename
- Docs-only / no Releases tab

## Claude's Discretion

- Redirect TTL, max asset size defaults, draft visibility, settings IA, stable download URLs after rename/transfer

## Deferred Ideas

- Auto changelog; soft-delete; cross-instance transfer; packages-from-releases (Phase 20)
