-- logical: 0036_oauth — OAuth2 provider surface (API-03): registered third-party
-- applications, single-use authorization codes, and hashed access tokens.
-- Secrets/codes/tokens are stored as SHA-256 hex only (hash-at-rest like PATs).
-- MySQL: TEXT cannot carry DEFAULT — use VARCHAR where a default is required.

CREATE TABLE IF NOT EXISTS oauth_applications (
  id                   CHAR(36)     PRIMARY KEY,
  owner_id             CHAR(36)     NOT NULL,
  name                 VARCHAR(200) NOT NULL,
  client_id            VARCHAR(64)  NOT NULL,
  client_secret_hash   CHAR(64)     NOT NULL,
  client_secret_prefix VARCHAR(64)  NOT NULL,
  redirect_uris_json   TEXT         NOT NULL,
  created_at           TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at           TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE KEY uk_oauth_apps_client_id (client_id),
  CONSTRAINT fk_oauth_apps_owner FOREIGN KEY (owner_id) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE INDEX idx_oauth_apps_owner ON oauth_applications(owner_id);

CREATE TABLE IF NOT EXISTS oauth_authorization_codes (
  id             CHAR(36)      PRIMARY KEY,
  code_hash      CHAR(64)      NOT NULL,
  application_id CHAR(36)      NOT NULL,
  user_id        CHAR(36)      NOT NULL,
  redirect_uri   VARCHAR(2048) NOT NULL,
  scopes         VARCHAR(512)  NOT NULL,
  expires_at     TIMESTAMP     NOT NULL,
  used_at        TIMESTAMP     NULL,
  created_at     TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE KEY uk_oauth_codes_hash (code_hash),
  CONSTRAINT fk_oauth_codes_app FOREIGN KEY (application_id) REFERENCES oauth_applications(id) ON DELETE CASCADE,
  CONSTRAINT fk_oauth_codes_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE INDEX idx_oauth_codes_app ON oauth_authorization_codes(application_id);
CREATE INDEX idx_oauth_codes_user ON oauth_authorization_codes(user_id);

CREATE TABLE IF NOT EXISTS oauth_access_tokens (
  id             CHAR(36)     PRIMARY KEY,
  application_id CHAR(36)     NOT NULL,
  user_id        CHAR(36)     NOT NULL,
  token_prefix   VARCHAR(64)  NOT NULL,
  token_hash     CHAR(64)     NOT NULL,
  scopes         VARCHAR(512) NOT NULL,
  expires_at     TIMESTAMP    NOT NULL,
  revoked_at     TIMESTAMP    NULL,
  last_used_at   TIMESTAMP    NULL,
  last_used_ip   VARCHAR(64)  NULL,
  created_at     TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE KEY uk_oauth_tokens_hash (token_hash),
  CONSTRAINT fk_oauth_tokens_app FOREIGN KEY (application_id) REFERENCES oauth_applications(id) ON DELETE CASCADE,
  CONSTRAINT fk_oauth_tokens_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE INDEX idx_oauth_tokens_app ON oauth_access_tokens(application_id);
CREATE INDEX idx_oauth_tokens_user ON oauth_access_tokens(user_id);
