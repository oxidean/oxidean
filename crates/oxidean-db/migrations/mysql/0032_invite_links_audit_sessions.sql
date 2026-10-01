-- logical: 0032_invite_links_audit_sessions — shareable invite links (optional
-- expiry + max seats), session client metadata, durable audit events.

-- Sessions: last-known client details (written at create, refreshed on touch).
ALTER TABLE sessions ADD COLUMN ip_address VARCHAR(64) NULL;
ALTER TABLE sessions ADD COLUMN user_agent VARCHAR(512) NULL;

-- Invites: email NULL = shareable link; expires_at NULL = never expires;
-- max_uses NULL = unlimited seats; use_count = seats consumed.
-- accepted_at remains the "fully consumed" marker.
ALTER TABLE instance_invites MODIFY COLUMN email VARCHAR(320) NULL;
ALTER TABLE instance_invites MODIFY COLUMN expires_at TIMESTAMP NULL;
ALTER TABLE instance_invites ADD COLUMN max_uses INT NULL;
ALTER TABLE instance_invites ADD COLUMN use_count INT NOT NULL DEFAULT 0;
UPDATE instance_invites SET max_uses = 1;
UPDATE instance_invites SET use_count = 1 WHERE accepted_at IS NOT NULL;

ALTER TABLE organization_invites MODIFY COLUMN email VARCHAR(320) NULL;
ALTER TABLE organization_invites MODIFY COLUMN expires_at TIMESTAMP NULL;
ALTER TABLE organization_invites ADD COLUMN max_uses INT NULL;
ALTER TABLE organization_invites ADD COLUMN use_count INT NOT NULL DEFAULT 0;
UPDATE organization_invites SET max_uses = 1;
UPDATE organization_invites SET use_count = 1 WHERE accepted_at IS NOT NULL;

ALTER TABLE repository_invites MODIFY COLUMN email VARCHAR(320) NULL;
ALTER TABLE repository_invites MODIFY COLUMN expires_at TIMESTAMP NULL;
ALTER TABLE repository_invites ADD COLUMN max_uses INT NULL;
ALTER TABLE repository_invites ADD COLUMN use_count INT NOT NULL DEFAULT 0;
UPDATE repository_invites SET max_uses = 1;
UPDATE repository_invites SET use_count = 1 WHERE accepted_at IS NOT NULL;

-- Durable auth/admin/invite event log (actor_username snapshot survives delete).
CREATE TABLE IF NOT EXISTS audit_events (
  id             CHAR(36)     PRIMARY KEY,
  actor_id       CHAR(36)     NULL,
  actor_username VARCHAR(39)  NOT NULL DEFAULT '',
  event_type     VARCHAR(64)  NOT NULL,
  target_type    VARCHAR(32)  NULL,
  target_id      CHAR(36)     NULL,
  detail         VARCHAR(512) NULL,
  ip_address     VARCHAR(64)  NULL,
  user_agent     VARCHAR(512) NULL,
  created_at     TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_audit_events_actor FOREIGN KEY (actor_id) REFERENCES users(id) ON DELETE SET NULL
) ENGINE=InnoDB;

CREATE INDEX idx_audit_events_actor_created
  ON audit_events(actor_id, created_at DESC, id DESC);
