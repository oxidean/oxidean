---
phase: 12-pull-requests
plan: "03"
subsystem: pulls
tags: [tracer, rpc, octane, chrome]
status: complete
completed: "2026-09-16"
---
# Phase 12 Plan 03: Tracer pull lifecycle + Pulls UI Summary

**Same-repo open/list/get/close/reopen with shared `#N`; Pulls chrome + list/new/detail shell.**

## What shipped
- `pull.*` RPC create/get/list/update/close/reopen
- Green `pull_lifecycle` nextest
- RepoChrome Pulls tab; `/pulls`, `/pulls/new`, `/pull/{n}`
- api-client pull types + client methods

## Deviations
- Empty-repo branch create needs seeded templates in tests (stack_id) — documented in test helper
