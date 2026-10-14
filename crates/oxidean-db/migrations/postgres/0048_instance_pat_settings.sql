-- logical: 0048_instance_pat_settings — instance PAT expiration policy overrides.
-- NULL columns mean "no admin override stored": the API layer falls back to the
-- OXIDEAN_PAT_* env defaults — the same env-default/override split as
-- instance_lfs_settings / instance_mcp_settings.
CREATE TABLE IF NOT EXISTS instance_pat_settings (
  id                  SMALLINT    PRIMARY KEY CHECK (id = 1),
  default_ttl_days    INTEGER     NULL,
  max_ttl_days        INTEGER     NULL,
  allow_never_expire  BOOLEAN     NULL,
  updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO instance_pat_settings (id) VALUES (1) ON CONFLICT (id) DO NOTHING;
