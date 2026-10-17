-- logical: 0050_action_job_tokens — per-job Actions bearer tokens (OXIDEAN_TOKEN).
-- One live token per job (job_id UNIQUE); minted at claim, revoked on terminal
-- status / run cancel, hard expiry backstop at expires_at.

CREATE TABLE IF NOT EXISTS action_job_tokens (
  id            CHAR(36) PRIMARY KEY,
  job_id        CHAR(36) NOT NULL,
  run_id        CHAR(36) NOT NULL,
  repository_id CHAR(36) NOT NULL,
  user_id       CHAR(36) NOT NULL,
  permission    VARCHAR(8) NOT NULL,
  token_hash    VARCHAR(128) NOT NULL,
  created_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  expires_at    TIMESTAMP NOT NULL,
  revoked_at    TIMESTAMP NULL,
  UNIQUE KEY uq_action_job_tokens_job (job_id),
  UNIQUE KEY uq_action_job_tokens_hash (token_hash),
  CONSTRAINT fk_action_job_tokens_job  FOREIGN KEY (job_id) REFERENCES action_jobs(id) ON DELETE CASCADE,
  CONSTRAINT fk_action_job_tokens_run  FOREIGN KEY (run_id) REFERENCES action_runs(id) ON DELETE CASCADE,
  CONSTRAINT fk_action_job_tokens_repo FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE,
  CONSTRAINT fk_action_job_tokens_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE INDEX idx_action_job_tokens_run_id ON action_job_tokens(run_id);
