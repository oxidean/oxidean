-- logical: 0046_oauth_device_codes — RFC 8628 device authorization grant for
-- the `ox` CLI (see sqlite sibling for rationale).

CREATE TABLE IF NOT EXISTS oauth_device_codes (
  id               CHAR(36)     PRIMARY KEY,
  device_code_hash CHAR(64)     NOT NULL UNIQUE,
  user_code        VARCHAR(32)  NOT NULL UNIQUE,
  user_id          CHAR(36)     NULL,
  scopes           TEXT         NOT NULL,
  status           VARCHAR(16)  NOT NULL DEFAULT 'pending',
  interval_secs    INT          NOT NULL DEFAULT 5,
  last_polled_at   DATETIME     NULL,
  expires_at       DATETIME     NOT NULL,
  approved_at      DATETIME     NULL,
  created_at       DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
  INDEX idx_oauth_device_codes_user (user_code)
);
