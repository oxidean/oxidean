-- Public deployments surface (GitHub Deployments API parity):
--  * run_id/job_id become nullable — API-created deployments carry no job.
--  * task/description/payload/creator/transient/production fields added.
--  * action_deployment_statuses records the status history each
--    `deployment_status` event is emitted from.

ALTER TABLE action_deployments
  ALTER COLUMN run_id DROP NOT NULL,
  ALTER COLUMN job_id DROP NOT NULL,
  ADD COLUMN task TEXT NOT NULL DEFAULT 'deploy',
  ADD COLUMN description TEXT NOT NULL DEFAULT '',
  ADD COLUMN payload JSONB NULL,
  ADD COLUMN creator_id TEXT NULL REFERENCES users(id) ON DELETE SET NULL,
  ADD COLUMN transient_environment BOOLEAN NOT NULL DEFAULT false,
  ADD COLUMN production_environment BOOLEAN NOT NULL DEFAULT false,
  ADD COLUMN seq BIGINT GENERATED ALWAYS AS IDENTITY;

CREATE INDEX idx_action_deployments_job ON action_deployments(job_id);

CREATE TABLE action_deployment_statuses (
  id              TEXT PRIMARY KEY,
  seq             BIGINT GENERATED ALWAYS AS IDENTITY,
  deployment_id   TEXT NOT NULL REFERENCES action_deployments(id) ON DELETE CASCADE,
  state           TEXT NOT NULL,
  description     TEXT NOT NULL DEFAULT '',
  environment     TEXT NULL,
  environment_url TEXT NULL,
  target_url      TEXT NULL,
  log_url         TEXT NULL,
  creator_id      TEXT NULL REFERENCES users(id) ON DELETE SET NULL,
  created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_action_deployment_statuses_dep ON action_deployment_statuses(deployment_id, created_at);
