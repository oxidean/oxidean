# WI-0003 code review — COL-07 auto-merge-when-green

## Scope reviewed

Full branch diff: db (migration 0105 + row/ops + dialect lists), core
types + proc classes, shared executor extraction, auto_merge
orchestrator, RPC handlers + dispatch + PAT classification, fire/disarm
seams, REST embed, rpc-gen client, three UI panels, 7 new tests.

## Findings

1. **`list_armed_pulls_for_head` builds SQL via `format!` + `AssertSqlSafe`.**
   The only interpolated values are compile-time constants (`PULL_COLS*`,
   `%PH%` placeholders replaced before bind) — no injection surface.
   Matches the crate's dialect-split convention. Acceptable, though a
   future tidy could collapse the three match arms.

2. **`pull.update` fires `maybe_fire_pull` on every edit.** One extra
   `find_pull_by_id` + early-out when unarmed — cheap and correct; the
   draft→ready flip needs this seam.

3. **Gate→execute race is GitHub-identical.** Between gate pass and
   `mark_pull_merged` a concurrent merge could land; `mark_pull_merged`
   last-writer-wins. The re-read at `maybe_fire_pull` entry narrows the
   window; no correctness hazard beyond a rare double-merge-commit write
   the base would also accept via fast-forward check.

4. **Armed state on queue-required bases is rejected at enable time**
   (`pull.merge_queue_required`). If a branch gains `required_merge_queue`
   protection *after* arming, `maybe_fire_pull` holds the arm forever
   until protection is removed or manually disarmed — documented
   behavior, mirrors "auto-merge never bypasses the queue".

5. **Repo `allow_auto_merge` flipped off mid-flight holds arms** (no
   merge, no disarm) — re-enabling resumes. Deliberate non-destructive
   choice over GitHub's stricter disarm.

6. **Enabler capability is re-evaluated at fire time** — a full loss of
   repo access holds the arm. Author-with-read can arm per spec
   ("author or maintainer"); required-approval protection still gates
   the actual merge, so no escalation.

7. **`disable_for_pull` emits `auto_merge_disabled` for every disarm
   seam** (push/retarget/explicit) — matches GitHub's webhook action
   names. `pull.close` clears silently (the `closed` event covers it).

8. **`PullRequestAction::{AutoMergeEnabled,AutoMergeDisabled}`** wired
   into the Actions notify path so `on: pull_request` `types:` filters
   see them; webhook `pull_request` events use the same action strings.

## Verification evidence

- `cargo nextest -p oxidean-api`: 1089/1089 (includes 7 new
  `pull_auto_merge` tests: enable validation, required-status fire,
  check-run fire, approval fire, head-push disarm, draft arm+ready fire,
  queue-required rejection).
- `cargo nextest` on db/core/api-core/pulls/check/repo/merge-queue/rest:
  431/431 (includes dialect ledger + enrichment suites).
- `make rpc-gen` + `make rpc-sync-check`: clean.
- `make web-lint`, `make web-format-check`: clean.
- Full-workspace `cargo check`: clean.
