-- logical: 0064_action_reusable_calls — reusable-workflow caller/callee
-- linkage (actions parity P13).
--
-- A callee job row's `caller_job_id` points at the `uses:` job that expanded
-- it; '' marks top-level jobs (NOT NULL keeps the dedupe tuple total —
-- NULL in a UNIQUE column never collides and would silently drop top-level
-- dedupe). The scheduler scopes needs/fail-fast matching to the caller
-- group so callee-internal `needs` resolve naturally without key rewriting.
--
-- `UNIQUE(run_id, job_key, matrix_key)` must include the caller: two caller
-- legs (or two callers of the same callee) emit callee rows that share
-- (job_key, matrix_key) — without the column they collide. SQLite rebuilds
-- the table to swap the constraint (0053 pattern); child tables reference
-- action_jobs(id) by name and are unaffected.

CREATE TABLE action_jobs_new (
  id            TEXT PRIMARY KEY,
  run_id        TEXT NOT NULL REFERENCES action_runs(id) ON DELETE CASCADE,
  job_key       TEXT NOT NULL,
  matrix_key    TEXT NOT NULL DEFAULT '',
  name          TEXT NOT NULL DEFAULT '',
  runs_on_json  TEXT NOT NULL DEFAULT '[]',
  status        TEXT NOT NULL DEFAULT 'queued'
    CHECK (status IN ('queued', 'waiting', 'in_progress', 'success', 'failure', 'cancelled', 'skipped')),
  payload_sha   TEXT NULL REFERENCES action_payloads(sha) ON DELETE SET NULL,
  needs_json    TEXT NOT NULL DEFAULT '[]',
  matrix_json   TEXT NOT NULL DEFAULT '{}',
  outputs_json  TEXT NOT NULL DEFAULT '{}',
  summary_md    TEXT NULL,
  concurrency_group TEXT NULL,
  attempt       INTEGER NOT NULL DEFAULT 1,
  timeout_at    TEXT NULL,
  runner_id     TEXT NULL REFERENCES action_runners(id) ON DELETE SET NULL,
  started_at    TEXT NULL,
  finished_at   TEXT NULL,
  approval_hold INTEGER NOT NULL DEFAULT 0,
  environment_name TEXT NULL,
  deployment_wait_minutes INTEGER NOT NULL DEFAULT 0,
  deployment_not_before TEXT NULL,
  heartbeat_at  TEXT NULL,
  log_index     INTEGER NOT NULL DEFAULT 0,
  caller_job_id TEXT NOT NULL DEFAULT '',
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  UNIQUE (run_id, job_key, matrix_key, caller_job_id)
);

INSERT INTO action_jobs_new
  (id, run_id, job_key, matrix_key, name, runs_on_json, status,
   payload_sha, needs_json, matrix_json, outputs_json, summary_md,
   concurrency_group, attempt, timeout_at, runner_id, started_at,
   finished_at, approval_hold, environment_name, deployment_wait_minutes,
   deployment_not_before, heartbeat_at, log_index, created_at, updated_at)
  SELECT id, run_id, job_key, matrix_key, name, runs_on_json, status,
         payload_sha, needs_json, matrix_json, outputs_json, summary_md,
         concurrency_group, attempt, timeout_at, runner_id, started_at,
         finished_at, approval_hold, environment_name, deployment_wait_minutes,
         deployment_not_before, heartbeat_at, log_index, created_at,
         updated_at FROM action_jobs;

DROP TABLE action_jobs;
ALTER TABLE action_jobs_new RENAME TO action_jobs;

CREATE INDEX IF NOT EXISTS idx_action_jobs_run_id ON action_jobs(run_id);
CREATE INDEX IF NOT EXISTS idx_action_jobs_status ON action_jobs(status);
CREATE INDEX IF NOT EXISTS idx_action_jobs_status_created ON action_jobs(status, created_at);
CREATE INDEX IF NOT EXISTS idx_action_jobs_caller
  ON action_jobs(run_id, caller_job_id);
