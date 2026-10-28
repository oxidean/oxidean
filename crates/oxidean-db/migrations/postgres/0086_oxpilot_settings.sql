-- logical: 0086_oxpilot_settings — singleton Oxpilot provider/settings
-- override row (U4). Every field is a sparse override: NULL = follow the
-- matching OXIDEAN_OXPILOT_* env var; a set value wins field-by-field.
-- `key_ciphertext`+`key_last4` are the write-only provider-key custody
-- pair — always set or cleared together, never returned by reads.
CREATE TABLE IF NOT EXISTS oxpilot_settings (
  id               SMALLINT     PRIMARY KEY CHECK (id = 1),
  enabled_override BOOLEAN      NULL,
  provider_id      TEXT         NULL,
  model            TEXT         NULL,
  base_url         TEXT         NULL,
  key_ciphertext   TEXT         NULL,
  key_last4        TEXT         NULL,
  user_budget      BIGINT       NULL,
  instance_budget  BIGINT       NULL,
  updated_at       TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

INSERT INTO oxpilot_settings (id) VALUES (1) ON CONFLICT (id) DO NOTHING;
