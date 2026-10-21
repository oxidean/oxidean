-- logical: 0053_action_job_payloads — durable job payloads + normalized claim
-- labels + attempt counters + scheduling columns (actions parity Phase 1).

CREATE TABLE action_payloads (
  sha           VARCHAR(64) PRIMARY KEY,
  repository_id CHAR(36) NOT NULL,
  workflow_path VARCHAR(255) NOT NULL,
  head_sha      VARCHAR(64) NOT NULL,
  payload_json  LONGTEXT NOT NULL,
  created_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_action_payloads_repo FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE INDEX idx_action_payloads_repo_head
  ON action_payloads(repository_id, head_sha);

CREATE TABLE action_job_labels (
  job_id CHAR(36) NOT NULL,
  label  VARCHAR(255) NOT NULL,
  base   VARCHAR(255) NOT NULL,
  PRIMARY KEY (job_id, label),
  CONSTRAINT fk_action_job_labels_job FOREIGN KEY (job_id) REFERENCES action_jobs(id) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE INDEX idx_action_job_labels_base ON action_job_labels(base);

ALTER TABLE action_jobs
  ADD COLUMN payload_sha VARCHAR(64) NULL,
  ADD COLUMN matrix_key VARCHAR(255) NOT NULL DEFAULT '',
  ADD COLUMN needs_json TEXT NOT NULL DEFAULT ('[]'),
  ADD COLUMN matrix_json TEXT NOT NULL DEFAULT ('{}'),
  ADD COLUMN outputs_json TEXT NOT NULL DEFAULT ('{}'),
  ADD COLUMN summary_md TEXT NULL,
  ADD COLUMN concurrency_group VARCHAR(255) NULL,
  ADD COLUMN attempt INT NOT NULL DEFAULT 1,
  ADD COLUMN timeout_at TIMESTAMP NULL;

ALTER TABLE action_jobs
  DROP INDEX uq_action_jobs_run_job,
  ADD UNIQUE KEY uq_action_jobs_run_job_matrix (run_id, job_key, matrix_key);

ALTER TABLE action_jobs
  ADD CONSTRAINT fk_action_jobs_payload FOREIGN KEY (payload_sha) REFERENCES action_payloads(sha) ON DELETE SET NULL;

INSERT INTO action_job_labels (job_id, label, base)
  SELECT j.id, je.value, SUBSTRING_INDEX(je.value, ':', 1)
  FROM action_jobs j
  JOIN JSON_TABLE(j.runs_on_json, '$[*]' COLUMNS (value VARCHAR(255) PATH '$')) AS je;

CREATE INDEX idx_action_jobs_status_created ON action_jobs(status, created_at);

ALTER TABLE action_runs
  ADD COLUMN attempt INT NOT NULL DEFAULT 1,
  ADD COLUMN event_payload_json LONGTEXT NULL,
  ADD COLUMN concurrency_group VARCHAR(255) NULL,
  ADD COLUMN concurrency_cancel_pending TINYINT(1) NOT NULL DEFAULT 0;
