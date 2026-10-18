-- logical: 0058_actions_stability — runner heartbeat, log ordering,
-- org + environment secrets (actions parity Phase 8).

-- Runner liveness per job: `heartbeat_at` bumps on every runner contact
-- (update_log/update_task); the sweeper requeues `in_progress` jobs whose
-- heartbeat went silent. `log_index` is the next expected log batch index —
-- replays drop idempotently, gaps 409 so the runner retries.
ALTER TABLE action_jobs ADD COLUMN heartbeat_at VARCHAR(64) NULL;
ALTER TABLE action_jobs ADD COLUMN log_index INT NOT NULL DEFAULT 0;

-- Org-level Actions secrets (GitHub org secrets): `visibility` selects the
-- repo reach — 'all' (every repo in the org), 'private' (private repos
-- only), 'selected' (rows in action_org_secret_repos).
CREATE TABLE action_org_secrets (
  id          VARCHAR(64) PRIMARY KEY,
  owner_id    CHAR(36) NOT NULL,
  name        VARCHAR(255) NOT NULL,
  ciphertext  TEXT NOT NULL,
  visibility  VARCHAR(16) NOT NULL DEFAULT 'all',
  created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  UNIQUE KEY uq_action_org_secrets_owner_name (owner_id, name),
  CONSTRAINT fk_action_org_secrets_owner FOREIGN KEY (owner_id) REFERENCES organizations(id) ON DELETE CASCADE
) ENGINE=InnoDB;
CREATE INDEX idx_action_org_secrets_owner ON action_org_secrets(owner_id);

CREATE TABLE action_org_secret_repos (
  secret_id     VARCHAR(64) NOT NULL,
  repository_id CHAR(36) NOT NULL,
  PRIMARY KEY (secret_id, repository_id),
  CONSTRAINT fk_action_org_secret_repos_secret FOREIGN KEY (secret_id) REFERENCES action_org_secrets(id) ON DELETE CASCADE,
  CONSTRAINT fk_action_org_secret_repos_repo FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE
) ENGINE=InnoDB;
CREATE INDEX idx_action_org_secret_repos_repo ON action_org_secret_repos(repository_id);

-- Environment-level secrets — scoped to one action_environments row, they
-- win over repo and org secrets for jobs targeting that environment.
CREATE TABLE action_environment_secrets (
  id             VARCHAR(64) PRIMARY KEY,
  environment_id VARCHAR(64) NOT NULL,
  name           VARCHAR(255) NOT NULL,
  ciphertext     TEXT NOT NULL,
  created_at     DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at     DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  UNIQUE KEY uq_action_env_secrets_env_name (environment_id, name),
  CONSTRAINT fk_action_env_secrets_env FOREIGN KEY (environment_id) REFERENCES action_environments(id) ON DELETE CASCADE
) ENGINE=InnoDB;
