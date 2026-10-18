-- Public deployments surface (GitHub Deployments API parity):
--  * run_id/job_id become nullable — API-created deployments carry no job.
--  * task/description/payload/creator/transient/production fields added.
--  * action_deployment_statuses records the status history each
--    `deployment_status` event is emitted from.

ALTER TABLE action_deployments
  MODIFY run_id CHAR(36) NULL,
  MODIFY job_id CHAR(36) NULL,
  ADD COLUMN task VARCHAR(64) NOT NULL DEFAULT 'deploy',
  ADD COLUMN description TEXT NOT NULL,
  ADD COLUMN payload JSON NULL,
  ADD COLUMN creator_id CHAR(36) NULL,
  ADD COLUMN transient_environment TINYINT(1) NOT NULL DEFAULT 0,
  ADD COLUMN production_environment TINYINT(1) NOT NULL DEFAULT 0,
  ADD COLUMN seq BIGINT NOT NULL AUTO_INCREMENT UNIQUE,
  ADD CONSTRAINT fk_action_deployments_creator FOREIGN KEY (creator_id) REFERENCES users(id) ON DELETE SET NULL;

CREATE INDEX idx_action_deployments_job ON action_deployments(job_id);

CREATE TABLE action_deployment_statuses (
  id              VARCHAR(64) PRIMARY KEY,
  seq             BIGINT NOT NULL AUTO_INCREMENT,
  deployment_id   VARCHAR(64) NOT NULL,
  state           VARCHAR(32) NOT NULL,
  description     TEXT NOT NULL,
  environment     VARCHAR(255) NULL,
  environment_url TEXT NULL,
  target_url      TEXT NULL,
  log_url         TEXT NULL,
  creator_id      CHAR(36) NULL,
  created_at      DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE KEY uq_action_deployment_statuses_seq (seq),
  CONSTRAINT fk_action_deployment_statuses_dep FOREIGN KEY (deployment_id) REFERENCES action_deployments(id) ON DELETE CASCADE,
  CONSTRAINT fk_action_deployment_statuses_creator FOREIGN KEY (creator_id) REFERENCES users(id) ON DELETE SET NULL
);
CREATE INDEX idx_action_deployment_statuses_dep ON action_deployment_statuses(deployment_id, created_at);
