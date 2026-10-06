---
phase: 19-actions-runners
reviewed: 2026-09-16T20:55:00Z
depth: standard
files_reviewed: 29
files_reviewed_list:
  - crates/oxidean-api/src/actions/mod.rs
  - crates/oxidean-api/src/actions/dispatch.rs
  - crates/oxidean-api/src/actions/events.rs
  - crates/oxidean-api/src/actions/hooks.rs
  - crates/oxidean-api/src/actions/logs.rs
  - crates/oxidean-api/src/actions/parse.rs
  - crates/oxidean-api/src/actions/rpc.rs
  - crates/oxidean-api/src/actions/runner_proto.rs
  - crates/oxidean-api/src/actions/secrets.rs
  - crates/oxidean-api/src/actions/statuses.rs
  - crates/oxidean-api/src/actions/tokens.rs
  - crates/oxidean-api/src/actions/workflow.rs
  - crates/oxidean-api/proto/runner.proto
  - crates/oxidean-api/build.rs
  - crates/oxidean-api/src/app.rs
  - crates/oxidean-api/src/ssh/server.rs
  - crates/oxidean-api/src/routes/git_smart_http.rs
  - crates/oxidean-db/src/actions.rs
  - crates/oxidean-db/migrations/sqlite/0021_actions.sql
  - crates/oxidean-db/migrations/postgres/0021_actions.sql
  - crates/oxidean-db/migrations/mysql/0021_actions.sql
  - crates/oxidean-core/src/action_types.rs
  - apps/web/src/lib/actions-queries.ts
  - apps/web/src/routes/$owner.$repo.actions.tsrx
  - apps/web/src/routes/$owner.$repo.actions.index.tsrx
  - apps/web/src/routes/$owner.$repo.actions.$run.tsrx
  - apps/web/src/routes/admin/runners.tsrx
  - apps/web/src/components/repo/actions-settings-panel.tsrx
  - docker/oxidean-runner/entrypoint.sh
findings:
  critical: 0
  warning: 7
  info: 4
  total: 11
  critical_remediated: 4
status: remediated_ci_green
---

# Phase 19: Code Review Report

**Reviewed:** 2026-09-16T20:55:00Z
**Depth:** standard
**Files Reviewed:** 29
**Status:** remediated_ci_green (CR-01…04 fixed; integrate CI green on `ee9e97ea`; ship held for manual test)

## Remediation (2026-09-16)

Criticals addressed on integrate branch (CI green; ship held for manual verify):

| ID | Status | Change |
|----|--------|--------|
| CR-01 | Fixed | SSH `list_refs` before/after → `ref_updates_from_lists`; `env_actions_enabled()` helper |
| CR-02 | Fixed | `notify_actions_for_pull` from `pull::create` / `synchronize_after_push` / `reopen` |
| CR-03 | Fixed | Re-read workflow YAML at claim; FetchTask JSON + proto `steps_json` / `secrets_json` |
| CR-04 | Fixed | Fail closed without `OXIDEAN_ACTIONS_SECRETS_KEY` / `OXIDEAN_SESSION_SECRET` |

Warnings (WR-01…07) and info items remain open; not ship-blocking for D-ACT-05/07/17.

## Summary

Phase 19 delivers a substantial Actions control plane (schema, YAML discovery/parse, HTTPS push enqueue, runner register/claim, secrets RPC, UI). Adversarial review found **4 critical** defects that break core success criteria or weaken secret confidentiality: SSH pushes never enqueue, PR lifecycle is unwired despite Phase 12 existing, FetchTask omits executable job payload, and Actions secret encryption falls back to a hardcoded key. Several warnings cover status URL contracts, log binding, DoS surface, and fail-open gates.

## Critical Issues

### CR-01: SSH push never enqueues Actions runs

**File:** `crates/oxidean-api/src/ssh/server.rs:202-247`
**Issue:** After successful SSH `receive-pack`, Actions is called with an empty updates slice (`&[]`). `dispatch::pick_push_tip` then returns empty `head_sha`, and `dispatch_push_inner` treats that as delete-only and returns without discovering workflows. HTTPS path correctly passes real ref updates (`git_smart_http.rs:567-578`). This violates D-ACT-05 (HTTPS **and** SSH) and silently drops CI for SSH-only workflows.
**Fix:** Capture ref updates from the receive-pack session (same shape as HTTPS) and pass them into `notify_push_actions` (and ideally webhooks/PR sync). At minimum, after a successful push, resolve the updated tip (e.g. HEAD / advertised refs) instead of `&[]`:

```rust
// Prefer real updates from the pack session; fallback to resolving HEAD tip.
crate::actions::notify_push_actions(
    &db, git, &repos_dir, &repo_id, &owner_slug, &repo_name,
    Some(&user.id),
    &updates, // must be non-empty when refs changed
    actions_enabled,
).await;
```

Also prefer `AppState.actions_enabled` (or a shared helper) over re-parsing env in the SSH task.

### CR-02: `pull_request` Actions never fire — hooks not wired into Phase 12

**File:** `crates/oxidean-api/src/actions/hooks.rs:1-15` (contract); `crates/oxidean-api/src/pull/` (no call sites)
**Issue:** `dispatch_pull_request` exists and is tested in isolation, but `crates/oxidean-api/src/pull` never imports or calls `actions::hooks` / `dispatch_pull_request`. Roadmap/ACT-02 and D-ACT-05 require PR open/synchronize/reopened to enqueue runs. On this branch Phase 12 is present, so the gap is an integration defect, not a missing-phase deferral.
**Fix:** Call `dispatch_pull_request` from PR create, head-SHA update, and reopen paths with a populated `PullRequestEvent` (bare path, head SHA/ref, actor, instance gate). Soft-fail like push dispatch so PR mutations still succeed.

### CR-03: FetchTask response cannot drive act_runner job execution

**File:** `crates/oxidean-api/src/actions/runner_proto.rs:134-186`; `crates/oxidean-api/proto/runner.proto:27-32`; `crates/oxidean-db/migrations/sqlite/0021_actions.sql:56-70`
**Issue:** Jobs are parsed with full `steps` at enqueue time, but steps are not persisted and `FetchTask` only returns `job_id` / `run_id` / `job_key` / `runs_on` / `secrets`. Pinned proto likewise has no workflow/steps (or secrets) fields. Official `oxidean-runner` (act_runner) cannot obtain executable task content. This breaks ACT-04/ACT-07 “register and run” and D-ACT-07 wire-compatibility intent beyond a claim/ack tracer.
**Fix:** Persist workflow/job payload (or re-read workflow YAML from git at claim time) and return a Gitea/act_runner-compatible task body including steps (and secrets). Align `runner.proto` + JSON handlers so clients share one schema.

### CR-04: Actions secrets encrypt with a hardcoded default key

**File:** `crates/oxidean-api/src/actions/secrets.rs:10-17`
**Issue:** Key material is `OXIDEAN_ACTIONS_SECRETS_KEY` → `OXIDEAN_SESSION_SECRET` → literal `"dev-only-actions-secrets-key"`. `OXIDEAN_SESSION_SECRET` is not referenced elsewhere in the API crate, so misconfigured/production deploys that omit `OXIDEAN_ACTIONS_SECRETS_KEY` encrypt all repo secrets under a publicly known passphrase (SHA-256 of that string). Anyone with DB read access can decrypt Actions secrets offline.
**Fix:** Fail closed when neither key env is set (refuse `put_secret` / decrypt outside tests), or derive only from a required production secret with no hardcoded fallback:

```rust
fn secrets_key_bytes() -> Result<[u8; 32], String> {
    let raw = std::env::var("OXIDEAN_ACTIONS_SECRETS_KEY")
        .or_else(|_| std::env::var("OXIDEAN_SESSION_SECRET"))
        .map_err(|_| "OXIDEAN_ACTIONS_SECRETS_KEY (or SESSION_SECRET) required".into())?;
    // hash raw → [u8;32]
}
```

## Warnings

### WR-01: Commit status `target_url` does not match Actions UI routes

**File:** `crates/oxidean-api/src/actions/statuses.rs:28-34,127-130`; `crates/oxidean-api/src/actions/dispatch.rs:161-171`
**Issue:** Queued publish always passes `owner_slug`/`repo_name` as `None`, so enqueue statuses get no URL. Update path builds `{origin}/actions/runs/{id}` (missing owner/repo). UI routes are `/{owner}/{repo}/actions/{run}` (no `/runs/` segment). Branch-protection links will 404 or be blank.
**Fix:** Thread owner/repo into `enqueue_run` / `publish_queued_for_run`, and build `{origin}/{owner}/{repo}/actions/{run_id}`.

### WR-02: `update_log` trusts client `run_id` instead of job’s run

**File:** `crates/oxidean-api/src/actions/runner_proto.rs:241-264`
**Issue:** Authz checks `job.runner_id`, but log path uses `req.run_id` unchecked against `job.run_id`. A compromised/buggy runner can write logs under arbitrary rejected-safe IDs (orphan files / disk fill) while the UI reads `{job.run_id}/{job_id}.log`.
**Fix:** Use `job.run_id` only (ignore or require equality with `req.run_id`).

### WR-03: Unbounded log chunk size enables disk DoS

**File:** `crates/oxidean-api/src/actions/runner_proto.rs:233-264`; `crates/oxidean-api/src/actions/logs.rs:21-43`
**Issue:** Authenticated runners may append arbitrarily large `chunk` strings with no max size or per-job quota. A stolen runner token can fill `OXIDEAN_ACTIONS_LOG_DIR`.
**Fix:** Reject chunks over a fixed limit (e.g. 1 MiB) and enforce a per-job total cap.

### WR-04: Repo Actions gate fails open on DB errors

**File:** `crates/oxidean-api/src/actions/dispatch.rs:68-72`; `crates/oxidean-api/src/actions/events.rs:61-66`
**Issue:** `get_repo_actions_enabled(...).await.unwrap_or(true)` treats DB failures as “enabled”, continuing dispatch. Prefer fail-closed (`unwrap_or(false)` or propagate error) so a broken DB does not enqueue work.
**Fix:** On `Err`, return early / log and skip enqueue.

### WR-05: Bootstrap registration token is reusable forever

**File:** `crates/oxidean-api/src/actions/tokens.rs:22-28`
**Issue:** `OXIDEAN_RUNNER_REGISTRATION_TOKEN` accepts with raw `==` and is never consumed. Leaked Compose/bootstrap tokens allow unlimited runner registration and subsequent secret exfiltration via FetchTask. Documented as intentional, but high-impact if left set in production.
**Fix:** Prefer one-time DB tokens for production; if env bootstrap remains, document mandatory rotation and consider constant-time compare + optional single-use/disable-after-N.

### WR-06: One invalid workflow aborts entire push dispatch

**File:** `crates/oxidean-api/src/actions/workflow.rs:105-110`; `crates/oxidean-api/src/actions/dispatch.rs:88-90`
**Issue:** `discover_workflows` returns `Err` on first parse failure, so sibling valid workflows never enqueue. Operators see “push succeeded, no runs” for repos with one broken YAML.
**Fix:** Soft-fail per file (create failed run / log ParseError) and continue discovering others.

### WR-07: Admin runners UI lacks loader-level sys-admin gate

**File:** `apps/web/src/routes/admin/runners.tsrx:9-12`
**Issue:** Unlike `admin/lfs` / `admin/auth`, this route has no `beforeLoad` role check. Non-admins still see chrome; RPC correctly returns forbidden. Inconsistent hardening / UX.
**Fix:** Mirror other admin routes’ sys-admin loader redirect.

## Info

### IN-01: Proto `FetchTaskResponse` omits `secrets` field present in JSON

**File:** `crates/oxidean-api/proto/runner.proto:27-32` vs `runner_proto.rs:140-141`
**Issue:** JSON injects secrets; pinned prost types do not. Clients generated from proto alone will miss D-ACT-17 injection.
**Fix:** Extend proto and keep JSON/proto fields in sync (see CR-03).

### IN-02: `instance_actions_enabled` is a no-op wrapper

**File:** `crates/oxidean-api/src/actions/dispatch.rs:14-16`
**Issue:** Function returns its argument unchanged; dead abstraction.
**Fix:** Inline or use for shared env parsing with SSH/HTTPS.

### IN-03: Runner `token_hash` indexed but not UNIQUE

**File:** `crates/oxidean-db/migrations/*/0021_actions.sql` (`action_runners.token_hash`)
**Issue:** Registration tokens table is UNIQUE; runner tokens are only indexed. Duplicate hashes are improbable but would make auth ambiguous.
**Fix:** Add UNIQUE constraint on `action_runners.token_hash`.

### IN-04: Steps parsed then discarded at enqueue

**File:** `crates/oxidean-api/src/actions/dispatch.rs:152-159`; `crates/oxidean-api/src/actions/parse.rs:40`
**Issue:** Related to CR-03 — `JobSpec.steps` exist only in memory during enqueue.
**Fix:** Persist or re-fetch at claim time as part of CR-03.

---

_Reviewed: 2026-09-16T20:55:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
