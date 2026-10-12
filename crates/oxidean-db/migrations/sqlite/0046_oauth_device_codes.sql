-- logical: 0046_oauth_device_codes — RFC 8628 device authorization grant for
-- the `ox` CLI: device_code (hashed) + human-typable user_code, pending until
-- a signed-in user approves/denies on the /oauth/device page. The poll-side
-- token exchange mints a classic PAT on first success (option (a) from the
-- CLI auth design) — no plaintext is stored at rest.

CREATE TABLE IF NOT EXISTS oauth_device_codes (
  id               TEXT PRIMARY KEY,
  device_code_hash TEXT NOT NULL UNIQUE,
  user_code        TEXT NOT NULL UNIQUE,        -- normalized (upper, no dash)
  user_id          TEXT NULL REFERENCES users(id) ON DELETE CASCADE,
  scopes           TEXT NOT NULL,               -- space-delimited classic PAT scopes
  status           TEXT NOT NULL DEFAULT 'pending',  -- pending|approved|denied|consumed
  interval_secs    INTEGER NOT NULL DEFAULT 5,
  last_polled_at   TEXT NULL,
  expires_at       TEXT NOT NULL,
  approved_at      TEXT NULL,
  created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);

CREATE INDEX IF NOT EXISTS idx_oauth_device_codes_user ON oauth_device_codes(user_code);
