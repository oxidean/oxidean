-- logical: 0057_actions_permissions_env_oidc — permissions breadth,
-- environments, OIDC issuer (actions parity Phase 7).

-- Default `GITHUB_TOKEN` grant when a workflow declares no `permissions:` —
-- 'read' (restricted: contents+packages read) or 'write' (permissive:
-- every scope write), GitHub's repo "Workflow permissions" setting.
ALTER TABLE repositories ADD COLUMN actions_default_permissions VARCHAR(16) NOT NULL DEFAULT 'read';

-- Job-token scope set — `permission` keeps the `contents` level for the
-- git/LFS synthetic-PAT path; `perms_json` freezes the full GitHub scope map
-- minted at claim ({scope: none|read|write}). `oidc_request_hash` binds the
-- per-job `ACTIONS_ID_TOKEN_REQUEST_TOKEN` (minted only when the granted set
-- carries `id-token: write`).
ALTER TABLE action_job_tokens ADD COLUMN perms_json TEXT NULL;
ALTER TABLE action_job_tokens ADD COLUMN oidc_request_hash VARCHAR(128) NULL;

-- `environment:` on a job — resolved env name + wait-timer state. A leg with
-- `deployment_not_before` in the future stays `waiting` until the scheduler
-- promotes it once the timestamp passes (GitHub environment "wait timer").
ALTER TABLE action_jobs ADD COLUMN environment_name VARCHAR(255) NULL;
ALTER TABLE action_jobs ADD COLUMN deployment_wait_minutes INT NOT NULL DEFAULT 0;
ALTER TABLE action_jobs ADD COLUMN deployment_not_before VARCHAR(64) NULL;

-- Named deployment environments per repository. `protection_json` holds the
-- GitHub protection-rule shape:
--   {"required_reviewers": [user_id...],
--    "wait_timer_minutes": n,
--    "deployment_branches": ["main", "release/*"]}  -- empty = all branches
CREATE TABLE action_environments (
  id            VARCHAR(64) PRIMARY KEY,
  repository_id CHAR(36) NOT NULL,
  name          VARCHAR(255) NOT NULL,
  protection_json TEXT NOT NULL,
  created_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  UNIQUE KEY uq_action_environments_repo_name (repository_id, name),
  CONSTRAINT fk_action_environments_repo FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE
) ENGINE=InnoDB;

-- Deployment records — one per environment-referencing job. `status` is not
-- stored: it derives from the job row so transitions need no mirror writes.
-- `url` is the environment.url the workflow declared (overridable per run).
CREATE TABLE action_deployments (
  id              VARCHAR(64) PRIMARY KEY,
  repository_id   CHAR(36) NOT NULL,
  environment     VARCHAR(255) NOT NULL,
  run_id          CHAR(36) NOT NULL,
  job_id          CHAR(36) NOT NULL,
  sha             VARCHAR(64) NOT NULL,
  ref             VARCHAR(512) NOT NULL,
  url             TEXT NULL,
  created_at      DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at      DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  CONSTRAINT fk_action_deployments_repo FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE,
  CONSTRAINT fk_action_deployments_run FOREIGN KEY (run_id) REFERENCES action_runs(id) ON DELETE CASCADE,
  CONSTRAINT fk_action_deployments_job FOREIGN KEY (job_id) REFERENCES action_jobs(id) ON DELETE CASCADE
) ENGINE=InnoDB;
CREATE INDEX idx_action_deployments_repo_env ON action_deployments(repository_id, environment);
CREATE INDEX idx_action_deployments_run ON action_deployments(run_id);

-- OIDC issuer signing keys — Ed25519 pairs; `private_der` is PKCS#8 hex.
-- `active` signs new tokens; rotated-out keys stay listed so JWKS keeps
-- verifying in-flight JWTs (GitHub rotates similarly).
CREATE TABLE action_oidc_keys (
  kid            VARCHAR(64) PRIMARY KEY,
  alg            VARCHAR(16) NOT NULL DEFAULT 'EdDSA',
  private_der    TEXT NOT NULL,
  public_jwk_json TEXT NOT NULL,
  active         TINYINT(1) NOT NULL DEFAULT 1,
  created_at     DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
