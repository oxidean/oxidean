-- logical: 0030_instance_invites_ban — instance email invites + soft-ban

ALTER TABLE users ADD COLUMN banned_at TIMESTAMPTZ NULL;

CREATE TABLE IF NOT EXISTS instance_invites (
  id          TEXT        PRIMARY KEY,
  email       TEXT        NOT NULL,
  token_hash  CHAR(64)    NOT NULL UNIQUE,
  expires_at  TIMESTAMPTZ NOT NULL,
  invited_by  TEXT        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  accepted_at TIMESTAMPTZ NULL,
  revoked_at  TIMESTAMPTZ NULL
);

CREATE INDEX IF NOT EXISTS idx_instance_invites_email_lower
  ON instance_invites (lower(email));
