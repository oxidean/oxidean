-- Deployment review records: one row per approve/reject decision on an
-- environment-held job (GitHub's pending-deployments review history).
-- `reviewer_username` is denormalized — a review is a point-in-time record and
-- survives username changes.
CREATE TABLE action_deployment_reviews (
  id                VARCHAR(64) PRIMARY KEY,
  repository_id     CHAR(36) NOT NULL,
  run_id            CHAR(36) NOT NULL,
  job_id            CHAR(36) NOT NULL,
  environment       VARCHAR(255) NOT NULL,
  reviewer_id       CHAR(36) NOT NULL,
  reviewer_username VARCHAR(255) NOT NULL,
  state             VARCHAR(32) NOT NULL,
  comment           TEXT NULL,
  created_at        DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_action_deployment_reviews_repo FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE,
  CONSTRAINT fk_action_deployment_reviews_run FOREIGN KEY (run_id) REFERENCES action_runs(id) ON DELETE CASCADE,
  CONSTRAINT fk_action_deployment_reviews_job FOREIGN KEY (job_id) REFERENCES action_jobs(id) ON DELETE CASCADE
) ENGINE=InnoDB;
CREATE INDEX idx_action_deployment_reviews_job ON action_deployment_reviews(job_id);
CREATE INDEX idx_action_deployment_reviews_run ON action_deployment_reviews(run_id);
