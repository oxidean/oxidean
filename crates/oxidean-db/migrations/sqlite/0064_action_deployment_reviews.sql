-- Deployment review records: one row per approve/reject decision on an
-- environment-held job (GitHub's pending-deployments review history).
-- `reviewer_username` is denormalized — a review is a point-in-time record and
-- survives username changes.
CREATE TABLE action_deployment_reviews (
  id                TEXT PRIMARY KEY,
  repository_id     TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  run_id            TEXT NOT NULL REFERENCES action_runs(id) ON DELETE CASCADE,
  job_id            TEXT NOT NULL REFERENCES action_jobs(id) ON DELETE CASCADE,
  environment       TEXT NOT NULL,
  reviewer_id       TEXT NOT NULL,
  reviewer_username TEXT NOT NULL,
  state             TEXT NOT NULL,
  comment           TEXT NULL,
  created_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);
CREATE INDEX idx_action_deployment_reviews_job ON action_deployment_reviews(job_id);
CREATE INDEX idx_action_deployment_reviews_run ON action_deployment_reviews(run_id);
