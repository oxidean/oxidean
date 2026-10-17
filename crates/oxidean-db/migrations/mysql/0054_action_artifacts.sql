-- logical: 0054_action_artifacts — artifact storage for upload/download-artifact
-- (actions parity Phase 4). Content lives on disk under
-- OXIDEAN_ACTIONS_ARTIFACTS_DIR; rows track metadata + retention.

CREATE TABLE action_artifacts (
  id            VARCHAR(64) PRIMARY KEY,
  repository_id VARCHAR(64) NOT NULL,
  run_id        VARCHAR(64) NOT NULL,
  job_id        VARCHAR(64) NULL,
  name          VARCHAR(255) NOT NULL,
  storage_path  VARCHAR(1024) NOT NULL,
  size_bytes    BIGINT NOT NULL DEFAULT 0,
  digest_sha256 VARCHAR(64) NOT NULL DEFAULT '',
  expires_at    TIMESTAMP NULL,
  created_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  UNIQUE KEY uq_action_artifacts_run_name (run_id, name),
  CONSTRAINT fk_action_artifacts_repo FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE,
  CONSTRAINT fk_action_artifacts_run FOREIGN KEY (run_id) REFERENCES action_runs(id) ON DELETE CASCADE,
  CONSTRAINT fk_action_artifacts_job FOREIGN KEY (job_id) REFERENCES action_jobs(id) ON DELETE SET NULL
) ENGINE=InnoDB;

CREATE INDEX idx_action_artifacts_expiry ON action_artifacts(expires_at);
