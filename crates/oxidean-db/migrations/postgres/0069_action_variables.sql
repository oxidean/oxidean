-- logical: 0066_action_variables — repository / organization / environment
-- configuration variables (`vars` context + ACTIONS_STEP_DEBUG plumbing).
-- Mirrors the secrets tables but stores plaintext — variables are not
-- secrets; GitHub returns values on GET.

-- Repo-level variables.
CREATE TABLE action_variables (
  id             TEXT PRIMARY KEY,
  repository_id  TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  name           TEXT NOT NULL,
  value          TEXT NOT NULL,
  created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (repository_id, name)
);
CREATE INDEX idx_action_variables_repository_id ON action_variables(repository_id);

-- Org-level variables — same `visibility` reach model as org secrets:
-- 'all' | 'private' | 'selected' (rows in action_org_variable_repos).
CREATE TABLE action_org_variables (
  id          TEXT PRIMARY KEY,
  owner_id    TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  name        TEXT NOT NULL,
  value       TEXT NOT NULL,
  visibility  TEXT NOT NULL DEFAULT 'all',
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (owner_id, name)
);
CREATE INDEX idx_action_org_variables_owner ON action_org_variables(owner_id);

CREATE TABLE action_org_variable_repos (
  variable_id   TEXT NOT NULL REFERENCES action_org_variables(id) ON DELETE CASCADE,
  repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  PRIMARY KEY (variable_id, repository_id)
);
CREATE INDEX idx_action_org_variable_repos_repo ON action_org_variable_repos(repository_id);

-- Environment-level variables — win over repo and org for jobs targeting
-- that environment (same precedence rule as environment secrets).
CREATE TABLE action_environment_variables (
  id             TEXT PRIMARY KEY,
  environment_id TEXT NOT NULL REFERENCES action_environments(id) ON DELETE CASCADE,
  name           TEXT NOT NULL,
  value          TEXT NOT NULL,
  created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (environment_id, name)
);
