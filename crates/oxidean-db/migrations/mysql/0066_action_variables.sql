-- logical: 0066_action_variables — repository / organization / environment
-- configuration variables (`vars` context + ACTIONS_STEP_DEBUG plumbing).
-- Mirrors the secrets tables but stores plaintext — variables are not
-- secrets; GitHub returns values on GET.

-- Repo-level variables.
CREATE TABLE action_variables (
  id             VARCHAR(64) PRIMARY KEY,
  repository_id  CHAR(36) NOT NULL,
  name           VARCHAR(255) NOT NULL,
  value          TEXT NOT NULL,
  created_at     DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at     DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  UNIQUE KEY uq_action_variables_repo_name (repository_id, name),
  CONSTRAINT fk_action_variables_repo FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE
) ENGINE=InnoDB;
CREATE INDEX idx_action_variables_repository_id ON action_variables(repository_id);

-- Org-level variables — same `visibility` reach model as org secrets:
-- 'all' | 'private' | 'selected' (rows in action_org_variable_repos).
CREATE TABLE action_org_variables (
  id          VARCHAR(64) PRIMARY KEY,
  owner_id    CHAR(36) NOT NULL,
  name        VARCHAR(255) NOT NULL,
  value       TEXT NOT NULL,
  visibility  VARCHAR(16) NOT NULL DEFAULT 'all',
  created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  UNIQUE KEY uq_action_org_variables_owner_name (owner_id, name),
  CONSTRAINT fk_action_org_variables_owner FOREIGN KEY (owner_id) REFERENCES organizations(id) ON DELETE CASCADE
) ENGINE=InnoDB;
CREATE INDEX idx_action_org_variables_owner ON action_org_variables(owner_id);

CREATE TABLE action_org_variable_repos (
  variable_id   VARCHAR(64) NOT NULL,
  repository_id CHAR(36) NOT NULL,
  PRIMARY KEY (variable_id, repository_id),
  CONSTRAINT fk_action_org_variable_repos_var FOREIGN KEY (variable_id) REFERENCES action_org_variables(id) ON DELETE CASCADE,
  CONSTRAINT fk_action_org_variable_repos_repo FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE
) ENGINE=InnoDB;
CREATE INDEX idx_action_org_variable_repos_repo ON action_org_variable_repos(repository_id);

-- Environment-level variables — win over repo and org for jobs targeting
-- that environment (same precedence rule as environment secrets).
CREATE TABLE action_environment_variables (
  id             VARCHAR(64) PRIMARY KEY,
  environment_id VARCHAR(64) NOT NULL,
  name           VARCHAR(255) NOT NULL,
  value          TEXT NOT NULL,
  created_at     DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at     DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  UNIQUE KEY uq_action_env_variables_env_name (environment_id, name),
  CONSTRAINT fk_action_env_variables_env FOREIGN KEY (environment_id) REFERENCES action_environments(id) ON DELETE CASCADE
) ENGINE=InnoDB;
