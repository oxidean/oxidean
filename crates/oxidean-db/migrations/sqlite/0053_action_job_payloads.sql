-- logical: 0053_action_job_payloads — durable job payloads + normalized claim
-- labels + attempt counters + scheduling columns (actions parity Phase 1).
--
-- action_payloads: steps/env/permissions captured at enqueue so fetch_task no
-- longer re-reads workflow YAML from the bare repo (racy + wasteful).
-- action_job_labels: normalized runs_on rows so claim matches in SQL instead
-- of scanning every queued job in Rust.
-- action_jobs rebuild (new CHECK set + UNIQUE(run_id, job_key, matrix_key)):
-- adds 'waiting'/'skipped' statuses and matrix legs sharing a job_key.
-- action_runs: real attempt counter drives run_attempt; concurrency +
-- event_payload columns land now so later phases are pure UPDATEs.
--
-- Table rebuild with a live dependent: action_job_tokens references
-- action_jobs(id), so it is parked in a scratch table and rebuilt after
-- action_jobs (same schema as 0050) — tokens survive the rebuild.

CREATE TABLE action_payloads (
  sha           TEXT PRIMARY KEY,
  repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  workflow_path TEXT NOT NULL,
  head_sha      TEXT NOT NULL,
  payload_json  TEXT NOT NULL,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);

CREATE INDEX IF NOT EXISTS idx_action_payloads_repo_head
  ON action_payloads(repository_id, head_sha);

CREATE TABLE action_job_tokens_mig AS SELECT * FROM action_job_tokens;
DROP TABLE action_job_tokens;

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
  concurrency_group TEXT NULL,
  attempt       INTEGER NOT NULL DEFAULT 1,
  timeout_at    TEXT NULL,
  runner_id     TEXT NULL REFERENCES action_runners(id) ON DELETE SET NULL,
  started_at    TEXT NULL,
  finished_at   TEXT NULL,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  UNIQUE (run_id, job_key, matrix_key)
);

INSERT INTO action_jobs_new
  (id, run_id, job_key, name, runs_on_json, status, runner_id,
   started_at, finished_at, created_at, updated_at)
  SELECT id, run_id, job_key, name, runs_on_json, status, runner_id,
         started_at, finished_at, created_at, updated_at FROM action_jobs;

DROP TABLE action_jobs;
ALTER TABLE action_jobs_new RENAME TO action_jobs;

CREATE INDEX IF NOT EXISTS idx_action_jobs_run_id ON action_jobs(run_id);
CREATE INDEX IF NOT EXISTS idx_action_jobs_status ON action_jobs(status);
CREATE INDEX IF NOT EXISTS idx_action_jobs_status_created ON action_jobs(status, created_at);

CREATE TABLE action_job_tokens (
  id            TEXT PRIMARY KEY,
  job_id        TEXT NOT NULL UNIQUE REFERENCES action_jobs(id) ON DELETE CASCADE,
  run_id        TEXT NOT NULL REFERENCES action_runs(id) ON DELETE CASCADE,
  repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  permission    TEXT NOT NULL CHECK (permission IN ('read', 'write')),
  token_hash    TEXT NOT NULL UNIQUE,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  expires_at    TEXT NOT NULL,
  revoked_at    TEXT NULL
);

INSERT INTO action_job_tokens SELECT * FROM action_job_tokens_mig;
DROP TABLE action_job_tokens_mig;

CREATE INDEX IF NOT EXISTS idx_action_job_tokens_run_id ON action_job_tokens(run_id);

-- Normalized claim labels: one row per runs_on entry. `base` (label up to the
-- first ':') is the match key — labels_match semantics reduce to
-- base(need) IN base(runner_labels).
CREATE TABLE action_job_labels (
  job_id TEXT NOT NULL REFERENCES action_jobs(id) ON DELETE CASCADE,
  label  TEXT NOT NULL,
  base   TEXT NOT NULL,
  PRIMARY KEY (job_id, label)
);

CREATE INDEX IF NOT EXISTS idx_action_job_labels_base ON action_job_labels(base);

INSERT INTO action_job_labels (job_id, label, base)
  SELECT j.id, je.value,
         CASE WHEN instr(je.value, ':') > 0
              THEN substr(je.value, 1, instr(je.value, ':') - 1)
              ELSE je.value END
  FROM action_jobs j, json_each(j.runs_on_json) AS je;

ALTER TABLE action_runs ADD COLUMN attempt INTEGER NOT NULL DEFAULT 1;
ALTER TABLE action_runs ADD COLUMN event_payload_json TEXT NULL;
ALTER TABLE action_runs ADD COLUMN concurrency_group TEXT NULL;
ALTER TABLE action_runs ADD COLUMN concurrency_cancel_pending INTEGER NOT NULL DEFAULT 0;
