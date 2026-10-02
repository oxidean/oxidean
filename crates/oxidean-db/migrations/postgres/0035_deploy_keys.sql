-- logical: 0033_deploy_keys — per-repo SSH deploy keys (GIT-23)
-- Deploy keys are transport-only credentials: they authorize git-upload-pack /
-- git-receive-pack for one repository and never act as an account identity.
-- Same public key may be attached to multiple repos but only once per repo
-- (UNIQUE(repo_id, fingerprint)); a fingerprint already registered as an
-- account key (ssh_public_keys) is rejected at the RPC layer.
CREATE TABLE IF NOT EXISTS deploy_keys (
  id             TEXT        PRIMARY KEY,
  repo_id        TEXT        NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  title          TEXT        NOT NULL,
  public_key     TEXT        NOT NULL,
  fingerprint    TEXT        NOT NULL,
  key_type       TEXT        NOT NULL,
  can_write      BOOLEAN     NOT NULL DEFAULT FALSE,
  last_used_at   TIMESTAMPTZ NULL,
  last_used_ip   TEXT        NULL,
  created_by     TEXT        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_deploy_keys_repo_fingerprint
  ON deploy_keys(repo_id, fingerprint);

CREATE INDEX IF NOT EXISTS idx_deploy_keys_fingerprint ON deploy_keys(fingerprint);
CREATE INDEX IF NOT EXISTS idx_deploy_keys_repo_id ON deploy_keys(repo_id);
