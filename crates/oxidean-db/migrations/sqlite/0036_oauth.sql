-- logical: 0033_oauth — OAuth2 provider surface (API-03): registered third-party
-- applications, single-use authorization codes, and hashed access tokens.
-- Secrets/codes/tokens are stored as SHA-256 hex only (hash-at-rest like PATs).

CREATE TABLE IF NOT EXISTS oauth_applications (
  id                   TEXT PRIMARY KEY,
  owner_id             TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  name                 TEXT NOT NULL,
  client_id            TEXT NOT NULL UNIQUE,
  client_secret_hash   TEXT NOT NULL,
  client_secret_prefix TEXT NOT NULL,
  redirect_uris_json   TEXT NOT NULL,
  created_at           TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  updated_at           TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);

CREATE INDEX IF NOT EXISTS idx_oauth_apps_owner ON oauth_applications(owner_id);

CREATE TABLE IF NOT EXISTS oauth_authorization_codes (
  id             TEXT PRIMARY KEY,
  code_hash      TEXT NOT NULL UNIQUE,
  application_id TEXT NOT NULL REFERENCES oauth_applications(id) ON DELETE CASCADE,
  user_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  redirect_uri   TEXT NOT NULL,
  scopes         TEXT NOT NULL,
  expires_at     TEXT NOT NULL,
  used_at        TEXT NULL,
  created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);

CREATE INDEX IF NOT EXISTS idx_oauth_codes_app ON oauth_authorization_codes(application_id);
CREATE INDEX IF NOT EXISTS idx_oauth_codes_user ON oauth_authorization_codes(user_id);

CREATE TABLE IF NOT EXISTS oauth_access_tokens (
  id             TEXT PRIMARY KEY,
  application_id TEXT NOT NULL REFERENCES oauth_applications(id) ON DELETE CASCADE,
  user_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  token_prefix   TEXT NOT NULL,
  token_hash     TEXT NOT NULL UNIQUE,
  scopes         TEXT NOT NULL,
  expires_at     TEXT NOT NULL,
  revoked_at     TEXT NULL,
  last_used_at   TEXT NULL,
  last_used_ip   TEXT NULL,
  created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);

CREATE INDEX IF NOT EXISTS idx_oauth_tokens_app ON oauth_access_tokens(application_id);
CREATE INDEX IF NOT EXISTS idx_oauth_tokens_user ON oauth_access_tokens(user_id);
