-- logical: 0057_actions_permissions_env_oidc — permissions breadth,
-- environments, OIDC issuer (actions parity Phase 7).

-- Default `GITHUB_TOKEN` grant when a workflow declares no `permissions:` —
-- 'read' (restricted: contents+packages read) or 'write' (permissive:
-- every scope write), GitHub's repo "Workflow permissions" setting.
ALTER TABLE repositories ADD COLUMN actions_default_permissions TEXT NOT NULL DEFAULT 'read';

-- Job-token scope set — `permission` keeps the `contents` level for the
-- git/LFS synthetic-PAT path; `perms_json` freezes the full GitHub scope map
-- minted at claim ({scope: none|read|write}). `oidc_request_hash` binds the
-- per-job `ACTIONS_ID_TOKEN_REQUEST_TOKEN` (minted only when the granted set
-- carries `id-token: write`).
ALTER TABLE action_job_tokens ADD COLUMN perms_json TEXT NULL;
ALTER TABLE action_job_tokens ADD COLUMN oidc_request_hash TEXT NULL;

-- `environment:` on a job — resolved env name + wait-timer state. A leg with
-- `deployment_not_before` in the future stays `waiting` until the scheduler
-- promotes it once the timestamp passes (GitHub environment "wait timer").
ALTER TABLE action_jobs ADD COLUMN environment_name TEXT NULL;
ALTER TABLE action_jobs ADD COLUMN deployment_wait_minutes INTEGER NOT NULL DEFAULT 0;
ALTER TABLE action_jobs ADD COLUMN deployment_not_before TEXT NULL;

-- Named deployment environments per repository. `protection_json` holds the
-- GitHub protection-rule shape:
--   {"required_reviewers": [user_id...],
--    "wait_timer_minutes": n,
--    "deployment_branches": ["main", "release/*"]}  -- empty = all branches
CREATE TABLE action_environments (
  id            TEXT PRIMARY KEY,
  repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  name          TEXT NOT NULL,
  protection_json TEXT NOT NULL DEFAULT '{}',
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (repository_id, name)
);

-- Deployment records — one per environment-referencing job. `status` is not
-- stored: it derives from the job row so transitions need no mirror writes.
-- `url` is the environment.url the workflow declared (overridable per run).
CREATE TABLE action_deployments (
  id              TEXT PRIMARY KEY,
  repository_id   TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  environment     TEXT NOT NULL,
  run_id          TEXT NOT NULL REFERENCES action_runs(id) ON DELETE CASCADE,
  job_id          TEXT NOT NULL REFERENCES action_jobs(id) ON DELETE CASCADE,
  sha             TEXT NOT NULL,
  ref             TEXT NOT NULL,
  url             TEXT NULL,
  created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_action_deployments_repo_env ON action_deployments(repository_id, environment);
CREATE INDEX idx_action_deployments_run ON action_deployments(run_id);

-- OIDC issuer signing keys — Ed25519 pairs; `private_der` is PKCS#8 hex.
-- `active` signs new tokens; rotated-out keys stay listed so JWKS keeps
-- verifying in-flight JWTs (GitHub rotates similarly).
CREATE TABLE action_oidc_keys (
  kid            TEXT PRIMARY KEY,
  alg            TEXT NOT NULL DEFAULT 'EdDSA',
  private_der    TEXT NOT NULL,
  public_jwk_json TEXT NOT NULL,
  active         BOOLEAN NOT NULL DEFAULT true,
  created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- `permission` widens to 'none': a `contents: none` job-token row exists
-- solely to bind the OIDC request hash — the synthesized PAT resolves to no
-- contents access.
ALTER TABLE action_job_tokens DROP CONSTRAINT action_job_tokens_permission_check;
ALTER TABLE action_job_tokens ADD CONSTRAINT action_job_tokens_permission_check
  CHECK (permission IN ('none', 'read', 'write'));
