-- logical: 0031_repository_invites — per-repo email invites (read|write|admin)

CREATE TABLE IF NOT EXISTS repository_invites (
  id            TEXT        PRIMARY KEY,
  repository_id TEXT        NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  email         TEXT        NOT NULL,
  permission    TEXT        NOT NULL,
  token_hash    CHAR(64)    NOT NULL UNIQUE,
  expires_at    TIMESTAMPTZ NOT NULL,
  invited_by    TEXT        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  accepted_at   TIMESTAMPTZ NULL,
  revoked_at    TIMESTAMPTZ NULL,
  CONSTRAINT repository_invites_permission_check
    CHECK (permission IN ('read', 'write', 'admin'))
);

CREATE INDEX IF NOT EXISTS idx_repository_invites_repository_id
  ON repository_invites(repository_id);

CREATE INDEX IF NOT EXISTS idx_repository_invites_email_lower
  ON repository_invites (lower(email));
