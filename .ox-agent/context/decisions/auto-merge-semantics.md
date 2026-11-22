# ADR: auto-merge semantics — event-driven fire on every check/status writer, disable-on-push

- **Status**: accepted (WI-0003 / COL-07)
- **Date**: 2026-11-22
- **Supersedes**: none — companion to `auto-merge-core-orchestrator.md`.

## Decision

Auto-merge is **event-driven**: each mutation that can flip the merge gate
re-evaluates armed pulls, and head movement **disarms** them.

- **Fire seams (re-evaluate + merge when `evaluate_merge_gate` passes)** —
  every writer that can satisfy a required context or approval:
  `check.runs.create/update` terminal transitions (api-check), the
  Actions job→check-run projection (`webhook/dispatch.rs`), the Actions
  job→commit-status writer (`actions/statuses.rs::upsert_job_status`),
  `repo.commitStatus.create`, `pull.reviews.submit`, and
  `pull.update` draft→ready.
- **Disarm seams** — head moves (`synchronize_pull_after_head_move` plus
  `pull.update`'s direct head refresh), base retarget, close (silent
  clear), explicit `pull.auto_merge.disable`.
- **No timers, no polling loops** — a failed evaluation leaves the PR
  armed; the next seam event retries.

## Rationale

- GitHub disables auto-merge when the PR head is pushed; the work item
  prescribes "disable-on-push semantics like GitHub". Disarm beats
  re-evaluate here because the stored head/verified tree is what the
  enabler armed.
- Actions-bound checks bypass `api-check` handlers entirely (they
  project via `webhook/dispatch.rs` + `actions/statuses.rs`), so
  hooking only the RPC writers would silently miss the most common CI
  producer — the seam set must cover all four status/check writers.
- Event-driven matches the merge queue's own resolution model
  (`after_suite_completed` et al.) — no new scheduler machinery.

## Consequences

- A required context satisfied mid-suite (sibling non-required runs
  still pending) fires immediately — check-run completion, not just
  suite completion, is the trigger granularity.
- Fire attempts are soft-fail and idempotent by construction: the row
  is re-loaded and re-verified (`state=="open"`, still armed) before
  executing, and `evaluate_merge_gate` is the single green test.
