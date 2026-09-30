-- logical: 0030_instance_invites_ban — instance email invites + soft-ban

ALTER TABLE users ADD COLUMN banned_at TEXT NULL;

CREATE TABLE IF NOT EXISTS instance_invites (
  id          TEXT PRIMARY KEY,
  email       TEXT NOT NULL,
  token_hash  TEXT NOT NULL UNIQUE,
  expires_at  TEXT NOT NULL,
  invited_by  TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  accepted_at TEXT NULL,
  revoked_at  TEXT NULL
);

CREATE INDEX IF NOT EXISTS idx_instance_invites_email
  ON instance_invites(email);
