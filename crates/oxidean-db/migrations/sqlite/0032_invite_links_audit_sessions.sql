-- logical: 0032_invite_links_audit_sessions — shareable invite links (optional
-- expiry + max seats), session client metadata, durable audit events.
-- SQLite cannot relax NOT NULL via ALTER, so invite tables are rebuilt.

-- Sessions: last-known client details (written at create, refreshed on touch).
ALTER TABLE sessions ADD COLUMN ip_address TEXT NULL;
ALTER TABLE sessions ADD COLUMN user_agent TEXT NULL;

PRAGMA foreign_keys = OFF;

-- instance_invites: email NULL = shareable link; expires_at NULL = never;
-- max_uses NULL = unlimited seats; use_count = seats consumed.
CREATE TABLE instance_invites_new (
  id          TEXT PRIMARY KEY,
  email       TEXT NULL,
  token_hash  TEXT NOT NULL UNIQUE,
  expires_at  TEXT NULL,
  invited_by  TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  accepted_at TEXT NULL,
  revoked_at  TEXT NULL,
  max_uses    INTEGER NULL,
  use_count   INTEGER NOT NULL DEFAULT 0
);
INSERT INTO instance_invites_new
  (id, email, token_hash, expires_at, invited_by, created_at, accepted_at,
   revoked_at, max_uses, use_count)
SELECT id, email, token_hash, expires_at, invited_by, created_at, accepted_at,
       revoked_at, 1, CASE WHEN accepted_at IS NULL THEN 0 ELSE 1 END
FROM instance_invites;
DROP TABLE instance_invites;
ALTER TABLE instance_invites_new RENAME TO instance_invites;
CREATE INDEX IF NOT EXISTS idx_instance_invites_email ON instance_invites(email);

CREATE TABLE organization_invites_new (
  id          TEXT PRIMARY KEY,
  org_id      TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  email       TEXT NULL,
  role        TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member')),
  token_hash  TEXT NOT NULL UNIQUE,
  expires_at  TEXT NULL,
  invited_by  TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  accepted_at TEXT NULL,
  revoked_at  TEXT NULL,
  max_uses    INTEGER NULL,
  use_count   INTEGER NOT NULL DEFAULT 0
);
INSERT INTO organization_invites_new
  (id, org_id, email, role, token_hash, expires_at, invited_by, created_at,
   accepted_at, revoked_at, max_uses, use_count)
SELECT id, org_id, email, role, token_hash, expires_at, invited_by, created_at,
       accepted_at, revoked_at, 1, CASE WHEN accepted_at IS NULL THEN 0 ELSE 1 END
FROM organization_invites;
DROP TABLE organization_invites;
ALTER TABLE organization_invites_new RENAME TO organization_invites;
CREATE INDEX IF NOT EXISTS idx_organization_invites_org_id
  ON organization_invites(org_id);

CREATE TABLE repository_invites_new (
  id            TEXT PRIMARY KEY,
  repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  email         TEXT NULL,
  permission    TEXT NOT NULL CHECK (permission IN ('read', 'write', 'admin')),
  token_hash    TEXT NOT NULL UNIQUE,
  expires_at    TEXT NULL,
  invited_by    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  accepted_at   TEXT NULL,
  revoked_at    TEXT NULL,
  max_uses      INTEGER NULL,
  use_count     INTEGER NOT NULL DEFAULT 0
);
INSERT INTO repository_invites_new
  (id, repository_id, email, permission, token_hash, expires_at, invited_by,
   created_at, accepted_at, revoked_at, max_uses, use_count)
SELECT id, repository_id, email, permission, token_hash, expires_at, invited_by,
       created_at, accepted_at, revoked_at, 1,
       CASE WHEN accepted_at IS NULL THEN 0 ELSE 1 END
FROM repository_invites;
DROP TABLE repository_invites;
ALTER TABLE repository_invites_new RENAME TO repository_invites;
CREATE INDEX IF NOT EXISTS idx_repository_invites_repository_id
  ON repository_invites(repository_id);
CREATE INDEX IF NOT EXISTS idx_repository_invites_email
  ON repository_invites(email);

PRAGMA foreign_keys = ON;

-- Durable auth/admin/invite event log (actor_username snapshot survives delete).
CREATE TABLE IF NOT EXISTS audit_events (
  id             TEXT PRIMARY KEY,
  actor_id       TEXT NULL REFERENCES users(id) ON DELETE SET NULL,
  actor_username TEXT NOT NULL DEFAULT '',
  event_type     TEXT NOT NULL,
  target_type    TEXT NULL,
  target_id      TEXT NULL,
  detail         TEXT NULL,
  ip_address     TEXT NULL,
  user_agent     TEXT NULL,
  created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);

CREATE INDEX IF NOT EXISTS idx_audit_events_actor_created
  ON audit_events(actor_id, created_at DESC, id DESC);
