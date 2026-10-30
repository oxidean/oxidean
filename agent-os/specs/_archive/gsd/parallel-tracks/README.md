# Remaining-phases integration track

**Status (2026-09-19):** Closed. Umbrella integrate branch `cursor/gsd-remaining-integrate-c82f` merged to `main` via [PR #16](https://github.com/oxidean/oxidean/pull/16) (2026-09-16).

| Phase | Integrate |
|-------|-----------|
| 12, 13, 16, 17, 18, 19, 21, 22 | Merged to `main` via integrate PR #16 |

Further v1.0 work lands on `main` (or short-lived feature branches), not the retired integrate branch.

## Historical merge gates (archived)

Phase PRs once targeted the integrate branch. Gates were: CI green (or maintainer-waived), human review, and mergeable onto then-current integrate.

GitHub Actions hosted runners are available again on this public repo. Prefer green CI; keep local validation before merge.

## Migration numbers (wave-2, historical)

| Phase | Migration |
|-------|-----------|
| 13 Branch protection | `0017_branch_protection` |
| 17 Notifications | `0018_notifications` |
| 18 Webhooks | `0019_webhooks` |
| 21 Social | `0020_social` |
| 16 Search | (none) |

**Former merge order into integrate:** 12 → 13 → parallel 16/17/18/21 → 19 → 22. Complete.
