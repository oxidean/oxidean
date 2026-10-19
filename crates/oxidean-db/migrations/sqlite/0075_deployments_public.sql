-- Public deployments surface (GitHub Deployments API parity):
--  * run_id/job_id become nullable — API-created deployments carry no job.
--  * task/description/payload/creator/transient/production fields added.
--  * action_deployment_statuses records the status history each
--    `deployment_status` event is emitted from.

CREATE TABLE action_deployments_new (
  id              TEXT PRIMARY KEY,
  repository_id   TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  environment     TEXT NOT NULL,
  run_id          TEXT NULL REFERENCES action_runs(id) ON DELETE CASCADE,
  job_id          TEXT NULL REFERENCES action_jobs(id) ON DELETE CASCADE,
  sha             TEXT NOT NULL,
  ref             TEXT NOT NULL,
  url             TEXT NULL,
  task            TEXT NOT NULL DEFAULT 'deploy',
  description     TEXT NOT NULL DEFAULT '',
  payload         TEXT NULL,
  creator_id      TEXT NULL REFERENCES users(id) ON DELETE SET NULL,
  transient_environment INTEGER NOT NULL DEFAULT 0,
  production_environment  INTEGER NOT NULL DEFAULT 0,
  created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);
-- `seq` comes from rowid: the implicit rowid is the monotonic insertion
-- order used as the created_at tiebreak in ORDER BY.
INSERT INTO action_deployments_new (id, repository_id, environment, run_id, job_id, sha, ref, url, created_at, updated_at)
  SELECT id, repository_id, environment, run_id, job_id, sha, ref, url, created_at, updated_at FROM action_deployments;
DROP TABLE action_deployments;
ALTER TABLE action_deployments_new RENAME TO action_deployments;
CREATE INDEX idx_action_deployments_repo_env ON action_deployments(repository_id, environment);
CREATE INDEX idx_action_deployments_run ON action_deployments(run_id);
CREATE INDEX idx_action_deployments_job ON action_deployments(job_id);

CREATE TABLE action_deployment_statuses (
  id              TEXT PRIMARY KEY,
  deployment_id   TEXT NOT NULL REFERENCES action_deployments(id) ON DELETE CASCADE,
  state           TEXT NOT NULL,
  description     TEXT NOT NULL DEFAULT '',
  environment     TEXT NULL,
  environment_url TEXT NULL,
  target_url      TEXT NULL,
  log_url         TEXT NULL,
  creator_id      TEXT NULL REFERENCES users(id) ON DELETE SET NULL,
  created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);
CREATE INDEX idx_action_deployment_statuses_dep ON action_deployment_statuses(deployment_id, created_at);
