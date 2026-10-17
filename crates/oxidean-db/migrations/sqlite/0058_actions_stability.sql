-- logical: 0058_actions_stability — runner heartbeat, log ordering,
-- org + environment secrets (actions parity Phase 8).

-- Runner liveness per job: `heartbeat_at` bumps on every runner contact
-- (update_log/update_task); the sweeper requeues `in_progress` jobs whose
-- heartbeat went silent. `log_index` is the next expected log batch index —
-- replays drop idempotently, gaps 409 so the runner retries.
ALTER TABLE action_jobs ADD COLUMN heartbeat_at TEXT NULL;
ALTER TABLE action_jobs ADD COLUMN log_index INTEGER NOT NULL DEFAULT 0;

-- Org-level Actions secrets (GitHub org secrets): `visibility` selects the
-- repo reach — 'all' (every repo in the org), 'private' (private repos
-- only), 'selected' (rows in action_org_secret_repos).
CREATE TABLE action_org_secrets (
  id          TEXT PRIMARY KEY,
  owner_id    TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  name        TEXT NOT NULL,
  ciphertext  TEXT NOT NULL,
  visibility  TEXT NOT NULL DEFAULT 'all',
  created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  UNIQUE (owner_id, name)
);
CREATE INDEX idx_action_org_secrets_owner ON action_org_secrets(owner_id);

CREATE TABLE action_org_secret_repos (
  secret_id     TEXT NOT NULL REFERENCES action_org_secrets(id) ON DELETE CASCADE,
  repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  PRIMARY KEY (secret_id, repository_id)
);
CREATE INDEX idx_action_org_secret_repos_repo ON action_org_secret_repos(repository_id);

-- Environment-level secrets — scoped to one action_environments row, they
-- win over repo and org secrets for jobs targeting that environment.
CREATE TABLE action_environment_secrets (
  id             TEXT PRIMARY KEY,
  environment_id TEXT NOT NULL REFERENCES action_environments(id) ON DELETE CASCADE,
  name           TEXT NOT NULL,
  ciphertext     TEXT NOT NULL,
  created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  UNIQUE (environment_id, name)
);
