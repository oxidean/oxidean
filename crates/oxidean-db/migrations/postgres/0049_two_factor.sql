-- logical: 0049_two_factor — TOTP factors, recovery codes, WebAuthn/passkey
-- credentials, and instance/org "require 2FA" enforcement flags (#130).
--
-- TOTP secrets are AES-256-GCM ciphertext (nonce||ct hex) — never plaintext.
-- Recovery codes store SHA-256 hashes only; plaintext is shown once at issue.
-- `confirmed_at NULL` = enrollment in progress (secret issued, code unverified).

CREATE TABLE IF NOT EXISTS user_totp_secrets (
  user_id       TEXT        PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
  secret_enc    TEXT        NOT NULL,
  confirmed_at  TIMESTAMPTZ NULL,
  last_used_at  TIMESTAMPTZ NULL,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS user_recovery_codes (
  id          TEXT        PRIMARY KEY,
  user_id     TEXT        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  code_hash   TEXT        NOT NULL UNIQUE,
  used_at     TIMESTAMPTZ NULL,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_user_recovery_codes_user
  ON user_recovery_codes (user_id);

CREATE TABLE IF NOT EXISTS webauthn_credentials (
  id            TEXT        PRIMARY KEY,
  user_id       TEXT        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  credential_id TEXT        NOT NULL UNIQUE,
  nickname      TEXT        NOT NULL DEFAULT '',
  passkey_json  TEXT        NOT NULL,
  transports    TEXT        NOT NULL DEFAULT '',
  sign_count    BIGINT      NOT NULL DEFAULT 0,
  last_used_at  TIMESTAMPTZ NULL,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_webauthn_credentials_user
  ON webauthn_credentials (user_id);

ALTER TABLE instance_auth_settings
  ADD COLUMN require_two_factor_local BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE instance_auth_settings
  ADD COLUMN require_two_factor_external BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE organizations
  ADD COLUMN require_two_factor BOOLEAN NOT NULL DEFAULT FALSE;
