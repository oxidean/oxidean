# GitHub Actions compatibility reference

This document is the contract for Oxidean's GitHub Actions–compatible CI.
Every row is one feature of the GitHub Actions specification — workflow
syntax, triggers, expressions, steps, environments, OIDC, and the REST
surface — with a status and machine-checked evidence.

The drift gate at `crates/oxidean-api/tests/actions_compat_doc.rs` parses
this file and enforces the contract: row IDs are unique and stable,
statuses are valid, every non-`n/a` row cites at least one evidence token,
every `case:` token names a conformance case in
`crates/oxidean-api/tests/conformance/cases.json`, every `test:` token
names a real test function, every conformance case is cited back by at
least one row, and the Summary table below matches the real row counts.

## How to read it

Rows use `| \`id\` | Feature | Status | Evidence | Notes |`, one feature
per row. Evidence tokens are comma-separated:

- `case:<id>` — a conformance corpus case; the runner executes the
  workflow end to end and asserts statuses and log content.
- `test:<path>::<fn>` — a Rust test function at that repo-relative path.

Statuses:

- `supported` — behaves like GitHub; the evidence exercises it.
- `partial` — works with a divergence spelled out in Notes; the evidence
  pins today's behaviour.
- `unsupported` — not implemented; the evidence pins today's loud
  behaviour (a validation-failure run, a `::warning::`, or a refusal).
  Nothing in this column silently mis-executes.
- `n/a` — a GitHub-platform-only feature (hosted runner images, larger
  runners, billing, enterprise policy). Notes name the Oxidean
  alternative when one exists.

Closing phases referenced in Notes: **P12** web UI; **P13** reusable
workflows, job `continue-on-error`, `vars`, remaining events, workflow
dirs, command gaps; **P15** PR test-merge;
**P16** runner scopes, badges, workflow disable, run delete, log
download, annotations, `workflow_job` webhook.

## Summary

| Section | supported | partial | unsupported | n/a |
|---------|-----------|---------|-------------|-----|
| 1. Workflow files & discovery | 8 | 0 | 0 | 0 |
| 2. Triggers & filters | 22 | 0 | 0 | 0 |
| 3. Events | 32 | 0 | 4 | 0 |
| 4. Workflow-level keys | 8 | 0 | 0 | 0 |
| 5. Jobs | 29 | 0 | 0 | 1 |
| 6. Steps | 19 | 0 | 0 | 1 |
| 7. Expressions | 15 | 0 | 0 | 0 |
| 8. Contexts | 24 | 0 | 0 | 0 |
| 9. Workflow commands & environment files | 22 | 0 | 0 | 0 |
| 10. Default environment variables | 48 | 0 | 0 | 0 |
| 11. Action metadata — action.yml | 17 | 0 | 0 | 1 |
| 12. Reusable workflows | 10 | 0 | 0 | 0 |
| 13. Artifacts | 13 | 0 | 0 | 0 |
| 14. Dependency caching | 5 | 0 | 0 | 0 |
| 15. Concurrency | 5 | 0 | 0 | 0 |
| 16. Environments & deployments | 11 | 0 | 0 | 0 |
| 17. `GITHUB_TOKEN` permissions | 9 | 0 | 0 | 0 |
| 18. OIDC | 9 | 0 | 0 | 0 |
| 19. Secrets & configuration variables | 13 | 0 | 0 | 0 |
| 20. Runners & labels | 6 | 0 | 0 | 3 |
| 21. Security behaviours | 9 | 0 | 0 | 0 |
| 22. Limits | 7 | 0 | 0 | 1 |
| 23. Run lifecycle & integrations | 16 | 0 | 0 | 1 |
| 24. GitHub REST API for Actions | 16 | 0 | 1 | 0 |

## 1. Workflow files & discovery (`file.`)

Source: <https://docs.github.com/en/actions/writing-workflows/workflow-syntax-for-github-actions#about-yaml-syntax-for-workflows>
and <https://docs.github.com/en/actions/using-workflows/triggering-a-workflow>

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `file.location` | Workflows live under `.github/workflows/` | supported | case:run-basic, test:crates/oxidean-api/tests/actions_workflow_parse.rs::actions_workflow_parse_discovers_github_workflows_path | Pushed and default-branch workflows are discovered from the git tree. |
| `file.extensions` | `.yml` and `.yaml` suffixes | supported | test:crates/oxidean-api/tests/actions_workflow_parse.rs::actions_workflow_parse_discovers_github_workflows_path | |
| `file.top-level-only` | Only flat files in the directory run — nested dirs ignored | supported | test:crates/oxidean-api/src/actions/workflow.rs::confine_workflow_path_is_flat_only | `ls_tree` on `.github/workflows` yields blobs only; nested trees are skipped. |
| `file.other-files` | Non-workflow files (`.md`, `.txt`, …) ignored | supported | test:crates/oxidean-api/tests/actions_workflow_parse.rs::actions_workflow_parse_unsupported_files_are_skipped | |
| `file.multiple` | Multiple workflows per repo, each independent | supported | test:crates/oxidean-api/tests/actions_workflow_parse.rs::actions_workflow_parse_bad_file_does_not_block_good_workflows | |
| `file.yaml-on-key` | `on:` parses as a key, not a YAML 1.1 boolean | supported | test:crates/oxidean-api/src/actions/parse.rs::parses_minimal_push_workflow | serde_yaml ≥0.9 uses the YAML 1.2 core schema. |
| `file.invalid` | Malformed/invalid YAML produces a failed-validation run | supported | case:runs-on-missing, test:crates/oxidean-api/tests/actions_phase8.rs::p8_discovery_errors_persist_as_failed_validation_runs | Parse errors never silently drop a workflow; a failed run records the error. |
| `file.unknown-keys` | Unknown top-level keys rejected | supported | test:crates/oxidean-api/src/actions/parse.rs::unknown_keys_fail_validation_at_every_level | Unknown keys at workflow, job, or step level flag `unsupported` — dispatch refuses the run (failed validation), matching GitHub's "Unexpected value" rejection. |

## 2. Triggers & filters (`on.`)

Source: <https://docs.github.com/en/actions/writing-workflows/choosing-when-your-workflow-runs/triggering-a-workflow>
and <https://docs.github.com/en/actions/writing-workflows/workflow-syntax-for-github-actions#on>

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `on.scalar-form` | `on: push` (bare scalar) | supported | case:run-basic | |
| `on.list-form` | `on: [push, pull_request]` | supported | case:trigger-list-form | |
| `on.map-form` | `on:` mapping with per-event filters | supported | case:trigger-map-form | |
| `on.branches` | `branches:` positive list for push/pull_request | supported | test:crates/oxidean-api/src/actions/filters.rs::branch_filters, test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_push_branch_and_tag_filters | Matches the pushed branch or PR base branch. |
| `on.branches-ignore` | `branches-ignore:` negative list | supported | test:crates/oxidean-api/src/actions/filters.rs::branch_ignore | Fires when the branch matches no ignore pattern. |
| `on.tags` | `tags:` constrains tag pushes only | supported | test:crates/oxidean-api/src/actions/filters.rs::tags_constrain_only_tag_pushes | A tags-only workflow never fires on branch pushes (GitHub's ref-kind rule). |
| `on.tags-ignore` | `tags-ignore:` negative list | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_push_branch_and_tag_filters | |
| `on.paths` | `paths:` at least one changed file must match | supported | test:crates/oxidean-api/src/actions/filters.rs::paths_positive_and_ignore, test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_push_paths_filters | |
| `on.paths-ignore` | `paths-ignore:` fires when any file escapes the list | supported | test:crates/oxidean-api/src/actions/filters.rs::paths_positive_and_ignore | |
| `on.types` | `types:` activity-type filter | supported | test:crates/oxidean-api/src/actions/filters.rs::types_filter, test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_pull_request_types_and_target_optin | Applies to pull_request/pull_request_target/release/issues. |
| `on.mixed-ignore` | Positive and `-ignore` list on the same event | supported | test:crates/oxidean-api/src/actions/parse.rs::mixed_positive_and_ignore_filters_fail_validation | A positive list plus its `-ignore` twin on the same event flags `unsupported` — dispatch refuses the run, matching GitHub's rejection. |
| `on.branches-and-paths` | `branches` and `paths` both apply (AND) | supported | test:crates/oxidean-api/src/actions/filters.rs::branch_and_path_filters_are_conjunctive | |
| `on.glob-syntax` | `*`, `**`, `?`, `[...]` classes, `!` negation | supported | test:crates/oxidean-api/src/actions/filters.rs::glob_basics, test:crates/oxidean-api/src/actions/filters.rs::glob_char_classes | `+(…)`/`@(…)` extglob syntax is not implemented — GitHub behaviour unverified whether docs-minimatch requires it. |
| `on.negation-order` | `!` patterns apply in order; a later positive re-includes | supported | test:crates/oxidean-api/src/actions/filters.rs::negation_last_match_wins | |
| `on.paths-tag-push` | `paths`/`paths-ignore` never evaluated for tag pushes | supported | test:crates/oxidean-api/src/actions/filters.rs::paths_never_gate_tag_pushes, test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_paths_filters_skipped_on_tag_push | |
| `on.skip-ci` | `[skip ci]`/`[ci skip]`/`[no ci]`/`[skip actions]`/`[actions skip]` suppress runs | supported | test:crates/oxidean-api/src/actions/dispatch.rs::skip_ci_markers_all_tokens, test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_skip_ci_marker_suppresses_push, case:push-skip-ci | Applied to push and pull_request events. |
| `on.branch-delete` | A branch-deletion push does not trigger `push` | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_branch_delete_and_tag_flood | |
| `on.tag-flood` | A push with more than 3 tags triggers no tag events | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_branch_delete_and_tag_flood | |
| `on.unsupported-event` | An event we do not implement warns instead of vanishing | supported | case:on-unsupported-event, test:crates/oxidean-api/src/actions/parse.rs::unimplemented_events_warn_loudly | The `on.<event>` name is emitted as a workflow warning; the workflow still runs for its other triggers. |
| `on.token-events` | GITHUB_TOKEN-caused events do not create runs (except dispatch) | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_release_and_issues | Every dispatch path gates on the caller's token class: git pushes and `repo.files.*` commits skip `push`/`create`/`delete`, RPC mutations carry `job_token_actor` into lifecycle + `pull_request` dispatch, and `workflow_dispatch`/`repository_dispatch` stay allowed (GitHub's documented exception). |
| `on.workflow-call` | `workflow_call` trigger — workflow is callable via `uses:` | supported | case:reusable-workflow-local, case:reusable-workflow-remote | A `workflow_call`-only workflow never self-dispatches; caller jobs resolve and expand it. |
| `on.pr-merge-sha` | `pull_request` runs against the merge-commit SHA | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_pull_request_test_merge_commit, test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_pull_request_merge_conflict_falls_back_to_head | `merge-tree --write-tree` synthesizes the commit against the live base tip; `github.sha` = merge, `github.ref` = `refs/pull/N/merge`. Unmergeable PRs fall back to the head sha + `refs/pull/N/head` — GitHub leaves such checks pending; we keep CI running (documented divergence). |

## 3. Events (`ev.`)

Source: <https://docs.github.com/en/actions/writing-workflows/choosing-when-your-workflow-runs/events-that-trigger-workflows>

Every event on the GitHub page gets a row. Supported events carry their
activity-type filter semantics; unsupported ones warn at parse (section 2,
`on.unsupported-event`) and never fire silently.

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `ev.push` | `push` — GITHUB_SHA = pushed head, GITHUB_REF = pushed ref | supported | case:run-basic, case:push-tag-ref, test:crates/oxidean-api/tests/actions_triggers.rs::actions_triggers_push_after_receive_pack | Filters: branches, tags, paths, and their -ignore twins. |
| `ev.pull_request` | `pull_request` — default types opened/synchronize/reopened | supported | case:pr-only, test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_pull_request_types_and_target_optin | GITHUB_SHA = the synthesized test-merge commit (`on.pr-merge-sha`). |
| `ev.pull_request_target` | `pull_request_target` — trusted-context PR runs | supported | case:prt-only, case:prt-push, test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_pull_request_types_and_target_optin | Requires the repo `actions_pull_request_target` opt-in; workflows read from the base ref. |
| `ev.schedule` | `schedule` — cron on the default branch | supported | test:crates/oxidean-api/src/actions/cron.rs::exact_and_ranges, test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_schedule_sweep_dedupes_per_minute | UTC; five-field cron; multiple entries allowed. |
| `ev.workflow_dispatch` | `workflow_dispatch` — manual run with typed inputs | supported | case:dispatch-only, test:crates/oxidean-api/tests/actions_endpoints.rs::dispatch_workflow_enqueues_run, test:crates/oxidean-api/src/actions/rpc.rs::dispatch_inputs_resolve_types_and_defaults | string/boolean/choice/number/environment types; required/default honoured; 25-input cap. |
| `ev.release` | `release` — types published/created/edited/prereleased/released | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_release_and_issues | |
| `ev.issues` | `issues` — all activity types, `types:` filter honoured | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_release_and_issues, test:crates/oxidean-api/tests/webhook_delivery.rs::actions_issue_label_delta_trigger_e2e, test:crates/oxidean-api/tests/webhook_delivery.rs::actions_issue_milestone_delta_trigger_e2e | `labeled`/`unlabeled` fire per label delta with `payload.label` naming the changed label; `milestoned`/`demilestoned` fire on milestone assignment/removal with `payload.milestone` (issue creation with a milestone emits `opened` then `milestoned`, matching GitHub ordering). |
| `ev.schedule-payload` | `github.event.schedule` carries the matched cron entry | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_schedule_sweep_dedupes_per_minute | `schedule_payload` emits `{"schedule": "<cron>", "repository": …}`; the test asserts the field lands on the run's event payload. |
| `ev.branch_protection_rule` | `branch_protection_rule` — created/edited/deleted | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_generic_events | Fires on `repo.branchProtection.*` mutations; `types:` honoured. Tag protection has no GitHub event analogue. |
| `ev.check_run` | `check_run` — created/completed/rerequested, `types:` honoured | supported | test:crates/oxidean-api/tests/checks.rs::check_events_trigger_runs_and_webhooks, test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_generic_events | Fires on `check.runs.*` (the public Checks API) and on every Actions job projection (queued→created, terminal→completed). Actions-projected runs emit webhooks only — they cannot re-trigger `on: check_run` workflows (GITHUB_TOKEN non-recursion; job-token API callers are suppressed too). |
| `ev.check_suite` | `check_suite` — requested/rerequested/completed, `types:` honoured | supported | test:crates/oxidean-api/tests/checks.rs::check_events_trigger_runs_and_webhooks, test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_generic_events | `requested`/`rerequested` fire on `check.suites.create`/`rerequest`; `completed` fires when every run under the suite is terminal (rolled up — failures beat cancelled beats success). Suites group per Actions run (`github-actions` app, `run_id` bound) or per (repo, sha, `api`) for API-created runs. |
| `ev.create` | `create` — branch/tag creation | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_generic_events | Fires on RPC branch/tag creation and receive-pack creates; `ref_type` distinguishes branch/tag in the payload. Job-token pushes are suppressed (GITHUB_TOKEN non-recursion). |
| `ev.delete` | `delete` — branch/tag deletion | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_generic_events | Fires on RPC and receive-pack deletes; job-token pushes suppressed. |
| `ev.deployment` | `deployment` — action `created` | supported | test:crates/oxidean-api/tests/deployments.rs::deployment_events_trigger_runs_and_webhooks, test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_generic_events | Fires on `deployment.create` (the public Deployments API); environment-job deployment projections emit webhooks only, preserving GITHUB_TOKEN non-recursion (job-token API callers are suppressed too). `deployment.ref` resolves branches/tags/sha server-side; `auto_merge`, `required_contexts`, `transient_environment`, and `production_environment` follow GitHub semantics. |
| `ev.deployment_status` | `deployment_status` — action `created` | supported | test:crates/oxidean-api/tests/deployments.rs::deployment_events_trigger_runs_and_webhooks, test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_generic_events | Fires on `deployment.status.create` and on every deployment-state transition (job transitions, auto-`inactive` supersede, `auto_inactive` on `success`). The deployment's `state` projects the latest status row, ordered by a monotonic sequence so same-second bursts stay deterministic. |
| `ev.discussion` | `discussion` | unsupported | case:on-unsupported-event | Warns at parse; never fires. P13. |
| `ev.discussion_comment` | `discussion_comment` | unsupported | case:on-unsupported-event | Warns at parse; never fires. P13. |
| `ev.fork` | `fork` | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_generic_events | Fires on `repo.fork` with a `forkee` payload; action `created`. |
| `ev.gollum` | `gollum` — wiki edits | supported | test:crates/oxidean-api/tests/wiki.rs::wiki_gollum_events_webhook_and_trigger, test:crates/oxidean-api/tests/wiki.rs::wiki_push_fanout_emits_gollum | Fires on `wiki.pages.create`/`update` and on every push to the `{name}.wiki.git` bare (Smart HTTP + SSH) — the payload carries a `pages` array with per-page `action` (`created`/`edited`), `page_name`, `title`, `sha`, `html_url`, `summary`. Deletes emit webhooks only (no `gollum` action; GitHub fires none). The wiki domain itself is a separate Git-backed unit: `{name}.wiki.git` clone/push over both transports gated on `wiki_enabled`, page CRUD + `wiki.revisions.list` RPC, `/{o}/{r}/wiki` UI with `_Sidebar`/`_Footer`/`_Header` chrome pages, and rename/transfer/delete/orphan-reconcile lifecycle for the wiki bare. |
| `ev.issue_comment` | `issue_comment` — created/edited/deleted | supported | test:crates/oxidean-api/tests/webhook_delivery.rs::actions_issue_comment_trigger_e2e, test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_generic_events | Fires on issue comments and PR conversation comments; `types:` honoured. |
| `ev.label` | `label` — created/edited/deleted | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_generic_events | Repo-scoped label CRUD; org labels (Oxidean extension) don't map to a repo context and don't fire. `types:` honoured. |
| `ev.merge_group` | `merge_group` — merge queue | unsupported | case:on-unsupported-event | Warns at parse; never fires. P13. |
| `ev.milestone` | `milestone` — created/opened/closed/edited/deleted, `types:` honoured | supported | test:crates/oxidean-api/tests/webhook_delivery.rs::actions_milestone_trigger_e2e, test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_generic_events | Reopening a closed milestone fires `opened` (GitHub's activity type). Milestone CRUD is repo-scoped under Issues (`milestone.*` RPC + `/issues/milestones` UI); `payload.milestone` carries the entity. |
| `ev.page_build` | `page_build` | unsupported | case:on-unsupported-event | Warns at parse; never fires. P13. |
| `ev.project` | `project` — created/edited/closed/reopened/deleted, `types:` honoured | supported | test:crates/oxidean-api/tests/projects.rs::projects_events_and_convert, test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_generic_events | Fires on `projects.*` board mutations; `projects.update` emits `edited` for title/body changes and `closed`/`reopened` for state transitions. |
| `ev.project_card` | `project_card` — created/edited/moved/converted/deleted, `types:` honoured | supported | test:crates/oxidean-api/tests/projects.rs::projects_events_and_convert, test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_generic_events | Fires on `projects.cards.*`. `converted` runs GitHub's note→issue flow: a real issue is created (`issues.opened` fires too — first line becomes the title, the trimmed rest the body) and the payload's `changes.note.from` carries the removed note. Column moves carry `changes.column_id.from`. |
| `ev.project_column` | `project_column` — created/edited/moved/deleted, `types:` honoured | supported | test:crates/oxidean-api/tests/projects.rs::projects_events_and_convert, test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_generic_events | Fires on `projects.columns.*`; renames emit `edited` with `changes.name.from`. |
| `ev.public` | `public` — repo goes public | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_generic_events | Fires on `repo.updateVisibility` when visibility changes non-public → public. GitHub defines no activity types. |
| `ev.pull_request_review` | `pull_request_review` — submitted/dismissed | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_generic_events | Fires on review submit and dismiss; payload carries review state/commit/PR context. `edited` is not an activity type here (no review-edit surface). |
| `ev.pull_request_review_comment` | `pull_request_review_comment` — created/edited/deleted | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_generic_events | Inline (diff) review comments; conversation comments fire `issue_comment` instead, matching GitHub. |
| `ev.pull_request_comment` | `issue_comment` on a PR (GitHub aliases it) | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_generic_events | Covered by `issue_comment` — PR conversation comments carry `issue.pull_request` in the payload like GitHub. |
| `ev.registry_package` | `registry_package` — published/updated | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_generic_events | Fires on generic/npm/OCI package pushes; `types:` honoured. |
| `ev.repository_dispatch` | `repository_dispatch` — external trigger | supported | test:crates/oxidean-api/tests/actions_endpoints.rs::dispatch_repository_enqueues_run | `repo.actions.dispatchRepository` (Write+): `event_type` is the activity type, `client_payload` lands as `github.event.client_payload`, `types:` filters apply. |
| `ev.status` | `status` — commit status posted | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_generic_events | Fires on `repo.commitStatus.create` (REST statuses API included). Actions' own publisher writes rows directly, bypassing the RPC, so job-token recursion can't occur. No activity types. |
| `ev.watch` | `watch` — repo starred | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_lifecycle_generic_events | Fires on star creation with action `started`; un-starring does not fire (GitHub has no `watch` delete activity). |
| `ev.workflow_run` | `workflow_run` — another workflow finished | supported | case:workflow-run-listen, case:workflow-run-stem, case:workflow-run-branch-filter | Fan-out on run completion/cancel/timeout; listeners resolve on the default branch (trusted), match `workflows:` by name or file stem, apply `types:`/`branches:` filters, and receive `github.event.workflow_run`. Only `completed` fires — `requested`/`in_progress` activity types are not emitted. |

## 4. Workflow-level keys (`wf.`)

Source: <https://docs.github.com/en/actions/writing-workflows/workflow-syntax-for-github-actions#about-yaml-syntax-for-workflows>

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `wf.name` | `name:` — workflow display name | supported | test:crates/oxidean-api/src/actions/parse.rs::parses_minimal_push_workflow | |
| `wf.run-name` | `run-name:` — per-run display name | supported | case:run-name, test:crates/oxidean-api/src/actions/parse.rs::run_name_parses_as_expression_source | Expression-evaluated at enqueue (`github`/`vars`/`inputs`); empty or absent falls back to the workflow name. |
| `wf.on` | `on:` — see section 2 | supported | case:run-basic | |
| `wf.env` | workflow-level `env:` map | supported | case:workflow-env, case:expr-env-layering | |
| `wf.defaults` | `defaults.run.shell` / `defaults.run.working-directory` | supported | case:run-defaults-shell, case:run-defaults-workdir, test:crates/oxidean-api/tests/actions_workflow_parse.rs::actions_workflow_parse_defaults_run_shell_and_workdir | |
| `wf.permissions` | workflow-level `permissions:` | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_permissions_full_map_in_payload | Detail in section 17. |
| `wf.concurrency` | workflow-level `concurrency:` | supported | case:concurrency-workflow | Detail in section 15. |
| `wf.jobs` | `jobs:` map | supported | case:run-basic | |

## 5. Jobs (`job.`)

Source: <https://docs.github.com/en/actions/writing-workflows/workflow-syntax-for-github-actions#jobs>
and <https://docs.github.com/en/actions/using-jobs/using-a-matrix-for-your-jobs>

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `job.id` | Job id rules; ids must be unique | supported | test:crates/oxidean-api/src/actions/parse.rs::job_id_rules_enforced | Empty, `/`, `\` rejected; duplicates fail YAML mapping. |
| `job.name` | `name:` — display name; expressions honoured | supported | case:job-name-override, case:expr-job-name | |
| `job.needs` | `needs:` — dependency DAG, implicit skip on failure | supported | case:needs-basic, case:needs-diamond, case:needs-fail-default, case:needs-skipped-dep, case:needs-matrix-dep, case:multi-job-parallel, case:reusable-needs-scope | Jobs with no `needs` run in parallel; skipped deps propagate as `skipped`; `needs` inside a called workflow scopes to callee legs. |
| `job.needs-result` | `needs.<id>.result` — success/failure/cancelled/skipped | supported | case:needs-failure-fn, case:needs-success-fn, case:needs-always | |
| `job.needs-outputs` | `needs.<id>.outputs.<name>` — step outputs flow through | supported | case:needs-outputs | |
| `job.needs-unknown` | `needs:` naming an undeclared job | supported | test:crates/oxidean-api/src/actions/parse.rs::structural_features_are_flagged_unsupported | Validation failure — GitHub rejects the file. |
| `job.needs-cycle` | Cyclic `needs:` | supported | case:needs-cycle | Validation failure, not a wedge. |
| `job.if` | `if:` on a job — implicit `success()` unless a status fn is present | supported | case:job-if, case:job-if-skip, case:needs-ctx-if | |
| `job.runs-on` | `runs-on:` string or array of labels | supported | case:runs-on-array, case:runs-on-nomatch, case:runs-on-expr | Expressions evaluated at enqueue. |
| `job.runs-on-group` | `runs-on: {group, labels}` object form | n/a | | GitHub-hosted runner-group syntax; parse fails loudly. Self-hosted labels cover the same routing. |
| `job.environment` | `environment:` — string, `{name, url}` object, or expression | supported | case:environment-job, test:crates/oxidean-api/tests/actions_phase7.rs::p7_environment_auto_create_and_deployments | Detail in section 16. |
| `job.concurrency` | job-level `concurrency:` group | supported | case:concurrency-job, case:concurrency-job-cancel | |
| `job.outputs` | `outputs:` map — values evaluated per leg | supported | case:job-outputs | |
| `job.outputs-secret-redact` | Secret values must not leak through job outputs | supported | test:crates/oxidean-runner/src/exec/mod.rs::secrets_masked_in_logs | Outputs are log-masked; a workflow can still print a secret deliberately — same residual risk as GitHub. |
| `job.env` | job-level `env:` | supported | case:job-env, case:expr-env-layering | |
| `job.defaults` | job-level `defaults.run` | supported | case:run-defaults-shell | Step > job > workflow precedence. |
| `job.timeout-minutes` | `timeout-minutes:` — expressions, fractional, 360 default | supported | case:timeout-minutes-job, case:job-timeout-kills | Absent → GitHub's 360-minute default; step value clipped by job deadline. |
| `job.strategy` | `strategy.matrix` — axes, include extend+append, exclude partial-match, expressions, fromJSON | supported | case:strategy-matrix, case:matrix-basic, case:matrix-include-exclude, test:crates/oxidean-api/src/actions/matrix.rs::include_merges_and_appends | |
| `job.matrix-empty` | Empty matrix fails loudly | supported | test:crates/oxidean-api/src/actions/matrix.rs::empty_axis_fails_loud | |
| `job.matrix-cap` | Over-256 expanded legs fail loudly | supported | case:matrix-over-256, test:crates/oxidean-api/src/actions/matrix.rs::over_256_legs_fails_loud | GitHub's documented 256-job limit. |
| `job.fail-fast` | `fail-fast:` cancels sibling legs on first failure; `false` disables | supported | case:matrix-fail-fast, case:matrix-fail-fast-off | |
| `job.max-parallel` | `max-parallel:` caps concurrent legs | supported | case:matrix-max-parallel | |
| `job.continue-on-error` | Job-level `continue-on-error:` — leg reports `failure` but the run stays green; exempt from `fail-fast` | supported | case:continue-on-error-job, case:continue-on-error-matrix, case:reusable-coe-callee | Evaluated per matrix leg (`${{ matrix.* }}` allowed). `needs.<job>.result` remains `failure` so dependents without a status function skip, matching GitHub. Rejected on `uses:` caller jobs (as GitHub does). |
| `job.container` | `container:` — image shorthand and object form | supported | case:container-job, test:crates/oxidean-runner/src/exec/containers.rs::spec_object_form | Requires a Docker-capable runner; absent Docker the job fails loudly. |
| `job.services` | `services:` — service containers | supported | case:services-job, test:crates/oxidean-runner/src/exec/containers.rs::spec_object_form | Host jobs reach services on published ports; container jobs reach them by service id. |
| `job.services-ports-ctx` | `job.services.<id>.ports` context | supported | case:services-job | `{id, network, ports}` populated per service — ports resolve from `HostConfig.PortBindings` (survives a dead service). |
| `job.uses` | `jobs.<id>.uses:` — call a reusable workflow | supported | case:reusable-workflow-local, case:reusable-workflow-remote, case:reusable-nested | Caller row stays durable and `in_progress` until callee legs close; expansion is deferred to the scheduler. |
| `job.with` | `jobs.<id>.with:` — inputs to a called workflow | supported | case:reusable-workflow-local, case:reusable-missing-input, case:reusable-undeclared-input, case:reusable-type-mismatch | Required/undeclared inputs and type mismatches fail the caller loudly. |
| `job.secrets` | `jobs.<id>.secrets:` — secrets to a called workflow, `inherit` | supported | case:reusable-secrets-map, case:reusable-secrets-none, case:reusable-inherit-secret, case:reusable-missing-secret | `inherit` passes the repo's secret set; required-but-absent callee secrets fail loudly. |
| `job.steps` | `steps:` — required unless `uses:` | supported | case:empty-steps | A job with neither `steps:` nor `uses:` is a validation failure, matching GitHub. |

## 6. Steps (`step.`)

Source: <https://docs.github.com/en/actions/writing-workflows/workflow-syntax-for-github-actions#jobsjob_idsteps>
and <https://docs.github.com/en/actions/using-workflows/using-a-shell>

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `step.id` | `id:` — addressable in `steps.<id>` contexts | supported | case:expr-steps-context | |
| `step.if` | `if:` — implicit `success()` unless a status function is used | supported | case:step-if, test:crates/oxidean-runner/src/exec/mod.rs::step_allowed_implicit_success_gate | |
| `step.if-false-literal` | `if: 'false'` — a non-empty string is truthy, bare `false` is not | supported | case:step-if-false-literal | GitHub's string-truthiness quirk pinned. |
| `step.if-cancel` | `always()`/`cancelled()` steps still run when the job is cancelled | supported | case:job-timeout-kills | Cancellation (timeout or server-side) marks the job `cancelled`, then remaining steps evaluate under the normal `if` gate — implicit-`success()` steps skip, `always()`/`cancelled()` drain with their own step timeout, `failure()` skips; post actions still run with `action_status=cancelled`. |
| `step.name` | `name:` — display name | supported | case:run-multi-step | |
| `step.uses-remote` | `uses: owner/repo[@path]@ref` — fetched and run | supported | case:uses-remote-composite, case:checkout-basic, case:checkout-path, case:uses-unknown, test:crates/oxidean-runner/src/exec/action.rs::ls_remote_resolves_local_repo | GitHub.com fetches work too (public smoke). An unresolvable `uses:` fails the step loudly (`uses-unknown`). |
| `step.uses-local` | `uses: ./path` — action in the same repo | supported | case:uses-local-composite | Requires a checkout first — same as GitHub. |
| `step.uses-docker` | `uses: docker://image` | supported | test:crates/oxidean-runner/src/exec/containers.rs::spec_missing_image_fails | Requires a Docker-capable runner. |
| `step.run` | `run:` — shell script; non-zero exit fails the step | supported | case:run-basic, case:exit-code-nonzero, case:run-fail, case:step-failure-flow, case:echo-unicode | A failed step fails the job and the run; UTF-8 output passes through ungarbled. |
| `step.shell-default` | Unset `shell:` → `bash -e {0}` staged file, no pipefail | supported | test:crates/oxidean-runner/src/exec/mod.rs::shell_argv_defaults | GitHub's documented default. |
| `step.shell-bash` | `shell: bash` → `bash --noprofile --norc -eo pipefail {0}` | supported | test:crates/oxidean-runner/src/exec/mod.rs::shell_argv_defaults | |
| `step.shell-others` | `sh`, `pwsh`, `python`, custom `{0}` templates | supported | case:run-shell-sh, case:shell-python, case:shell-pwsh, case:shell-custom | pwsh/python run the staged script file. |
| `step.shell-windows` | `cmd`, `powershell` builtins | n/a | | Windows-hosted shell defaults; Oxidean runners are POSIX-first. `shell: pwsh` works wherever pwsh is installed. |
| `step.working-directory` | `working-directory:` — must exist | supported | case:run-workdir, case:run-workdir-missing | Missing dir fails the step loudly. |
| `step.env` | step-level `env:` | supported | case:run-env-step, case:run-env-multiline | |
| `step.with` | `with:` inputs to an action | supported | case:checkout-with-inputs | |
| `step.with-on-run` | `with:` on a `run:` step has no effect | supported | test:crates/oxidean-api/src/actions/parse.rs::captured_features_warn_not_unsupported | Warned at parse, matching the docs' "no effect" note. |
| `step.continue-on-error` | `continue-on-error:` — step failure does not fail the job | supported | case:continue-on-error-step, case:step-conclusion-coe | `steps.<id>.outcome` keeps `failure`; `conclusion` normalizes to `success`. |
| `step.timeout-minutes` | `timeout-minutes:` per step — expressions allowed | supported | case:timeout-minutes-step | Clipped by the job deadline. |
| `step.args-entrypoint` | `with.args`/`with.entrypoint` on docker actions | supported | case:docker-uses-args, test:crates/oxidean-runner/src/exec/action.rs::split_arg_string_honors_quotes | `with.entrypoint` → `--entrypoint`; `with.args` is a single string split with quote grouping. Bare `args:`/`entrypoint:` step keys are rejected as unknown keys — matching GitHub outright. |

## 7. Expressions (`expr.`)

Source: <https://docs.github.com/en/actions/reference/evaluate-expressions-in-workflows-and-actions>
and <https://docs.github.com/en/actions/learn-github-actions/expressions>

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `expr.syntax` | `${{ }}` interpolation anywhere in a workflow | supported | case:expr-in-run, test:crates/oxidean-actions-expr/src/eval.rs::interpolation | |
| `expr.bare` | Bare expressions in `if:` (no delimiters needed) | supported | test:crates/oxidean-actions-expr/src/eval.rs::eval_bool_accepts_bare_or_delimited | |
| `expr.literals` | null, true/false, numbers, strings with `''` escaping | supported | test:crates/oxidean-actions-expr/src/eval.rs::literals_and_identity | Exponent (`1e3`) and hex literals: GitHub docs do not define them — GitHub behaviour unverified; ours rejects non-decimal forms loudly. |
| `expr.coercion` | Type coercion table (null/bool/''/NaN → 0/false) | supported | test:crates/oxidean-actions-expr/src/eval.rs::truthiness_and_logic_operands | |
| `expr.equality` | Loose `==`, case-insensitive strings, objects by reference | supported | test:crates/oxidean-actions-expr/src/eval.rs::equality_is_loose_and_case_insensitive | |
| `expr.ordering` | `<` `<=` `>` `>=` with coercion | supported | test:crates/oxidean-actions-expr/src/eval.rs::ordering | |
| `expr.logic` | `&&` and `&#124;&#124;` return operands, `!` coerces to bool | supported | test:crates/oxidean-actions-expr/src/eval.rs::truthiness_and_logic_operands | |
| `expr.member-access` | Case-insensitive property access, `[index]`/`['key']`, `.*` object filters | supported | test:crates/oxidean-actions-expr/src/eval.rs::member_index_deref | |
| `expr.functions-string` | contains/startsWith/endsWith/format/toJSON/fromJSON/join — case-insensitive | supported | test:crates/oxidean-actions-expr/src/eval.rs::functions, case:expr-functions | `format` handles `{{`/`}}` escapes; `toJSON` pretty-prints. |
| `expr.status-fns` | success()/failure()/cancelled()/always() | supported | test:crates/oxidean-actions-expr/src/eval.rs::status_functions, test:crates/oxidean-actions-expr/src/eval.rs::status_fn_detection | |
| `expr.hashfiles` | hashFiles — multi-pattern, `!` exclusion, empty on no match | supported | case:expr-hashfiles, test:crates/oxidean-runner/src/exec/mod.rs::hash_files_is_deterministic | |
| `expr.secrets-context` | `secrets.*` in expressions | supported | case:secrets-context, test:crates/oxidean-actions-expr/src/eval.rs::secrets_context_is_plain_lookup | |
| `expr.malformed` | Malformed expressions fail loudly at parse | supported | case:expr-malformed-run, case:expr-malformed-if, test:crates/oxidean-api/src/actions/parse.rs::malformed_expressions_flag_unsupported | No silent literal passthrough. |
| `expr.if-implicit-success` | `if:` without a status fn implies `success() &&` | supported | test:crates/oxidean-runner/src/exec/mod.rs::step_allowed_implicit_success_gate | |
| `expr.eval-errors` | Runtime eval errors fail the step/job loudly | supported | case:expr-malformed-run | |

## 8. Contexts (`ctx.`, `avail.`)

Source: <https://docs.github.com/en/actions/reference/workflows-and-actions/contexts>

The `avail.*` rows pin the context-availability table: which contexts each
workflow key may reference. Oxidean evaluates expressions wherever they
appear; the availability notes call out where GitHub restricts a context
that we also restrict.

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `ctx.github` | `github` context — sha, ref, ref_name, ref_type, event_name, event payload, repository*, actor*, workflow*, job, run_*, server_url, api_url, graphql_url, ref_protected, retention_days, triggering_actor, action*, base_ref, head_ref, workspace, token | supported | case:expr-github-ctx, case:ctx-github-extra, test:crates/oxidean-runner/src/exec/context.rs::runner_context | `graphql_url` is a shape-only value (no GraphQL endpoint). `github.action`/`action_repository`/`action_ref` are per-step static names incl. `if:` self-reference; `action_path` resolves inside action bodies; `action_status` is set for post phases. `triggering_actor` keeps the original actor on reruns (GitHub restamps). |
| `ctx.env` | `env` context — workflow/job/step layering | supported | case:expr-env-layering, case:workflow-env | |
| `ctx.vars` | `vars` context — configuration variables | supported | case:expr-vars-empty, case:vars-context, test:crates/oxidean-api/tests/actions_vars.rs::actions_vars_merge_precedence | Repo/org/environment stores; narrowest scope wins on collisions; values are plaintext by design. |
| `ctx.job` | `job` context — status, container, services | supported | case:services-job, case:container-job, test:crates/oxidean-runner/src/exec/context.rs::sync_job_ctx_preserves_container_and_services | `status` merges across transitions; `container` carries `{id, network}`; `services.<id>` carries `{id, network, ports}` (container → host port). |
| `ctx.jobs` | `jobs.<id>.result` / `.outputs` | supported | case:needs-outputs | |
| `ctx.steps` | `steps.<id>.outcome` / `.conclusion` / `.outputs` | supported | case:expr-steps-context, case:step-conclusion-coe | |
| `ctx.runner` | `runner` context — os, arch, name, temp, tool_cache, debug, environment | supported | case:ctx-github-extra, test:crates/oxidean-runner/src/exec/mod.rs::debug_flags_resolve_from_env_vars_and_secrets | `environment` is always `self-hosted`; `debug` = "1" when `ACTIONS_RUNNER_DEBUG` resolves on (env/vars/secrets, incl. rerun debug logging). |
| `ctx.secrets` | `secrets` context | supported | case:secrets-context | |
| `ctx.strategy` | `strategy` context — job-index, job-total, fail-fast, max-parallel | supported | case:strategy-context | Stamped per leg at emit (`job-index` is expansion order). Native runner only — the gitea proto narrows `strategy.matrix` per leg so stock `act_runner` sees index 0/total 1. |
| `ctx.matrix` | `matrix` context | supported | case:matrix-basic | |
| `ctx.needs` | `needs.<id>.result` / `.outputs` | supported | case:needs-outputs, case:needs-failure-fn | |
| `ctx.inputs` | `inputs` context — workflow_dispatch / workflow_call inputs | supported | test:crates/oxidean-api/src/actions/rpc.rs::dispatch_inputs_resolve_types_and_defaults, case:reusable-workflow-local | Mirrors `github.event.inputs` — typed values resolve in callee steps. |
| `avail.on` | Contexts in `on:` filters | supported | case:trigger-map-form | Filters are pattern lists, not expressions — matching GitHub. |
| `avail.env` | Contexts in `env:` at all levels | supported | case:expr-env-layering | |
| `avail.job-if` | Contexts in `jobs.<id>.if` | supported | case:job-if, case:needs-ctx-if | needs/github/env/vars/inputs available — no steps context, matching GitHub. |
| `avail.job-outputs` | Contexts in `jobs.<id>.outputs` | supported | case:job-outputs | jobs/needs/steps available per GitHub. |
| `avail.runs-on` | Contexts in `runs-on` | supported | case:runs-on-expr | |
| `avail.strategy` | Contexts in `strategy.matrix` | supported | test:crates/oxidean-api/src/actions/matrix.rs::expressions_evaluate | github/env/vars/inputs at enqueue — no needs/steps, matching GitHub's table. |
| `avail.concurrency` | Contexts in `concurrency` | supported | case:concurrency-job | github/vars/inputs at enqueue. |
| `avail.container` | Contexts in `container`/`services` | supported | case:container-job | Expressions in the spec evaluate at the runner. |
| `avail.environment` | Contexts in `environment.name`/`url` | supported | case:environment-job | |
| `avail.step-fields` | Contexts in step `name`/`run`/`uses`/`working-directory`/`env`/`with`/`if`/`timeout-minutes` | supported | case:expr-in-run, case:expr-steps-context | steps context populated progressively during the job. |
| `avail.permissions` | Contexts in `permissions` | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_permissions_full_map_in_payload | Literal scope values only — GitHub does not permit expressions here either. |
| `avail.outputs-step` | `$GITHUB_OUTPUT` → `steps.<id>.outputs` | supported | case:needs-outputs | |

## 9. Workflow commands & environment files (`cmd.`, `envfile.`)

Source: <https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-commands>

Log commands (`::name::`) pass through the step-output stream;
the runner interprets the behaviour-changing commands itself: file-targeted
commands (`set-output`, `save-state`, `set-env`, `add-path`) write into the
same `runner_temp` files the `$GITHUB_*` protocol drains, `add-mask` extends
the job mask list, `stop-commands` pauses interpretation, and `group`/
`endgroup`/`debug` translate to the `##[…]` log markers the UI folds.
Uninterpreted commands (`add-matcher`/`remove-matcher`) produce a
`::warning::` — never a silent no-op. Annotation commands translate to the
`##[level]msg` text GitHub's own logs carry plus an `##[ann]{json}` marker
the server strips at ingest and persists as a structured annotation row;
`repo.actions.jobAnnotations` serves them and the run page renders a panel.

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `cmd.error` | `::error file=…,line=…::msg` — error annotation | supported | test:crates/oxidean-runner/src/exec/commands.rs::annotation_props_decode_into_marker, case:annotations | Logs `##[error]msg` (GitHub's log form) + persists a structured row (`path`, `line`/`endLine`, `col`/`endColumn`, `title`); the run page lists it and `repo.actions.jobAnnotations` serves it. |
| `cmd.warning` | `::warning::` annotation | supported | test:crates/oxidean-api/src/actions/logs.rs::extract_annotations_strips_markers_and_parses_fields | Same as `cmd.error`; severity `warning`. |
| `cmd.notice` | `::notice::` annotation | supported | case:annotations | Same as `cmd.error`; severity `notice`. |
| `cmd.debug` | `::debug::` shown only with ACTIONS_STEP_DEBUG | supported | test:crates/oxidean-runner/src/exec/commands.rs::debug_lines_gate_on_step_debug | Forwarded when `ACTIONS_STEP_DEBUG` (or `ACTIONS_RUNNER_DEBUG`) is truthy in job env, `vars`, or secrets; otherwise dropped. |
| `cmd.group` | `::group::`/`::endgroup::` collapsible log sections | supported | test:crates/oxidean-runner/src/exec/commands.rs::group_commands_emit_markers | Translated to `##[group]`/`##[endgroup]` markers; the log viewer folds them. |
| `cmd.add-mask` | `::add-mask::` registers a runtime mask | supported | test:crates/oxidean-runner/src/exec/commands.rs::add_mask_extends_masks | Appends to the job's mask list alongside repo/env secrets. |
| `cmd.set-output` | `::set-output::` (deprecated upstream) | supported | test:crates/oxidean-runner/src/exec/commands.rs::file_commands_land_in_runner_temp_files | Applies via `$GITHUB_OUTPUT` + deprecation warning — deprecated upstream but still interpreted there. |
| `cmd.save-state` | `::save-state::` (deprecated upstream) | supported | test:crates/oxidean-runner/src/exec/commands.rs::file_commands_land_in_runner_temp_files | Applies via `$GITHUB_STATE` + deprecation warning. |
| `cmd.set-env` | `::set-env::` — disabled upstream unless ACTIONS_ALLOW_UNSECURE_COMMANDS | supported | test:crates/oxidean-runner/src/exec/commands.rs::file_commands_land_in_runner_temp_files | Applies via `$GITHUB_ENV` + deprecation warning (Gitea/`act` parity — GitHub hard-fails it by default). |
| `cmd.add-path` | `::add-path::` — same disabled-by-default family | supported | test:crates/oxidean-runner/src/exec/commands.rs::file_commands_land_in_runner_temp_files | Applies via `$GITHUB_PATH` + deprecation warning. |
| `cmd.stop-commands` | `::stop-commands::{token}`/`::{token}::` | supported | test:crates/oxidean-runner/src/exec/commands.rs::stop_commands_suppresses_until_token | All lines literal until the matching `::{token}::`; a missing token warns once. |
| `cmd.echo` | `::echo::on` / `::echo::off` | supported | test:crates/oxidean-runner/src/exec/commands.rs::echo_toggle_is_consumed | Interpreted no-op — this runner never echoes commands, so the toggle has nothing to gate. |
| `cmd.add-matcher` | `::add-matcher::`/`::remove-matcher owner=o::` problem matchers | supported | test:crates/oxidean-runner/src/exec/commands.rs::add_matcher_registers_and_remove_drops, case:problem-matchers | Full `IssueMatcher` port — multi-pattern `loop` state machine, ANSI strip before matching, same-owner clobber + prepend ordering, hit resets other matchers, error/warning/notice severity map (default `error`), `file` kept only when it resolves under the workspace, 10-annotation-per-level cap, matcher state scoped per step. Matcher paths resolve workspace-relative (GitHub uses the action dir for `uses:` steps — divergence). |
| `cmd.escaping` | `%25 %0D %0A %3A %2C` command-data escaping | supported | test:crates/oxidean-runner/src/exec/commands.rs::escaped_command_data_decodes | `%0D`/`%0A`/`%25` decode in payloads; `%3A`/`%2C` additionally decode in property values. |
| `envfile.env` | `$GITHUB_ENV` — `KEY=value` and `KEY<<DELIM` heredoc, later steps see it | supported | test:crates/oxidean-runner/src/exec/mod.rs::env_file_simple_and_blank_lines, test:crates/oxidean-runner/src/exec/mod.rs::env_file_heredoc, case:run-env-step | |
| `envfile.env-later-wins` | Later entries override earlier ones | supported | test:crates/oxidean-runner/src/exec/mod.rs::env_file_later_wins | |
| `envfile.output` | `$GITHUB_OUTPUT` → `steps.<id>.outputs.*` | supported | case:needs-outputs, test:crates/oxidean-runner/src/exec/mod.rs::path_file_parsing | |
| `envfile.path` | `$GITHUB_PATH` — prepended for later steps only | supported | test:crates/oxidean-runner/src/exec/mod.rs::file_commands_merge_env_and_path | Container steps prepend onto the image PATH, not the host's. |
| `envfile.step-summary` | `$GITHUB_STEP_SUMMARY` → rendered step summary | supported | case:step-summary, test:crates/oxidean-api/tests/workflow_conformance.rs::step_summary_persists_on_job | |
| `envfile.state` | `$GITHUB_STATE` → `STATE_*` in an action's post step | supported | case:uses-node-post, test:crates/oxidean-runner/src/exec/mod.rs::env_file_simple_and_blank_lines | Scoped to the writing action — `run:` writes are cleared, not leaked. |
| `envfile.reserved` | Reserved names cannot be overridden via env files or secrets | supported | test:crates/oxidean-runner/src/exec/mod.rs::secrets_cannot_override_reserved_vars | Warns and skips the entry. |
| `envfile.heredoc-collision` | Delimiter line inside the value truncates it | supported | test:crates/oxidean-runner/src/exec/mod.rs::env_file_heredoc | Same truncation rule as GitHub; an unterminated heredoc consumes the file tail — GitHub behaves identically. |

## 10. Default environment variables (`env.`)

Source: <https://docs.github.com/en/actions/reference/workflows-and-actions/variables#default-environment-variables>

One row per documented variable. Values are emitted as both `GITHUB_*`
and `OXIDEAN_*` twins (same value, both spellings) unless noted.

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `env.ci` | `CI` — always `true` | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | `OXIDEAN=true` twin too. |
| `env.github-action` | `GITHUB_ACTION` — current action name/id | supported | test:crates/oxidean-runner/src/exec/mod.rs::step_action_identity_naming | `__run`/`__run_N` for `run:` steps, `__<uses-slug>` (`_N` dedup) for `uses:`. Composite-action nesting does not re-scope the name. |
| `env.github-action-path` | `GITHUB_ACTION_PATH` — the action's checked-out dir | supported | test:crates/oxidean-runner/src/exec/action.rs::resolve_inputs_applies_scalar_and_expr_defaults | Set inside `uses:` steps. |
| `env.github-action-repository` | `GITHUB_ACTION_REPOSITORY` | supported | test:crates/oxidean-runner/src/exec/mod.rs::step_action_identity_naming | `owner/repo` (or `./local`/`docker://` spec) plus `GITHUB_ACTION_REF` for `uses:` steps. |
| `env.github-actions` | `GITHUB_ACTIONS=true` | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.github-actor` | `GITHUB_ACTOR` — triggering user | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.github-actor-id` | `GITHUB_ACTOR_ID` | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.github-api-url` | `GITHUB_API_URL` — REST root | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | `{server_url}/api`. |
| `env.github-base-ref` | `GITHUB_BASE_REF` — PR base, else empty | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_pull_request_head_ref | |
| `env.github-env` | `GITHUB_ENV` — env file path | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | Oxidean-side name is `OXIDEAN_ENV_FILE` (see section 9). |
| `env.github-event-name` | `GITHUB_EVENT_NAME` | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.github-event-path` | `GITHUB_EVENT_PATH` — event.json path | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values, test:crates/oxidean-runner/src/exec/mod.rs::event_json_pull_request | |
| `env.github-graphql-url` | `GITHUB_GRAPHQL_URL` | supported | case:ctx-github-extra | Emitted as `{server_url}/api/graphql` (paired `OXIDEAN_GRAPHQL_URL`) — no GraphQL endpoint exists, so the variable is a stable shape value for interpolation. |
| `env.github-head-ref` | `GITHUB_HEAD_REF` — PR source branch, else empty | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_pull_request_head_ref | |
| `env.github-job` | `GITHUB_JOB` — job id | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.github-output` | `GITHUB_OUTPUT` | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.github-path` | `GITHUB_PATH` | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.github-ref` | `GITHUB_REF` — full ref | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_tag_ref | |
| `env.github-ref-name` | `GITHUB_REF_NAME` — short ref | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_tag_ref | |
| `env.github-ref-protected` | `GITHUB_REF_PROTECTED` | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_oidc_token_endpoint_flow | Evaluated at claim via the same branch/tag rule match as the OIDC `ref_protected` claim; failure degrades to `false`. |
| `env.github-ref-type` | `GITHUB_REF_TYPE` — branch/tag | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_tag_ref | |
| `env.github-repository` | `GITHUB_REPOSITORY` — `owner/repo` | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.github-repository-id` | `GITHUB_REPOSITORY_ID` | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.github-repository-owner` | `GITHUB_REPOSITORY_OWNER` | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.github-repository-owner-id` | `GITHUB_REPOSITORY_OWNER_ID` | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.github-retention-days` | `GITHUB_RETENTION_DAYS` | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | The instance's actions retention window (`sweeps::log_retention_days`), sent on the claim. |
| `env.github-run-attempt` | `GITHUB_RUN_ATTEMPT` | supported | test:crates/oxidean-api/tests/actions_runner_protocol.rs::actions_runner_protocol_rerun_bumps_run_attempt | |
| `env.github-run-id` | `GITHUB_RUN_ID` | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.github-run-number` | `GITHUB_RUN_NUMBER` | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.github-server-url` | `GITHUB_SERVER_URL` | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.github-sha` | `GITHUB_SHA` — run commit | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.github-step-summary` | `GITHUB_STEP_SUMMARY` | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.github-triggering-actor` | `GITHUB_TRIGGERING_ACTOR` | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | Equals `triggered_by`; GitHub restamps it with the re-running actor on re-runs — Oxidean keeps the original. |
| `env.github-workflow` | `GITHUB_WORKFLOW` — workflow name | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.github-workflow-ref` | `GITHUB_WORKFLOW_REF` | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.github-workflow-sha` | `GITHUB_WORKFLOW_SHA` | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.github-workspace` | `GITHUB_WORKSPACE` — checkout dir | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.github-token` | `GITHUB_TOKEN` — scoped job token | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_token_masked_in_logs | Masked in logs; revoked at job end. |
| `env.runner-arch` | `RUNNER_ARCH` — X64/ARM64/… | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.runner-debug` | `RUNNER_DEBUG` — set when debug logging enabled | supported | test:crates/oxidean-runner/src/exec/mod.rs::debug_flags_resolve_from_env_vars_and_secrets | `RUNNER_DEBUG=1` is injected into the job env when `ACTIONS_RUNNER_DEBUG` is truthy (env/vars/secrets); an explicit `env:` entry wins. |
| `env.runner-environment` | `RUNNER_ENVIRONMENT` — github-hosted/self-hosted | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | Emitted as `self-hosted` — all Oxidean runners are self-hosted (GitHub emits the same constant for self-hosted fleets). |
| `env.runner-name` | `RUNNER_NAME` | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.runner-os` | `RUNNER_OS` — Linux/macOS/Windows spelling | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | |
| `env.runner-temp` | `RUNNER_TEMP` — per-job temp dir | supported | test:crates/oxidean-runner/src/exec/mod.rs::runner_temp_dir_files | |
| `env.runner-tool-cache` | `RUNNER_TOOL_CACHE` | supported | test:crates/oxidean-runner/src/exec/mod.rs::job_env_values | setup-* actions honour it. |
| `env.actions-id-token-request` | `ACTIONS_ID_TOKEN_REQUEST_URL`/`_TOKEN` — OIDC mint credentials | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_id_token_request_credentials | Present only when `permissions.id-token: write` survives provenance gating. |
| `env.actions-cache-url` | `ACTIONS_CACHE_URL`/`RESULTS_URL`/`RUNTIME_TOKEN`/`RUNTIME_URL` | supported | case:cache-service-absent,test:crates/oxidean-api/tests/actions_cache_service.rs::cache_roundtrip_exact_restore | Per-job runtime token minted at claim; `RESULTS_URL`/`RUNTIME_URL` mirror `CACHE_URL`. |
| `env.actions-step-debug` | `ACTIONS_STEP_DEBUG` — enables `::debug::` | supported | test:crates/oxidean-runner/src/exec/commands.rs::debug_lines_gate_on_step_debug, test:crates/oxidean-api/tests/actions_vars.rs::actions_vars_injected_into_fetch_task | Read from job `env:`, `vars`, and secrets (truthy `"true"`/`"1"`); `ACTIONS_RUNNER_DEBUG` implies the same log surface. |

## 11. Action metadata — action.yml (`action.`)

Source: <https://docs.github.com/en/actions/sharing-automations/creating-actions/metadata-syntax-for-github-actions>

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `action.filename` | `action.yml` and `action.yaml` both read | supported | test:crates/oxidean-runner/src/exec/action.rs::resolve_inputs_applies_scalar_and_expr_defaults | `load_action_spec` tries both names. |
| `action.name-description` | `name:`/`description:`/`author:` | supported | case:uses-local-composite | Informational; carried through. |
| `action.inputs` | `inputs.<id>` — description, required, default | supported | case:checkout-with-inputs, case:action-metadata-warnings | |
| `action.input-defaults` | Scalar defaults (bool/number) stringified; expression defaults evaluated | supported | test:crates/oxidean-runner/src/exec/action.rs::resolve_inputs_applies_scalar_and_expr_defaults | `debug: false` reaches the action as `INPUT_DEBUG=false`; `${{ github.token }}` defaults resolve. |
| `action.input-env-names` | `INPUT_<NAME>` env names preserve hyphens | supported | test:crates/oxidean-runner/src/exec/action.rs::input_env_names_preserve_hyphens | Matches @actions/core's space-only underscore rule. |
| `action.inputs-required` | `required: true` without a value | supported | case:action-metadata-warnings | GitHub does not fail the step (docs: "will not automatically return an error"); modern runners warn — we warn the same way. |
| `action.unexpected-input` | `with:` keys not declared in `inputs:` warn | supported | case:action-metadata-warnings | `::warning::unexpected input(s)` in the log. |
| `action.deprecation` | `deprecationMessage:` surfaces as a warning | supported | case:action-metadata-warnings | |
| `action.outputs` | `outputs:` — composite `outputs.<id>.value` evaluated | supported | case:uses-local-composite, case:composite-env | |
| `action.runs-using` | `runs.using:` — composite, node*, docker | supported | case:uses-local-composite, case:uses-node-action, test:crates/oxidean-runner/src/exec/containers.rs::spec_missing_image_fails | |
| `action.node-versions` | `using: node12/16/20/24` | supported | case:uses-node-action, case:node-version-migration, test:crates/oxidean-runner/src/exec/action.rs::node_major_selection | Declared major resolves tool-cache → PATH → nodejs.org download; `node12/16` coerce to 20 with an EOL warning (GitHub removed them from `externals/`); `node20` warns deprecation; `FORCE_JAVASCRIPT_ACTIONS_TO_NODE24=true` (workflow env over runner env) upgrades to 24; `ACTIONS_ALLOW_USE_UNSECURE_NODE_VERSION` accepted (inert — our default stays the declared version, not node24-forced). |
| `action.pre-post` | `pre:`/`post:` lifecycle, `pre-if`/`post-if` default `always()` | supported | case:uses-node-pre-if, case:uses-node-post | |
| `action.post-order` | Post steps run in reverse order | supported | case:uses-node-post | |
| `action.docker-runtime` | docker `image`/`entrypoint`/`args`/`pre-entrypoint`/`post-entrypoint`/`env`, workspace mounted | supported | test:crates/oxidean-runner/src/exec/containers.rs::spec_object_form | Requires a Docker-capable runner. |
| `action.composite` | composite steps — run/shell/uses nesting, inputs context | supported | case:uses-local-composite, case:composite-env | |
| `action.branding` | `branding:` icon/color | n/a | | Cosmetic metadata; stored nowhere, never blocks execution. |
| `action.state-save` | `save-state`/`STATE_*` pre-to-post channel | supported | case:uses-node-post | Via `$GITHUB_STATE`. |
| `action.docker-step-args` | `with.args:`/`with.entrypoint:` overrides for docker actions | supported | case:docker-uses-args | Step `with.*` values replace the action.yml `entrypoint`/`args` (GitHub precedence). Also applies to `docker://` `uses:` steps. |

## 12. Reusable workflows (`reuse.`)

Source: <https://docs.github.com/en/actions/sharing-automations/reusing-workflows>

Implemented — a `uses:` caller job stays a durable row in the run and
expands into durable callee job rows once its `needs` are satisfied.
`continue-on-error` on a caller remains a validation failure, matching
GitHub.

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `reuse.call` | `jobs.<id>.uses:` — call a reusable workflow | supported | case:reusable-workflow-local, case:reusable-workflow-remote | Local `./.github/workflows/x.yml` and remote `owner/repo/path@ref` targets. |
| `reuse.trigger` | `on: workflow_call` — define a callable workflow | supported | case:reusable-workflow-local, case:reusable-no-trigger | Non-callable targets (`push`/`workflow_dispatch`-only) fail the call loudly. |
| `reuse.inputs` | `on.workflow_call.inputs` typed inputs | supported | case:reusable-missing-input, case:reusable-type-mismatch, case:reusable-undeclared-input | Defaults satisfy `required:`; undeclared caller keys and type mismatches fail loudly. |
| `reuse.outputs` | `on.workflow_call.outputs` — materialized onto the caller | supported | case:reusable-workflow-local | `needs.<caller>.outputs.*` resolves against callee `jobs.*` outputs. |
| `reuse.secrets` | `jobs.<id>.secrets` / `secrets: inherit` | supported | case:reusable-secrets-map, case:reusable-secrets-none, case:reusable-inherit-secret | `secrets: {}` passes nothing; `inherit` exposes the repo set; `required:` secrets must resolve. |
| `reuse.with` | `jobs.<id>.with` inputs to the call | supported | case:reusable-workflow-local | Expressions evaluate against the caller's context at expansion. |
| `reuse.strategy` | matrix over a call — one caller row per leg, fan-out capped | supported | case:reusable-matrix-call, case:reusable-fanout, case:reusable-fanout-accumulated | GitHub's 256-leg matrix cap applies inside callees; accumulated call fan-out is bounded separately. |
| `reuse.nesting` | Up to 4 nested call levels | supported | case:reusable-nested, case:reusable-depth, case:reusable-recursion | Depth beyond GitHub's limit and self-recursion fail loudly. |
| `reuse.permissions` | Called workflow can only reduce permissions | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_reusable_callee_permissions_clamped_to_caller | Every callee scope resolves per job then clamps to the caller leg's frozen permission map — `GITHUB_TOKEN` can only be downgraded downstream. |
| `reuse.env-context` | `env`/`steps`/`continue-on-error` on a caller job are not permitted by GitHub | supported | case:reusable-caller-keywords, test:crates/oxidean-api/src/actions/parse.rs::structural_features_are_flagged_unsupported | Callers reject GitHub-disallowed keywords at parse — loud validation failure. |

## 13. Artifacts (`art.`)

Source: <https://github.com/actions/upload-artifact#readme> and
<https://github.com/actions/download-artifact#readme> (v4) plus
<https://docs.github.com/en/actions/using-workflows/storing-workflow-data-as-artifacts>

Upload/download are built-in actions on the runner — no marketplace fetch
needed. Search-path semantics follow upstream `findFilesToUpload`
(search-path root, multi-path LCA).

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `art.upload` | `actions/upload-artifact` uploads named artifacts | supported | case:artifact-upload-download | v4 semantics: immutable artifacts, `overwrite` replaces. |
| `art.search-path-root` | Archive root = the pattern's search path (multi-pattern → LCA) | supported | case:artifact-search-path-root, test:crates/oxidean-runner/src/exec/artifacts.rs::archive_root_follows_search_paths | Single literal file → parent dir. Dir pattern with one deep match → contents relative to the dir. |
| `art.if-no-files-found` | `if-no-files-found:` warn/error/ignore | supported | case:artifact-no-files-error | `error` fails the step; `warn` logs `::warning::`. |
| `art.hidden-files` | `include-hidden-files:` — dot-segments excluded by default | supported | test:crates/oxidean-runner/src/exec/artifacts.rs::archive_root_follows_search_paths | v4 default changed upstream; we match (hidden files skipped unless opted in). |
| `art.retention-days` | `retention-days:` per-artifact expiry | supported | test:crates/oxidean-api/tests/actions_phase8.rs::p8_retention_sweep_compresses_terminal_and_deletes_aged | Server sweep enforces expiry; artifact-level override accepted. |
| `art.overwrite` | `overwrite: true` replaces an existing artifact | supported | case:artifact-overwrite | v4 requires this flag to re-upload a name. |
| `art.compression-level` | `compression-level:` | supported | case:artifact-compression-level | 0 stores an uncompressed tar; 1–9 drive `gzip -N` (default 6 via `tar czf`); invalid values warn and fall back. Download auto-detects (`tar xf`). Codec is gzip vs upstream zstd — level semantics honored. |
| `art.download` | `actions/download-artifact` — by name into `path:` | supported | case:artifact-upload-download, case:artifact-pattern-download | |
| `art.download-pattern` | `pattern:` glob over artifact names | supported | case:artifact-pattern-download | |
| `art.merge-multiple` | `merge-multiple: true` flattens matched artifacts into `path:` | supported | case:artifact-pattern-download | |
| `art.download-missing` | Missing artifact name fails the step | supported | case:artifact-missing | |
| `art.cross-repo` | `github-token`/`repository`/`run-id` download from another repo/run | supported | test:crates/oxidean-api/tests/actions_runner_protocol.rs::artifacts_token_matrix_and_repository_param, test:crates/oxidean-api/tests/actions_runner_protocol.rs::artifacts_runner_and_unauthenticated_reads, test:crates/oxidean-runner/src/exec/artifacts.rs::download_cross_run_sends_supplied_credentials_and_ids | `github-token` defaults to `github.token` (job token, repo-pinned + `actions:read`); PATs authorize on scope + repo ACL (outsiders 404). `repository` must resolve to the run's repo — mismatch 404s like GitHub. |
| `art.digest` | `outputs.digest` — artifact SHA-256 | supported | case:artifact-upload-download | Emitted as `artifact-digest` (`sha256:<hex>` of the tar.gz). GitHub v4.4+ also emits `artifact-id`/`artifact-url`; digest hex vs upstream base64 differs cosmetically. |

## 14. Dependency caching (`cache.`)

Source: <https://docs.github.com/en/actions/using-workflows/caching-dependencies-to-speed-up-workflows>
and <https://github.com/actions/cache#readme>

The `_apis/artifactcache` service implements GitHub's cache REST contract
(reserve → upload → commit → lookup → download) under per-job
`ACTIONS_RUNTIME_TOKEN`s minted at claim. Entries are scoped
`repo + key + version + ref`; `restore-keys` fall back by prefix, newest
first. Repo usage is capped (`OXIDEAN_ACTIONS_CACHE_MAX_BYTES`, 10 GiB
default) with LRU eviction on commit; abandoned `reserved` rows and
expired tokens are reaped by the actions sweep. `repo.actions.listCaches`
/`deleteCache` expose the management surface `gh cache` mirrors.

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `cache.action` | `actions/cache` restore+save | supported | test:crates/oxidean-api/tests/actions_cache_service.rs::cache_roundtrip_exact_restore | Reserve/upload/commit/lookup/download round-trip covered. |
| `cache.keys` | `key`/`restore-keys` prefix fallback | supported | test:crates/oxidean-api/tests/actions_cache_service.rs::cache_restore_key_prefix_fallback | Committed rows only; newest wins. |
| `cache.save-only` | `actions/cache/save` | supported | test:crates/oxidean-api/tests/actions_cache_service.rs::cache_immutable_save_409 | Unique key/version/scope conflict surfaces `409`. |
| `cache.restore-only` | `actions/cache/restore` | supported | test:crates/oxidean-api/tests/actions_cache_service.rs::cache_reserved_not_restorable | Reserved rows never restore; lookup 404s unknown keys. |
| `cache.lookup-only` | `lookup-only` dry-run | supported | test:crates/oxidean-api/tests/actions_cache_service.rs::cache_roundtrip_exact_restore | Same `GET /caches` lookup — hit/miss identical sans download. |

## 15. Concurrency (`conc.`)

Source: <https://docs.github.com/en/actions/how-tos/writing-workflows/choosing-what-your-workflow-does/control-the-concurrency-of-workflows-and-jobs>

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `conc.group` | `concurrency:` — bare group string or `{group, cancel-in-progress}` | supported | case:concurrency-workflow, case:concurrency-job | Workflow- and job-level; job group wins for its legs. |
| `conc.case-insensitive` | Group names match case-insensitively | supported | case:concurrency-cancel | Groups are normalized to lowercase at enqueue. |
| `conc.cancel-in-progress` | `cancel-in-progress: true` supersedes running same-group runs/jobs | supported | case:concurrency-cancel, case:concurrency-job-cancel | |
| `conc.pending-supersede` | At most one running + one pending per group; a newer pending cancels the older pending | supported | case:concurrency-pending, case:concurrency-job-pending, test:crates/oxidean-db/tests/dialect_actions.rs::dialect_actions_concurrency_gate | Claim gate serializes same-group jobs (any run — matrix legs too) and blocks a run while another same-group run is in progress; enqueue cancels older pending runs/jobs. Postgres claims take per-group advisory locks; MySQL uses `GET_LOCK`; SQLite is writer-serialized. |
| `conc.expression-group` | `group` evaluated as an expression | supported | case:concurrency-job | github/vars/inputs contexts at enqueue. |

## 16. Environments & deployments (`envr.`)

Source: <https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/control-deployments-with-environments>

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `envr.auto-create` | A job naming an environment creates it if missing | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_environment_auto_create_and_deployments | |
| `envr.required-reviewers` | `required_reviewers` hold the job for approval | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_environment_required_reviewers_hold | `waiting_approval` until a reviewer releases it. |
| `envr.reviewer-cap` | Max 6 reviewers per environment | supported | test:crates/oxidean-api/tests/actions_endpoints.rs::environment_reviewer_cap | `environments.upsert` rejects >6 resolved reviewers; duplicate tokens dedupe before the count. |
| `envr.prevent-self-review` | `prevent_self_review` — the deployer cannot approve | supported | test:crates/oxidean-api/tests/actions_ui_surface.rs::review_deployment_membership_and_self_review_gates | Stored in the environment's protection JSON; the review endpoint rejects with `repo.actions.self_review_forbidden` and `can_review` hides the control. |
| `envr.wait-timer` | `wait_timer_minutes` delays the deployment | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_environment_wait_timer | GitHub caps at 30 days; ours accepts any non-negative value — cap not enforced. |
| `envr.branch-policy` | `deployment_branches` allowlist fails non-matching refs | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_environment_branch_allowlist_fails_loudly | Loud failure on disallowed refs, not a silent skip. |
| `envr.rejection-fails` | Rejected approval fails the job | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_environment_required_reviewers_hold | |
| `envr.secret-precedence` | Environment secrets override repo secrets, which override org secrets | supported | test:crates/oxidean-api/tests/actions_phase8.rs::p8_org_env_secret_merge_precedence_and_audit | |
| `envr.url` | `environment.url` → job outputs `url` | supported | case:environment-job | |
| `envr.deployments` | Deployment records per job + public Deployments API | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_environment_auto_create_and_deployments, test:crates/oxidean-api/tests/deployments.rs::deployment_crud_statuses_and_supersede | Environment jobs project a deployment row + status history; `deployment.*` RPCs mirror GitHub's REST surface (ref resolution, `auto_merge`, `required_contexts`, `auto_inactive`, inactive-gated delete). |
| `envr.admin-bypass` | Repo admins can release a held deployment | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_environment_required_reviewers_hold | Matches GitHub's admin override. |

## 17. `GITHUB_TOKEN` permissions (`perm.`)

Source: <https://docs.github.com/en/actions/security-for-github-actions/security-guides/automatic-token-authentication>
and <https://docs.github.com/en/actions/writing-workflows/workflow-syntax-for-github-actions#permissions>

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `perm.scopes` | All 16 scopes — actions, attestations, checks, contents, deployments, discussions, id-token, issues, models, packages, pages, pull-requests, repository-projects, security-events, statuses | supported | test:crates/oxidean-api/src/actions/parse.rs::permissions_map_contents_write | Every scope name recognized; unknown keys flag `unsupported`. |
| `perm.read-all` | `permissions: read-all` | supported | test:crates/oxidean-api/src/actions/parse.rs::permissions_read_all_and_write_all_scalars | |
| `perm.write-all` | `permissions: write-all` | supported | test:crates/oxidean-api/src/actions/parse.rs::permissions_read_all_and_write_all_scalars | |
| `perm.none` | `permissions: {}` — every scope none | supported | test:crates/oxidean-api/src/actions/parse.rs::permissions_map_without_contents_defaults_to_none | |
| `perm.unspecified-none` | A map resets unlisted scopes to `none` | supported | test:crates/oxidean-api/src/actions/parse.rs::permissions_map_contents_write | |
| `perm.job-override` | A job-level block replaces the workflow-level one | supported | test:crates/oxidean-api/src/actions/parse.rs::effective_contents_permission_job_overrides_workflow | |
| `perm.default-floor` | Unspecified `contents` falls back to the repo default | supported | test:crates/oxidean-api/src/actions/parse.rs::effective_contents_permission_defaults_to_read, test:crates/oxidean-api/tests/actions_phase7.rs::p7_default_permissions_floor | |
| `perm.fork-read-only` | Fork PR runs get read-only tokens | supported | test:crates/oxidean-api/tests/actions_runner_protocol.rs::actions_runner_protocol_pull_request_provenance_gate | Provenance-gated; every `write` grant drops to `read` across the whole scope map at token mint (`id-token` included — `write` gates OIDC, so fork runs cannot mint identity tokens), and fork runs hold for approval. |
| `perm.id-token` | `id-token: write` mints OIDC credentials | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_id_token_request_credentials, case:oidc-curl | Only scope that changes job env today; other scopes are recorded in the payload for the runner/API surface. |

## 18. OIDC (`oidc.`)

Source: <https://docs.github.com/en/actions/concepts/security/openid-connect>
and <https://docs.github.com/en/actions/reference/oidc>

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `oidc.issuer` | `iss` = `{public_origin}/api/actions/oidc` (path issuer, like GHES) | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_oidc_token_endpoint_flow | |
| `oidc.discovery` | `{iss}/.well-known/openid-configuration` — served under `/api` so the web proxy passes it | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_oidc_token_endpoint_flow | issuer field byte-matches the token `iss`. |
| `oidc.jwks` | `jwks_uri` = `{iss}/jwks`; RSA keys only | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_oidc_token_endpoint_flow | kty RSA, kid, alg RS256, use sig. |
| `oidc.rs256` | Tokens signed RS256 with RSA-2048 via aws-lc-rs | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_oidc_token_endpoint_flow | Cloud-provider-compatible: AWS STS and GCP STS accept RS256/RSA. EdDSA was dropped for this. |
| `oidc.claims` | Full documented claim set — sub formats for environment/pull_request/ref, actor*, base_ref, event_name, head_ref, job_workflow_ref/sha, ref, ref_protected, ref_type, repository*, repository_visibility, run_attempt, run_id, run_number, runner_environment, sha, workflow*, iss/aud/exp/iat/nbf/jti | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_oidc_token_endpoint_flow | Sub precedence: environment > pull_request > ref. |
| `oidc.aud` | `audience`/`aud` query parameter sets `aud` | supported | case:oidc-curl | `curl -H "Authorization: bearer $ACTIONS_ID_TOKEN_REQUEST_TOKEN" "$ACTIONS_ID_TOKEN_REQUEST_URL&audience=…"` works verbatim — the URL already carries `?api-version=2.0`. |
| `oidc.request-url` | `ACTIONS_ID_TOKEN_REQUEST_URL` ends `?api-version=2.0` for `&audience=` append | supported | case:oidc-curl, test:crates/oxidean-api/tests/actions_phase7.rs::p7_id_token_request_credentials | Matches @actions/core's `${URL}&audience=${aud}` contract. |
| `oidc.key-at-rest` | RSA private key encrypted at rest with the Actions secrets key | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_oidc_token_endpoint_flow | No secrets key configured → the token endpoint errors instead of minting. |
| `oidc.mint-gate` | `id-token: write` required; credentials per-job | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_id_token_request_credentials | |

## 19. Secrets & configuration variables (`secret.`, `var.`)

Source: <https://docs.github.com/en/actions/security-for-github-actions/security-guides/using-secrets-in-github-actions>
and <https://docs.github.com/en/actions/how-tos/writing-workflows/choosing-what-your-workflow-does/store-information-in-variables>

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `secret.repo` | Repository secrets — create/list/delete via RPC | supported | test:crates/oxidean-api/tests/actions_secrets.rs::actions_secrets_enable_toggle_persists | |
| `secret.env` | Environment secrets | supported | test:crates/oxidean-api/tests/actions_phase8.rs::p8_org_env_secret_merge_precedence_and_audit | |
| `secret.org` | Organization secrets with visibility scoping | supported | test:crates/oxidean-api/tests/actions_phase8.rs::p8_org_secret_selected_visibility_scopes_repos | |
| `secret.precedence` | env > repo > org precedence | supported | test:crates/oxidean-api/tests/actions_phase8.rs::p8_org_env_secret_merge_precedence_and_audit | |
| `secret.name-rules` | Name must be `[A-Z_][A-Z0-9_]*`-ish, no `GITHUB_` prefix | supported | test:crates/oxidean-api/src/actions/secrets.rs::validate_name_rejects_empty | |
| `secret.size` | 48 KiB value cap, non-empty | supported | test:crates/oxidean-api/tests/actions_secrets.rs::actions_secrets_value_size_cap | GitHub's documented limit. |
| `secret.encryption` | Encrypted at rest; never echoed back | supported | test:crates/oxidean-api/tests/actions_secrets.rs::actions_secrets_encrypt_never_stores_plaintext_blob, test:crates/oxidean-api/src/actions/secrets.rs::encrypt_fails_closed_without_key | Fails closed without a configured secrets key. |
| `secret.injection` | Secrets land in the `secrets` context and as step env | supported | case:secrets-env, case:secrets-context | |
| `secret.masking` | Secret values masked (`***`) in logs | supported | case:secrets-mask, test:crates/oxidean-runner/src/exec/mod.rs::secrets_masked_in_logs | |
| `secret.reserved` | A secret cannot override a reserved `GITHUB_*`/`RUNNER_*` name | supported | test:crates/oxidean-runner/src/exec/mod.rs::secrets_cannot_override_reserved_vars | Warns and skips. |
| `secret.fork-pr` | Secrets withheld on untrusted fork PRs | supported | test:crates/oxidean-api/tests/actions_runner_protocol.rs::actions_runner_protocol_pull_request_provenance_gate | Fork runs also hold for approval (see `sec.fork-approval`). |
| `var.vars-context` | `vars` configuration variables | supported | case:vars-context, test:crates/oxidean-api/tests/actions_vars.rs::actions_vars_merge_precedence | Repo/org/env stores; visibility-aware org reach; narrowest scope wins. |
| `var.cases-sensitivity` | Variable/secret names in contexts are case-insensitive lookups | supported | test:crates/oxidean-actions-expr/src/eval.rs::secrets_context_is_plain_lookup | Context keys resolve case-insensitively like GitHub. |

## 20. Runners & labels (`runner.`)

Source: <https://docs.github.com/en/actions/how-tos/write-workflows/choose-where-workflows-run/choose-the-runner-for-a-job>
and <https://docs.github.com/en/actions/hosting-your-own-runners>

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `runner.self-hosted` | Self-hosted runners register and claim jobs | supported | test:crates/oxidean-api/tests/actions_runner_protocol.rs::actions_runner_protocol_register | Gitea runner.v1 protocol — stock `act_runner` works too. |
| `runner.labels` | `runs-on` labels match runner-declared labels (all must match) | supported | test:crates/oxidean-api/tests/actions_runner_protocol.rs::actions_runner_protocol_fetch_task_label_match, case:runs-on-nomatch | |
| `runner.labels-array` | `runs-on: [a, b]` — every label must be present | supported | case:runs-on-array | |
| `runner.group` | `runs-on: {group: …}` runner groups | n/a | | GitHub-hosted grouping; labels cover routing. Parse fails loudly today. |
| `runner.hosted-images` | `ubuntu-latest`/`windows-latest` image pool | n/a | | No hosted fleet; `ubuntu-latest` on a self-hosted runner is just a label the runner declares. |
| `runner.larger-runners` | Larger/hosted runner SKUs | n/a | | Bring-your-own hardware. |
| `runner.registration-token` | Short-lived registration tokens | supported | test:crates/oxidean-api/tests/actions_secrets.rs::actions_secrets_admin_create_registration_token | |
| `runner.env-bootstrap` | Env-based runner bootstrap token | supported | test:crates/oxidean-api/tests/actions_runner_protocol.rs::actions_runner_protocol_env_bootstrap_token | |
| `runner.docker-mode` | Runner can execute jobs inside Docker containers | supported | case:container-job | Declared via labels; absent Docker the job fails loudly. |

## 21. Security behaviours (`sec.`)

Source: <https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions>

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `sec.fork-secrets` | Fork PRs get no secrets and a read-only token | supported | test:crates/oxidean-api/tests/actions_runner_protocol.rs::actions_runner_protocol_pull_request_provenance_gate | |
| `sec.fork-approval` | First-time/external fork contributors hold for approval | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_fork_approval_hold_and_release | |
| `sec.prt-optin` | `pull_request_target` requires the repo opt-in flag | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_pull_request_types_and_target_optin | Base-ref checkout enforced — a fork PR cannot reach a privileged run by rewriting its own workflow file. |
| `sec.prt-workflow-source` | `pull_request_target` runs the base-ref workflow | supported | case:prt-only | |
| `sec.pr-workflow-source` | `pull_request` runs the merged-tree workflow | supported | case:pr-only | Workflows evaluate on the test-merge tree (head content merged into base); head-only fallback when unmergeable. |
| `sec.token-recursion` | Job-token events never re-trigger workflows | supported | test:crates/oxidean-api/tests/actions_triggers_p6.rs::p6_pull_request_types_and_target_optin | `job_token_actor` early-returns lifecycle and PR dispatch; push transports (HTTP smart-git, `repo.files.*` commits, SSH — which job tokens can't authenticate) all gate the same way. Webhooks still deliver. |
| `sec.action-policy` | Instance allowlist / full-SHA pinning for actions | supported | test:crates/oxidean-api/src/actions/policy.rs::allowlist_enforces_owner_and_repo, test:crates/oxidean-api/tests/actions_phase8.rs::p8_policy_violation_fails_validation_loudly | `peter-evans/*` style owner and repo allowlists; `@<full-sha>` pinning enforcible. |
| `sec.script-injection` | `${{ }}` in `run:` is text-substituted — injection-resistant guidance applies | supported | case:expr-in-run | Same model as GitHub; mitigations are workflow-authoring concerns. |
| `sec.log-masking` | Secrets and job token masked in logs | supported | test:crates/oxidean-runner/src/exec/mod.rs::secrets_masked_in_logs, test:crates/oxidean-runner/src/exec/mod.rs::job_token_masked_in_logs | `::add-mask::` dynamic masks are not interpreted (see `cmd.add-mask`). |

## 22. Limits (`limit.`)

Source: <https://docs.github.com/en/actions/administering-github-actions/usage-limits-billing-and-administration>

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `limit.matrix` | Max 256 jobs per matrix | supported | case:matrix-over-256, test:crates/oxidean-api/src/actions/matrix.rs::over_256_legs_fails_loud | Loud error naming the cap. |
| `limit.dispatch-inputs` | Max 25 `workflow_dispatch` inputs | supported | test:crates/oxidean-api/src/actions/rpc.rs::dispatch_inputs_cap_25 | |
| `limit.job-timeout` | Default 360-minute job timeout | supported | case:timeout-minutes-job | |
| `limit.workflow-size` | Max workflow file bytes | supported | test:crates/oxidean-api/tests/actions_phase8.rs::p8_discovery_errors_persist_as_failed_validation_runs | `MAX_WORKFLOW_BYTES`; oversize becomes a discovery error. |
| `limit.log-size` | Bounded job log with a truncation marker | supported | test:crates/oxidean-api/tests/actions_phase8.rs::p8_bounded_log_cap_writes_truncation_marker_once | |
| `limit.log-stream` | Log index ordering / CAS | supported | test:crates/oxidean-api/tests/actions_phase8.rs::p8_log_index_cas_orders_batches | |
| `limit.retention` | Artifact/log retention sweep | supported | test:crates/oxidean-api/tests/actions_phase8.rs::p8_retention_sweep_compresses_terminal_and_deletes_aged | Instance-configured window. |
| `limit.api-rate` | REST API rate limiting | n/a | | No rate limiter today; self-hosted instances are trusted by definition. |

## 23. Run lifecycle & integrations (`life.`)

Source: <https://docs.github.com/en/actions/managing-workflow-runs>
and <https://docs.github.com/en/actions/monitoring-and-troubleshooting-workflows>

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `life.rerun-all` | Re-run whole run | supported | test:crates/oxidean-api/tests/workflow_conformance.rs::rerun_resets_and_regates, test:crates/oxidean-api/tests/actions_endpoints.rs::rerun_and_cancel_run | Attempt counter bumps; `needs`/approval gates re-apply. |
| `life.rerun-failed` | Re-run failed/cancelled jobs only | supported | test:crates/oxidean-api/src/actions/rpc.rs::rerun_run | `failed_only` flag. |
| `life.rerun-job` | Re-run a single job | supported | test:crates/oxidean-api/src/actions/rpc.rs::rerun_run | `job_id` targeting. |
| `life.cancel` | Cancel a run | supported | test:crates/oxidean-api/tests/actions_endpoints.rs::rerun_and_cancel_run | Runners see cancel on next update. |
| `life.debug-rerun` | Re-run with step-debug logging | supported | test:crates/oxidean-api/tests/actions_endpoints.rs::rerun_and_cancel_run, test:crates/oxidean-runner/src/exec/mod.rs::debug_flags_resolve_from_env_vars_and_secrets | `rerunRun` accepts `debug`; requeued jobs stamp `debug_logging`, surfaced at claim as `ACTIONS_STEP_DEBUG`/`ACTIONS_RUNNER_DEBUG` vars on both runner protocols; the web re-run menu carries the checkbox. |
| `life.delete-run` | Delete run history | supported | test:crates/oxidean-api/tests/actions_endpoints.rs::rerun_and_cancel_run | `repo.actions.deleteRun` (Write+) removes a finished run — jobs, tokens, and artifact rows cascade; the run's log dir and artifact payloads are unlinked. In-flight runs must be cancelled first. P16. |
| `life.disable-workflow` | Disable/enable a workflow file | supported | test:crates/oxidean-api/tests/actions_endpoints.rs::workflow_enable_disable | `repo.actions.setWorkflowEnabled` (Write+) upserts a per-(repo, path) flag; `listWorkflows` marks `disabled`, dispatch errors `workflow_disabled`, push/pull/schedule/lifecycle loops skip it, and `workflow_call` fails the caller. |
| `life.status-badge` | Workflow status badge endpoint | supported | test:crates/oxidean-api/tests/actions_endpoints.rs::workflow_badge_svg | `GET /{owner}/{repo}/actions/workflows/{file}/badge.svg` (proxied by the web tier + Traefik) and `/api/repos/.../badge.svg`; `?branch=`/`?event=` filters; anonymous on public repos, 404 on private. |
| `life.log-download` | Download a run's full log archive | supported | test:crates/oxidean-api/tests/actions_endpoints.rs::download_run_logs_zip | `GET /api/repos/{o}/{r}/actions/runs/{run}/logs` returns `logs.zip`; single-job raw logs at `.../jobs/{job}/logs`. The archive follows GitHub's `{job}/{seq}_{step}.txt` layout — runner preamble lands in `0_setup.txt`, post-step tail in `N_teardown.txt`, and markerless logs (skipped/failed legs) stay flat `{job}.txt`. |
| `life.log-tail` | Live log tail while a job runs | supported | test:crates/oxidean-api/tests/actions_ui_surface.rs::job_log_stream_sse_pushes_appends, test:crates/oxidean-api/tests/actions_rpc.rs::actions_rpc_job_logs | Two transports: `repo.actions.getJobLog` stays the portable long-poll RPC; `GET …/actions/jobs/{id}/logs/stream` is an SSE push tail for the web UI — connect sends a full `offset:0` snapshot, appends push instantly (broadcast wake on every flushed append, 2s tick as backstop), a shrunk file (rerun wipe) re-sends a replacement snapshot, and `done:true` is the last frame. The run page opens an `EventSource` and writes frames into the same query cache the poll fills; repeated stream drops fall back to the poll RPC. |
| `life.log-retention` | Logs compress + expire on a retention sweep | supported | test:crates/oxidean-api/tests/actions_phase8.rs::p8_gzip_roundtrip_preserves_offsets_and_merges_late_appends, test:crates/oxidean-api/tests/actions_phase8.rs::p8_retention_sweep_compresses_terminal_and_deletes_aged | |
| `life.commit-status` | Commit status / required checks integration | supported | test:crates/oxidean-api/tests/actions_endpoints.rs::rerun_and_cancel_run, test:crates/oxidean-api/tests/checks.rs::check_run_satisfies_required_context | Classic commit statuses under context `{workflow} / {job_key}`: enqueue publishes each job's real state (queued/waiting → pending, `if:`-skipped → success, validation-failed → failure); runner updates refresh; cancel → error, deadline-fail/fail-fast-cancel → error/failure via `actions::statuses`. Branch-protection `required_status_contexts` folds check runs in by name — a same-named check run is the newer writer and overrides a legacy commit status; non-completed runs count as pending, completed runs as their conclusion (`success`/`neutral`/`skipped` pass). |
| `life.workflow-run-webhook` | `workflow_run` webhook fires on run lifecycle | supported | case:workflow-run-listen | `workflow_run` deliveries go out on `requested` (enqueue/rerun), `in_progress` (first claim), and `completed` (terminal, including cancel and deadline-fail) via `webhook::dispatch::notify_workflow_run`; `on: workflow_run` downstream enqueues emit `requested` too. |
| `life.workflow-job-webhook` | `workflow_job` webhook | supported | test:crates/oxidean-api/tests/webhook_delivery.rs::webhook_workflow_job_lifecycle | `workflow_job` deliveries go out on `queued`/`waiting` at leg insert and every queued promotion (needs resolve, matrix slot, approval/timer release, requeue), `in_progress` at runner claim and reusable-call expansion, and `completed` on every terminal transition via the commit-status choke points; `conclusion` carries the internal status. |
| `life.notifications` | Email/Slack notifications on failure | n/a | | No notification subsystem on Oxidean. |
| `life.ui` | Runs list/detail in the web UI | supported | test:crates/oxidean-api/tests/actions_endpoints.rs::get_run_anonymous_includes_jobs_and_timestamps | Run page, typed dispatch form, settings panels, env deployments/reviews, artifact management. |
| `life.approval-gate` | Deployment approval releases a held job | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_environment_required_reviewers_hold | |

## 24. GitHub REST API for Actions (`api.`)

Source: <https://docs.github.com/en/rest/actions>

One row per endpoint group, mapped to the Oxidean RPC equivalent. The
Oxidean surface is RPC, not REST — `api.*` rows describe the operation
class, not URL parity.

| `id` | Feature | Status | Evidence | Notes |
|------|---------|--------|----------|-------|
| `api.runs` | List/get workflow runs | supported | test:crates/oxidean-api/tests/actions_endpoints.rs::list_runs_paginates, test:crates/oxidean-api/tests/actions_endpoints.rs::get_run_anonymous_includes_jobs_and_timestamps | `actions_listRuns` / `actions_getRun` RPC. |
| `api.runs-mutate` | Cancel / rerun (all, failed, one job) | supported | test:crates/oxidean-api/tests/actions_endpoints.rs::rerun_and_cancel_run | `actions_rerunRun` / `actions_cancelRun`. |
| `api.runs-delete` | Delete a workflow run | supported | test:crates/oxidean-api/tests/actions_endpoints.rs::rerun_and_cancel_run | `repo.actions.deleteRun` (Write+). |
| `api.jobs` | List jobs / get a job | supported | test:crates/oxidean-api/tests/actions_endpoints.rs::get_run_anonymous_includes_jobs_and_timestamps | Jobs embed in the run detail. |
| `api.logs` | Stream/download job logs | supported | test:crates/oxidean-api/tests/actions_rpc.rs::actions_rpc_job_logs, test:crates/oxidean-api/tests/actions_endpoints.rs::download_run_logs_zip | Per-job log reads via RPC; run-level `logs.zip` archive at `GET /api/repos/{o}/{r}/actions/runs/{run}/logs`. |
| `api.artifacts` | List/download artifacts | supported | test:crates/oxidean-api/tests/actions_phase8.rs::p8_retention_sweep_compresses_terminal_and_deletes_aged | Runner protocol + artifact endpoint. |
| `api.cache` | Cache endpoints | supported | case:cache-service-absent,test:crates/oxidean-api/tests/actions_cache_service.rs::cache_roundtrip_exact_restore | `_apis/artifactcache` REST surface; repo management via `repo.actions.listCaches`/`deleteCache`. |
| `api.runners` | Runner registration / tokens / labels | supported | test:crates/oxidean-api/tests/actions_runner_protocol.rs::actions_runner_protocol_register, test:crates/oxidean-api/tests/actions_runner_protocol.rs::actions_runner_protocol_declare_labels | Plus the gitea runner.v1 bridge for stock act_runner. |
| `api.secrets` | Secret CRUD (repo/env/org) | supported | test:crates/oxidean-api/tests/actions_secrets.rs::actions_secrets_list_names_only_never_echoes_value | Values are write-only. |
| `api.variables` | Configuration-variable CRUD | supported | test:crates/oxidean-api/tests/actions_vars.rs::actions_vars_repo_rpc_roundtrip | `repo.actions.variables.*`, `repo.actions.environments.variables*`, `org.actions.variables.*`; values are plaintext and listable. |
| `api.environments` | Environment CRUD + protection rules | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_environment_auto_create_and_deployments | |
| `api.oidc` | OIDC discovery + token mint | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_oidc_token_endpoint_flow | `/api/actions/oidc` subtree. |
| `api.workflows` | List workflows / dispatch | supported | test:crates/oxidean-api/tests/actions_endpoints.rs::list_workflows_discovers_dispatchable_anonymous, test:crates/oxidean-api/tests/actions_endpoints.rs::dispatch_workflow_enqueues_run | |
| `api.checks` | Check suites / runs / annotations (Checks API) | supported | test:crates/oxidean-api/tests/checks.rs::check_crud_annotations_and_acl, test:crates/oxidean-api/tests/checks.rs::check_actions_job_projection | `check.suites.*` / `check.runs.*` / `check.annotations.list` mirror GitHub's Checks REST surface — GitHub status/conclusion vocabulary, output title+summary, ≤50 annotations per create/update, `rerequest` on both levels. Every Actions job projects a check run under a per-run `github-actions` suite; `output.annotations` merges stored rows with the job's log-extracted `::error::`/`::warning::`/`::notice::` command annotations. |
| `api.permissions-req` | Token-scope enforcement matching `permissions:` | supported | test:crates/oxidean-api/tests/actions_phase7.rs::p7_permissions_full_map_in_payload, test:crates/oxidean-api/tests/actions_phase7.rs::p7_job_token_scope_map_gates_rpc, test:crates/oxidean-api/tests/checks.rs::check_job_token_does_not_recurse | The granted scope map rides the bearer identity — `issues`/`pull-requests`/`actions`/`statuses`/`deployments`/`environments`/`pages`/`checks`/`repository-projects` gate their procedure families, `contents`/`packages` ride dedicated fields, and `id-token: write` gates OIDC. Scopes without a procedure domain (`discussions`, `attestations`, `security-events`) carry no grants. |
| `api.projects` | Project boards / columns / cards (Projects API) | supported | test:crates/oxidean-api/tests/projects.rs::projects_crud_and_cards, test:crates/oxidean-api/tests/projects.rs::projects_unit_flag_and_acl | `projects.*` / `projects.columns.*` / `projects.cards.*` mirror GitHub's classic Projects REST surface — per-repo `#N` numbering, position-ordered columns and cards, note or issue/pull-linked cards with `content_url`, archive filtering, and note→issue conversion. Repo-scoped and gated by the `repository_projects` token family plus the Projects repo unit (`repo.projects.setEnabled`). |
| `api.github-rest-compat` | github.com REST URL compatibility for third-party actions | unsupported | test:crates/oxidean-api/src/actions/templates.rs::classify_uses | Actions that call api.github.com directly (labeler, github-script, stale, …) are flagged `GithubApi` in the curated caveats — no shim layer exists. decision: not planned as a REST shim; the caveat table marks them. |
