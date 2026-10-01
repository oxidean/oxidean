-- logical: 0032_invite_links_audit_sessions — shareable invite links (optional
-- expiry + max seats), session client metadata, durable audit events.

-- Sessions: last-known client details (written at create, refreshed on touch).
ALTER TABLE sessions ADD COLUMN ip_address TEXT NULL;
ALTER TABLE sessions ADD COLUMN user_agent TEXT NULL;

-- Invites: email NULL = shareable link; expires_at NULL = never expires;
-- max_uses NULL = unlimited seats; use_count = seats consumed.
-- accepted_at remains the "fully consumed" marker.
ALTER TABLE instance_invites ALTER COLUMN email DROP NOT NULL;
ALTER TABLE instance_invites ALTER COLUMN expires_at DROP NOT NULL;
ALTER TABLE instance_invites ADD COLUMN max_uses INTEGER NULL;
ALTER TABLE instance_invites ADD COLUMN use_count INTEGER NOT NULL DEFAULT 0;
UPDATE instance_invites SET max_uses = 1;
UPDATE instance_invites SET use_count = 1 WHERE accepted_at IS NOT NULL;

ALTER TABLE organization_invites ALTER COLUMN email DROP NOT NULL;
ALTER TABLE organization_invites ALTER COLUMN expires_at DROP NOT NULL;
ALTER TABLE organization_invites ADD COLUMN max_uses INTEGER NULL;
ALTER TABLE organization_invites ADD COLUMN use_count INTEGER NOT NULL DEFAULT 0;
UPDATE organization_invites SET max_uses = 1;
UPDATE organization_invites SET use_count = 1 WHERE accepted_at IS NOT NULL;

ALTER TABLE repository_invites ALTER COLUMN email DROP NOT NULL;
ALTER TABLE repository_invites ALTER COLUMN expires_at DROP NOT NULL;
ALTER TABLE repository_invites ADD COLUMN max_uses INTEGER NULL;
ALTER TABLE repository_invites ADD COLUMN use_count INTEGER NOT NULL DEFAULT 0;
UPDATE repository_invites SET max_uses = 1;
UPDATE repository_invites SET use_count = 1 WHERE accepted_at IS NOT NULL;

-- Durable auth/admin/invite event log (actor_username snapshot survives delete).
CREATE TABLE IF NOT EXISTS audit_events (
  id             TEXT        PRIMARY KEY,
  actor_id       TEXT        NULL REFERENCES users(id) ON DELETE SET NULL,
  actor_username VARCHAR(39) NOT NULL DEFAULT '',
  event_type     TEXT        NOT NULL,
  target_type    TEXT        NULL,
  target_id      TEXT        NULL,
  detail         TEXT        NULL,
  ip_address     TEXT        NULL,
  user_agent     TEXT        NULL,
  created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_audit_events_actor_created
  ON audit_events(actor_id, created_at DESC, id DESC);
