-- logical: 0030_instance_invites_ban — instance email invites + soft-ban

ALTER TABLE users ADD COLUMN banned_at TIMESTAMP NULL;

CREATE TABLE IF NOT EXISTS instance_invites (
  id          CHAR(36)     PRIMARY KEY,
  email       VARCHAR(320) NOT NULL,
  token_hash  CHAR(64)     NOT NULL,
  expires_at  TIMESTAMP    NOT NULL,
  invited_by  CHAR(36)     NOT NULL,
  created_at  TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
  accepted_at TIMESTAMP    NULL,
  revoked_at  TIMESTAMP    NULL,
  UNIQUE KEY uk_instance_invites_token_hash (token_hash),
  CONSTRAINT fk_instance_invites_invited_by FOREIGN KEY (invited_by) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE INDEX idx_instance_invites_email ON instance_invites(email);
