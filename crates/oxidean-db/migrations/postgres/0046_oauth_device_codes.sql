-- logical: 0046_oauth_device_codes — RFC 8628 device authorization grant for
-- the `ox` CLI (see sqlite sibling for rationale).

CREATE TABLE IF NOT EXISTS oauth_device_codes (
  id               TEXT        PRIMARY KEY,
  device_code_hash CHAR(64)    NOT NULL UNIQUE,
  user_code        TEXT        NOT NULL UNIQUE,
  user_id          TEXT        NULL REFERENCES users(id) ON DELETE CASCADE,
  scopes           TEXT        NOT NULL,
  status           TEXT        NOT NULL DEFAULT 'pending',
  interval_secs    INTEGER     NOT NULL DEFAULT 5,
  last_polled_at   TIMESTAMPTZ NULL,
  expires_at       TIMESTAMPTZ NOT NULL,
  approved_at      TIMESTAMPTZ NULL,
  created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_oauth_device_codes_user ON oauth_device_codes(user_code);
