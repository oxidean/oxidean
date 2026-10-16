-- logical: 0049_two_factor — TOTP factors, recovery codes, WebAuthn/passkey
-- credentials, and instance/org "require 2FA" enforcement flags (#130).
--
-- TOTP secrets are AES-256-GCM ciphertext (nonce||ct hex) — never plaintext.
-- Recovery codes store SHA-256 hashes only; plaintext is shown once at issue.
-- `confirmed_at NULL` = enrollment in progress (secret issued, code unverified).

CREATE TABLE IF NOT EXISTS user_totp_secrets (
  user_id       CHAR(36)    PRIMARY KEY,
  secret_enc    TEXT        NOT NULL,
  confirmed_at  TIMESTAMP   NULL,
  last_used_at  TIMESTAMP   NULL,
  created_at    TIMESTAMP   NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at    TIMESTAMP   NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_totp_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE TABLE IF NOT EXISTS user_recovery_codes (
  id          CHAR(36)    PRIMARY KEY,
  user_id     CHAR(36)    NOT NULL,
  code_hash   VARCHAR(64) NOT NULL,
  used_at     TIMESTAMP   NULL,
  created_at  TIMESTAMP   NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE KEY uk_recovery_code_hash (code_hash),
  KEY idx_recovery_codes_user (user_id),
  CONSTRAINT fk_recovery_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE TABLE IF NOT EXISTS webauthn_credentials (
  id            CHAR(36)     PRIMARY KEY,
  user_id       CHAR(36)     NOT NULL,
  credential_id VARCHAR(255) NOT NULL,
  nickname      VARCHAR(80)  NOT NULL DEFAULT '',
  passkey_json  TEXT         NOT NULL,
  transports    VARCHAR(255) NOT NULL DEFAULT '',
  sign_count    BIGINT       NOT NULL DEFAULT 0,
  last_used_at  TIMESTAMP    NULL,
  created_at    TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE KEY uk_webauthn_credential_id (credential_id),
  KEY idx_webauthn_credentials_user (user_id),
  CONSTRAINT fk_webauthn_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB;

ALTER TABLE instance_auth_settings
  ADD COLUMN require_two_factor_local BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE instance_auth_settings
  ADD COLUMN require_two_factor_external BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE organizations
  ADD COLUMN require_two_factor BOOLEAN NOT NULL DEFAULT FALSE;
