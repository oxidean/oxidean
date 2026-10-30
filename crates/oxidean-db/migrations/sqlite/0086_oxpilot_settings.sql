-- logical: 0086_oxpilot_settings — singleton Oxpilot provider/settings
-- override row (U4). Every field is a sparse override: NULL = follow the
-- matching OXIDEAN_OXPILOT_* env var; a set value wins field-by-field.
-- `key_ciphertext`+`key_last4` are the write-only provider-key custody
-- pair — always set or cleared together, never returned by reads.
CREATE TABLE IF NOT EXISTS oxpilot_settings (
  id               INTEGER  PRIMARY KEY CHECK (id = 1),
  enabled_override INTEGER  NULL,
  provider_id      TEXT     NULL,
  model            TEXT     NULL,
  base_url         TEXT     NULL,
  key_ciphertext   TEXT     NULL,
  key_last4        TEXT     NULL,
  user_budget      INTEGER  NULL,
  instance_budget  INTEGER  NULL,
  updated_at       TEXT     NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);

INSERT OR IGNORE INTO oxpilot_settings (id) VALUES (1);
