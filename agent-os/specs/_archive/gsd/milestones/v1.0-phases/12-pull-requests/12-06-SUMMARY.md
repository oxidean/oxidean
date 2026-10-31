# Phase 12 Plan 06: Merge + settings Summary

**pull.merge / mergeSettings RPC (earlier) plus merge panel UI, admin strategy toggles, API docs.**

## What Landed
- Merge method picker + delete-branch checkbox on PR detail
- Admin merge strategy checkboxes on repo settings
- docs/API.md pull.* + closing keywords

## Verification
- `cargo nextest run -p oxidean-api -E 'test(pull_merge)'`
- `make web-lint` / `make web-format-check`
- Vitest pulls integration

## Commits
- `51b6768` feat(12-06): pull.merge and mergeSettings RPC
- `0dff587` feat(12-06): merge panel UI, strategy settings, API docs
