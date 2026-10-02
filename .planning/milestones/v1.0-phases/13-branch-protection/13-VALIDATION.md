# Phase 13: Branch Protection — Validation

**Nyquist / Wave 0:** Discoverable RED stubs before implementation plans turn green.

## Requirements checklist

| ID | Wave 0 stub surfaces |
|----|----------------------|
| ORG-05 | `branch_protection_rpc` CRUD stubs; Settings branches Vitest stub |
| ORG-06 | `branch_protect_push` hook deny stub; dialect migration stub |
| PR-08 | `branch_protect_merge` merge-blocked stub; PR blockers Vitest stub |

## Suggested filters

```bash
cargo nextest run -p oxidean-api -E 'test(branch_protect) | test(commit_status)'
cargo test -p oxidean-db --test dialect_branch_protection
bun --cwd apps/web exec vitest run src/routes/\$owner.\$repo.settings.branches.integration.test.ts
bun --cwd apps/web exec vitest run src/routes/\$owner.\$repo.pull.protection.integration.test.ts
```

## Wave 0 files (created by 13-00 / 13-01)

- `crates/oxidean-api/tests/branch_protection_rpc.rs`
- `crates/oxidean-api/tests/branch_protect_push.rs`
- `crates/oxidean-api/tests/branch_protect_merge.rs`
- `crates/oxidean-api/tests/commit_status_rpc.rs`
- `crates/oxidean-db/tests/dialect_branch_protection.rs`
- `apps/web/src/routes/$owner.$repo.settings.branches.integration.test.ts`
- `apps/web/src/routes/$owner.$repo.pull.protection.integration.test.ts`


## Gate status (integrate honesty pass)

**Gate status: GREEN** — Wave 0 stubs greened; phase shipped on integrate `cursor/gsd-remaining-integrate-c82f` (2026-09-16).
