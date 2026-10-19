-- logical: 0053_action_job_payloads — durable job payloads + normalized claim
-- labels + attempt counters + scheduling columns (actions parity Phase 1).

CREATE TABLE action_payloads (
  sha           TEXT PRIMARY KEY,
  repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  workflow_path TEXT NOT NULL,
  head_sha      TEXT NOT NULL,
  payload_json  TEXT NOT NULL,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_action_payloads_repo_head
  ON action_payloads(repository_id, head_sha);

CREATE TABLE action_job_labels (
  job_id TEXT NOT NULL REFERENCES action_jobs(id) ON DELETE CASCADE,
  label  TEXT NOT NULL,
  base   TEXT NOT NULL,
  PRIMARY KEY (job_id, label)
);

CREATE INDEX IF NOT EXISTS idx_action_job_labels_base ON action_job_labels(base);

ALTER TABLE action_jobs
  ADD COLUMN payload_sha TEXT NULL REFERENCES action_payloads(sha) ON DELETE SET NULL,
  ADD COLUMN matrix_key TEXT NOT NULL DEFAULT '',
  ADD COLUMN needs_json TEXT NOT NULL DEFAULT '[]',
  ADD COLUMN matrix_json TEXT NOT NULL DEFAULT '{}',
  ADD COLUMN outputs_json TEXT NOT NULL DEFAULT '{}',
  ADD COLUMN summary_md TEXT NULL,
  ADD COLUMN concurrency_group TEXT NULL,
  ADD COLUMN attempt INTEGER NOT NULL DEFAULT 1,
  ADD COLUMN timeout_at TIMESTAMPTZ NULL;

ALTER TABLE action_jobs DROP CONSTRAINT action_jobs_run_job_unique;
ALTER TABLE action_jobs ADD CONSTRAINT action_jobs_run_job_matrix_unique
  UNIQUE (run_id, job_key, matrix_key);

ALTER TABLE action_jobs DROP CONSTRAINT action_jobs_status_check;
ALTER TABLE action_jobs ADD CONSTRAINT action_jobs_status_check
  CHECK (status IN ('queued', 'waiting', 'in_progress', 'success', 'failure', 'cancelled', 'skipped'));

INSERT INTO action_job_labels (job_id, label, base)
  SELECT j.id, je.value, split_part(je.value, ':', 1)
  FROM action_jobs j
  CROSS JOIN LATERAL json_array_elements_text(j.runs_on_json::json) AS je(value);

CREATE INDEX IF NOT EXISTS idx_action_jobs_status_created ON action_jobs(status, created_at);

ALTER TABLE action_runs
  ADD COLUMN attempt INTEGER NOT NULL DEFAULT 1,
  ADD COLUMN event_payload_json TEXT NULL,
  ADD COLUMN concurrency_group TEXT NULL,
  ADD COLUMN concurrency_cancel_pending BOOLEAN NOT NULL DEFAULT false;
