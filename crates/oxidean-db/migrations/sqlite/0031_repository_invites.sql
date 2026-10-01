-- logical: 0031_repository_invites — per-repo email invites (read|write|admin)

CREATE TABLE IF NOT EXISTS repository_invites (
  id            TEXT PRIMARY KEY,
  repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  email         TEXT NOT NULL,
  permission    TEXT NOT NULL CHECK (permission IN ('read', 'write', 'admin')),
  token_hash    TEXT NOT NULL UNIQUE,
  expires_at    TEXT NOT NULL,
  invited_by    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  accepted_at   TEXT NULL,
  revoked_at    TEXT NULL
);

CREATE INDEX IF NOT EXISTS idx_repository_invites_repository_id
  ON repository_invites(repository_id);

CREATE INDEX IF NOT EXISTS idx_repository_invites_email
  ON repository_invites(email);
