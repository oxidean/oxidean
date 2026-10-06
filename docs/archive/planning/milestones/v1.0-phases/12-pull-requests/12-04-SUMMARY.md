# Phase 12 Plan 04: Files, commits, comments Summary

**PR detail commits/files RPC, conversation comments with resolve/outdated, Conversation|Commits|Files tabs.**

## What Landed
- `pull.files` / `pull.commits` / `pull.comments.*` + DB helpers
- Detail tab UI: conversation (Write|Preview), commits list, unified/split files + line comment affordance
- Outdated line comments on head SHA refresh / base retarget via `pull.update`

## Verification
- `cargo nextest run -p oxidean-api -E 'test(pull_files) | test(pull_comments)'`
- `make web-lint` / `make web-format-check`
- Vitest pulls integration (8 pass / 3 expected fail)

## Commits
- `b66fa41` feat(12-04): PR files, commits, comments, and detail tabs
